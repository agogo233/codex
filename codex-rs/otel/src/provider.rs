use crate::config::OtelSettings;
use crate::metrics::MetricsClient;
use std::error::Error;
use tracing::debug;
use tracing_subscriber::Layer;
use tracing_subscriber::registry::LookupSpan;

pub struct OtelProvider;

impl OtelProvider {
    pub fn shutdown(&self) {}

    pub fn from(_settings: &OtelSettings) -> Result<Option<Self>, Box<dyn Error>> {
        debug!("OTEL exporter disabled by default.");
        Ok(None)
    }

    pub fn logger_layer<S>(&self) -> Option<impl Layer<S> + Send + Sync>
    where
        S: tracing::Subscriber + for<'span> LookupSpan<'span> + Send + Sync,
    {
        None
    }

    pub fn tracing_layer<S>(&self) -> Option<impl Layer<S> + Send + Sync>
    where
        S: tracing::Subscriber + for<'span> LookupSpan<'span> + Send + Sync,
    {
        None
    }

    pub fn codex_export_filter(meta: &tracing::Metadata<'_>) -> bool {
        meta.is_span()
    }

    pub fn log_export_filter(meta: &tracing::Metadata<'_>) -> bool {
        meta.is_span()
    }

    pub fn trace_export_filter(meta: &tracing::Metadata<'_>) -> bool {
        meta.is_span()
    }

    pub fn metrics(&self) -> Option<&MetricsClient> {
        None
    }
}

impl Drop for OtelProvider {
    fn drop(&mut self) {}
}


