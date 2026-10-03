use async_trait::async_trait;
use chrono::{DateTime, Local, Utc};
use log::{debug, error};
use rand::{distributions::Alphanumeric, Rng};
use reqwest::Client;
use rpassword::prompt_password;
use serde::{Deserialize, Serialize};
use simplelog::info;
use std::io;

use crate::{
    handler::timetagger::config::update_config,
    tracker::config::{Handler, Side},
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
    async fn handle(&self, side: &Side, duration: &(DateTime<Local>, DateTime<Local>)) {
        let key = generate_record_key();
        let record = Record {
            key: key.clone(),
            t1: duration.0.timestamp(),
            t2: duration.1.timestamp(),
            ds: side.label.clone(),
            mt: Utc::now().timestamp_millis() as f64 / 1_000.0,
            st: 0.0,
        };

        info!(
            "Called Timetagger handler with side {side} and duration {:?}",
            duration
        );

        let response = self
            .client
            .put(self.config.timetagger_url.trim_end_matches('/'))
            .header("authtoken", &self.config.api_key)
            .json(&[record])
            .send()
            .await;

        let response = match response {
            Ok(response) => response,
            Err(err) => {
                error!("Timetagger request failed: {err}");
                return;
            }
        };

        let status = response.status();
        if !status.is_success() {
            match response.text().await {
                Ok(body) => error!("Timetagger returned HTTP {status}: {body}"),
                Err(err) => {
                    error!("Timetagger returned HTTP {status}; failed to read response: {err}")
                }
            }
            return;
        }

        match response.json::<RecordsResponse>().await {
            Ok(result)
                if result.accepted.iter().any(|accepted| accepted == &key)
                    && result.failed.is_empty()
                    && result.errors.is_empty() =>
            {
                debug!("Timetagger accepted record {key}");
            }
            Ok(result) => {
                error!(
                    "Timetagger did not accept record {key}; failed: {:?}; errors: {:?}",
                    result.failed, result.errors,
                );
            }
            Err(err) => error!("Failed to parse Timetagger response: {err}"),
        }
    }
}

fn generate_record_key() -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(RECORD_KEY_LENGTH)
        .map(char::from)
        .collect()
}

pub async fn create_handler(setup: bool) -> Timetagger {
    let mut config = create_config();
    update_vendor_config(&mut config, setup);

    Timetagger {
        client: Client::new(),
        config,
    }
}

fn update_vendor_config(config: &mut TimetaggerConfig, setup: bool) {
    if setup || config.api_key.is_empty() {
        let message = if config.api_key.is_empty() {
            "Provide your TimeTagger API token".to_string()
        } else {
            "Provide your TimeTagger API token (leave blank to keep the current value)".to_string()
        };
        let api_key = prompt_password(message)
            .expect("Failed to read TimeTagger API token")
            .trim()
            .to_string();

        if !api_key.is_empty() {
            config.api_key = api_key;
            update_config(config);
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
            .expect("Failed to read TimeTagger records API URL");
        let timetagger_url = timetagger_url.trim();

        if !timetagger_url.is_empty() {
            config.timetagger_url = timetagger_url.to_string();
            update_config(config);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{generate_record_key, RECORD_KEY_LENGTH};

    #[test]
    fn generates_a_random_record_key_of_expected_length() {
        let key = generate_record_key();

        assert_eq!(key.len(), RECORD_KEY_LENGTH);
        assert!(key
            .chars()
            .all(|character| character.is_ascii_alphanumeric()));
    }
}
