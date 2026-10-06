use anyhow::{bail, Context as _, Result};
use async_trait::async_trait;
use chrono::{DateTime, Local, Utc};
use log::debug;
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
    async fn handle(
        &self,
        side: &Side,
        duration: &(DateTime<Local>, DateTime<Local>),
    ) -> Result<()> {
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
    use super::{generate_record_key, Timetagger, TimetaggerConfig, RECORD_KEY_LENGTH};
    use crate::tracker::config::{Handler, Side};
    use chrono::Local;
    use reqwest::Client;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[test]
    fn generates_a_random_record_key_of_expected_length() {
        let key = generate_record_key();

        assert_eq!(key.len(), RECORD_KEY_LENGTH);
        assert!(key
            .chars()
            .all(|character| character.is_ascii_alphanumeric()));
    }

    async fn submit_to_mock(status: &str, accepted: bool) -> anyhow::Result<()> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let status = status.to_string();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut buffer = [0; 1024];
            let body = loop {
                let size = stream.read(&mut buffer).await.unwrap();
                assert!(size > 0, "client closed before sending the request");
                request.extend_from_slice(&buffer[..size]);
                if let Some(end) = request.windows(4).position(|part| part == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&request[..end]);
                    let length: usize = headers
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length:")
                                .map(|length| length.trim().parse().unwrap())
                        })
                        .unwrap();
                    if request.len() >= end + 4 + length {
                        break serde_json::from_slice::<serde_json::Value>(
                            &request[end + 4..end + 4 + length],
                        )
                        .unwrap();
                    }
                }
            };
            let key = body[0]["key"].as_str().unwrap();
            let response = if accepted {
                serde_json::json!({ "accepted": [key] })
            } else {
                serde_json::json!({ "failed": [key], "errors": ["record rejected"] })
            }
            .to_string();
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",
                response.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        });
        let handler = Timetagger {
            client: Client::builder().no_proxy().build()?,
            config: TimetaggerConfig {
                timetagger_url: format!("http://{address}/api/v2/records"),
                api_key: "test-token".to_string(),
            },
        };
        let now = Local::now();
        let result = handler
            .handle(
                &Side {
                    side_num: 1,
                    label: "work".to_string(),
                    configurable: true,
                },
                &(now, now),
            )
            .await;
        server.await?;
        result
    }

    #[tokio::test]
    async fn accepted_record_succeeds() {
        submit_to_mock("200 OK", true).await.unwrap();
    }

    #[tokio::test]
    async fn rejected_record_returns_an_error() {
        let error = submit_to_mock("200 OK", false).await.unwrap_err();
        assert!(error.to_string().contains("did not accept record"));
    }

    #[tokio::test]
    async fn http_error_returns_context() {
        let error = submit_to_mock("401 Unauthorized", false).await.unwrap_err();
        let message = format!("{error:#}");
        assert!(message.contains("TimeTagger rejected the time entry"));
        assert!(message.contains("401"));
    }
}
