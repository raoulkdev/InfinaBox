//! The loopback bridge the InfinaBox MCP server (running inside the user's
//! agent CLI) uses to reach this running app: run/stop the game, read its
//! errors and output. Protocol: `infinabox_mcp_server::bridge_protocol`.
//!
//! Per connection: the first line must be a `Hello` carrying this launch's
//! token (compared in constant time), or the connection is closed without
//! a reply. After that, each `BridgeRequest` line gets one `BridgeResponse`
//! line, one at a time. Anything malformed (bad JSON, an unknown method, a
//! line over `MAX_LINE_BYTES`, invalid UTF-8) closes the connection.
//!
//! Split like `godot.rs`: `serve`/`serve_connection` take a
//! `BridgeHandler` and no `AppHandle`, so they're tested over real TCP
//! against the real MCP-server bridge client; `start` is the thin wrapper
//! that plugs in the app's `GameManager`.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use infinabox_mcp_server::bridge_protocol::{BridgeRequest, BridgeResponse, Hello};
use rand::Rng;
use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Manager};

use super::godot::{self, GameHost, GameManager};

/// Longest accepted request line (a `Hello` is a token plus a path).
const MAX_LINE_BYTES: usize = 64 * 1024;
/// The MCP-server client drops replies over 4 MiB; stay under that.
const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024 - 1;
/// Budget for the output/error lists inside a reply, leaving room for the
/// envelope; the oldest entries are dropped first to fit.
const MAX_LIST_BYTES: usize = 3 * 1024 * 1024;
/// Total time (not per read) an unauthenticated connection gets to send its
/// hello line, so a client dripping a byte at a time can't hold one open.
const HELLO_DEADLINE: Duration = Duration::from_secs(3);
/// An authenticated connection idle this long is closed (the client opens
/// a fresh connection per request anyway).
const IDLE_TIMEOUT: Duration = Duration::from_secs(120);
const WRITE_TIMEOUT: Duration = Duration::from_secs(10);
/// Connections still waiting for their hello; more are closed straight
/// away. Counted apart from authenticated ones, so sockets without the
/// token can never use up the slots the agent needs.
const MAX_PENDING: usize = 32;
/// Authenticated connections served at once; more get a "too many
/// connections" error reply.
const MAX_AUTHENTICATED: usize = 32;
/// Pause after a failed `accept` (e.g. out of file descriptors), so a
/// persistent error doesn't spin the accept loop.
const ACCEPT_ERROR_BACKOFF: Duration = Duration::from_millis(100);
pub const TOO_MANY_CONNECTIONS: &str =
    "The InfinaBox app is busy with too many bridge connections; try again in a moment.";

/// Where the bridge is listening and the per-launch token clients must
/// present. `None` until `start` has bound the listener.
#[derive(Clone, Debug)]
pub struct BridgeInfo {
    pub addr: String,
    pub token: String,
}

#[derive(Default)]
pub struct BridgeState(pub Mutex<Option<BridgeInfo>>);

/// Answers one authenticated request. `project` is the connection's
/// `Hello.project`.
pub trait BridgeHandler: Send + Sync + 'static {
    fn handle(&self, project: &str, request: BridgeRequest) -> BridgeResponse;
}

/// Serves the bridge from a `GameManager`: the same calls the game
/// commands make.
///
/// Scope: `RunGame` runs whichever Godot project the connection's
/// `Hello.project` names, and `StopGame`/`RecentErrors`/`RecentOutput` act
/// on whatever game is running now, even one started for another project
/// (`GameStatus` reports which). That's acceptable because only the user's
/// own agent, launched by this app, has the token.
pub struct GameBridge {
    pub manager: Arc<GameManager>,
    pub host: Arc<dyn GameHost>,
}

fn ok(data: serde_json::Value) -> BridgeResponse {
    BridgeResponse::Ok { data }
}

fn result(r: Result<serde_json::Value, String>) -> BridgeResponse {
    match r {
        Ok(data) => ok(data),
        Err(message) => BridgeResponse::Error { message },
    }
}

impl GameBridge {
    fn status(&self) -> serde_json::Value {
        let (state, project) = self.manager.status();
        json!({ "state": state, "project": project })
    }
}

