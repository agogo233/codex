use crate::metrics::error::Result;
use std::time::Duration;

#[derive(Debug)]
pub struct Timer;

impl Timer {
    pub(crate) fn new() -> Self {
        Self
    }

    pub fn record(&self, _additional_tags: &[(&str, &str)]) -> Result<()> {
        Ok(())
    }
}

impl Drop for Timer {
    fn drop(&mut self) {}
}
