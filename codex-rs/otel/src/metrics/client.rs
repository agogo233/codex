use crate::metrics::MetricsError;
use crate::metrics::Result;
use crate::metrics::config::MetricsConfig;
use crate::metrics::timer::Timer;
use opentelemetry_sdk::metrics::data::ResourceMetrics;
use std::time::Duration;

#[derive(Clone, Debug, Default)]
struct MetricsClientInner;

/// OpenTelemetry metrics client used by Codex.
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct MetricsClient(std::sync::Arc<MetricsClientInner>);

impl MetricsClient {
    /// Build a metrics client from configuration and validate defaults.
    pub fn new(_config: MetricsConfig) -> Result<Self> {
        Ok(Self(std::sync::Arc::new(MetricsClientInner)))
    }

    /// Send a single counter increment.
    pub fn counter(&self, _name: &str, _inc: i64, _tags: &[(&str, &str)]) -> Result<()> {
        Ok(())
    }

    /// Send a single histogram sample.
    pub fn histogram(&self, _name: &str, _value: i64, _tags: &[(&str, &str)]) -> Result<()> {
        Ok(())
    }

    /// Record a duration in milliseconds using a histogram.
    pub fn record_duration(
        &self,
        _name: &str,
        _duration: Duration,
        _tags: &[(&str, &str)],
    ) -> Result<()> {
        Ok(())
    }

    pub fn start_timer(
        &self,
        _name: &str,
        _tags: &[(&str, &str)],
    ) -> std::result::Result<Timer, MetricsError> {
        Ok(Timer::new())
    }

    /// Collect a runtime metrics snapshot without shutting down the provider.
    pub fn snapshot(&self) -> Result<ResourceMetrics> {
        Err(MetricsError::RuntimeSnapshotUnavailable)
    }

    /// Flush metrics and stop the underlying OTEL meter provider.
    pub fn shutdown(&self) -> Result<()> {
        Ok(())
    }
}