impl BridgeHandler for GameBridge {
    fn handle(&self, project: &str, request: BridgeRequest) -> BridgeResponse {
        match request {
            BridgeRequest::RunGame => result(
                self.manager
                    .run(&self.host, project)
                    .map(|()| self.status()),
            ),
            BridgeRequest::StopGame => {
                result(self.manager.stop(&self.host).map(|()| self.status()))
            }
            BridgeRequest::GameStatus => ok(self.status()),
            BridgeRequest::RecentErrors { limit } => ok(newest_within(
                self.manager.recent_errors(limit),
                MAX_LIST_BYTES,
            )),
            BridgeRequest::RecentOutput { lines } => ok(newest_within(
                self.manager.recent_output(lines),
                MAX_LIST_BYTES,
            )),
        }
    }
}

/// The newest `items` (kept oldest first) whose JSON fits in `budget`
/// bytes. Output lines can each be up to 64 KiB, so a full buffer
/// wouldn't fit in one reply.
fn newest_within<T: Serialize>(items: Vec<T>, budget: usize) -> serde_json::Value {
    let mut kept = Vec::new();
    let mut used = 2; // []
    for item in items.into_iter().rev() {
        let value = serde_json::to_value(item).unwrap_or(serde_json::Value::Null);
        let size = value.to_string().len() + 1;
        if used + size > budget {
            break;
        }
        used += size;
        kept.push(value);
    }
    kept.reverse();
    serde_json::Value::Array(kept)
}

/// Compares without an early exit, so response timing says nothing about
/// how much of a guessed token was right.
fn tokens_match(given: &str, expected: &str) -> bool {
    let (a, b) = (given.as_bytes(), expected.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let diff = a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y));
    std::hint::black_box(diff) == 0
}

/// A fresh random 32-byte token, hex-encoded.
fn new_token() -> String {
    let mut bytes = [0u8; 32];
    // ThreadRng is a CSPRNG seeded from the OS.
    rand::rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Reads one `\n`-terminated line of at most `MAX_LINE_BYTES`. `Ok(None)`
/// at a clean end of stream; an error for an overlong, cut-off, or
/// non-UTF-8 line.
fn read_line(reader: &mut impl BufRead) -> io::Result<Option<String>> {
    let mut buf = Vec::new();
    let n = reader
        .by_ref()
        .take(MAX_LINE_BYTES as u64 + 1)
        .read_until(b'\n', &mut buf)?;
    if n == 0 {
        return Ok(None);
    }
    if buf.pop() != Some(b'\n') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "line too long or cut off",
        ));
    }
    if buf.last() == Some(&b'\r') {
        buf.pop();
    }
    String::from_utf8(buf)
        .map(Some)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

fn invalid(e: serde_json::Error) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, e)
}

/// A socket read with an optional overall deadline: before each read the
/// read timeout is set to the time left, so the deadline holds however the
/// peer spreads its bytes out.
struct DeadlineStream {
    stream: TcpStream,
    deadline: Option<Instant>,
}

impl Read for DeadlineStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if let Some(deadline) = self.deadline {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "hello deadline passed",
                ));
            }
            self.stream.set_read_timeout(Some(left))?;
        }
        self.stream.read(buf)
    }
}

/// One taken slot of a connection budget, given back on drop (including
/// when the connection's thread panics).
struct Slot(Arc<AtomicUsize>);

