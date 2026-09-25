//! Client half of the bridge to the running InfinaBox app (the protocol is
//! the frozen contract in `bridge_protocol.rs`): connect over loopback TCP,
//! send the `Hello`, send one request, read one response.
//!
//! Every failure comes back as a plain message the model can relay to the
//! user. Nothing here ever reports success it didn't get from the app.

use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

use crate::bridge_protocol::{BridgeRequest, BridgeResponse, Hello};

/// The message every "can't reach the app" failure starts with.
pub const APP_NOT_RUNNING: &str = "InfinaBox app is not running";

/// Connecting is loopback, so this only trips when something is badly wrong.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
/// Starting a game (which can first install the addon) is the slowest
/// request; this bounds how long a hung app can stall the agent.
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(60);

/// Where the app's bridge is and how to authenticate, as passed by the app
/// in the environment when it launched the agent.
#[derive(Clone, Debug, PartialEq)]
pub struct BridgeConfig {
    pub addr: String,
    pub token: String,
    pub project: String,
}

impl BridgeConfig {
    /// `None` when any of the three variables is missing or empty, which
    /// means this server wasn't launched by the app.
    pub fn from_env() -> Option<Self> {
        let get = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
        Some(Self {
            addr: get(crate::ENV_BRIDGE_ADDR)?,
            token: get(crate::ENV_BRIDGE_TOKEN)?,
            project: get(crate::ENV_PROJECT)?,
        })
    }
}

/// Sends one request over a fresh connection and returns the app's `data`
/// on success, or a readable error message.
pub async fn call(
    config: Option<&BridgeConfig>,
    request: &BridgeRequest,
) -> Result<serde_json::Value, String> {
    let config = config.ok_or_else(|| {
        format!(
            "{APP_NOT_RUNNING}: this MCP server wasn't started by the InfinaBox app \
             (the bridge address isn't set). Open the project in InfinaBox and try again."
        )
    })?;

    let stream = match tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect(&config.addr)).await
    {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => {
            return Err(format!(
                "{APP_NOT_RUNNING} (couldn't connect to it at {}: {e}).",
                config.addr
            ));
        }
        Err(_) => {
            return Err(format!(
                "{APP_NOT_RUNNING} (connecting to it at {} timed out).",
                config.addr
            ));
        }
    };

    match tokio::time::timeout(RESPONSE_TIMEOUT, exchange(stream, config, request)).await {
        Ok(result) => result,
        Err(_) => Err(format!(
            "The InfinaBox app didn't answer within {}s.",
            RESPONSE_TIMEOUT.as_secs()
        )),
    }
}

