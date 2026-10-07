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

#[cfg(test)]
mod tests {
    use super::Traggo;
    use crate::{test_support::time_entry, tracker::config::Handler};

    #[tokio::test]
    async fn returns_an_explicit_not_implemented_error() {
        let error = Traggo {}.handle(&time_entry(1, "Work")).await.unwrap_err();

        assert_eq!(error.to_string(), "Traggo integration is not implemented");
    }
}