impl Slot {
    fn take(counter: &Arc<AtomicUsize>, max: usize) -> Option<Slot> {
        counter
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| {
                (n < max).then_some(n + 1)
            })
            .ok()
            .map(|_| Slot(counter.clone()))
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// The two connection budgets (see `MAX_PENDING`/`MAX_AUTHENTICATED`).
#[derive(Clone, Default)]
struct Budgets {
    pending: Arc<AtomicUsize>,
    authenticated: Arc<AtomicUsize>,
}

fn write_response(writer: &mut TcpStream, response: &BridgeResponse) -> io::Result<()> {
    let mut out = serde_json::to_string(response).map_err(invalid)?;
    if out.len() > MAX_RESPONSE_BYTES {
        out = serde_json::to_string(&BridgeResponse::Error {
            message: "The reply was too large to send over the bridge.".into(),
        })
        .map_err(invalid)?;
    }
    out.push('\n');
    writer.write_all(out.as_bytes())?;
    writer.flush()
}

/// Serves one connection until the client closes it or misbehaves.
/// `pending` is the pre-auth slot this connection holds; it's swapped for
/// an authenticated one once the hello checks out. Returns why it ended
/// (the accept loop ignores it).
fn serve_connection(
    stream: TcpStream,
    token: &str,
    handler: &dyn BridgeHandler,
    budgets: &Budgets,
    pending: Slot,
) -> io::Result<()> {
    stream.set_write_timeout(Some(WRITE_TIMEOUT))?;
    let mut writer = stream.try_clone()?;
    let mut reader = BufReader::new(DeadlineStream {
        stream,
        deadline: Some(Instant::now() + HELLO_DEADLINE),
    });

    let Some(first) = read_line(&mut reader)? else {
        return Ok(());
    };
    let hello: Hello = serde_json::from_str(&first).map_err(invalid)?;
    if !tokens_match(&hello.hello, token) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "wrong bridge token",
        ));
    }
    let inner = reader.get_mut();
    inner.deadline = None;
    inner.stream.set_read_timeout(Some(IDLE_TIMEOUT))?;
    let authenticated = Slot::take(&budgets.authenticated, MAX_AUTHENTICATED);
    drop(pending);
    let Some(_authenticated) = authenticated else {
        // A token holder deserves a real answer. Read its request first:
        // closing with unread data would reset the connection and could
        // lose the reply.
        let _ = read_line(&mut reader);
        let busy = BridgeResponse::Error {
            message: TOO_MANY_CONNECTIONS.into(),
        };
        return write_response(&mut writer, &busy);
    };

    while let Some(line) = read_line(&mut reader)? {
        let request: BridgeRequest = serde_json::from_str(&line).map_err(invalid)?;
        let response = handler.handle(&hello.project, request);
        write_response(&mut writer, &response)?;
    }
    Ok(())
}

/// Accepts connections until `incoming` ends (a listener's never does),
/// each on its own thread. Blocks.
fn serve_incoming(
    incoming: impl Iterator<Item = io::Result<TcpStream>>,
    token: String,
    handler: Arc<dyn BridgeHandler>,
) {
    let token: Arc<str> = token.into();
    let budgets = Budgets::default();
    for stream in incoming {
        let stream = match stream {
            Ok(stream) => stream,
            Err(_) => {
                thread::sleep(ACCEPT_ERROR_BACKOFF);
                continue;
            }
        };
        // Without the token nobody is owed a reply: just close.
        let Some(pending) = Slot::take(&budgets.pending, MAX_PENDING) else {
            continue;
        };
        let (token, handler, budgets) = (token.clone(), handler.clone(), budgets.clone());
        thread::spawn(move || {
            let _ = serve_connection(stream, &token, handler.as_ref(), &budgets, pending);
        });
    }
}

fn serve(listener: TcpListener, token: String, handler: Arc<dyn BridgeHandler>) {
    serve_incoming(listener.incoming(), token, handler);
}

/// Binds `127.0.0.1:0` and returns the listener, its address, and a fresh
/// token.
fn bind() -> io::Result<(TcpListener, String, String)> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let addr = listener.local_addr()?.to_string();
    Ok((listener, addr, new_token()))
}

