use crate::handler::example::config::{create_config, update_config, ExampleConfig};
use crate::tracker::config::{Handler, Side, TimeEntry};
use crate::tracker::side_project::prompt_side_projects;
use anyhow::{Context as _, Result};
use async_trait::async_trait;
use reqwest::header::CONTENT_TYPE;
use reqwest::Client;
use simplelog::info;

pub mod config;

#[derive(Debug, Default)]
pub struct Example {
    client: Client,
    config: ExampleConfig,
}

#[async_trait]
impl Handler for Example {
    async fn handle(&self, entry: &TimeEntry) -> Result<()> {
        info!(
            "Called Example handler with side {}, project \"{}\" and duration {:?}",
            entry.side,
            self.config.project_id_for_side(entry.side.side_num),
            entry
        );

        let url = self.config.base_url.trim_end_matches('/').to_string();
        super::http::send_with_retry(
            "Example time-entry request",
            super::http::ResponseBodyPolicy::Ignore,
            || {
                self.client
                    .post(&url)
                    .header(CONTENT_TYPE, "application/json")
                    .header("x-api-key", &self.config.api_key)
            },
        )
        .await
        .context("failed to submit time entry to Example handler")?;
        Ok(())
    }
}

pub async fn create_handler(setup: bool, sides: &[Side]) -> Result<Example> {
    let mut config = create_config()?;
    let client = Client::builder()
        .build()
        .context("failed to create Example HTTP client")?;
    update_vendor_config(&mut config, setup, sides)?;

    Ok(Example { client, config })
}

fn update_vendor_config(config: &mut ExampleConfig, setup: bool, sides: &[Side]) -> Result<()> {
    if setup && prompt_side_projects("Example", sides, &mut config.side_projects)? {
        update_config(config)?;
    }

    if setup || config.api_key.is_empty() {
        let mut api_key = String::new();
        let mut message = String::from("Provide your Example API key");
        if config.api_key.is_empty() {
            message.push_str("\n leave blank to skip");
        }
        info!("{message}");

        std::io::stdin()
            .read_line(&mut api_key)
            .context("failed to read Example API key")?;
        api_key = api_key.trim().to_string();

        if !api_key.is_empty() {
            config.api_key = api_key;
            update_config(config)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{config::ExampleConfig, Example};
    use crate::{
        test_support::{time_entry, MockServer},
        tracker::config::Handler,
    };
    use reqwest::Client;

    #[tokio::test]
    async fn sends_api_key_and_surfaces_http_errors() {
        let server = MockServer::start(4, |index, _| {
            if index == 0 {
                (200, "{}".into())
            } else {
                (500, "{}".into())
            }
        });
        let config = ExampleConfig {
            base_url: format!("{}/records", server.url()),
            api_key: "token".into(),
            ..ExampleConfig::default()
        };
        let handler = Example {
            client: Client::new(),
            config,
        };

        handler.handle(&time_entry(1, "Work")).await.unwrap();
        let error = handler.handle(&time_entry(1, "Break")).await.unwrap_err();

        assert!(
            format!("{error:#}").contains("Example time-entry request failed with HTTP status 500")
        );
        let requests = server.finish();
        assert!(requests[0].starts_with("POST /records HTTP/1.1"));
        assert!(requests[0]
            .to_ascii_lowercase()
            .contains("x-api-key: token"));
        assert!(requests[1].starts_with("POST /records HTTP/1.1"));
        assert!(requests[2].starts_with("POST /records HTTP/1.1"));
        assert!(requests[3].starts_with("POST /records HTTP/1.1"));
    }
}
