//! The daemon's HTTP API, on 127.0.0.1 only. Every request needs
//! `Authorization: Bearer <token>`, the token from `daemon.json`.
//!
//! - `GET /status`: [`DaemonStatus`].
//! - `GET /tests`, `GET /tests/{id}`: [`TestInfo`]s.
//! - `POST /tests` `{"spec": TestSpec, "priority": 0}`: adds a test
//!   (programs as absolute paths).
//! - `GET /tests/{id}/games`: its [`GameEntry`]s; `GET
//!   /tests/{id}/games/{pair}/{game}`: a game's record, as text; `GET
//!   /tests/{id}/live/{pair}/{game}`: a running game so far, a
//!   [`LiveGame`] (later moves come as `movePlayed` events); `GET
//!   /tests/{id}/output/{pair}/{game}`: server-sent events, each a
//!   [`GameOutput`] (`engineOutput`), the game's engine output from now
//!   on.
//! - `POST /tests/{id}/archive`: zips a test that isn't active into
//!   `archive/` and takes it out, `{"archive": path}`; `DELETE
//!   /tests/{id}`: deletes one, `{"archive": null}`.
//! - `GET /archive`: the archived tests, [`ArchivedTest`]s, newest
//!   first; `POST /archive/{file}/restore`: puts one back in the queue
//!   ([`TestInfo`]); `DELETE /archive/{file}`: deletes one, `{}`.
//! - `POST /tests/{id}/pause`, `/resume`, `/stop`; `POST
//!   /tests/{id}/priority` `{"priority": n}`; `POST /tests/{id}/max-pairs`
//!   `{"maxPairs": n | null}`; `POST /tests/{id}/undecided-below`
//!   `{"undecidedBelow": chance | null}`; `POST /tests/{id}/concurrency`
//!   `{"concurrency": n}` (games at once).
//! - `POST /slots` `{"slots": n}`; `POST /shutdown` `{"now": bool}`;
//!   `POST /shutdown/cancel` calls off a shutdown that isn't `now`.
//! - `GET /events`: server-sent events, each a [`DaemonEvent`] named by
//!   its `type`.
//!
//! Errors are `{"error": "..."}` with 400 (refused), 404 (no such test)
//! or 500.

use std::convert::Infallible;
use std::path::Path;
use std::sync::Arc;

use axum::extract::{Path as UrlPath, Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::{get, post};
use axum::{Router, body::Bytes};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use tokio::net::TcpListener;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::{Stream, StreamExt};

use crate::daemon::{Daemon, DaemonEvent, DaemonInfo, GameOutput, INFO_FILE};
use crate::error::TourneyError;
use crate::spec::TestSpec;

#[cfg(doc)]
use crate::{
    daemon::ArchivedTest, daemon::DaemonStatus, daemon::TestInfo, results::GameEntry, runner::LiveGame,
};

#[derive(Clone)]
struct AppState {
    daemon: Daemon,
    token: Arc<str>,
}

struct ApiError(TourneyError);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match &self.0 {
            TourneyError::NoTest(_) => StatusCode::NOT_FOUND,
            TourneyError::Refused(_) | TourneyError::Spec(_) => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, Json(serde_json::json!({ "error": self.0.to_string() }))).into_response()
    }
}

impl From<TourneyError> for ApiError {
    fn from(e: TourneyError) -> ApiError {
        ApiError(e)
    }
}

type ApiResult<T> = Result<Json<T>, ApiError>;

/// Binds a port on 127.0.0.1 (0 picks one) and writes `daemon.json` with
/// it and a new token.
pub async fn bind(data_dir: &Path, port: u16) -> Result<(TcpListener, DaemonInfo), TourneyError> {
    let listener = TcpListener::bind(("127.0.0.1", port))
        .await
        .map_err(TourneyError::io(format!("127.0.0.1:{port}")))?;
    let port = listener.local_addr().map_err(TourneyError::io("127.0.0.1"))?.port();
    let mut bytes = [0u8; 24];
    getrandom::fill(&mut bytes).map_err(|e| TourneyError::Refused(format!("no random token: {e}")))?;
    let info = DaemonInfo {
        port,
        token: bytes.iter().map(|b| format!("{b:02x}")).collect(),
        pid: std::process::id(),
        version: env!("CARGO_PKG_VERSION").into(),
    };
    write_private(&data_dir.join(INFO_FILE), &serde_json::to_string_pretty(&info).expect("info serializes"))?;
    Ok((listener, info))
}

/// Writes a file only the user can read.
fn write_private(path: &Path, text: &str) -> Result<(), TourneyError> {
    use std::io::Write;
    let tmp = path.with_extension("json.tmp");
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(&tmp).map_err(TourneyError::io(&tmp))?;
    file.write_all(text.as_bytes()).map_err(TourneyError::io(&tmp))?;
    drop(file);
    std::fs::rename(&tmp, path).map_err(TourneyError::io(path))
}

