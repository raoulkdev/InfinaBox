//! A tiny local HTTP server for the provider tests (the network is never used
//! in tests). It records every request and answers each with one canned reply.

use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use tiny_http::{Header, Response, Server};

#[derive(Clone, Debug)]
pub struct Seen {
    pub method: String,
    /// Path and query, e.g. `/v1/tts`.
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Seen {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
    pub fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.body).expect("request body is JSON")
    }
}

pub struct Canned {
    pub status: u16,
    pub content_type: &'static str,
    pub body: Vec<u8>,
}

impl Canned {
    pub fn new(status: u16, content_type: &'static str, body: impl Into<Vec<u8>>) -> Self {
        Self { status, content_type, body: body.into() }
    }
    pub fn json(status: u16, body: &str) -> Self {
        Self::new(status, "application/json", body)
    }
}

pub struct Fake {
    pub base: String,
    pub seen: Arc<Mutex<Vec<Seen>>>,
    server: Arc<Server>,
    thread: Option<JoinHandle<()>>,
}

impl Fake {
    pub fn start(canned: Canned) -> Fake {
        let server = Arc::new(Server::http("127.0.0.1:0").expect("bind fake server"));
        let base = format!("http://{}", server.server_addr().to_ip().expect("ip address"));
        let seen = Arc::new(Mutex::new(Vec::new()));
        let (s, log) = (server.clone(), seen.clone());
        let thread = std::thread::spawn(move || {
            for mut req in s.incoming_requests() {
                let mut body = String::new();
                let _ = std::io::Read::read_to_string(req.as_reader(), &mut body);
                log.lock().unwrap().push(Seen {
                    method: req.method().to_string(),
                    url: req.url().to_string(),
                    headers: req
                        .headers()
                        .iter()
                        .map(|h| (h.field.to_string(), h.value.to_string()))
                        .collect(),
                    body,
                });
                let header = Header::from_bytes("Content-Type", canned.content_type).unwrap();
                let _ = req.respond(
                    Response::from_data(canned.body.clone())
                        .with_status_code(canned.status)
                        .with_header(header),
                );
            }
        });
        Fake { base, seen, server, thread: Some(thread) }
    }

    /// The one request received (panics if there wasn't exactly one).
    pub fn only_request(&self) -> Seen {
        let seen = self.seen.lock().unwrap();
        assert_eq!(seen.len(), 1, "expected exactly one request");
        seen[0].clone()
    }
}

impl Drop for Fake {
    fn drop(&mut self) {
        self.server.unblock();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// A small real PNG (a 4x4 red image) for image replies.
pub fn png_bytes(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(w, h, image::Rgba([200, 30, 30, 255]));
    let mut out = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).unwrap();
    out
}
