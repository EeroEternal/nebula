//! Lightweight online TTFT / TPOT predictor for homogeneous inference pools.
//!
//! Inspired by `llm-d`'s Latency Predictor: uses stratified bucketing over
//! `(kv_cache_usage, prefix_match_ratio)` so rare traffic regimes are retained,
//! trains per-model Ridge-regularized linear models online from observed
//! request latencies, and returns `None` until warmed (or on solver failure)
//! so the router falls back cleanly to heuristic scoring.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

const DEFAULT_MIN_SAMPLES: usize = 10;
const DEFAULT_BUCKET_CAP: usize = 64;
const DEFAULT_RETRAIN_INTERVAL: usize = 10;
const RIDGE_LAMBDA: f64 = 1e-2;

#[derive(Debug, Clone, Copy, Default)]
pub struct LatencyFeatures {
    pub input_tokens: f64,
    pub pending_requests: f64,
    pub running_requests: f64,
    pub kv_cache_usage: f64,
    pub prefix_match_ratio: f64,
}

impl LatencyFeatures {
    fn ttft_vec(&self) -> [f64; 5] {
        let uncached_ratio = (1.0 - self.prefix_match_ratio.clamp(0.0, 1.0)).max(0.0);
        [
            1.0,
            (self.input_tokens.max(0.0) / 1000.0) * uncached_ratio,
            self.pending_requests.max(0.0),
            self.kv_cache_usage.clamp(0.0, 1.0),
            self.running_requests.max(0.0),
        ]
    }

    fn tpot_vec(&self) -> [f64; 4] {
        [
            1.0,
            self.running_requests.max(0.0),
            self.pending_requests.max(0.0),
            self.kv_cache_usage.clamp(0.0, 1.0),
        ]
    }

    fn bucket_key(&self) -> (u8, u8) {
        let kv_b = (self.kv_cache_usage.clamp(0.0, 0.999) * 10.0).floor() as u8;
        let pfx_b = (self.prefix_match_ratio.clamp(0.0, 0.999) * 4.0).floor() as u8;
        (kv_b, pfx_b)
    }
}

#[derive(Clone)]
struct Sample {
    features: LatencyFeatures,
    ttft_ms: Option<f64>,
    tpot_ms: Option<f64>,
}

#[derive(Default)]
struct ModelPredictorState {
    buckets: HashMap<(u8, u8), VecDeque<Sample>>,
    ttft_weights: Option<[f64; 5]>,
    tpot_weights: Option<[f64; 4]>,
    samples_since_retrain: usize,
    total_samples: usize,
}

pub struct OnlineLatencyPredictor {
    enabled: AtomicBool,
    min_samples: usize,
    bucket_cap: usize,
    retrain_interval: usize,
    models: Mutex<HashMap<String, ModelPredictorState>>,
}

impl Default for OnlineLatencyPredictor {
    fn default() -> Self {
        let enabled = std::env::var("NEBULA_LATENCY_PREDICTOR")
            .ok()
            .map(|v| {
                let s = v.trim().to_ascii_lowercase();
                s == "1" || s == "true" || s == "yes"
            })
            .unwrap_or(false);
        Self::new(enabled)
    }
}

