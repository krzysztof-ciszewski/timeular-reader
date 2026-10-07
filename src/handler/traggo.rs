use crate::tracker::config::{Handler, TimeEntry};
use anyhow::{bail, Result};
use async_trait::async_trait;

pub struct TraggoConfig {}
#[derive(Debug, Default)]
pub struct Traggo {}

#[async_trait]
impl Handler for Traggo {
    async fn handle(&self, _entry: &TimeEntry) -> Result<()> {
        bail!("Traggo integration is not implemented")
    }
}

pub async fn create_handler(_setup: bool) -> Traggo {
    Traggo {}
}
