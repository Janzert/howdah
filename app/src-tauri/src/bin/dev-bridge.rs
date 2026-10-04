//! Development bridge: the app's backend (session, engine controller,
//! engine list) served over HTTP, so the frontend can run in a plain browser
//! from Vite. In dev builds outside Tauri, `app/src/lib/devBridge.ts`
//! forwards every `invoke` here and replays the event stream as Tauri
//! events. Vite proxies `/bridge` to this server.
//!
//! - `POST /invoke/<command>` with the JSON arguments (or no body) returns
//!   the command's JSON result, or an `ApiError` with status 400.
//! - `GET /events` is a server-sent event stream; each event's name is the
//!   Tauri event name (`game://changed`, `engine://output`, `analysis://update`).
//!
//! Run: `cargo run -p howdah --features dev-bridge --bin dev-bridge`
//! with optional `--port <n>` (default 1421) and `--config-dir <dir>`
//! (default: `dev-bridge-config` next to the binary, so the app's own
//! engine list is untouched). It listens on localhost only.

use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Json};
use axum::routing::{get, post};
use howdah_lib::backend::{Backend, EventSink};
use howdah_lib::engines::EngineRegistry;
use serde_json::Value;
use tokio::sync::broadcast;
use tokio_stream::{Stream, StreamExt, wrappers::BroadcastStream};

struct Broadcast(broadcast::Sender<(String, Value)>);

impl EventSink for Broadcast {
    fn emit(&self, event: &str, payload: Value) {
        // No subscribers is fine: the browser may not be open yet.
        let _ = self.0.send((event.to_string(), payload));
    }
}

#[derive(Clone)]
struct App {
    backend: Arc<Backend>,
    events: broadcast::Sender<(String, Value)>,
}

#[tokio::main]
async fn main() {
    let mut port: u16 = 1421;
    let mut config_dir: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--port" => port = args.next().and_then(|p| p.parse().ok()).expect("--port needs a number"),
            "--config-dir" => config_dir = Some(args.next().expect("--config-dir needs a path").into()),
            _ => {
                eprintln!("usage: dev-bridge [--port <n>] [--config-dir <dir>]");
                std::process::exit(2);
            }
        }
    }
    let config_dir = config_dir.unwrap_or_else(|| {
        let exe = std::env::current_exe().expect("current exe");
        exe.parent().expect("exe dir").join("dev-bridge-config")
    });

    // The controller spawns its tasks through tauri::async_runtime.
    tauri::async_runtime::set(tokio::runtime::Handle::current());
    let (tx, _) = broadcast::channel(1024);
    let registry = EngineRegistry::load(config_dir.join("engines.json"));
    let backend = Arc::new(Backend::new(registry, Arc::new(Broadcast(tx.clone()))));
    let app = App { backend, events: tx };

    let router =
        Router::new().route("/invoke/{cmd}", post(invoke)).route("/events", get(events)).with_state(app);
    let addr = ("127.0.0.1", port);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap_or_else(|e| {
        eprintln!("dev-bridge: can't listen on 127.0.0.1:{port}: {e}");
        std::process::exit(1);
    });
    eprintln!("dev-bridge: listening on http://127.0.0.1:{port} (engines: {})", config_dir.display());
    axum::serve(listener, router).await.expect("server");
}

async fn invoke(State(app): State<App>, Path(cmd): Path<String>, body: Bytes) -> impl IntoResponse {
    let args = if body.is_empty() { Ok(Value::Null) } else { serde_json::from_slice(&body) };
    let args = match args {
        Ok(a) => a,
        Err(e) => return (StatusCode::BAD_REQUEST, format!("bad JSON: {e}")).into_response(),
    };
    match app.backend.dispatch(&cmd, &args).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(e)).into_response(),
    }
}

async fn events(State(app): State<App>) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = BroadcastStream::new(app.events.subscribe()).filter_map(|msg| {
        // A lagging client misses events; the next `game://changed` carries
        // the full view, so it recovers.
        let (name, payload) = msg.ok()?;
        Some(Ok(Event::default().event(name).json_data(payload).expect("JSON payload")))
    });
    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}
