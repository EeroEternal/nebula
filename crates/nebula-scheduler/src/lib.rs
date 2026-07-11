use anyhow::Result;

pub mod metrics;
pub mod scale;

#[derive(Debug, Default)]
pub struct Scheduler {}

impl Scheduler {
    pub fn new() -> Self {
        Self {}
    }

    pub fn tick(&self) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod reconcile_test;
