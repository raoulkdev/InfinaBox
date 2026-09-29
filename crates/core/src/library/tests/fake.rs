//! A fake HTTP server standing in for the network, on 127.0.0.1.

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use tiny_http::{Header, Response, Server};

pub struct Reply {
    pub status: u16,
    pub body: Vec<u8>,
    pub headers: Vec<(String, String)>,
    pub delay: Duration,
}

impl Reply {
    pub fn ok(body: impl Into<Vec<u8>>) -> Self {
        Self {
            status: 200,
            body: body.into(),
            headers: Vec::new(),
            delay: Duration::ZERO,
        }
    }
    pub fn status(status: u16) -> Self {
        Self {
            status,
            ..Self::ok("")
        }
    }
    pub fn redirect(to: &str) -> Self {
        Self {
            headers: vec![("Location".into(), to.into())],
            ..Self::status(302)
        }
    }
    pub fn delayed(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }
}

pub struct FakeServer {
    pub base: String,
    /// Every request's User-Agent and path+query, in arrival order.
    pub seen: Arc<Mutex<Vec<(String, String)>>>,
    server: Arc<Server>,
}

impl FakeServer {
    /// `route` maps a path+query (like `/assets?t=models`) and the server's
    /// own base (for building links) to a reply.
    pub fn start(route: impl Fn(&str, &str) -> Reply + Send + Sync + 'static) -> Self {
        let server = Arc::new(Server::http("127.0.0.1:0").unwrap());
        let port = server.server_addr().to_ip().unwrap().port();
        let base = format!("http://127.0.0.1:{port}");
        let seen = Arc::new(Mutex::new(Vec::new()));
        let route = Arc::new(route);
        let (srv, log, base2) = (server.clone(), seen.clone(), base.clone());
        thread::spawn(move || {
            for request in srv.incoming_requests() {
                let route = route.clone();
                let log = log.clone();
                let base = base2.clone();
                thread::spawn(move || {
                    let url = request.url().to_string();
                    let agent = request
                        .headers()
                        .iter()
                        .find(|h| h.field.equiv("User-Agent"))
                        .map(|h| h.value.to_string())
                        .unwrap_or_default();
                    log.lock().unwrap().push((agent, url.clone()));
                    let reply = route(&url, &base);
                    thread::sleep(reply.delay);
                    let mut response =
                        Response::from_data(reply.body).with_status_code(reply.status);
                    for (k, v) in reply.headers {
                        response.add_header(Header::from_bytes(k, v).unwrap());
                    }
                    let _ = request.respond(response);
                });
            }
        });
        Self { base, seen, server }
    }

    pub fn paths(&self) -> Vec<String> {
        self.seen
            .lock()
            .unwrap()
            .iter()
            .map(|s| s.1.clone())
            .collect()
    }
}

impl Drop for FakeServer {
    fn drop(&mut self) {
        self.server.unblock();
    }
}