async fn exchange(
    stream: TcpStream,
    config: &BridgeConfig,
    request: &BridgeRequest,
) -> Result<serde_json::Value, String> {
    let (read, mut write) = stream.into_split();
    let hello = Hello {
        hello: config.token.clone(),
        project: config.project.clone(),
    };
    let mut out = serde_json::to_string(&hello).map_err(|e| e.to_string())?;
    out.push('\n');
    out.push_str(&serde_json::to_string(request).map_err(|e| e.to_string())?);
    out.push('\n');

    // The app closes the connection on a bad token, so a write or read
    // failure here is most likely that (or the app quitting mid-request).
    let lost = |e: std::io::Error| {
        format!(
            "Lost the connection to the InfinaBox app ({e}). If this keeps happening, \
             restart the agent from InfinaBox so it gets a fresh bridge token."
        )
    };
    write.write_all(out.as_bytes()).await.map_err(lost)?;
    write.flush().await.map_err(lost)?;

    let mut line = String::new();
    let n = BufReader::new(read)
        .read_line(&mut line)
        .await
        .map_err(lost)?;
    if n == 0 {
        return Err(
            "The InfinaBox app closed the connection without answering (it rejects \
             connections with an outdated token; restart the agent from InfinaBox)."
                .to_string(),
        );
    }
    match serde_json::from_str::<BridgeResponse>(line.trim_end()) {
        Ok(BridgeResponse::Ok { data }) => Ok(data),
        Ok(BridgeResponse::Error { message }) => Err(message),
        Err(e) => Err(format!(
            "The InfinaBox app sent a reply this server doesn't understand: {e}"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    const TOKEN: &str = "test-token-0123456789";

    /// A minimal in-test app side of the protocol: checks the hello, then
    /// answers every request line by echoing what it received. Closes the
    /// connection on a bad token, as the real app must.
    async fn fake_app() -> (String, tokio::task::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        let handle = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (read, mut write) = stream.into_split();
            let mut lines = BufReader::new(read).lines();
            let mut seen = Vec::new();

            let first = lines.next_line().await.unwrap().unwrap();
            seen.push(first.clone());
            let hello: Hello = serde_json::from_str(&first).unwrap();
            if hello.hello != TOKEN {
                return seen; // drop = close
            }
            while let Ok(Some(line)) = lines.next_line().await {
                seen.push(line.clone());
                let req: BridgeRequest = serde_json::from_str(&line).unwrap();
                let resp = match req {
                    BridgeRequest::StopGame => BridgeResponse::Error {
                        message: "No game is running.".into(),
                    },
                    other => BridgeResponse::Ok {
                        data: serde_json::json!({ "echo": other, "project": hello.project }),
                    },
                };
                let mut s = serde_json::to_string(&resp).unwrap();
                s.push('\n');
                write.write_all(s.as_bytes()).await.unwrap();
            }
            seen
        });
        (addr, handle)
    }

    fn config(addr: &str, token: &str) -> BridgeConfig {
        BridgeConfig {
            addr: addr.into(),
            token: token.into(),
            project: "/games/my-game".into(),
        }
    }

    #[tokio::test]
    async fn sends_hello_then_request_and_returns_data() {
        let (addr, app) = fake_app().await;
        let data = call(
            Some(&config(&addr, TOKEN)),
            &BridgeRequest::RecentErrors { limit: 7 },
        )
        .await
        .unwrap();
        assert_eq!(
            data,
            serde_json::json!({
                "echo": { "method": "recent_errors", "params": { "limit": 7 } },
                "project": "/games/my-game"
            })
        );
        let seen = app.await.unwrap();
        assert_eq!(
            seen,
            vec![
                format!(r#"{{"hello":"{TOKEN}","project":"/games/my-game"}}"#),
                r#"{"method":"recent_errors","params":{"limit":7}}"#.to_string(),
            ]
        );
    }

    #[tokio::test]
    async fn app_error_response_becomes_error() {
        let (addr, _app) = fake_app().await;
        let err = call(Some(&config(&addr, TOKEN)), &BridgeRequest::StopGame)
            .await
            .unwrap_err();
        assert_eq!(err, "No game is running.");
    }

    #[tokio::test]
    async fn bad_token_is_an_error_not_a_success() {
        let (addr, app) = fake_app().await;
        let err = call(Some(&config(&addr, "wrong-token")), &BridgeRequest::RunGame)
            .await
            .unwrap_err();
        assert!(
            err.contains("closed the connection") || err.contains("Lost the connection"),
            "unexpected error: {err}"
        );
        // The app only ever saw the hello; the request was never served.
        assert_eq!(app.await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn missing_config_says_app_not_running() {
        let err = call(None, &BridgeRequest::GameStatus).await.unwrap_err();
        assert!(err.starts_with(APP_NOT_RUNNING), "{err}");
    }

    #[tokio::test]
    async fn nothing_listening_says_app_not_running() {
        // Bind then drop to get a loopback port with nothing on it.
        let addr = {
            let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
            l.local_addr().unwrap().to_string()
        };
        let err = call(Some(&config(&addr, TOKEN)), &BridgeRequest::GameStatus)
            .await
            .unwrap_err();
        assert!(err.starts_with(APP_NOT_RUNNING), "{err}");
    }
}
