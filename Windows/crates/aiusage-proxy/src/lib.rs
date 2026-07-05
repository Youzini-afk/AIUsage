use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use aiusage_core::ProxyTrack;
use axum::{
    body::{to_bytes, Body},
    extract::State,
    http::{header, HeaderMap, HeaderName, Method, Request, Response, StatusCode},
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use futures_util::TryStreamExt;
use serde::{Deserialize, Serialize};
use serde_json::json;
use thiserror::Error;
use tokio::{
    net::TcpListener,
    sync::{oneshot, Mutex},
    task::JoinHandle,
};

const MAX_PROXY_BODY_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProxyRuntimeState {
    Stopped,
    Starting,
    Running,
    Stopping,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyHealth {
    pub track: ProxyTrack,
    pub state: ProxyRuntimeState,
    pub listening_port: Option<u16>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProxyProtocol {
    OpenAiResponses,
    OpenAiChatCompletions,
    AnthropicMessages,
    Passthrough,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyRuntimeConfig {
    pub track: ProxyTrack,
    pub node_id: String,
    pub label: String,
    pub bind_host: String,
    pub port: u16,
    pub upstream_base_url: String,
    pub upstream_api_key: Option<String>,
    pub client_key: Option<String>,
    pub protocol: ProxyProtocol,
    pub default_model: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyRequestLog {
    pub track: ProxyTrack,
    pub node_id: String,
    pub protocol: ProxyProtocol,
    pub method: String,
    pub path: String,
    pub status: u16,
    pub started_at_epoch_ms: u128,
}

#[derive(Debug, Error)]
pub enum ProxyError {
    #[error("invalid bind host: {0}")]
    InvalidBindHost(String),
    #[error("invalid upstream URL: {0}")]
    InvalidUpstreamUrl(String),
    #[error("proxy IO failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("proxy upstream request failed: {0}")]
    Upstream(#[from] reqwest::Error),
    #[error("proxy task failed: {0}")]
    Join(#[from] tokio::task::JoinError),
}

pub type ProxyResult<T> = Result<T, ProxyError>;

#[derive(Clone, Debug)]
struct ProxyRuntimeContext {
    config: ProxyRuntimeConfig,
    client: reqwest::Client,
    local_addr: SocketAddr,
}

#[derive(Debug)]
struct RunningProxy {
    config: ProxyRuntimeConfig,
    local_addr: SocketAddr,
    shutdown: Option<oneshot::Sender<()>>,
    task: JoinHandle<()>,
}

#[derive(Clone, Debug, Default)]
pub struct ProxySupervisor {
    running: Arc<Mutex<HashMap<ProxyTrack, RunningProxy>>>,
}

impl ProxySupervisor {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn start(&self, config: ProxyRuntimeConfig) -> ProxyResult<ProxyHealth> {
        self.stop(config.track.clone()).await?;

        let bind_ip = config
            .bind_host
            .parse::<IpAddr>()
            .map_err(|_| ProxyError::InvalidBindHost(config.bind_host.clone()))?;
        let listener = TcpListener::bind(SocketAddr::new(bind_ip, config.port)).await?;
        let local_addr = listener.local_addr()?;
        let context = Arc::new(ProxyRuntimeContext {
            config: config.clone(),
            client: reqwest::Client::new(),
            local_addr,
        });
        let router = proxy_router(context);
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        let task = tokio::spawn(async move {
            let server = axum::serve(listener, router).with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            });
            if let Err(error) = server.await {
                eprintln!("AIUsage proxy server stopped with error: {error}");
            }
        });

        let health = ProxyHealth {
            track: config.track.clone(),
            state: ProxyRuntimeState::Running,
            listening_port: Some(local_addr.port()),
        };

        self.running.lock().await.insert(
            config.track.clone(),
            RunningProxy {
                config,
                local_addr,
                shutdown: Some(shutdown_tx),
                task,
            },
        );

        Ok(health)
    }

    pub async fn stop(&self, track: ProxyTrack) -> ProxyResult<ProxyHealth> {
        let Some(mut running) = self.running.lock().await.remove(&track) else {
            return Ok(ProxyHealth {
                track,
                state: ProxyRuntimeState::Stopped,
                listening_port: None,
            });
        };

        if let Some(shutdown) = running.shutdown.take() {
            let _ = shutdown.send(());
        }
        running.task.await?;

        Ok(ProxyHealth {
            track,
            state: ProxyRuntimeState::Stopped,
            listening_port: None,
        })
    }

    pub async fn health(&self, track: ProxyTrack) -> ProxyHealth {
        self.running
            .lock()
            .await
            .get(&track)
            .map(|running| ProxyHealth {
                track: running.config.track.clone(),
                state: ProxyRuntimeState::Running,
                listening_port: Some(running.local_addr.port()),
            })
            .unwrap_or(ProxyHealth {
                track,
                state: ProxyRuntimeState::Stopped,
                listening_port: None,
            })
    }

    pub async fn all_health(&self) -> Vec<ProxyHealth> {
        let mut result = Vec::new();
        for track in all_proxy_tracks() {
            result.push(self.health(track).await);
        }
        result
    }
}

pub fn foundation_proxy_health() -> Vec<ProxyHealth> {
    all_proxy_tracks()
        .into_iter()
        .map(|track| ProxyHealth {
            track,
            state: ProxyRuntimeState::Stopped,
            listening_port: None,
        })
        .collect()
}

pub fn all_proxy_tracks() -> Vec<ProxyTrack> {
    vec![
        ProxyTrack::ClaudeCode,
        ProxyTrack::Codex,
        ProxyTrack::OpenCode,
        ProxyTrack::Global,
    ]
}

pub fn format_sse_event(event: Option<&str>, data: &str) -> String {
    let mut output = String::new();
    if let Some(event) = event.filter(|event| !event.trim().is_empty()) {
        output.push_str("event: ");
        output.push_str(event);
        output.push('\n');
    }

    if data.is_empty() {
        output.push_str("data:\n");
    } else {
        for line in data.lines() {
            output.push_str("data: ");
            output.push_str(line);
            output.push('\n');
        }
    }
    output.push('\n');
    output
}

pub fn build_upstream_url(base_url: &str, incoming_path_and_query: &str) -> ProxyResult<String> {
    let base = base_url.trim().trim_end_matches('/');
    if base.is_empty() {
        return Err(ProxyError::InvalidUpstreamUrl(base_url.into()));
    }

    let (path, query) = incoming_path_and_query
        .split_once('?')
        .map(|(path, query)| (path, Some(query)))
        .unwrap_or((incoming_path_and_query, None));
    let suffix = if base.ends_with("/v1") && path == "/v1" {
        ""
    } else if base.ends_with("/v1") && path.starts_with("/v1/") {
        &path[3..]
    } else {
        path
    };

    let mut url = format!("{base}{suffix}");
    if let Some(query) = query.filter(|query| !query.is_empty()) {
        url.push('?');
        url.push_str(query);
    }
    Ok(url)
}

fn proxy_router(context: Arc<ProxyRuntimeContext>) -> Router {
    Router::new()
        .route("/health", get(health_handler))
        .fallback(proxy_handler)
        .with_state(context)
}

async fn health_handler(State(context): State<Arc<ProxyRuntimeContext>>) -> Json<ProxyHealth> {
    Json(ProxyHealth {
        track: context.config.track.clone(),
        state: ProxyRuntimeState::Running,
        listening_port: Some(context.local_addr.port()),
    })
}

async fn proxy_handler(
    State(context): State<Arc<ProxyRuntimeContext>>,
    request: Request<Body>,
) -> Response<Body> {
    let started_at_epoch_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    let (parts, body) = request.into_parts();

    if !request_has_client_key(&parts.headers, context.config.client_key.as_deref()) {
        return json_error(
            StatusCode::UNAUTHORIZED,
            "invalid_client_key",
            "invalid client key",
        );
    }

    let body = match to_bytes(body, MAX_PROXY_BODY_BYTES).await {
        Ok(body) => body,
        Err(error) => {
            return json_error(
                StatusCode::BAD_REQUEST,
                "invalid_request_body",
                &error.to_string(),
            )
        }
    };
    let path_and_query = parts
        .uri
        .path_and_query()
        .map(|path| path.as_str())
        .unwrap_or("/");
    let upstream_url = match build_upstream_url(&context.config.upstream_base_url, path_and_query) {
        Ok(url) => url,
        Err(error) => {
            return json_error(
                StatusCode::BAD_GATEWAY,
                "invalid_upstream_url",
                &error.to_string(),
            )
        }
    };

    let method = parts.method.clone();
    let upstream_response =
        match send_upstream_request(&context, method.clone(), &parts.headers, upstream_url, body)
            .await
        {
            Ok(response) => response,
            Err(error) => {
                return json_error(
                    StatusCode::BAD_GATEWAY,
                    "upstream_error",
                    &error.to_string(),
                )
            }
        };

    let status = upstream_response.status();
    let _log = ProxyRequestLog {
        track: context.config.track.clone(),
        node_id: context.config.node_id.clone(),
        protocol: context.config.protocol.clone(),
        method: method.to_string(),
        path: path_and_query.to_string(),
        status: status.as_u16(),
        started_at_epoch_ms,
    };

    stream_upstream_response(upstream_response)
}

async fn send_upstream_request(
    context: &ProxyRuntimeContext,
    method: Method,
    headers: &HeaderMap,
    upstream_url: String,
    body: bytes::Bytes,
) -> reqwest::Result<reqwest::Response> {
    let mut request = context.client.request(method, upstream_url).body(body);
    for (name, value) in headers {
        if should_forward_header(name) && should_forward_auth_header(name, &context.config) {
            request = request.header(name, value);
        }
    }
    request = apply_upstream_auth(request, &context.config, headers);
    request.send().await
}

fn stream_upstream_response(upstream_response: reqwest::Response) -> Response<Body> {
    let status = upstream_response.status();
    let mut builder = Response::builder().status(status);
    for (name, value) in upstream_response.headers() {
        if should_forward_response_header(name) {
            builder = builder.header(name, value);
        }
    }
    let stream = upstream_response
        .bytes_stream()
        .map_err(std::io::Error::other);
    builder.body(Body::from_stream(stream)).unwrap_or_else(|_| {
        json_error(
            StatusCode::BAD_GATEWAY,
            "response_build_failed",
            "failed to build proxy response",
        )
    })
}

fn apply_upstream_auth(
    request: reqwest::RequestBuilder,
    config: &ProxyRuntimeConfig,
    incoming_headers: &HeaderMap,
) -> reqwest::RequestBuilder {
    let Some(api_key) = config
        .upstream_api_key
        .as_deref()
        .map(str::trim)
        .filter(|api_key| !api_key.is_empty())
    else {
        return request;
    };

    match config.protocol {
        ProxyProtocol::AnthropicMessages => {
            let request = request.header("x-api-key", api_key);
            if incoming_headers.get("anthropic-version").is_some() {
                request
            } else {
                request.header("anthropic-version", "2023-06-01")
            }
        }
        ProxyProtocol::OpenAiResponses
        | ProxyProtocol::OpenAiChatCompletions
        | ProxyProtocol::Passthrough => request.bearer_auth(api_key),
    }
}

fn request_has_client_key(headers: &HeaderMap, expected: Option<&str>) -> bool {
    let Some(expected) = expected
        .map(str::trim)
        .filter(|expected| !expected.is_empty())
    else {
        return true;
    };

    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(|value| value == expected)
        .unwrap_or(false)
        || headers
            .get("x-api-key")
            .and_then(|value| value.to_str().ok())
            .map(|value| value == expected)
            .unwrap_or(false)
}

fn should_forward_header(name: &HeaderName) -> bool {
    !matches!(
        name.as_str(),
        "host" | "connection" | "content-length" | "transfer-encoding"
    )
}

fn should_forward_auth_header(name: &HeaderName, config: &ProxyRuntimeConfig) -> bool {
    let has_upstream_key = config
        .upstream_api_key
        .as_deref()
        .map(str::trim)
        .is_some_and(|api_key| !api_key.is_empty());
    if !has_upstream_key {
        return true;
    }

    !matches!(name.as_str(), "authorization" | "x-api-key")
}

fn should_forward_response_header(name: &HeaderName) -> bool {
    !matches!(
        name.as_str(),
        "connection" | "content-length" | "transfer-encoding"
    )
}

fn json_error(status: StatusCode, code: &str, message: &str) -> Response<Body> {
    (
        status,
        Json(json!({
            "error": {
                "code": code,
                "message": message,
            }
        })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    use axum::{routing::post, Json};
    use serde_json::Value;

    #[test]
    fn foundation_health_tracks_all_proxy_families() {
        assert_eq!(foundation_proxy_health().len(), 4);
    }

    #[test]
    fn formats_multiline_sse_event() {
        assert_eq!(
            format_sse_event(Some("message"), "{\"a\":1}\n{\"b\":2}"),
            "event: message\ndata: {\"a\":1}\ndata: {\"b\":2}\n\n"
        );
    }

    #[test]
    fn normalizes_upstream_v1_paths() {
        assert_eq!(
            build_upstream_url("https://api.example.com/v1", "/v1/responses?stream=true")
                .expect("url"),
            "https://api.example.com/v1/responses?stream=true"
        );
        assert_eq!(
            build_upstream_url("https://api.example.com", "/v1/messages").expect("url"),
            "https://api.example.com/v1/messages"
        );
    }

    #[tokio::test]
    async fn proxy_rejects_invalid_client_key() {
        let supervisor = ProxySupervisor::new();
        let health = supervisor
            .start(test_config("http://127.0.0.1:9/v1"))
            .await
            .expect("proxy should start");
        let port = health.listening_port.expect("port");
        let response = reqwest::Client::new()
            .post(format!("http://127.0.0.1:{port}/v1/responses"))
            .bearer_auth("wrong")
            .send()
            .await
            .expect("proxy should respond");

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        supervisor
            .stop(ProxyTrack::Codex)
            .await
            .expect("proxy should stop");
    }

    #[tokio::test]
    async fn proxy_forwards_request_to_upstream_with_auth() {
        let mut upstream = TestUpstream::start().await;
        let supervisor = ProxySupervisor::new();
        let mut config = test_config(&format!("http://{}/v1", upstream.addr));
        config.upstream_api_key = Some("upstream-key".into());
        let health = supervisor.start(config).await.expect("proxy should start");
        let port = health.listening_port.expect("port");

        let response: Value = reqwest::Client::new()
            .post(format!("http://127.0.0.1:{port}/v1/responses"))
            .bearer_auth("client-key")
            .json(&json!({"model":"gpt-5"}))
            .send()
            .await
            .expect("proxy request")
            .json()
            .await
            .expect("proxy json response");

        assert_eq!(response["ok"], true);
        let captured = (&mut upstream.captured).await.expect("upstream capture");
        assert_eq!(captured.path, "/v1/responses");
        assert_eq!(
            captured.authorization.as_deref(),
            Some("Bearer upstream-key")
        );
        assert!(captured.body.contains("gpt-5"));

        supervisor
            .stop(ProxyTrack::Codex)
            .await
            .expect("proxy should stop");
        upstream.shutdown();
    }

    fn test_config(upstream_base_url: &str) -> ProxyRuntimeConfig {
        ProxyRuntimeConfig {
            track: ProxyTrack::Codex,
            node_id: "node-1".into(),
            label: "Node 1".into(),
            bind_host: Ipv4Addr::LOCALHOST.to_string(),
            port: 0,
            upstream_base_url: upstream_base_url.into(),
            upstream_api_key: None,
            client_key: Some("client-key".into()),
            protocol: ProxyProtocol::OpenAiResponses,
            default_model: Some("gpt-5".into()),
        }
    }

    #[derive(Debug)]
    struct CapturedRequest {
        path: String,
        authorization: Option<String>,
        body: String,
    }

    struct TestUpstream {
        addr: SocketAddr,
        captured: oneshot::Receiver<CapturedRequest>,
        shutdown: Option<oneshot::Sender<()>>,
    }

    impl TestUpstream {
        async fn start() -> Self {
            let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
                .await
                .expect("upstream bind");
            let addr = listener.local_addr().expect("upstream addr");
            let (capture_tx, captured) = oneshot::channel();
            let capture_tx = Arc::new(Mutex::new(Some(capture_tx)));
            let (shutdown_tx, shutdown_rx) = oneshot::channel();
            let router = Router::new()
                .route("/v1/responses", post(capture_handler))
                .with_state(capture_tx);
            tokio::spawn(async move {
                let server = axum::serve(listener, router).with_graceful_shutdown(async {
                    let _ = shutdown_rx.await;
                });
                let _ = server.await;
            });
            Self {
                addr,
                captured,
                shutdown: Some(shutdown_tx),
            }
        }

        fn shutdown(mut self) {
            if let Some(shutdown) = self.shutdown.take() {
                let _ = shutdown.send(());
            }
        }
    }

    async fn capture_handler(
        State(capture_tx): State<Arc<Mutex<Option<oneshot::Sender<CapturedRequest>>>>>,
        request: Request<Body>,
    ) -> Json<Value> {
        let (parts, body) = request.into_parts();
        let body = to_bytes(body, MAX_PROXY_BODY_BYTES).await.expect("body");
        if let Some(sender) = capture_tx.lock().await.take() {
            let _ = sender.send(CapturedRequest {
                path: parts
                    .uri
                    .path_and_query()
                    .map(|path| path.as_str().to_string())
                    .unwrap_or_default(),
                authorization: parts
                    .headers
                    .get(header::AUTHORIZATION)
                    .and_then(|value| value.to_str().ok())
                    .map(ToOwned::to_owned),
                body: String::from_utf8_lossy(&body).to_string(),
            });
        }
        Json(json!({"ok": true}))
    }
}
