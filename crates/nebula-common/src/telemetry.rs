use opentelemetry::trace::TracerProvider as TracerProviderTrait;
use opentelemetry::KeyValue;
use opentelemetry_otlp::{WithExportConfig, WithHttpConfig};
use opentelemetry_sdk::trace::TracerProvider;
use opentelemetry_sdk::Resource;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Initialize tracing with optional OTLP export to xtrace.
///
/// - `service_name`: identifies this component (e.g. "nebula-gateway")
/// - `otlp_endpoint`: if `Some`, traces are exported via OTLP/HTTP to this base URL
///   (e.g. "http://10.21.11.92:8742/api/public/otel"). The exporter appends `/v1/traces`.
/// - `otlp_token`: bearer token for xtrace authentication
/// - `log_format`: `"text"` (default human-readable) or `"json"` (structured JSON lines)
///
/// Returns an optional `SdkTracerProvider` that the caller should keep alive
/// and call `shutdown()` on before exit.
pub fn init_tracing(
    service_name: &str,
    otlp_endpoint: Option<&str>,
    otlp_token: Option<&str>,
    log_format: &str,
) -> Option<TracerProvider> {
    let use_json = log_format.eq_ignore_ascii_case("json");

    if let Some(endpoint) = otlp_endpoint {
        let mut headers = std::collections::HashMap::new();
        if let Some(token) = otlp_token {
            if !token.is_empty() {
                headers.insert("Authorization".to_string(), format!("Bearer {token}"));
            }
        }

        let build_exporter = || {
            opentelemetry_otlp::SpanExporter::builder()
                .with_http()
                .with_endpoint(endpoint)
                .with_headers(headers.clone())
                .build()
        };

        if use_json {
            let env_filter = EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info"));
            let fmt_layer = tracing_subscriber::fmt::layer().json();

            let exporter = match build_exporter() {
                Ok(e) => e,
                Err(err) => {
                    eprintln!("failed to create OTLP exporter: {err}, falling back to stdout only");
                    tracing_subscriber::registry()
                        .with(env_filter)
                        .with(fmt_layer)
                        .init();
                    return None;
                }
            };

            let provider = TracerProvider::builder()
                .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
                .with_resource(Resource::new([KeyValue::new("service.name", service_name.to_string())]))
                .build();

            let otel_layer = tracing_opentelemetry::layer()
                .with_tracer(provider.tracer(service_name.to_string()));

            tracing_subscriber::registry()
                .with(env_filter)
                .with(fmt_layer)
                .with(otel_layer)
                .init();

            tracing::info!(endpoint, service_name, "OTLP tracing enabled (json)");
            Some(provider)
        } else {
            let env_filter = EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info"));
            let fmt_layer = tracing_subscriber::fmt::layer();

            let exporter = match build_exporter() {
                Ok(e) => e,
                Err(err) => {
                    eprintln!("failed to create OTLP exporter: {err}, falling back to stdout only");
                    tracing_subscriber::registry()
                        .with(env_filter)
                        .with(fmt_layer)
                        .init();
                    return None;
                }
            };

            let provider = TracerProvider::builder()
                .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
                .with_resource(Resource::new([KeyValue::new("service.name", service_name.to_string())]))
                .build();

            let otel_layer = tracing_opentelemetry::layer()
                .with_tracer(provider.tracer(service_name.to_string()));

            tracing_subscriber::registry()
                .with(env_filter)
                .with(fmt_layer)
                .with(otel_layer)
                .init();

            tracing::info!(endpoint, service_name, "OTLP tracing enabled");
            Some(provider)
        }
    } else if use_json {
        let env_filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("info"));
        let fmt_layer = tracing_subscriber::fmt::layer().json();
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .init();
        None
    } else {
        let env_filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("info"));
        let fmt_layer = tracing_subscriber::fmt::layer();
        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .init();
        None
    }
}

use opentelemetry::global;
use opentelemetry::propagation::{Extractor, Injector};
use tracing_opentelemetry::OpenTelemetrySpanExt;

struct HeaderInjector<'a>(&'a mut axum::http::HeaderMap);

impl<'a> Injector for HeaderInjector<'a> {
    fn set(&mut self, key: &str, value: String) {
        if let (Ok(name), Ok(val)) = (
            axum::http::header::HeaderName::from_bytes(key.as_bytes()),
            axum::http::header::HeaderValue::from_bytes(value.as_bytes()),
        ) {
            self.0.insert(name, val);
        }
    }
}

pub fn inject_trace_context(headers: &mut axum::http::HeaderMap) {
    let context = tracing::Span::current().context();
    global::get_text_map_propagator(|propagator| {
        propagator.inject_context(&context, &mut HeaderInjector(headers));
    });
}

struct HeaderExtractor<'a>(&'a axum::http::HeaderMap);

impl<'a> Extractor for HeaderExtractor<'a> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(|v| v.to_str().ok())
    }

    fn keys(&self) -> Vec<&str> {
        self.0.keys().map(|k| k.as_str()).collect()
    }
}

pub async fn trace_context_middleware(
    req: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let parent_context = global::get_text_map_propagator(|propagator| {
        propagator.extract(&HeaderExtractor(req.headers()))
    });
    tracing::Span::current().set_parent(parent_context);
    next.run(req).await
}

