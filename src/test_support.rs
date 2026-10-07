use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    thread::{self, JoinHandle},
};

use chrono::{Local, TimeZone};

use crate::tracker::config::{Side, TimeEntry};

pub(crate) struct MockServer {
    url: String,
    worker: JoinHandle<Vec<String>>,
}

impl MockServer {
    pub(crate) fn start<F>(request_count: usize, respond: F) -> Self
    where
        F: Fn(usize, &str) -> (u16, String) + Send + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let worker = thread::spawn(move || {
            let mut requests = Vec::with_capacity(request_count);
            for index in 0..request_count {
                let (mut stream, _) = listener.accept().unwrap();
                let request = read_request(&mut stream);
                let (status, body) = respond(index, &request);
                let reason = if status < 400 {
                    "OK"
                } else {
                    "Internal Server Error"
                };
                write!(
                    stream,
                    "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
                stream.flush().unwrap();
                requests.push(request);
            }
            requests
        });

        Self {
            url: format!("http://{address}"),
            worker,
        }
    }

    pub(crate) fn url(&self) -> &str {
        &self.url
    }

    pub(crate) fn finish(self) -> Vec<String> {
        self.worker.join().unwrap()
    }
}

fn read_request(stream: &mut TcpStream) -> String {
    let mut request = Vec::new();
    let mut chunk = [0; 4096];

    loop {
        let bytes_read = stream.read(&mut chunk).unwrap();
        if bytes_read == 0 {
            break;
        }
        request.extend_from_slice(&chunk[..bytes_read]);

        if let Some(header_end) = request.windows(4).position(|part| part == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&request[..header_end]);
            let content_length = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            if request.len() >= header_end + 4 + content_length {
                break;
            }
        }
    }

    String::from_utf8(request).unwrap()
}

pub(crate) fn time_entry(side_num: u8, label: &str) -> TimeEntry {
    TimeEntry {
        side: Side {
            side_num,
            label: label.to_string(),
            configurable: true,
        },
        start: Local.timestamp_opt(1_700_000_000, 0).single().unwrap(),
        end: Local.timestamp_opt(1_700_003_661, 0).single().unwrap(),
    }
}

pub(crate) fn request_body(request: &str) -> &str {
    request.split_once("\r\n\r\n").unwrap().1
}