/// Serves the daemon until it has shut down, then removes `daemon.json`
/// (if it's still ours).
pub async fn serve(daemon: Daemon, listener: TcpListener, info: DaemonInfo) -> Result<(), TourneyError> {
    let state = AppState { daemon: daemon.clone(), token: info.token.as_str().into() };
    let app = Router::new()
        .route("/status", get(status))
        .route("/tests", get(list).post(submit))
        .route("/tests/{id}", get(test_info).delete(delete))
        .route("/tests/{id}/archive", post(archive))
        .route("/tests/{id}/games", get(games))
        .route("/tests/{id}/games/{pair}/{game}", get(record))
        .route("/tests/{id}/live/{pair}/{game}", get(live))
        .route("/tests/{id}/output/{pair}/{game}", get(output))
        .route("/tests/{id}/pause", post(pause))
        .route("/tests/{id}/resume", post(resume))
        .route("/tests/{id}/stop", post(stop))
        .route("/tests/{id}/priority", post(priority))
        .route("/tests/{id}/max-pairs", post(max_pairs))
        .route("/tests/{id}/undecided-below", post(undecided_below))
        .route("/tests/{id}/concurrency", post(concurrency))
        .route("/archive", get(archived))
        .route("/archive/{file}", axum::routing::delete(delete_archived))
        .route("/archive/{file}/restore", post(restore))
        .route("/slots", post(slots))
        .route("/shutdown", post(shutdown))
        .route("/shutdown/cancel", post(cancel_shutdown))
        .route("/events", get(events))
        .layer(middleware::from_fn_with_state(state.clone(), authorize))
        .with_state(state);
    // Event streams never end on their own, so the server is dropped
    // (closing them) once the daemon is done rather than drained.
    let result = tokio::select! {
        r = axum::serve(listener, app) => r.map_err(TourneyError::io("127.0.0.1")),
        () = daemon.closed() => Ok(()),
    };
    if DaemonInfo::read(daemon.data_dir()).is_some_and(|i| i.token == info.token) {
        let _ = std::fs::remove_file(daemon.data_dir().join(INFO_FILE));
    }
    result
}

async fn authorize(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let expected = format!("Bearer {}", state.token);
    let given = request.headers().get(header::AUTHORIZATION).and_then(|v| v.to_str().ok());
    if given != Some(expected.as_str()) {
        return (StatusCode::UNAUTHORIZED, Json(serde_json::json!({ "error": "wrong or missing token" })))
            .into_response();
    }
    next.run(request).await
}

/// Reads a JSON body, treating an empty one as `{}`.
fn body<T: DeserializeOwned>(bytes: &Bytes) -> Result<T, ApiError> {
    let text = if bytes.is_empty() { &b"{}"[..] } else { &bytes[..] };
    serde_json::from_slice(text)
        .map_err(|e| ApiError(TourneyError::Refused(format!("bad request body: {e}"))))
}

async fn status(State(s): State<AppState>) -> Json<crate::daemon::DaemonStatus> {
    Json(s.daemon.status())
}

async fn list(State(s): State<AppState>) -> Json<Vec<crate::daemon::TestInfo>> {
    Json(s.daemon.list())
}

#[derive(Deserialize)]
struct Submit {
    spec: TestSpec,
    #[serde(default)]
    priority: i32,
}

async fn submit(State(s): State<AppState>, bytes: Bytes) -> ApiResult<crate::daemon::TestInfo> {
    let req: Submit = body(&bytes)?;
    req.spec.validate()?;
    // Copying the engines is file work.
    let daemon = s.daemon.clone();
    let info = tokio::task::spawn_blocking(move || daemon.submit(req.spec, req.priority))
        .await
        .expect("submit doesn't panic")?;
    Ok(Json(info))
}

async fn test_info(
    State(s): State<AppState>,
    UrlPath(id): UrlPath<String>,
) -> ApiResult<crate::daemon::TestInfo> {
    Ok(Json(s.daemon.info(&id)?))
}

async fn games(
    State(s): State<AppState>,
    UrlPath(id): UrlPath<String>,
) -> ApiResult<Vec<crate::results::GameEntry>> {
    Ok(Json(s.daemon.games(&id)?))
}

async fn record(
    State(s): State<AppState>,
    UrlPath((id, pair, game)): UrlPath<(String, u32, u8)>,
) -> Result<String, ApiError> {
    Ok(s.daemon.record(&id, pair, game)?)
}

async fn live(
    State(s): State<AppState>,
    UrlPath((id, pair, game)): UrlPath<(String, u32, u8)>,
) -> ApiResult<crate::runner::LiveGame> {
    Ok(Json(s.daemon.live(&id, pair, game)?))
}