/// Binds `127.0.0.1:0`, generates the token, stores both in `BridgeState`,
/// and serves connections on a background thread. Called once from the
/// app's `setup` hook.
pub fn start(app: &AppHandle) -> Result<(), String> {
    let (listener, addr, token) =
        bind().map_err(|e| format!("Couldn't start the InfinaBox bridge: {e}"))?;
    let handler: Arc<dyn BridgeHandler> = Arc::new(GameBridge {
        manager: godot::manager(app),
        host: godot::app_host(app),
    });
    *app.state::<BridgeState>()
        .0
        .lock()
        .unwrap_or_else(|p| p.into_inner()) = Some(BridgeInfo {
        addr,
        token: token.clone(),
    });
    thread::Builder::new()
        .name("infinabox-bridge".into())
        .spawn(move || serve(listener, token, handler))
        .map_err(|e| format!("Couldn't start the InfinaBox bridge: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::godot::test_support::*;
    use super::*;
    use infinabox_core::godot::{GameOutputLine, GameState, OutputStream};
    use infinabox_mcp_server::bridge_client::{self, BridgeConfig};

    /// Records what it was asked and answers with it.
    #[derive(Default)]
    struct EchoHandler {
        seen: Mutex<Vec<(String, BridgeRequest)>>,
    }

    impl BridgeHandler for EchoHandler {
        fn handle(&self, project: &str, request: BridgeRequest) -> BridgeResponse {
            self.seen
                .lock()
                .unwrap()
                .push((project.to_string(), request.clone()));
            match request {
                BridgeRequest::StopGame => BridgeResponse::Error {
                    message: "nothing to stop".into(),
                },
                other => ok(json!({ "echo": other, "project": project })),
            }
        }
    }

    fn spawn_bridge(handler: Arc<dyn BridgeHandler>) -> (String, String) {
        let (listener, addr, token) = bind().unwrap();
        let t = token.clone();
        thread::spawn(move || serve(listener, t, handler));
        (addr, token)
    }

    /// One call through the real MCP-server client.
    fn call(
        addr: &str,
        token: &str,
        project: &str,
        request: BridgeRequest,
    ) -> Result<serde_json::Value, String> {
        let config = BridgeConfig {
            addr: addr.into(),
            token: token.into(),
            project: project.into(),
        };
        tauri::async_runtime::block_on(bridge_client::call(Some(&config), &request))
    }

    /// Sends raw bytes and returns everything the app sent back before it
    /// closed the connection.
    fn raw_exchange(addr: &str, input: &[u8]) -> String {
        let mut stream = TcpStream::connect(addr).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        // The app may close before reading everything; that's the point.
        let _ = stream.write_all(input);
        let _ = stream.shutdown(std::net::Shutdown::Write);
        let mut out = String::new();
        let _ = stream.read_to_string(&mut out);
        out
    }

    #[test]
    fn tokens_are_random_hex_and_compared_exactly() {
        let (a, b) = (new_token(), new_token());
        assert_eq!(a.len(), 64);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
        assert!(tokens_match(&a, &a.clone()));
        assert!(!tokens_match(&a, &b));
        assert!(!tokens_match(&a[..63], &a));
        assert!(!tokens_match("", &a));
    }

    #[test]
    fn every_request_type_round_trips_through_the_real_client() {
        let handler = Arc::new(EchoHandler::default());
        let (addr, token) = spawn_bridge(handler.clone());
        let requests = [
            BridgeRequest::RunGame,
            BridgeRequest::GameStatus,
            BridgeRequest::RecentErrors { limit: 7 },
            BridgeRequest::RecentOutput { lines: 9 },
        ];
        for request in &requests {
            let data = call(&addr, &token, "/games/g", request.clone()).unwrap();
            assert_eq!(data, json!({ "echo": request, "project": "/games/g" }));
        }
        // An app-side error arrives as an error, never as success.
        let err = call(&addr, &token, "/games/g", BridgeRequest::StopGame).unwrap_err();
        assert_eq!(err, "nothing to stop");

        let seen = handler.seen.lock().unwrap().clone();
        assert_eq!(seen.len(), 5);
        assert!(seen.iter().all(|(p, _)| p == "/games/g"));
    }

    #[test]
    fn a_wrong_token_is_closed_without_serving_anything() {
        let handler = Arc::new(EchoHandler::default());
        let (addr, token) = spawn_bridge(handler.clone());
        let mut wrong = token.clone().into_bytes();
        wrong[0] = if wrong[0] == b'a' { b'b' } else { b'a' };
        let wrong = String::from_utf8(wrong).unwrap();
        let err = call(&addr, &wrong, "/games/g", BridgeRequest::RunGame).unwrap_err();
        assert!(
            err.contains("closed the connection") || err.contains("Lost the connection"),
            "{err}"
        );
        assert!(handler.seen.lock().unwrap().is_empty());
    }

    #[test]
    fn malformed_input_closes_the_connection() {
        let handler = Arc::new(EchoHandler::default());
        let (addr, token) = spawn_bridge(handler.clone());
        let hello = format!(r#"{{"hello":"{token}","project":"/p"}}"#);

        // Not a Hello at all.
        assert_eq!(raw_exchange(&addr, b"{\"method\":\"run_game\"}\n"), "");
        assert_eq!(raw_exchange(&addr, b"garbage\n"), "");
        // A good hello, then an unknown method: closed with no reply, and
        // the valid request after it is never served.
        let input =
            format!("{hello}\n{{\"method\":\"format_disk\"}}\n{{\"method\":\"game_status\"}}\n");
        assert_eq!(raw_exchange(&addr, input.as_bytes()), "");
        // An overlong line.
        let long = format!("{hello}\n{}\n", "x".repeat(MAX_LINE_BYTES + 10));
        assert_eq!(raw_exchange(&addr, long.as_bytes()), "");
        // Invalid UTF-8.
        let mut bad = format!("{hello}\n").into_bytes();
        bad.extend_from_slice(b"\xff\xfe\n");
        assert_eq!(raw_exchange(&addr, &bad), "");
        assert!(handler.seen.lock().unwrap().is_empty());

        // Several requests on one connection: one reply line each, in order.
        let input = format!(
            "{hello}\n{{\"method\":\"game_status\"}}\n{{\"method\":\"recent_errors\",\"params\":{{\"limit\":2}}}}\n"
        );
        let out = raw_exchange(&addr, input.as_bytes());
        let replies: Vec<BridgeResponse> = out
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(
            replies,
            vec![
                ok(json!({ "echo": { "method": "game_status" }, "project": "/p" })),
                ok(json!({
                    "echo": { "method": "recent_errors", "params": { "limit": 2 } },
                    "project": "/p"
                })),
            ]
        );
    }

    #[test]
    fn long_lists_are_trimmed_to_fit_a_reply() {
        let lines: Vec<GameOutputLine> = (0..100)
            .map(|i| GameOutputLine {
                stream: OutputStream::Stdout,
                text: format!("{i:03}{}", "x".repeat(1000)),
            })
            .collect();
        let value = newest_within(lines, 10_000);
        let kept = value.as_array().unwrap();
        assert!(value.to_string().len() <= 10_000);
        assert!(kept.len() >= 8 && kept.len() < 100, "{}", kept.len());
        // The newest are kept, oldest first.
        assert!(kept.last().unwrap()["text"]
            .as_str()
            .unwrap()
            .starts_with("099"));
        assert!(kept[0]["text"].as_str().unwrap() < kept[1]["text"].as_str().unwrap());
    }

    /// The whole path the agent uses: real client → bridge → GameManager →
    /// a real child process.
    #[cfg(unix)]
    #[test]
    fn the_agent_can_run_inspect_and_stop_the_game_over_the_bridge() {
        let tools = TempDir::new("tools");
        let godot_bin = fake_godot(
            tools.path(),
            "exit 0",
            "echo 'game says hi'\n\
             echo 'SCRIPT ERROR: kaboom' >&2\n\
             echo '   at: _process (res://player.gd:12)' >&2\n\
             exec sleep 30",
        );
        let project = fixture_project("clean");
        let path = project.path().to_str().unwrap().to_string();
        let host = TestHost::new(Ok(godot_bin), &[]);
        let manager = Arc::new(GameManager::default());
        let (addr, token) = spawn_bridge(Arc::new(GameBridge {
            manager: manager.clone(),
            host: host_of(&host),
        }));

        let status = call(&addr, &token, &path, BridgeRequest::GameStatus).unwrap();
        assert_eq!(status, json!({ "state": "stopped", "project": null }));

        let ran = call(&addr, &token, &path, BridgeRequest::RunGame).unwrap();
        assert_eq!(ran, json!({ "state": "running", "project": path }));
        host.wait_for(Duration::from_secs(20), |e| {
            e.iter().any(|e| matches!(e, Event::Error(_)))
        });

        let errors = call(
            &addr,
            &token,
            &path,
            BridgeRequest::RecentErrors { limit: 5 },
        )
        .unwrap();
        assert_eq!(
            errors,
            json!([{
                "message": "kaboom",
                "file": "res://player.gd",
                "line": 12,
                "raw": "SCRIPT ERROR: kaboom\n   at: _process (res://player.gd:12)"
            }])
        );
        let output = call(
            &addr,
            &token,
            &path,
            BridgeRequest::RecentOutput { lines: 50 },
        )
        .unwrap();
        assert!(
            output
                .as_array()
                .unwrap()
                .contains(&json!({ "stream": "stdout", "text": "game says hi" })),
            "{output}"
        );

        let stopped = call(&addr, &token, &path, BridgeRequest::StopGame).unwrap();
        assert_eq!(stopped["state"], "stopped");
        assert_eq!(manager.state(), GameState::Stopped);

        // A project that isn't a Godot game: a plain error, not success.
        let other = TempDir::new("not-godot");
        let err = call(
            &addr,
            &token,
            other.path().to_str().unwrap(),
            BridgeRequest::RunGame,
        )
        .unwrap_err();
        assert_eq!(err, godot::NOT_A_GODOT_GAME);
    }

    /// Regression: sockets without the token dripping one byte at a time
    /// used to hold every connection slot indefinitely (a per-read timeout
    /// never fired), locking the real agent out. Now each gets
    /// `HELLO_DEADLINE` in total, after which the agent gets through.
    #[test]
    fn dripping_clients_without_the_token_cannot_lock_out_the_agent() {
        let (addr, token) = spawn_bridge(Arc::new(EchoHandler::default()));
        let started = Instant::now();
        let drippers: Vec<_> = (0..MAX_PENDING)
            .map(|_| {
                let mut stream = TcpStream::connect(&addr).unwrap();
                thread::spawn(move || {
                    // Returns how long the app kept this socket open.
                    let opened = Instant::now();
                    while opened.elapsed() < Duration::from_secs(20) {
                        if stream.write_all(b" ").is_err() {
                            return opened.elapsed();
                        }
                        thread::sleep(Duration::from_millis(200));
                    }
                    opened.elapsed()
                })
            })
            .collect();
        // Let the accept loop hand every dripper a pending slot.
        thread::sleep(Duration::from_millis(300));
        assert!(call(&addr, &token, "/g", BridgeRequest::GameStatus).is_err());

        for dripper in drippers {
            let held = dripper.join().unwrap();
            assert!(
                held < HELLO_DEADLINE + Duration::from_secs(2),
                "held {held:?}"
            );
        }
        call(&addr, &token, "/g", BridgeRequest::GameStatus).unwrap();
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    /// Opens a connection, authenticates, and proves it with one request;
    /// the socket stays open (and holds its slot) until dropped.
    fn authenticated_connection(addr: &str, token: &str) -> TcpStream {
        let stream = TcpStream::connect(addr).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut writer = stream.try_clone().unwrap();
        writeln!(writer, r#"{{"hello":"{token}","project":"/p"}}"#).unwrap();
        writeln!(writer, r#"{{"method":"game_status"}}"#).unwrap();
        let mut reply = String::new();
        BufReader::new(stream.try_clone().unwrap())
            .read_line(&mut reply)
            .unwrap();
        assert!(reply.contains("\"ok\""), "{reply}");
        stream
    }

    #[test]
    fn authenticated_and_pending_connections_have_separate_budgets() {
        let (addr, token) = spawn_bridge(Arc::new(EchoHandler::default()));
        // Idle token holders don't use up the pending slots...
        let held: Vec<_> = (0..MAX_AUTHENTICATED - 1)
            .map(|_| authenticated_connection(&addr, &token))
            .collect();
        call(&addr, &token, "/g", BridgeRequest::GameStatus).unwrap();
        // ...but once their own budget is full, a new one is told why.
        let last = authenticated_connection(&addr, &token);
        let err = call(&addr, &token, "/g", BridgeRequest::GameStatus).unwrap_err();
        assert_eq!(err, TOO_MANY_CONNECTIONS);

        drop(last);
        drop(held);
        let deadline = Instant::now() + Duration::from_secs(10);
        while call(&addr, &token, "/g", BridgeRequest::GameStatus).is_err() {
            assert!(Instant::now() < deadline, "slots were never given back");
            thread::sleep(Duration::from_millis(50));
        }
    }

    struct PanickingHandler;

    impl BridgeHandler for PanickingHandler {
        fn handle(&self, _project: &str, request: BridgeRequest) -> BridgeResponse {
            match request {
                BridgeRequest::RunGame => panic!("handler bug (expected in this test)"),
                _ => ok(json!("fine")),
            }
        }
    }

    /// A panicking connection thread used to keep its slot forever.
    #[test]
    fn a_panicking_handler_gives_its_slot_back() {
        let (addr, token) = spawn_bridge(Arc::new(PanickingHandler));
        for _ in 0..MAX_AUTHENTICATED + 5 {
            assert!(call(&addr, &token, "/g", BridgeRequest::RunGame).is_err());
        }
        assert_eq!(
            call(&addr, &token, "/g", BridgeRequest::GameStatus).unwrap(),
            json!("fine")
        );
    }

    /// A persistent accept error (e.g. out of file descriptors) backs off
    /// instead of spinning.
    #[test]
    fn accept_errors_back_off() {
        let errors = (0..5).map(|_| Err(io::Error::other("EMFILE")));
        let started = Instant::now();
        serve_incoming(errors, new_token(), Arc::new(EchoHandler::default()));
        assert!(started.elapsed() >= ACCEPT_ERROR_BACKOFF * 5);
    }
}
