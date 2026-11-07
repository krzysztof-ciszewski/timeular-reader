use std::time::Duration;

use anyhow::{Context as _, Result};
use log::warn;
use reqwest::{RequestBuilder, Response, StatusCode};

const MAX_ATTEMPTS: u32 = 3;
const INITIAL_BACKOFF: Duration = Duration::from_millis(200);

pub(super) async fn send_with_retry<F>(
    operation: &'static str,
    mut build_request: F,
) -> Result<Response>
where
    F: FnMut() -> RequestBuilder,
{
    for attempt in 1..=MAX_ATTEMPTS {
        match build_request().send().await {
            Ok(response) if response.status().is_success() => return Ok(response),
            Ok(response) => {
                let status = response.status();
                if !is_retryable_status(status) || attempt == MAX_ATTEMPTS {
                    anyhow::bail!("{operation} failed with HTTP status {status}");
                }

                warn!("{operation} returned HTTP {status}; retrying ({attempt}/{MAX_ATTEMPTS})");
            }
            Err(error) => {
                if !is_retryable_transport_error(&error) || attempt == MAX_ATTEMPTS {
                    return Err(error)
                        .with_context(|| format!("{operation} failed after {attempt} attempt(s)"));
                }

                warn!("{operation} encountered a transient transport error; retrying ({attempt}/{MAX_ATTEMPTS})");
            }
        }

        tokio::time::sleep(INITIAL_BACKOFF * 2u32.pow(attempt - 1)).await;
    }

    unreachable!("the retry loop returns or fails on its final attempt")
}

fn is_retryable_status(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

fn is_retryable_transport_error(error: &reqwest::Error) -> bool {
    error.is_timeout() || error.is_connect()
}

#[cfg(test)]
mod tests {
    use super::{is_retryable_status, is_retryable_transport_error, send_with_retry};
    use crate::test_support::MockServer;
    use reqwest::StatusCode;

    #[test]
    fn retries_only_rate_limits_and_server_errors() {
        assert!(is_retryable_status(StatusCode::TOO_MANY_REQUESTS));
        assert!(is_retryable_status(StatusCode::SERVICE_UNAVAILABLE));
        assert!(!is_retryable_status(StatusCode::BAD_REQUEST));
        assert!(!is_retryable_status(StatusCode::UNAUTHORIZED));
        assert!(!is_retryable_status(StatusCode::NOT_FOUND));
    }

    #[tokio::test]
    async fn retries_only_transient_transport_errors() {
        let timeout = reqwest::Client::new()
            .get("http://127.0.0.1:1")
            .timeout(std::time::Duration::ZERO)
            .send()
            .await
            .unwrap_err();
        let invalid_request = reqwest::Client::new()
            .get("not-a-url")
            .send()
            .await
            .unwrap_err();

        assert!(is_retryable_transport_error(&timeout));
        assert!(!is_retryable_transport_error(&invalid_request));
    }

    #[tokio::test]
    async fn retries_transient_status_then_succeeds() {
        let server = MockServer::start(2, |index, _| {
            if index == 0 {
                (503, "{}".into())
            } else {
                (200, "{}".into())
            }
        });
        let client = reqwest::Client::new();

        send_with_retry("test request", || client.get(server.url()))
            .await
            .unwrap();

        assert_eq!(server.finish().len(), 2);
    }

    #[tokio::test]
    async fn does_not_retry_permanent_client_errors() {
        let server = MockServer::start(1, |_, _| (400, "{}".into()));
        let client = reqwest::Client::new();

        let error = send_with_retry("test request", || client.get(server.url()))
            .await
            .unwrap_err();

        assert!(error.to_string().contains("HTTP status 400"));
        assert_eq!(server.finish().len(), 1);
    }
}