async fn output(
    State(s): State<AppState>,
    UrlPath((id, pair, game)): UrlPath<(String, u32, u8)>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = BroadcastStream::new(s.daemon.subscribe_output()).filter_map(move |o| {
        // As with `/events`, a client too slow to keep up misses lines.
        let o: GameOutput = o.ok()?;
        (o.test == id && o.pair == pair && o.game == game)
            .then(|| Ok(Event::default().event("engineOutput").json_data(&o).expect("output serializes")))
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

async fn archive(State(s): State<AppState>, UrlPath(id): UrlPath<String>) -> ApiResult<serde_json::Value> {
    let zip = s.daemon.remove(&id, true).await?;
    Ok(Json(serde_json::json!({ "archive": zip })))
}

async fn delete(State(s): State<AppState>, UrlPath(id): UrlPath<String>) -> ApiResult<serde_json::Value> {
    s.daemon.remove(&id, false).await?;
    Ok(Json(serde_json::json!({ "archive": null })))
}

async fn archived(State(s): State<AppState>) -> Json<Vec<crate::daemon::ArchivedTest>> {
    // Reading the zips is file work.
    let daemon = s.daemon.clone();
    Json(tokio::task::spawn_blocking(move || daemon.archived()).await.expect("listing doesn't panic"))
}

async fn restore(
    State(s): State<AppState>,
    UrlPath(file): UrlPath<String>,
) -> ApiResult<crate::daemon::TestInfo> {
    let daemon = s.daemon.clone();
    let info =
        tokio::task::spawn_blocking(move || daemon.restore(&file)).await.expect("restoring doesn't panic")?;
    Ok(Json(info))
}

async fn delete_archived(
    State(s): State<AppState>,
    UrlPath(file): UrlPath<String>,
) -> ApiResult<serde_json::Value> {
    s.daemon.delete_archived(&file)?;
    Ok(Json(serde_json::json!({})))
}

async fn pause(
    State(s): State<AppState>,
    UrlPath(id): UrlPath<String>,
) -> ApiResult<crate::daemon::TestInfo> {
    Ok(Json(s.daemon.pause(&id)?))
}

async fn resume(
    State(s): State<AppState>,
    UrlPath(id): UrlPath<String>,
) -> ApiResult<crate::daemon::TestInfo> {
    Ok(Json(s.daemon.resume(&id)?))
}

async fn stop(State(s): State<AppState>, UrlPath(id): UrlPath<String>) -> ApiResult<crate::daemon::TestInfo> {
    Ok(Json(s.daemon.stop(&id)?))
}

#[derive(Deserialize)]
struct Priority {
    priority: i32,
}

async fn priority(
    State(s): State<AppState>,
    UrlPath(id): UrlPath<String>,
    bytes: Bytes,
) -> ApiResult<crate::daemon::TestInfo> {
    let req: Priority = body(&bytes)?;
    Ok(Json(s.daemon.set_priority(&id, req.priority)?))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Concurrency {
    concurrency: u32,
}

async fn concurrency(
    State(s): State<AppState>,
    UrlPath(id): UrlPath<String>,
    bytes: Bytes,
) -> ApiResult<crate::daemon::TestInfo> {
    let req: Concurrency = body(&bytes)?;
    Ok(Json(s.daemon.set_concurrency(&id, req.concurrency)?))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MaxPairs {
    max_pairs: Option<u32>,
}

async fn max_pairs(
    State(s): State<AppState>,
    UrlPath(id): UrlPath<String>,
    bytes: Bytes,
) -> ApiResult<crate::daemon::TestInfo> {
    let req: MaxPairs = body(&bytes)?;
    Ok(Json(s.daemon.set_max_pairs(&id, req.max_pairs)?))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UndecidedBelow {
    undecided_below: Option<f64>,
}

async fn undecided_below(
    State(s): State<AppState>,
    UrlPath(id): UrlPath<String>,
    bytes: Bytes,
) -> ApiResult<crate::daemon::TestInfo> {
    let req: UndecidedBelow = body(&bytes)?;
    Ok(Json(s.daemon.set_undecided_below(&id, req.undecided_below)?))
}

#[derive(Deserialize)]
struct Slots {
    slots: u32,
}

async fn slots(State(s): State<AppState>, bytes: Bytes) -> ApiResult<crate::daemon::DaemonStatus> {
    let req: Slots = body(&bytes)?;
    s.daemon.set_slots(req.slots);
    Ok(Json(s.daemon.status()))
}

#[derive(Deserialize)]
struct Shutdown {
    #[serde(default)]
    now: bool,
}

async fn shutdown(State(s): State<AppState>, bytes: Bytes) -> ApiResult<crate::daemon::DaemonStatus> {
    let req: Shutdown = body(&bytes)?;
    s.daemon.shutdown(req.now);
    Ok(Json(s.daemon.status()))
}

async fn cancel_shutdown(State(s): State<AppState>) -> ApiResult<crate::daemon::DaemonStatus> {
    s.daemon.cancel_shutdown()?;
    Ok(Json(s.daemon.status()))
}

async fn events(State(s): State<AppState>) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = BroadcastStream::new(s.daemon.subscribe()).filter_map(|e: Result<DaemonEvent, _>| {
        // A client too slow to keep up misses events; it can ask again.
        let e = e.ok()?;
        Some(Ok(Event::default().event(e.name()).json_data(&e).expect("events serialize")))
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}
