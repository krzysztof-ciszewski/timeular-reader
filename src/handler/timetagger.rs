use anyhow::{bail, Context as _, Result};
use async_trait::async_trait;
use chrono::Utc;
use log::debug;
use rand::{distributions::Alphanumeric, Rng};
use reqwest::Client;
use rpassword::prompt_password;
use serde::{Deserialize, Serialize};
use simplelog::info;
use std::io;

use crate::{
    handler::timetagger::config::update_config,
    tracker::config::{Handler, TimeEntry},
};

use self::config::{create_config, TimetaggerConfig};

pub mod config;

const RECORD_KEY_LENGTH: usize = 32;

#[derive(Debug, Default)]
pub struct Timetagger {
    client: Client,
    config: TimetaggerConfig,
}

#[derive(Deserialize)]
struct RecordsResponse {
    #[serde(default)]
    accepted: Vec<String>,
    #[serde(default)]
    failed: Vec<String>,
    #[serde(default)]
    errors: Vec<String>,
}

#[derive(Serialize)]
struct Record {
    key: String,
    t1: i64,
    t2: i64,
    ds: String,
    mt: f64,
    st: f64,
}

#[async_trait]
impl Handler for Timetagger {
    async fn handle(&self, entry: &TimeEntry) -> Result<()> {
        let key = generate_record_key();
        let record = Record {
            key: key.clone(),
            t1: entry.start.timestamp(),
            t2: entry.end.timestamp(),
            ds: entry.side.label.clone(),
            mt: Utc::now().timestamp_millis() as f64 / 1_000.0,
            st: 0.0,
        };

        info!(
            "Called Timetagger handler with side {} and duration {:?}",
            entry.side, entry
        );

        let response = self
            .client
            .put(self.config.timetagger_url.trim_end_matches('/'))
            .header("authtoken", &self.config.api_key)
            .json(&[record])
            .send()
            .await
            .context("failed to send time entry to TimeTagger")?
            .error_for_status()
            .context("TimeTagger rejected the time entry")?;

        let result = response
            .json::<RecordsResponse>()
            .await
            .context("failed to parse TimeTagger response")?;
        if !result.accepted.iter().any(|accepted| accepted == &key)
            || !result.failed.is_empty()
            || !result.errors.is_empty()
        {
            bail!(
                "TimeTagger did not accept record {key}; failed: {:?}; errors: {:?}",
                result.failed,
                result.errors
            );
        }
        debug!("TimeTagger accepted record {key}");
        Ok(())
    }
}

fn generate_record_key() -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(RECORD_KEY_LENGTH)
        .map(char::from)
        .collect()
}

pub async fn create_handler(setup: bool) -> Result<Timetagger> {
    let mut config = create_config()?;
    update_vendor_config(&mut config, setup)?;

    Ok(Timetagger {
        client: Client::builder()
            .build()
            .context("failed to create TimeTagger HTTP client")?,
        config,
    })
}

fn update_vendor_config(config: &mut TimetaggerConfig, setup: bool) -> Result<()> {
    if setup || config.api_key.is_empty() {
        let message = if config.api_key.is_empty() {
            "Provide your TimeTagger API token".to_string()
        } else {
            "Provide your TimeTagger API token (leave blank to keep the current value)".to_string()
        };
        let api_key = prompt_password(message)
            .context("failed to read TimeTagger API token")?
            .trim()
            .to_string();

        if !api_key.is_empty() {
            config.api_key = api_key;
            update_config(config)?;
        }
    }

    if setup || config.timetagger_url.is_empty() {
        let message = if config.timetagger_url.is_empty() {
            "Provide your TimeTagger records API URL".to_string()
        } else {
            format!(
                "Provide your TimeTagger records API URL (currently {}, leave blank to keep it)",
                config.timetagger_url
            )
        };
        info!("{message}");

        let mut timetagger_url = String::new();
        io::stdin()
            .read_line(&mut timetagger_url)
            .context("failed to read TimeTagger records API URL")?;
        let timetagger_url = timetagger_url.trim();

        if !timetagger_url.is_empty() {
            config.timetagger_url = timetagger_url.to_string();
            update_config(config)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{generate_record_key, Timetagger, RECORD_KEY_LENGTH};
    use crate::{
        test_support::{request_body, time_entry, MockServer},
        tracker::config::Handler,
    };
    use reqwest::Client;
    use serde_json::json;

    #[test]
    fn generates_a_random_record_key_of_expected_length() {
        let key = generate_record_key();

        assert_eq!(key.len(), RECORD_KEY_LENGTH);
        assert!(key
            .chars()
            .all(|character| character.is_ascii_alphanumeric()));
    }

    #[tokio::test]
    async fn sends_record_and_requires_server_acceptance() {
        let server = MockServer::start(1, |_, request| {
            let request: serde_json::Value = serde_json::from_str(request_body(request)).unwrap();
            let key = request[0]["key"].as_str().unwrap();
            (
                200,
                json!({"accepted": [key], "failed": [], "errors": []}).to_string(),
            )
        });
        let config = super::config::TimetaggerConfig {
            timetagger_url: server.url().into(),
            api_key: "token".into(),
        };
        let entry = time_entry(1, "Focus");

        let handler = Timetagger {
            client: Client::new(),
            config,
        };
        handler.handle(&entry).await.unwrap();

        let requests = server.finish();
        let request = &requests[0];
        assert!(request.starts_with("PUT / HTTP/1.1"));
        assert!(request.to_ascii_lowercase().contains("authtoken: token"));
        let records: serde_json::Value = serde_json::from_str(request_body(request)).unwrap();
        assert_eq!(records.as_array().unwrap().len(), 1);
        assert_eq!(records[0]["t1"], entry.start.timestamp());
        assert_eq!(records[0]["t2"], entry.end.timestamp());
        assert_eq!(records[0]["ds"], entry.side.label);
        assert_eq!(records[0]["st"], 0.0);
        assert_eq!(records[0]["key"].as_str().unwrap().len(), RECORD_KEY_LENGTH);
    }

    #[tokio::test]
    async fn reports_a_response_that_does_not_accept_the_record() {
        let server = MockServer::start(1, |_, _| {
            (
                200,
                r#"{"accepted":[],"failed":["record"],"errors":[]}"#.into(),
            )
        });
        let config = super::config::TimetaggerConfig {
            timetagger_url: server.url().into(),
            ..super::config::TimetaggerConfig::default()
        };

        let handler = Timetagger {
            client: Client::new(),
            config,
        };
        let error = handler.handle(&time_entry(1, "Work")).await.unwrap_err();

        assert!(format!("{error:#}").contains("TimeTagger did not accept record"));
        server.finish();
    }
}
