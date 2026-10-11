//! A client for the daemon's HTTP API (see [`crate::server`]).

use std::path::{Path, PathBuf};

use reqwest::header;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::json;

use crate::daemon::{DaemonEvent, DaemonInfo, DaemonStatus, INFO_FILE, TestInfo};
use crate::results::GameEntry;
use crate::spec::TestSpec;

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("no daemon is running for {0} (start one with `howdah-tourney daemon`)")]
    NotRunning(PathBuf),
    #[error("the daemon didn't answer: {0}")]
    Http(#[from] reqwest::Error),
    /// The daemon refused the request; its message.
    #[error("{0}")]
    Daemon(String),
    #[error("unexpected reply from the daemon: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Clone)]
pub struct Client {
    base: String,
    auth: String,
    http: reqwest::Client,
}

impl Client {
    pub fn new(info: &DaemonInfo) -> Client {
        Client {
            base: format!("http://127.0.0.1:{}", info.port),
            auth: format!("Bearer {}", info.token),
            // Never through a proxy: the daemon is on this machine.
            http: reqwest::Client::builder().no_proxy().build().expect("a plain HTTP client builds"),
        }
    }

    /// A client for the daemon serving `data_dir`, from its `daemon.json`.
    /// It may have died since; the first request says.
    pub fn connect(data_dir: &Path) -> Result<Client, ClientError> {
        DaemonInfo::read(data_dir)
            .map(|i| Client::new(&i))
            .ok_or_else(|| ClientError::NotRunning(data_dir.join(INFO_FILE)))
    }

    async fn send(&self, request: reqwest::RequestBuilder) -> Result<reqwest::Response, ClientError> {
        let response = request.header(header::AUTHORIZATION, &self.auth).send().await?;
        if response.status().is_success() {
            return Ok(response);
        }
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        let message = serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|v| v.get("error")?.as_str().map(str::to_string))
            .unwrap_or_else(|| format!("{status}: {text}"));
        Err(ClientError::Daemon(message))
    }

    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, ClientError> {
        let response = self.send(self.http.get(format!("{}{path}", self.base))).await?;
        Ok(serde_json::from_slice(&response.bytes().await?)?)
    }

    async fn post<T: DeserializeOwned>(&self, path: &str, body: &impl Serialize) -> Result<T, ClientError> {
        let request = self
            .http
            .post(format!("{}{path}", self.base))
            .header(header::CONTENT_TYPE, "application/json")
            .body(serde_json::to_vec(body)?);
        let response = self.send(request).await?;
        Ok(serde_json::from_slice(&response.bytes().await?)?)
    }

    pub async fn status(&self) -> Result<DaemonStatus, ClientError> {
        self.get("/status").await
    }

    pub async fn list(&self) -> Result<Vec<TestInfo>, ClientError> {
        self.get("/tests").await
    }

    pub async fn info(&self, id: &str) -> Result<TestInfo, ClientError> {
        self.get(&format!("/tests/{id}")).await
    }

    pub async fn games(&self, id: &str) -> Result<Vec<GameEntry>, ClientError> {
        self.get(&format!("/tests/{id}/games")).await
    }

    pub async fn record(&self, id: &str, pair: u32, game: u8) -> Result<String, ClientError> {
        let response =
            self.send(self.http.get(format!("{}/tests/{id}/games/{pair}/{game}", self.base))).await?;
        Ok(response.text().await?)
    }

    /// Adds a test. Its engines' programs must be absolute paths.
    pub async fn submit(&self, spec: &TestSpec, priority: i32) -> Result<TestInfo, ClientError> {
        self.post("/tests", &json!({ "spec": spec, "priority": priority })).await
    }

    pub async fn pause(&self, id: &str) -> Result<TestInfo, ClientError> {
        self.post(&format!("/tests/{id}/pause"), &json!({})).await
    }

    pub async fn resume(&self, id: &str) -> Result<TestInfo, ClientError> {
        self.post(&format!("/tests/{id}/resume"), &json!({})).await
    }

    pub async fn stop(&self, id: &str) -> Result<TestInfo, ClientError> {
        self.post(&format!("/tests/{id}/stop"), &json!({})).await
    }

    pub async fn set_priority(&self, id: &str, priority: i32) -> Result<TestInfo, ClientError> {
        self.post(&format!("/tests/{id}/priority"), &json!({ "priority": priority })).await
    }

    pub async fn set_max_pairs(&self, id: &str, max_pairs: Option<u32>) -> Result<TestInfo, ClientError> {
        self.post(&format!("/tests/{id}/max-pairs"), &json!({ "maxPairs": max_pairs })).await
    }

    pub async fn set_slots(&self, slots: u32) -> Result<DaemonStatus, ClientError> {
        self.post("/slots", &json!({ "slots": slots })).await
    }

    pub async fn shutdown(&self, now: bool) -> Result<DaemonStatus, ClientError> {
        self.post("/shutdown", &json!({ "now": now })).await
    }

    /// The daemon's events from now on.
    pub async fn events(&self) -> Result<EventStream, ClientError> {
        let response = self.send(self.http.get(format!("{}/events", self.base))).await?;
        Ok(EventStream { response, buf: Vec::new() })
    }
}

/// Server-sent events from the daemon.
pub struct EventStream {
    response: reqwest::Response,
    buf: Vec<u8>,
}

impl EventStream {
    /// The next event; None when the daemon closes the stream.
    pub async fn next(&mut self) -> Option<Result<DaemonEvent, ClientError>> {
        loop {
            if let Some(end) = self.buf.windows(2).position(|w| w == b"\n\n") {
                let block: Vec<u8> = self.buf.drain(..end + 2).collect();
                let text = String::from_utf8_lossy(&block);
                let data: Vec<&str> =
                    text.lines().filter_map(|l| l.strip_prefix("data:")).map(str::trim_start).collect();
                if data.is_empty() {
                    continue; // a keep-alive comment
                }
                return Some(serde_json::from_str(&data.join("\n")).map_err(ClientError::from));
            }
            match self.response.chunk().await {
                Ok(Some(bytes)) => self.buf.extend_from_slice(&bytes),
                Ok(None) => return None,
                Err(e) => return Some(Err(e.into())),
            }
        }
    }
}
