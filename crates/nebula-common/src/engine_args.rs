//! Encode `ModelConfig` into engine-native CLI flags for Placement `extra_args`.
//!
//! Scheduler writes these; Node adapters append them before engine-local defaults.
//! Flags must match the target engine dialect — never send vLLM flags to SGLang.

use crate::capability::{resolve_engine_type, validate_engine_type};
use crate::model_request::ModelConfig;

/// Build Placement `extra_args` for a resolved engine type.
///
/// Returns `None` when there are no config-derived flags.
pub fn build_engine_extra_args(
    engine_type: Option<&str>,
    config: &ModelConfig,
) -> Result<Option<Vec<String>>, String> {
    let engine = validate_engine_type(engine_type)?;
    Ok(match engine.as_str() {
        "vllm" => build_vllm_extra_args(config),
        "sglang" => build_sglang_extra_args(config),
        other => {
            return Err(format!(
                "no CLI encoder for engine_type '{other}' (resolved from {:?})",
                engine_type
            ));
        }
    })
}

/// Same as [`build_engine_extra_args`] but treats unknown/missing type as default `vllm`.
pub fn build_engine_extra_args_lenient(
    engine_type: Option<&str>,
    config: &ModelConfig,
) -> Option<Vec<String>> {
    let engine = resolve_engine_type(engine_type);
    match engine.as_str() {
        "sglang" => build_sglang_extra_args(config),
        _ => build_vllm_extra_args(config),
    }
}

fn build_vllm_extra_args(cfg: &ModelConfig) -> Option<Vec<String>> {
    let mut args = Vec::new();

    if let Some(tp) = cfg.tensor_parallel_size {
        args.push("--tensor-parallel-size".to_string());
        args.push(tp.to_string());
    }
    if let Some(util) = cfg.gpu_memory_utilization {
        args.push("--gpu-memory-utilization".to_string());
        args.push(util.to_string());
    }
    if let Some(max_len) = cfg.max_model_len {
        args.push("--max-model-len".to_string());
        args.push(max_len.to_string());
    }
    if let Some(name) = cfg.served_model_name.as_deref() {
        args.push("--served-model-name".to_string());
        args.push(name.to_string());
    }
    if let Some(dtype) = cfg.kv_cache_dtype.as_deref() {
        args.push("--kv-cache-dtype".to_string());
        args.push(dtype.to_string());
    }
    if cfg.trust_remote_code == Some(true) {
        args.push("--trust-remote-code".to_string());
    }
    if cfg.enable_expert_parallel == Some(true) {
        args.push("--enable-expert-parallel".to_string());
    }
    if let Some(block) = cfg.block_size {
        args.push("--block-size".to_string());
        args.push(block.to_string());
    }
    if let Some(mode) = cfg.tokenizer_mode.as_deref() {
        args.push("--tokenizer-mode".to_string());
        args.push(mode.to_string());
    }
    if let Some(mods) = cfg.lora_modules.as_ref() {
        if !mods.is_empty() {
            args.push("--enable-lora".to_string());
            args.push("--lora-modules".to_string());
            args.push(mods.join(" "));
        }
    }
    match cfg.enable_prefix_caching {
        Some(true) => args.push("--enable-prefix-caching".to_string()),
        Some(false) => args.push("--no-enable-prefix-caching".to_string()),
        None => {}
    }
    if let Some(spec) = cfg.speculative_config.as_ref() {
        if let Some(spec_json) = encode_vllm_speculative_json(spec) {
            args.push("--speculative-config".to_string());
            args.push(spec_json);
        }
    }

    if args.is_empty() {
        None
    } else {
        Some(args)
    }
}

