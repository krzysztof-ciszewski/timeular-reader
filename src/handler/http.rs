use std::time::Duration;
use std::{error::Error as _, io};

use anyhow::{Context as _, Result};
use log::warn;
use reqwest::{RequestBuilder, StatusCode};

const MAX_ATTEMPTS: u32 = 3;
const INITIAL_BACKOFF: Duration = Duration::from_millis(200);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

pub(super) async fn send_with_retry<F>(operation: &'static str, build_request: F) -> Result<Vec<u8>>
where
    F: FnMut() -> RequestBuilder,
{
    send_with_retry_timeout(operation, REQUEST_TIMEOUT, build_request).await
}

async fn send_with_retry_timeout<F>(
    operation: &'static str,
    request_timeout: Duration,
    mut build_request: F,
) -> Result<Vec<u8>>
where
    F: FnMut() -> RequestBuilder,
{
    for attempt in 1..=MAX_ATTEMPTS {
        match build_request().timeout(request_timeout).send().await {
            Ok(response) if response.status().is_success() => match response.bytes().await {
                Ok(body) => return Ok(body.to_vec()),
                Err(error) if is_retryable_transport_error(&error) && attempt < MAX_ATTEMPTS => {
                    warn!("{operation} response body failed to transfer; retrying ({attempt}/{MAX_ATTEMPTS})");
                }
                Err(error) => {
                    return Err(error).with_context(|| {
                        format!("{operation} response body failed after {attempt} attempt(s)")
                    });
                }
            },
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
        is_retryable_status, is_retryable_transport_error, send_with_retry, send_with_retry_timeout,
    };
    use crate::test_support::MockServer;
    use reqwest::StatusCode;
    use std::{
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        thread::{self, JoinHandle},
        time::Duration,
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

        let body = send_with_retry("test request", || client.get(server.url()))
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

        let body = send_with_retry("test request", || client.get(&url))
            .await
            .unwrap();

        assert_eq!(body, b"{}");
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

        assert!(send_with_retry("test request", || client.get(&url))
            .await
            .is_err());
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

        let body = send_with_retry("test request", || client.get(&url))
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

        let body = send_with_retry_timeout("test request", Duration::from_millis(50), || {
            client.get(&url)
        })
        .await
        .unwrap();

        assert_eq!(body, b"{}");
        worker.join().unwrap();
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
