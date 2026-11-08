use std::time::{Duration, SystemTime};
use std::{error::Error as _, io};

use anyhow::{Context as _, Result};
use log::warn;
use reqwest::{header::HeaderValue, RequestBuilder, Response, StatusCode};

const MAX_ATTEMPTS: u32 = 3;
const INITIAL_BACKOFF: Duration = Duration::from_millis(200);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_RETRY_AFTER: Duration = Duration::from_secs(30);
const MAX_ERROR_BODY_DRAIN_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ResponseBodyPolicy {
    Ignore,
    ReadOnce,
    RetryTransientTransfer,
}

pub(super) async fn send_with_retry<F>(
    operation: &'static str,
    body_policy: ResponseBodyPolicy,
    build_request: F,
) -> Result<Vec<u8>>
where
    F: FnMut() -> RequestBuilder,
{
    send_with_retry_timeout(operation, REQUEST_TIMEOUT, body_policy, build_request).await
}

async fn send_with_retry_timeout<F>(
    operation: &'static str,
    request_timeout: Duration,
    body_policy: ResponseBodyPolicy,
    mut build_request: F,
) -> Result<Vec<u8>>
where
    F: FnMut() -> RequestBuilder,
{
    for attempt in 1..=MAX_ATTEMPTS {
        let mut delay = INITIAL_BACKOFF * 2u32.pow(attempt - 1);
        match build_request().timeout(request_timeout).send().await {
            Ok(response) if response.status().is_success() => {
                if body_policy == ResponseBodyPolicy::Ignore {
                    return Ok(Vec::new());
                }
                match response.bytes().await {
                    Ok(body) => return Ok(body.to_vec()),
                    Err(error)
                        if body_policy == ResponseBodyPolicy::RetryTransientTransfer
                            && is_retryable_transport_error(&error)
                            && attempt < MAX_ATTEMPTS =>
                    {
                        warn!("{operation} response body failed to transfer; retrying ({attempt}/{MAX_ATTEMPTS})");
                    }
                    Err(error) => {
                        return Err(error.without_url()).with_context(|| {
                            format!("{operation} response body failed after {attempt} attempt(s)")
                        });
                    }
                }
            }
            Ok(response) => {
                let status = response.status();
                if !is_retryable_status(status) || attempt == MAX_ATTEMPTS {
                    anyhow::bail!("{operation} failed with HTTP status {status}");
                }

                if let Some(retry_after) = response
                    .headers()
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|value| parse_retry_after(value, SystemTime::now()))
                {
                    delay = retry_after;
                }
                if !discard_response_body(response).await {
                    warn!("{operation} could not drain the retry response body");
                }
                warn!("{operation} returned HTTP {status}; retrying ({attempt}/{MAX_ATTEMPTS})");
            }
            Err(error) => {
                if !is_retryable_transport_error(&error) || attempt == MAX_ATTEMPTS {
                    return Err(error.without_url())
                        .with_context(|| format!("{operation} failed after {attempt} attempt(s)"));
                }

                warn!("{operation} encountered a transient transport error; retrying ({attempt}/{MAX_ATTEMPTS})");
            }
        }

        tokio::time::sleep(delay).await;
    }

    unreachable!("the retry loop returns or fails on its final attempt")
}

async fn discard_response_body(mut response: Response) -> bool {
    let mut remaining = MAX_ERROR_BODY_DRAIN_BYTES;
    while remaining > 0 {
        match response.chunk().await {
            Ok(Some(chunk)) => {
                if chunk.len() > remaining {
                    return false;
                }
                remaining -= chunk.len();
            }
            Ok(None) => return true,
            Err(_) => return false,
        }
    }

    false
}

fn parse_retry_after(value: &HeaderValue, now: SystemTime) -> Option<Duration> {
    let value = value.to_str().ok()?.trim();
    let delay = if !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()) {
        Duration::from_secs(value.parse().unwrap_or(u64::MAX))
    } else {
        httpdate::parse_http_date(value)
            .ok()?
            .duration_since(now)
            .unwrap_or(Duration::ZERO)
    };
    Some(delay.min(MAX_RETRY_AFTER))
}