impl OnlineLatencyPredictor {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled: AtomicBool::new(enabled),
            min_samples: DEFAULT_MIN_SAMPLES,
            bucket_cap: DEFAULT_BUCKET_CAP,
            retrain_interval: DEFAULT_RETRAIN_INTERVAL,
            models: Mutex::new(HashMap::new()),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }

    pub fn record_sample(
        &self,
        model_uid: &str,
        features: LatencyFeatures,
        ttft_ms: Option<f64>,
        tpot_ms: Option<f64>,
    ) {
        if !self.is_enabled() {
            return;
        }
        let valid_ttft = ttft_ms.filter(|v| v.is_finite() && *v >= 0.0);
        let valid_tpot = tpot_ms.filter(|v| v.is_finite() && *v >= 0.0);
        if valid_ttft.is_none() && valid_tpot.is_none() {
            return;
        }

        let mut map = self.models.lock().unwrap_or_else(|e| e.into_inner());
        let st = map.entry(model_uid.to_string()).or_default();
        let bkey = features.bucket_key();
        let bucket = st.buckets.entry(bkey).or_default();
        if bucket.len() >= self.bucket_cap {
            bucket.pop_front();
        }
        bucket.push_back(Sample {
            features,
            ttft_ms: valid_ttft,
            tpot_ms: valid_tpot,
        });
        st.total_samples += 1;
        st.samples_since_retrain += 1;

        let should_retrain = (st.ttft_weights.is_none() && st.total_samples >= self.min_samples)
            || st.samples_since_retrain >= self.retrain_interval;
        if should_retrain && st.total_samples >= self.min_samples {
            st.samples_since_retrain = 0;
            let ttft_rows: Vec<([f64; 5], f64)> = st
                .buckets
                .values()
                .flat_map(|q| q.iter())
                .filter_map(|s| s.ttft_ms.map(|y| (s.features.ttft_vec(), y)))
                .collect();
            if ttft_rows.len() >= self.min_samples {
                st.ttft_weights = fit_ridge::<5>(&ttft_rows, RIDGE_LAMBDA);
            }
            let tpot_rows: Vec<([f64; 4], f64)> = st
                .buckets
                .values()
                .flat_map(|q| q.iter())
                .filter_map(|s| s.tpot_ms.map(|y| (s.features.tpot_vec(), y)))
                .collect();
            if tpot_rows.len() >= self.min_samples {
                st.tpot_weights = fit_ridge::<4>(&tpot_rows, RIDGE_LAMBDA);
            }
        }
    }

    pub fn predict_ttft_ms(&self, model_uid: &str, features: &LatencyFeatures) -> Option<f64> {
        if !self.is_enabled() {
            return None;
        }
        let map = self.models.lock().unwrap_or_else(|e| e.into_inner());
        let w = map.get(model_uid)?.ttft_weights?;
        let x = features.ttft_vec();
        let pred: f64 = w.iter().zip(x.iter()).map(|(wi, xi)| wi * xi).sum();
        if pred.is_finite() {
            Some(pred.max(0.0))
        } else {
            None
        }
    }

    pub fn predict_tpot_ms(&self, model_uid: &str, features: &LatencyFeatures) -> Option<f64> {
        if !self.is_enabled() {
            return None;
        }
        let map = self.models.lock().unwrap_or_else(|e| e.into_inner());
        let w = map.get(model_uid)?.tpot_weights?;
        let x = features.tpot_vec();
        let pred: f64 = w.iter().zip(x.iter()).map(|(wi, xi)| wi * xi).sum();
        if pred.is_finite() {
            Some(pred.max(0.0))
        } else {
            None
        }
    }

    pub fn bucket_count(&self, model_uid: &str) -> usize {
        self.models
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(model_uid)
            .map(|s| s.buckets.len())
            .unwrap_or(0)
    }
}

