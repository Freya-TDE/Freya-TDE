//! A small HTTP server for the tests: answers each request with the next
//! canned JSON and records what it got.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;

use serde_json::Value;

/// A recorded request.
#[derive(Debug)]
pub struct Seen {
    pub line: String,
    pub headers: Vec<(String, String)>,
    pub body: Value,
}

impl Seen {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// A server answering each request with the next `(status, body)`.
pub fn server(answers: Vec<(u16, Value)>) -> (String, mpsc::Receiver<Seen>) {
    raw_server(
        answers
            .into_iter()
            .map(|(status, body)| (status, "application/json", body.to_string()))
            .collect(),
    )
}

/// A server answering each request with the next `(status, content type,
/// body)`, e.g. server-sent events.
pub fn raw_server(answers: Vec<(u16, &'static str, String)>) -> (String, mpsc::Receiver<Seen>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = format!("http://{}", listener.local_addr().unwrap());
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for (status, content_type, text) in answers {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let mut headers = Vec::new();
            loop {
                let mut h = String::new();
                reader.read_line(&mut h).unwrap();
                let h = h.trim_end();
                if h.is_empty() {
                    break;
                }
                let (k, v) = h.split_once(':').unwrap();
                headers.push((k.trim().to_owned(), v.trim().to_owned()));
            }
            let len = headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                .map_or(0, |(_, v)| v.parse().unwrap());
            let mut buf = vec![0; len];
            reader.read_exact(&mut buf).unwrap();
            let body_in = serde_json::from_slice(&buf).unwrap_or(Value::Null);
            tx.send(Seen {
                line: line.trim_end().to_owned(),
                headers,
                body: body_in,
            })
            .unwrap();
            let mut out = stream;
            write!(
                out,
                "HTTP/1.1 {status} X\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
                text.len()
            )
            .unwrap();
        }
    });
    (addr, rx)
}