fn encode_vllm_speculative_json(spec: &crate::model_request::SpeculativeConfig) -> Option<String> {
    let mut map = serde_json::Map::new();
    if let Some(method) = spec.method {
        map.insert(
            "method".to_string(),
            serde_json::Value::String(method.as_vllm_method().to_string()),
        );
    }
    if let Some(ref model) = spec.draft_model {
        map.insert(
            "model".to_string(),
            serde_json::Value::String(model.clone()),
        );
    }
    if let Some(k) = spec.num_speculative_tokens {
        map.insert(
            "num_speculative_tokens".to_string(),
            serde_json::Value::Number(k.into()),
        );
    }
    if let Some(max_l) = spec.prompt_lookup_max {
        map.insert(
            "prompt_lookup_max".to_string(),
            serde_json::Value::Number(max_l.into()),
        );
    }
    if let Some(min_l) = spec.prompt_lookup_min {
        map.insert(
            "prompt_lookup_min".to_string(),
            serde_json::Value::Number(min_l.into()),
        );
    }
    if map.is_empty() {
        None
    } else {
        serde_json::to_string(&serde_json::Value::Object(map)).ok()
    }
}

fn build_sglang_extra_args(cfg: &ModelConfig) -> Option<Vec<String>> {
    let mut args = Vec::new();

    if let Some(tp) = cfg.tensor_parallel_size {
        args.push("--tp".to_string());
        args.push(tp.to_string());
    }
    // ModelConfig.gpu_memory_utilization maps to SGLang mem-fraction-static.
    if let Some(util) = cfg.gpu_memory_utilization {
        args.push("--mem-fraction-static".to_string());
        args.push(util.to_string());
    }
    if let Some(max_len) = cfg.max_model_len {
        args.push("--context-length".to_string());
        args.push(max_len.to_string());
    }
    if let Some(mods) = cfg.lora_modules.as_ref() {
        if !mods.is_empty() {
            // SGLang LoRA is enabled via --lora-paths; keep modules as path list.
            args.push("--lora-paths".to_string());
            args.push(mods.join(" "));
        }
    }
    if cfg.enable_prefix_caching == Some(false) {
        args.push("--disable-radix-cache".to_string());
    }
    if let Some(spec) = cfg.speculative_config.as_ref() {
        if let Some(algo) = spec.method.and_then(|m| m.as_sglang_algorithm()) {
            args.push("--speculative-algorithm".to_string());
            args.push(algo.to_string());
        }
        if let Some(ref draft) = spec.draft_model {
            args.push("--speculative-draft-model-path".to_string());
            args.push(draft.clone());
        }
        if let Some(k) = spec.num_speculative_tokens {
            args.push("--speculative-num-draft-tokens".to_string());
            args.push(k.to_string());
        }
    }

    if args.is_empty() {
        None
    } else {
        Some(args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_request::{SpeculativeConfig, SpeculativeMethod};

    fn sample_cfg() -> ModelConfig {
        ModelConfig {
            tensor_parallel_size: Some(2),
            gpu_memory_utilization: Some(0.85),
            max_model_len: Some(8192),
            required_vram_mb: None,
            lora_modules: None,
            served_model_name: None,
            kv_cache_dtype: None,
            trust_remote_code: None,
            enable_expert_parallel: None,
            block_size: None,
            tokenizer_mode: None,
            enable_prefix_caching: None,
            speculative_config: None,
        }
    }

    #[test]
    fn vllm_uses_tensor_parallel_and_gpu_memory_flags() {
        let args = build_engine_extra_args(Some("vllm"), &sample_cfg())
            .unwrap()
            .unwrap();
        assert!(args
            .windows(2)
            .any(|w| { w[0] == "--tensor-parallel-size" && w[1] == "2" }));
        assert!(args
            .windows(2)
            .any(|w| { w[0] == "--gpu-memory-utilization" && w[1] == "0.85" }));
        assert!(args
            .windows(2)
            .any(|w| w[0] == "--max-model-len" && w[1] == "8192"));
        assert!(!args
            .iter()
            .any(|a| a == "--tp" || a == "--mem-fraction-static"));
    }

    #[test]
    fn sglang_uses_tp_and_mem_fraction_flags() {
        let args = build_engine_extra_args(Some("sglang"), &sample_cfg())
            .unwrap()
            .unwrap();
        assert!(args.windows(2).any(|w| w[0] == "--tp" && w[1] == "2"));
        assert!(args
            .windows(2)
            .any(|w| { w[0] == "--mem-fraction-static" && w[1] == "0.85" }));
        assert!(args
            .windows(2)
            .any(|w| w[0] == "--context-length" && w[1] == "8192"));
        assert!(!args
            .iter()
            .any(|a| a == "--tensor-parallel-size" || a == "--gpu-memory-utilization"));
    }

    #[test]
    fn unknown_engine_errors() {
        let err = build_engine_extra_args(Some("tensorrt"), &sample_cfg()).unwrap_err();
        assert!(err.contains("unknown engine_type"));
    }

    #[test]
    fn vllm_deepseek_v4_flags() {
        let cfg = ModelConfig {
            kv_cache_dtype: Some("fp8".into()),
            trust_remote_code: Some(true),
            enable_expert_parallel: Some(true),
            block_size: Some(256),
            tokenizer_mode: Some("deepseek_v4".into()),
            served_model_name: Some("deepseek-v4-flash".into()),
            ..sample_cfg()
        };
        let args = build_engine_extra_args(Some("vllm"), &cfg)
            .unwrap()
            .unwrap();
        assert!(args
            .windows(2)
            .any(|w| w[0] == "--kv-cache-dtype" && w[1] == "fp8"));
        assert!(args.iter().any(|a| a == "--trust-remote-code"));
        assert!(args.iter().any(|a| a == "--enable-expert-parallel"));
        assert!(args
            .windows(2)
            .any(|w| w[0] == "--block-size" && w[1] == "256"));
        assert!(args
            .windows(2)
            .any(|w| w[0] == "--tokenizer-mode" && w[1] == "deepseek_v4"));
    }

    #[test]
    fn encodes_speculative_dflash2_and_prefix_caching_for_vllm_and_sglang() {
        let cfg = ModelConfig {
            enable_prefix_caching: Some(true),
            speculative_config: Some(SpeculativeConfig {
                method: Some(SpeculativeMethod::Dflash2),
                draft_model: Some("Qwen/Qwen3.8-27B-DFlash2".into()),
                num_speculative_tokens: Some(16),
                prompt_lookup_max: Some(8),
                prompt_lookup_min: Some(2),
            }),
            ..sample_cfg()
        };
        let vllm_args = build_engine_extra_args(Some("vllm"), &cfg)
            .unwrap()
            .unwrap();
        assert!(vllm_args.iter().any(|a| a == "--enable-prefix-caching"));
        let spec_pair = vllm_args
            .windows(2)
            .find(|w| w[0] == "--speculative-config")
            .expect("vllm --speculative-config");
        let parsed: serde_json::Value = serde_json::from_str(&spec_pair[1]).unwrap();
        assert_eq!(parsed["method"], "draft_model");
        assert_eq!(parsed["model"], "Qwen/Qwen3.8-27B-DFlash2");
        assert_eq!(parsed["num_speculative_tokens"], 16);
        assert_eq!(parsed["prompt_lookup_max"], 8);

        let sg_cfg = ModelConfig {
            enable_prefix_caching: Some(false),
            speculative_config: Some(SpeculativeConfig {
                method: Some(SpeculativeMethod::Dflash2),
                draft_model: Some("Qwen/Qwen3.8-27B-DFlash2".into()),
                num_speculative_tokens: Some(16),
                prompt_lookup_max: None,
                prompt_lookup_min: None,
            }),
            ..sample_cfg()
        };
        let sg_args = build_engine_extra_args(Some("sglang"), &sg_cfg)
            .unwrap()
            .unwrap();
        assert!(sg_args.iter().any(|a| a == "--disable-radix-cache"));
        assert!(sg_args
            .windows(2)
            .any(|w| w[0] == "--speculative-algorithm" && w[1] == "EAGLE"));
        assert!(sg_args.windows(2).any(
            |w| w[0] == "--speculative-draft-model-path" && w[1] == "Qwen/Qwen3.8-27B-DFlash2"
        ));
        assert!(sg_args
            .windows(2)
            .any(|w| w[0] == "--speculative-num-draft-tokens" && w[1] == "16"));
        assert!(!sg_args.iter().any(|a| a == "--prompt-lookup-max"));
    }
}