/// Solve `(X^T X + lambda * I) w = X^T y` using Gaussian elimination with
/// partial pivoting. Returns `None` if the system is singular.
#[allow(clippy::needless_range_loop)]
fn fit_ridge<const D: usize>(rows: &[([f64; D], f64)], lambda: f64) -> Option<[f64; D]> {
    let mut a = [[0.0_f64; D]; D];
    let mut b = [0.0_f64; D];
    for (x, y) in rows {
        for i in 0..D {
            b[i] += x[i] * y;
            for j in 0..D {
                a[i][j] += x[i] * x[j];
            }
        }
    }
    for i in 0..D {
        // Do not penalize the intercept (index 0)
        if i > 0 {
            a[i][i] += lambda;
        } else {
            a[i][i] += 1e-6;
        }
    }

    // Gaussian elimination with partial pivoting
    for col in 0..D {
        let mut pivot_row = col;
        let mut max_val = a[col][col].abs();
        for row in (col + 1)..D {
            let v = a[row][col].abs();
            if v > max_val {
                max_val = v;
                pivot_row = row;
            }
        }
        if max_val < 1e-12 {
            return None;
        }
        if pivot_row != col {
            a.swap(col, pivot_row);
            b.swap(col, pivot_row);
        }
        let pivot = a[col][col];
        for row in (col + 1)..D {
            let factor = a[row][col] / pivot;
            for j in col..D {
                a[row][j] -= factor * a[col][j];
            }
            b[row] -= factor * b[col];
        }
    }

    let mut w = [0.0_f64; D];
    for i in (0..D).rev() {
        let mut sum = b[i];
        for j in (i + 1)..D {
            sum -= a[i][j] * w[j];
        }
        w[i] = sum / a[i][i];
        if !w[i].is_finite() {
            return None;
        }
    }
    Some(w)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cold_predictor_returns_none_for_heuristic_fallback() {
        let pred = OnlineLatencyPredictor::new(true);
        let f = LatencyFeatures {
            input_tokens: 1000.0,
            pending_requests: 2.0,
            running_requests: 1.0,
            kv_cache_usage: 0.3,
            prefix_match_ratio: 0.0,
        };
        assert!(pred.predict_ttft_ms("m1", &f).is_none());
        assert!(pred.predict_tpot_ms("m1", &f).is_none());
    }

    #[test]
    fn online_predictor_learns_ttft_and_tpot_linearly() {
        let pred = OnlineLatencyPredictor::new(true);
        for i in 0..20 {
            let input_tokens = 500.0 + (i as f64) * 200.0;
            let pending = (i % 5) as f64;
            let running = ((i + 1) % 4) as f64;
            let kv = 0.1 + ((i % 4) as f64) * 0.2;
            let prefix_hit = if i % 2 == 0 { 0.0 } else { 0.5 };
            let f = LatencyFeatures {
                input_tokens,
                pending_requests: pending,
                running_requests: running,
                kv_cache_usage: kv,
                prefix_match_ratio: prefix_hit,
            };
            let uncached_k = (input_tokens / 1000.0) * (1.0 - prefix_hit);
            let true_ttft = 20.0 + 15.0 * uncached_k + 30.0 * pending;
            let true_tpot = 10.0 + 4.0 * running + 2.0 * pending;
            pred.record_sample("m1", f, Some(true_ttft), Some(true_tpot));
        }

        let probe = LatencyFeatures {
            input_tokens: 2000.0,
            pending_requests: 3.0,
            running_requests: 2.0,
            kv_cache_usage: 0.4,
            prefix_match_ratio: 0.5,
        };
        let p_ttft = pred.predict_ttft_ms("m1", &probe).expect("trained ttft");
        let p_tpot = pred.predict_tpot_ms("m1", &probe).expect("trained tpot");
        // Expected: ttft = 20 + 15*(2*0.5) + 30*3 = 125.0; tpot = 10 + 4*2 + 2*3 = 24.0
        assert!((p_ttft - 125.0).abs() < 2.0, "p_ttft={p_ttft}");
        assert!((p_tpot - 24.0).abs() < 1.0, "p_tpot={p_tpot}");
    }

    #[test]
    fn stratified_bucketing_retains_rare_cold_regimes() {
        let pred = OnlineLatencyPredictor::new(true);
        // Record 10 rare cold samples
        for i in 0..10 {
            pred.record_sample(
                "m1",
                LatencyFeatures {
                    input_tokens: 1000.0 + (i as f64) * 100.0,
                    pending_requests: 0.0,
                    running_requests: 1.0,
                    kv_cache_usage: 0.05,
                    prefix_match_ratio: 0.0,
                },
                Some(50.0),
                None,
            );
        }
        // Flood with 200 warm high-kv samples in a different bucket
        for _ in 0..200 {
            pred.record_sample(
                "m1",
                LatencyFeatures {
                    input_tokens: 1000.0,
                    pending_requests: 5.0,
                    running_requests: 4.0,
                    kv_cache_usage: 0.85,
                    prefix_match_ratio: 0.9,
                },
                Some(120.0),
                None,
            );
        }
        assert_eq!(pred.bucket_count("m1"), 2);
    }
}