fn is_retryable_status(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

fn is_retryable_transport_error(error: &reqwest::Error) -> bool {
    if error.is_timeout() || error.is_connect() || error.is_body() {
        return true;
    }

    let mut source = error.source();
    while let Some(cause) = source {
        if cause
            .downcast_ref::<reqwest::Error>()
            .is_some_and(|error| error.is_timeout() || error.is_connect() || error.is_body())
            || cause.downcast_ref::<hyper::Error>().is_some_and(|error| {
                error.is_closed() || error.is_incomplete_message() || error.is_timeout()
            })
            || cause.downcast_ref::<io::Error>().is_some_and(|error| {
                matches!(
                    error.kind(),
                    io::ErrorKind::ConnectionReset
                        | io::ErrorKind::ConnectionAborted
                        | io::ErrorKind::BrokenPipe
                        | io::ErrorKind::UnexpectedEof
                        | io::ErrorKind::TimedOut
                )
            })
        {
            return true;
        }
        source = cause.source();
    }

    false
}

#[cfg(test)]
mod tests {
    use super::{
        is_retryable_status, is_retryable_transport_error, parse_retry_after, send_with_retry,
        send_with_retry_timeout, ResponseBodyPolicy, MAX_RETRY_AFTER,
    };
    use crate::test_support::MockServer;
    use reqwest::{header::HeaderValue, StatusCode};
    use std::{
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc, Mutex,
        },
        thread::{self, JoinHandle},
        time::{Duration, Instant, SystemTime},
    };

    fn start_server<F>(request_count: usize, respond: F) -> (String, JoinHandle<()>)
    where
        F: Fn(usize, TcpStream) + Send + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let worker = thread::spawn(move || {
            for index in 0..request_count {
                let (stream, _) = listener.accept().unwrap();
                respond(index, stream);
            }
        });
        (format!("http://{address}"), worker)
    }

    fn respond_ok(mut stream: TcpStream, body: &[u8]) {
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .unwrap();
        stream.write_all(body).unwrap();
        stream.flush().unwrap();
    }

    #[test]
    fn retries_only_rate_limits_and_server_errors() {
        assert!(is_retryable_status(StatusCode::TOO_MANY_REQUESTS));
        assert!(is_retryable_status(StatusCode::SERVICE_UNAVAILABLE));
        assert!(!is_retryable_status(StatusCode::BAD_REQUEST));
        assert!(!is_retryable_status(StatusCode::UNAUTHORIZED));
        assert!(!is_retryable_status(StatusCode::NOT_FOUND));
    }

    #[test]
    fn parses_and_caps_retry_after_values() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        let parse = |value: &str| parse_retry_after(&HeaderValue::from_str(value).unwrap(), now);

        assert_eq!(parse("1"), Some(Duration::from_secs(1)));
        assert_eq!(parse("0"), Some(Duration::ZERO));
        assert_eq!(parse("999999999999999999999999"), Some(MAX_RETRY_AFTER));
        assert_eq!(
            parse(&httpdate::fmt_http_date(now + Duration::from_secs(10))),
            Some(Duration::from_secs(10))
        );
        assert_eq!(
            parse(&httpdate::fmt_http_date(now + Duration::from_secs(60))),
            Some(MAX_RETRY_AFTER)
        );
        assert_eq!(
            parse(&httpdate::fmt_http_date(now - Duration::from_secs(1))),
            Some(Duration::ZERO)
        );
        assert_eq!(parse("invalid"), None);
        assert_eq!(parse("-1"), None);
        assert_eq!(parse("1.5"), None);
        assert_eq!(parse(""), None);
    }

    #[tokio::test]
    async fn honors_retry_after_for_rate_limits_and_service_unavailability() {
        for status in [429, 503] {
            let response_sent = Arc::new(Mutex::new(None));
            let response_timestamp = Arc::clone(&response_sent);
            let (url, worker) = start_server(2, move |index, mut stream| {
                let mut request = [0; 1024];
                let _ = stream.read(&mut request).unwrap();
                if index == 0 {
                    *response_timestamp.lock().unwrap() = Some(Instant::now());
                    write!(
                        stream,
                        "HTTP/1.1 {status} Retry\r\nRetry-After: 1\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    )
                    .unwrap();
                    stream.flush().unwrap();
                } else {
                    let elapsed = response_timestamp.lock().unwrap().unwrap().elapsed();
                    assert!(elapsed >= Duration::from_secs(1));
                    respond_ok(stream, b"{}");
                }
            });
            let client = reqwest::Client::new();

            send_with_retry("test request", ResponseBodyPolicy::Ignore, || {
                client.get(&url)
            })
            .await
            .unwrap();

            worker.join().unwrap();
        }
    }

    #[tokio::test]
    async fn falls_back_to_exponential_backoff_for_invalid_or_missing_retry_after() {
        for header in ["", "Retry-After: invalid\r\n"] {
            let response_sent = Arc::new(Mutex::new(None));
            let response_timestamp = Arc::clone(&response_sent);
            let (url, worker) = start_server(2, move |index, mut stream| {
                let mut request = [0; 1024];
                let _ = stream.read(&mut request).unwrap();
                if index == 0 {
                    *response_timestamp.lock().unwrap() = Some(Instant::now());
                    write!(
                        stream,
                        "HTTP/1.1 503 Retry\r\n{header}Content-Length: 0\r\nConnection: close\r\n\r\n"
                    )
                    .unwrap();
                    stream.flush().unwrap();
                } else {
                    let elapsed = response_timestamp.lock().unwrap().unwrap().elapsed();
                    assert!(elapsed >= Duration::from_millis(200));
                    respond_ok(stream, b"{}");
                }
            });
            let client = reqwest::Client::new();

            send_with_retry("test request", ResponseBodyPolicy::Ignore, || {
                client.get(&url)
            })
            .await
            .unwrap();

            worker.join().unwrap();
        }
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

        let body = send_with_retry(
            "test request",
            ResponseBodyPolicy::RetryTransientTransfer,
            || client.get(server.url()),
        )
        .await
        .unwrap();

        assert_eq!(body, b"{}");
        assert_eq!(server.finish().len(), 2);
    }

    #[tokio::test]
    async fn retries_after_the_server_drops_the_first_connection() {
        let (url, worker) = start_server(2, |index, mut stream| {
            if index == 0 {
                drop(stream);
            } else {
                let mut request = [0; 1024];
                let _ = stream.read(&mut request).unwrap();
                respond_ok(stream, b"{}");
            }
        });
        let client = reqwest::Client::new();

        let body = send_with_retry("test request", ResponseBodyPolicy::Ignore, || {
            client.get(&url)
        })
        .await
        .unwrap();

        assert!(body.is_empty());
        worker.join().unwrap();
    }

    #[tokio::test]
    async fn does_not_retry_a_malformed_http_response() {
        let (url, worker) = start_server(1, |_, mut stream| {
            let mut request = [0; 1024];
            let _ = stream.read(&mut request).unwrap();
            stream.write_all(b"not an HTTP response\r\n\r\n").unwrap();
            stream.flush().unwrap();
        });
        let client = reqwest::Client::new();
        let build_attempts = Arc::new(AtomicUsize::new(0));
        let attempts = Arc::clone(&build_attempts);

        assert!(
            send_with_retry("test request", ResponseBodyPolicy::Ignore, || {
                attempts.fetch_add(1, Ordering::SeqCst);
                client.get(&url)
            })
            .await
            .is_err()
        );
        assert_eq!(build_attempts.load(Ordering::SeqCst), 1);
        worker.join().unwrap();
    }

    #[tokio::test]
    async fn retries_after_a_truncated_response_body() {
        let (url, worker) = start_server(2, |index, mut stream| {
            let mut request = [0; 1024];
            let _ = stream.read(&mut request).unwrap();
            if index == 0 {
                stream
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 20\r\nConnection: close\r\n\r\n{",
                    )
                    .unwrap();
                stream.flush().unwrap();
            } else {
                respond_ok(stream, b"{}");
            }
        });
        let client = reqwest::Client::new();

        let body = send_with_retry(
            "test request",
            ResponseBodyPolicy::RetryTransientTransfer,
            || client.get(&url),
        )
        .await
        .unwrap();

        assert_eq!(body, b"{}");
        worker.join().unwrap();
    }

    #[tokio::test]
    async fn request_timeout_covers_response_body_transfer() {
        let (url, worker) = start_server(2, |index, mut stream| {
            let mut request = [0; 1024];
            let _ = stream.read(&mut request).unwrap();
            if index == 0 {
                stream
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n")
                    .unwrap();
                stream.flush().unwrap();
                thread::sleep(Duration::from_millis(150));
            } else {
                respond_ok(stream, b"{}");
            }
        });
        let client = reqwest::Client::new();

        let body = send_with_retry_timeout(
            "test request",
            Duration::from_millis(50),
            ResponseBodyPolicy::RetryTransientTransfer,
            || client.get(&url),
        )
        .await
        .unwrap();

        assert_eq!(body, b"{}");
        worker.join().unwrap();
    }

    #[tokio::test]
    async fn does_not_retry_a_successful_response_body_transfer_failure_when_read_once() {
        let (url, worker) = start_server(1, |_, mut stream| {
            let mut request = [0; 1024];
            let _ = stream.read(&mut request).unwrap();
            stream
                .write_all(
                    b"HTTP/1.1 201 Created\r\nContent-Length: 20\r\nConnection: close\r\n\r\n{",
                )
                .unwrap();
            stream.flush().unwrap();
        });
        let client = reqwest::Client::new();
        let build_attempts = Arc::new(AtomicUsize::new(0));
        let attempts = Arc::clone(&build_attempts);

        assert!(
            send_with_retry("test request", ResponseBodyPolicy::ReadOnce, || {
                attempts.fetch_add(1, Ordering::SeqCst);
                client.get(&url)
            })
            .await
            .is_err()
        );
        assert_eq!(build_attempts.load(Ordering::SeqCst), 1);
        worker.join().unwrap();
    }

    #[tokio::test]
    async fn does_not_include_request_url_in_transport_errors() {
        let client = reqwest::Client::new();
        let url = "http://127.0.0.1:1/records?token=secret";

        let error = send_with_retry_timeout(
            "test request",
            Duration::from_millis(10),
            ResponseBodyPolicy::Ignore,
            || client.get(url),
        )
        .await
        .unwrap_err();

        assert!(!format!("{error:#}").contains("secret"));
        assert!(!format!("{error:#}").contains("token="));
    }

    #[tokio::test]
    async fn does_not_retry_permanent_client_errors() {
        let server = MockServer::start(1, |_, _| (400, "{}".into()));
        let client = reqwest::Client::new();

        let error = send_with_retry("test request", ResponseBodyPolicy::Ignore, || {
            client.get(server.url())
        })
        .await
        .unwrap_err();

        assert!(error.to_string().contains("HTTP status 400"));
        assert_eq!(server.finish().len(), 1);
    }
}
