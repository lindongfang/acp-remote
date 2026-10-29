use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use axum::{
    Json, Router,
    body::{Body, to_bytes},
    extract::{Extension, Path as AxumPath, Request, State},
    http::{
        HeaderMap, HeaderValue, Method, StatusCode,
        header::{
            CACHE_CONTROL, CONTENT_ENCODING, CONTENT_SECURITY_POLICY, CONTENT_TYPE, HOST, ORIGIN,
            REFERRER_POLICY, RETRY_AFTER, X_CONTENT_TYPE_OPTIONS,
        },
    },
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{any, get},
};
use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;

use crate::{
    claims::{Id128, NodeId, Nonce256, PublicKeySec1, UnixSeconds, verify_pair_status},
    config::NodeConfig,
    ipc::IpcControl,
    pairing::{PairingError, PairingService},
    strict_json::{MAX_PROTOCOL_JSON_DEPTH, parse_object},
    trust::TrustError,
};

pub(crate) const MAX_REQUEST_BYTES: usize = 16 * 1024;
const RATE_WINDOW: Duration = Duration::from_secs(60);
const SOURCE_IDLE: Duration = Duration::from_mins(5);
const MAX_SOURCE_BUCKETS: usize = 1_024;
const STATUS_INTERVAL: Duration = Duration::from_secs(1);
const MAX_STATUS_ENTRIES: usize = 1_024;
const CONTENT_SECURITY_POLICY_VALUE: &str = concat!(
    "default-src 'self'; ",
    "base-uri 'none'; ",
    "form-action 'self'; ",
    "frame-ancestors 'none'; ",
    "object-src 'none'"
);

#[derive(RustEmbed)]
#[folder = "../clients/web/dist"]
struct WebAssets;

#[derive(Clone, Copy, Debug)]
pub(crate) struct PeerAddress(pub SocketAddr);

#[derive(Clone)]
struct PairingHttpState {
    pairing: Arc<PairingService>,
    origin: Arc<str>,
    control: IpcControl,
    limiter: RateLimiter,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Endpoint {
    Lookup,
    Claim,
    Challenge,
    Status,
}

impl Endpoint {
    const fn index(self) -> usize {
        match self {
            Self::Lookup => 0,
            Self::Claim => 1,
            Self::Challenge => 2,
            Self::Status => 3,
        }
    }

    const fn limits(self) -> (u16, u16) {
        match self {
            Self::Lookup => (5, 30),
            Self::Claim | Self::Challenge | Self::Status => (30, 120),
        }
    }
}

#[derive(Clone)]
struct RateLimiter {
    state: Arc<Mutex<RateState>>,
}

struct RateState {
    sources: HashMap<IpAddr, SourceWindow>,
    global_started: Instant,
    global_counts: [u16; 4],
    statuses: HashMap<Id128, Instant>,
}

struct SourceWindow {
    started: Instant,
    counts: [u16; 4],
    last_seen: Instant,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RateLimitRejection {
    retry_after_seconds: u64,
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self {
            state: Arc::new(Mutex::new(RateState {
                sources: HashMap::new(),
                global_started: Instant::now(),
                global_counts: [0; 4],
                statuses: HashMap::new(),
            })),
        }
    }
}

impl RateLimiter {
    fn check(&self, peer: SocketAddr, endpoint: Endpoint) -> Result<(), RateLimitRejection> {
        self.check_at(peer, endpoint, Instant::now())
    }

    fn check_at(
        &self,
        peer: SocketAddr,
        endpoint: Endpoint,
        now: Instant,
    ) -> Result<(), RateLimitRejection> {
        let source = peer.ip();
        let mut state = self.state.lock().map_err(|_| RateLimitRejection {
            retry_after_seconds: 1,
        })?;
        prune_rate_state(&mut state, now);
        {
            let RateState {
                global_started,
                global_counts,
                ..
            } = &mut *state;
            reset_window_if_due(global_started, global_counts, now);
        }

        let index = endpoint.index();
        state.global_counts[index] = state.global_counts[index].saturating_add(1);
        let (source_limit, global_limit) = endpoint.limits();

        let source_started = if let Some(window) = state.sources.get_mut(&source) {
            reset_window_if_due(&mut window.started, &mut window.counts, now);
            window.counts[index] = window.counts[index].saturating_add(1);
            window.last_seen = now;
            Some(window.started)
        } else if state.sources.len() >= MAX_SOURCE_BUCKETS {
            None
        } else {
            let mut window = SourceWindow {
                started: now,
                counts: [0; 4],
                last_seen: now,
            };
            window.counts[index] = 1;
            state.sources.insert(source, window);
            Some(now)
        };

        let source_count = source_started
            .and_then(|_| state.sources.get(&source))
            .map_or(u16::MAX, |window| window.counts[index]);
        let source_over = source_count > source_limit;
        let global_over = state.global_counts[index] > global_limit;
        if !source_over && !global_over {
            return Ok(());
        }

        let source_deadline = source_started
            .and_then(|started| started.checked_add(RATE_WINDOW))
            .unwrap_or(now);
        let global_deadline = state.global_started.checked_add(RATE_WINDOW).unwrap_or(now);
        let deadline = if source_over && global_over {
            std::cmp::max(source_deadline, global_deadline)
        } else if source_over {
            source_deadline
        } else {
            global_deadline
        };
        let rejection = RateLimitRejection {
            retry_after_seconds: seconds_until(deadline, now),
        };
        drop(state);
        Err(rejection)
    }

    fn check_pairing_status(&self, pairing_id: Id128) -> Result<(), RateLimitRejection> {
        self.check_pairing_status_at(pairing_id, Instant::now())
    }

    fn check_pairing_status_at(
        &self,
        pairing_id: Id128,
        now: Instant,
    ) -> Result<(), RateLimitRejection> {
        let mut state = self.state.lock().map_err(|_| RateLimitRejection {
            retry_after_seconds: 1,
        })?;
        state
            .statuses
            .retain(|_, last| now.saturating_duration_since(*last) < STATUS_INTERVAL);
        if state.statuses.contains_key(&pairing_id) {
            return Err(RateLimitRejection {
                retry_after_seconds: 1,
            });
        }
        if state.statuses.len() >= MAX_STATUS_ENTRIES {
            return Err(RateLimitRejection {
                retry_after_seconds: 1,
            });
        }
        state.statuses.insert(pairing_id, now);
        drop(state);
        Ok(())
    }
}

fn prune_rate_state(state: &mut RateState, now: Instant) {
    state
        .sources
        .retain(|_, source| now.saturating_duration_since(source.last_seen) < SOURCE_IDLE);
}

fn reset_window_if_due(started: &mut Instant, counts: &mut [u16; 4], now: Instant) {
    if now.saturating_duration_since(*started) >= RATE_WINDOW {
        *started = now;
        *counts = [0; 4];
    }
}

fn seconds_until(deadline: Instant, now: Instant) -> u64 {
    let remaining = deadline.saturating_duration_since(now).as_millis();
    let rounded_up = remaining.saturating_add(999) / 1_000;
    u64::try_from(rounded_up).unwrap_or(u64::MAX).max(1)
}

pub(crate) fn router(pairing: Arc<PairingService>, origin: String, control: IpcControl) -> Router {
    let state = PairingHttpState {
        pairing,
        origin: Arc::from(origin),
        control,
        limiter: RateLimiter::default(),
    };
    Router::new()
        .route("/api/access/v1/pair/lookup", any(lookup))
        .route("/api/access/v1/pair/claim", any(claim))
        .route("/api/access/v1/pair/challenge", any(challenge))
        .route("/api/access/v1/pair/status", any(status))
        .route("/", get(web_index))
        .route("/index.html", get(web_index))
        .route("/assets/{*path}", get(web_asset))
        .fallback(not_found)
        .layer(middleware::from_fn(security_headers))
        .with_state(state)
}

async fn web_index() -> Response {
    web_asset_response("index.html")
}

async fn web_asset(AxumPath(path): AxumPath<String>) -> Response {
    web_asset_response(&format!("assets/{path}"))
}

fn web_asset_response(path: &str) -> Response {
    let Some(asset) = WebAssets::get(path) else {
        return error_response(ApiError::not_found());
    };
    let content_type = mime_guess::from_path(path).first_or_octet_stream();
    let mut response = Body::from(asset.data.into_owned()).into_response();
    let value = HeaderValue::from_str(content_type.as_ref())
        .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream"));
    response.headers_mut().insert(CONTENT_TYPE, value);
    response
}

async fn lookup(
    State(state): State<PairingHttpState>,
    peer: Option<Extension<PeerAddress>>,
    headers: HeaderMap,
    request: Request,
) -> Response {
    let request: LookupRequest =
        match decode_request(&state, peer, &headers, request, Endpoint::Lookup).await {
            Ok(request) => request,
            Err(error) => return error_response(error),
        };
    let lookup = match state
        .pairing
        .lookup(&request.code, request.request_id)
        .await
    {
        Ok(lookup) => lookup,
        Err(error) => return error_response(pairing_error(&error)),
    };
    let challenge = lookup.challenge;
    success_response(LookupResponse {
        v: 1,
        pairing_id: lookup.pairing_id,
        node_id: challenge.node_id,
        node_public_key: challenge.node_public_key,
        origin: challenge.origin,
        request_id: request.request_id,
        expires_at: lookup.expires_at,
        challenge_id: challenge.challenge_id,
        nonce: challenge.nonce,
        iat: challenge.iat,
        exp: challenge.exp,
    })
}

async fn claim(
    State(state): State<PairingHttpState>,
    peer: Option<Extension<PeerAddress>>,
    headers: HeaderMap,
    request: Request,
) -> Response {
    let request: ClaimRequest =
        match decode_request(&state, peer, &headers, request, Endpoint::Claim).await {
            Ok(request) => request,
            Err(error) => return error_response(error),
        };
    match state.pairing.claim(&request.proof).await {
        Ok(result) => success_response(HostProofResponse {
            v: 1,
            host_proof: result.host_proof,
        }),
        Err(error) => error_response(pairing_error(&error)),
    }
}

async fn challenge(
    State(state): State<PairingHttpState>,
    peer: Option<Extension<PeerAddress>>,
    headers: HeaderMap,
    request: Request,
) -> Response {
    let request: ChallengeRequest =
        match decode_request(&state, peer, &headers, request, Endpoint::Challenge).await {
            Ok(request) => request,
            Err(error) => return error_response(error),
        };
    let challenge = match state
        .pairing
        .status_challenge(
            request.pairing_id,
            request.client_public_key,
            request.request_id,
        )
        .await
    {
        Ok(challenge) => challenge,
        Err(error) => return error_response(pairing_error(&error)),
    };
    success_response(ChallengeResponse {
        v: 1,
        node_id: challenge.node_id,
        node_public_key: challenge.node_public_key,
        origin: challenge.origin,
        request_id: challenge.request_id.unwrap_or(request.request_id),
        challenge_id: challenge.challenge_id,
        nonce: challenge.nonce,
        iat: challenge.iat,
        exp: challenge.exp,
    })
}

async fn status(
    State(state): State<PairingHttpState>,
    peer: Option<Extension<PeerAddress>>,
    headers: HeaderMap,
    request: Request,
) -> Response {
    let request: StatusRequest =
        match decode_request(&state, peer, &headers, request, Endpoint::Status).await {
            Ok(request) => request,
            Err(error) => return error_response(error),
        };
    let Ok(proof) = verify_pair_status(&request.proof) else {
        return error_response(ApiError::pairing_invalid());
    };
    if let Err(rejection) = state.limiter.check_pairing_status(proof.pairing_id) {
        return error_response(ApiError::rate_limited(rejection.retry_after_seconds));
    }
    match state.pairing.status(&request.proof).await {
        Ok(result) => success_response(HostProofResponse {
            v: 1,
            host_proof: result.host_proof,
        }),
        Err(error) => error_response(pairing_error(&error)),
    }
}

async fn decode_request<T: DeserializeOwned>(
    state: &PairingHttpState,
    peer: Option<Extension<PeerAddress>>,
    headers: &HeaderMap,
    request: Request,
    endpoint: Endpoint,
) -> Result<T, ApiError> {
    if request.method() != Method::POST {
        return Err(ApiError::method_not_allowed());
    }
    if !state.control.is_ready() {
        return Err(ApiError::node_stopping());
    }
    let Some(Extension(PeerAddress(peer))) = peer else {
        return Err(ApiError::invalid_request());
    };
    if let Err(rejection) = state.limiter.check(peer, endpoint) {
        return Err(ApiError::rate_limited(rejection.retry_after_seconds));
    }
    validate_transport_headers(headers, &state.origin)?;
    validate_content_length(headers)?;
    let body = to_bytes(request.into_body(), MAX_REQUEST_BYTES)
        .await
        .map_err(|_| ApiError::invalid_request())?;
    if body.is_empty() {
        return Err(ApiError::invalid_request());
    }
    parse_protocol_request(&body)
}

fn validate_transport_headers(headers: &HeaderMap, expected_origin: &str) -> Result<(), ApiError> {
    if headers.contains_key(CONTENT_ENCODING) {
        return Err(ApiError::invalid_request());
    }
    if !validate_origin_and_host(headers, expected_origin) {
        return Err(ApiError::invalid_request());
    }
    let Some(content_type) = single_header(headers, CONTENT_TYPE) else {
        return Err(ApiError::invalid_request());
    };
    let Ok(content_type) = content_type.to_str() else {
        return Err(ApiError::invalid_request());
    };
    if !content_type.eq_ignore_ascii_case("application/json") {
        return Err(ApiError::invalid_request());
    }
    Ok(())
}

pub(crate) fn validate_origin_and_host(headers: &HeaderMap, expected_origin: &str) -> bool {
    let Some(origin) = single_header(headers, ORIGIN) else {
        return false;
    };
    if origin.to_str().ok() != Some(expected_origin) {
        return false;
    }
    let Some(host) = single_header(headers, HOST) else {
        return false;
    };
    let Ok(host) = host.to_str() else {
        return false;
    };
    host_matches(host, expected_origin)
}

fn host_matches(host: &str, expected_origin: &str) -> bool {
    if host.is_empty()
        || host
            .bytes()
            .any(|byte| byte.is_ascii_whitespace() || byte == b',' || byte == b'/')
    {
        return false;
    }
    NodeConfig::normalize_origin(&format!("https://{host}"))
        .is_ok_and(|origin| origin == expected_origin)
}

fn single_header(
    headers: &HeaderMap,
    name: axum::http::header::HeaderName,
) -> Option<&HeaderValue> {
    let mut values = headers.get_all(name).iter();
    let value = values.next()?;
    values.next().is_none().then_some(value)
}

fn validate_content_length(headers: &HeaderMap) -> Result<(), ApiError> {
    let mut values = headers.get_all(axum::http::header::CONTENT_LENGTH).iter();
    let Some(value) = values.next() else {
        return Ok(());
    };
    if values.next().is_some() {
        return Err(ApiError::invalid_request());
    }
    let length = value
        .to_str()
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .ok_or_else(ApiError::invalid_request)?;
    if length > MAX_REQUEST_BYTES {
        return Err(ApiError::invalid_request());
    }
    Ok(())
}

fn parse_protocol_request<T: DeserializeOwned>(body: &[u8]) -> Result<T, ApiError> {
    let object =
        parse_object(body, MAX_PROTOCOL_JSON_DEPTH).map_err(|_| ApiError::invalid_request())?;
    match object.get("v") {
        Some(Value::Number(version)) if version.as_u64() == Some(1) => {}
        Some(Value::Number(version)) if version.as_u64().is_some() => {
            return Err(ApiError::unsupported_version());
        }
        _ => return Err(ApiError::invalid_request()),
    }
    serde_json::from_value(Value::Object(object)).map_err(|_| ApiError::invalid_request())
}

pub(crate) async fn security_headers(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    headers.insert(
        CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(CONTENT_SECURITY_POLICY_VALUE),
    );
    headers.insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    response
}

async fn not_found() -> Response {
    error_response(ApiError::not_found())
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct LookupRequest {
    #[serde(rename = "v")]
    _v: u8,
    code: String,
    request_id: Id128,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ClaimRequest {
    #[serde(rename = "v")]
    _v: u8,
    proof: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ChallengeRequest {
    #[serde(rename = "v")]
    _v: u8,
    pairing_id: Id128,
    client_public_key: PublicKeySec1,
    request_id: Id128,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct StatusRequest {
    #[serde(rename = "v")]
    _v: u8,
    proof: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LookupResponse {
    v: u8,
    pairing_id: Id128,
    node_id: NodeId,
    node_public_key: PublicKeySec1,
    origin: String,
    request_id: Id128,
    expires_at: u64,
    challenge_id: Id128,
    nonce: Nonce256,
    iat: UnixSeconds,
    exp: UnixSeconds,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ChallengeResponse {
    v: u8,
    node_id: NodeId,
    node_public_key: PublicKeySec1,
    origin: String,
    request_id: Id128,
    challenge_id: Id128,
    nonce: Nonce256,
    iat: UnixSeconds,
    exp: UnixSeconds,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct HostProofResponse {
    v: u8,
    host_proof: String,
}

#[derive(Debug, Serialize)]
struct ErrorEnvelope {
    v: u8,
    error: ErrorBody,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ErrorBody {
    code: &'static str,
    retry_after_seconds: Option<u64>,
}

#[derive(Clone, Copy, Debug)]
struct ApiError {
    status: StatusCode,
    code: &'static str,
    retry_after_seconds: Option<u64>,
}

impl ApiError {
    const fn invalid_request() -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "invalid_request",
            retry_after_seconds: None,
        }
    }

    const fn unsupported_version() -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "unsupported_version",
            retry_after_seconds: None,
        }
    }

    const fn pairing_invalid() -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            code: "pairing_invalid",
            retry_after_seconds: None,
        }
    }

    const fn rate_limited(retry_after_seconds: u64) -> Self {
        Self {
            status: StatusCode::TOO_MANY_REQUESTS,
            code: "rate_limited",
            retry_after_seconds: Some(retry_after_seconds),
        }
    }

    const fn storage_unavailable() -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            code: "storage_unavailable",
            retry_after_seconds: None,
        }
    }

    const fn node_stopping() -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            code: "node_stopping",
            retry_after_seconds: None,
        }
    }

    const fn method_not_allowed() -> Self {
        Self {
            status: StatusCode::METHOD_NOT_ALLOWED,
            code: "invalid_request",
            retry_after_seconds: None,
        }
    }

    const fn not_found() -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code: "invalid_request",
            retry_after_seconds: None,
        }
    }
}

const fn pairing_error(error: &PairingError) -> ApiError {
    match error {
        PairingError::RequestConflict => ApiError {
            status: StatusCode::CONFLICT,
            code: "request_conflict",
            retry_after_seconds: None,
        },
        PairingError::StorageUnavailable
        | PairingError::CapacityExhausted
        | PairingError::RandomUnavailable
        | PairingError::ClockUnavailable
        | PairingError::Trust(
            TrustError::StorageUnavailable
            | TrustError::Database(_)
            | TrustError::Storage(_)
            | TrustError::CapacityExhausted
            | TrustError::ClientIdCollision,
        ) => ApiError::storage_unavailable(),
        PairingError::PairingInvalid
        | PairingError::AlreadyApproved
        | PairingError::Trust(_)
        | PairingError::Jws(_)
        | PairingError::Challenge(_) => ApiError::pairing_invalid(),
    }
}

fn error_response(error: ApiError) -> Response {
    let mut response = (
        error.status,
        Json(ErrorEnvelope {
            v: 1,
            error: ErrorBody {
                code: error.code,
                retry_after_seconds: error.retry_after_seconds,
            },
        }),
    )
        .into_response();
    if let Some(retry_after_seconds) = error.retry_after_seconds
        && let Ok(value) = HeaderValue::from_str(&retry_after_seconds.to_string())
    {
        response.headers_mut().insert(RETRY_AFTER, value);
    }
    response
}

fn success_response(value: impl Serialize) -> Response {
    Json(value).into_response()
}

#[cfg(test)]
mod tests {
    use std::{
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use p256::ecdsa::SigningKey;
    use serde_json::Value;
    use tower::ServiceExt;

    use super::*;
    use crate::{
        claims::{PairClaim, PairStatus, verify_claim_result},
        identity::NodeIdentity,
        jws::{JwsType, sign},
        storage::Storage,
    };

    const TEST_ORIGIN: &str = "https://node.example.test";
    static FIXTURE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    struct Fixture {
        directory: PathBuf,
        identity: Arc<NodeIdentity>,
        storage: Storage,
        pairing: Arc<PairingService>,
        router: Router,
    }

    impl Fixture {
        async fn new() -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("test clock")
                .as_nanos();
            let sequence = FIXTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let directory = std::env::temp_dir().join(format!(
                "acp-remote-pairing-http-{}-{nonce}-{sequence}",
                std::process::id(),
            ));
            std::fs::create_dir(&directory).expect("test directory");
            let storage = Storage::open(&directory.join("node.db"))
                .await
                .expect("test storage");
            let identity = Arc::new(NodeIdentity::for_test([7_u8; 32]));
            let pairing = Arc::new(
                PairingService::new(
                    identity.clone(),
                    TEST_ORIGIN.to_owned(),
                    storage.trust_store(),
                )
                .expect("pairing service"),
            );
            let (control, _) =
                IpcControl::new(identity.node_id().to_owned(), TEST_ORIGIN.to_owned());
            let router = router(pairing.clone(), TEST_ORIGIN.to_owned(), control);
            Self {
                directory,
                identity,
                storage,
                pairing,
                router,
            }
        }

        async fn close(self) {
            let Self {
                directory,
                identity,
                storage,
                pairing,
                router,
            } = self;
            drop(router);
            drop(pairing);
            drop(identity);
            storage.close().await;
            std::fs::remove_dir_all(directory).expect("test cleanup");
        }
    }

    fn request(path: &str, body: impl Into<Body>, peer_port: u16) -> Request<Body> {
        request_with_peer(path, body, SocketAddr::from(([127, 0, 0, 1], peer_port)))
    }

    fn request_with_peer(path: &str, body: impl Into<Body>, peer: SocketAddr) -> Request<Body> {
        Request::builder()
            .method(Method::POST)
            .uri(path)
            .header(HOST, "node.example.test")
            .header(ORIGIN, TEST_ORIGIN)
            .header(CONTENT_TYPE, "application/json")
            .extension(PeerAddress(peer))
            .body(body.into())
            .expect("request")
    }

    async fn exchange(router: &Router, request: Request<Body>) -> (StatusCode, HeaderMap, Value) {
        let response = router.clone().oneshot(request).await.expect("response");
        let status = response.status();
        let headers = response.headers().clone();
        let body = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("response body");
        let value = serde_json::from_slice(&body).expect("JSON response");
        (status, headers, value)
    }

    fn public_key(key: &SigningKey) -> PublicKeySec1 {
        PublicKeySec1::new(
            key.verifying_key()
                .to_sec1_point(false)
                .as_bytes()
                .try_into()
                .expect("65-byte key"),
        )
        .expect("valid key")
    }

    fn claim_for(lookup: &Value, pairing_id: Id128, key: &SigningKey) -> String {
        let claim = PairClaim {
            v: 1,
            node_id: serde_json::from_value(lookup["nodeId"].clone()).expect("node ID"),
            origin: lookup["origin"].as_str().expect("origin").to_owned(),
            request_id: serde_json::from_value(lookup["requestId"].clone()).expect("request ID"),
            challenge_id: serde_json::from_value(lookup["challengeId"].clone())
                .expect("challenge ID"),
            nonce: serde_json::from_value(lookup["nonce"].clone()).expect("nonce"),
            iat: serde_json::from_value(lookup["iat"].clone()).expect("issued at"),
            exp: serde_json::from_value(lookup["exp"].clone()).expect("expiry"),
            pairing_id,
            client_public_key: public_key(key),
            display_name: "Browser Alice".to_owned(),
        };
        sign(
            JwsType::PairClaim,
            &serde_json::to_vec(&claim).expect("claim JSON"),
            key,
        )
        .expect("claim proof")
    }

    fn status_for(challenge: &Value, pairing_id: Id128, key: &SigningKey) -> String {
        let proof = PairStatus {
            v: 1,
            node_id: serde_json::from_value(challenge["nodeId"].clone()).expect("node ID"),
            origin: challenge["origin"].as_str().expect("origin").to_owned(),
            request_id: serde_json::from_value(challenge["requestId"].clone()).expect("request ID"),
            challenge_id: serde_json::from_value(challenge["challengeId"].clone())
                .expect("challenge ID"),
            nonce: serde_json::from_value(challenge["nonce"].clone()).expect("nonce"),
            iat: serde_json::from_value(challenge["iat"].clone()).expect("issued at"),
            exp: serde_json::from_value(challenge["exp"].clone()).expect("expiry"),
            pairing_id,
            client_public_key: public_key(key),
        };
        sign(
            JwsType::PairStatus,
            &serde_json::to_vec(&proof).expect("status JSON"),
            key,
        )
        .expect("status proof")
    }

    #[tokio::test]
    async fn lookup_returns_a_bound_challenge_and_refresh_invalidates_the_old_one() {
        let fixture = Fixture::new().await;
        let (ticket, _events) = fixture.pairing.create_pairing().await.expect("pairing");
        let request_id = Id128::new([1; 16]);
        let body = serde_json::json!({
            "v": 1,
            "code": crate::pairing::format_pairing_code(&ticket.code),
            "requestId": request_id,
        });
        let (status, headers, first) = exchange(
            &fixture.router,
            request(
                "/api/access/v1/pair/lookup",
                serde_json::to_vec(&body).expect("request body"),
                50_001,
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(headers[CACHE_CONTROL], "no-store");
        assert_eq!(headers[REFERRER_POLICY], "no-referrer");
        assert_eq!(first["v"], 1);
        assert_eq!(first["pairingId"], ticket.pairing_id.to_base64url());
        assert_eq!(first["requestId"], request_id.to_base64url());
        assert_eq!(first["origin"], TEST_ORIGIN);

        let second = exchange(
            &fixture.router,
            request(
                "/api/access/v1/pair/lookup",
                serde_json::to_vec(&serde_json::json!({
                    "v": 1,
                    "code": ticket.code,
                    "requestId": Id128::new([2; 16]),
                }))
                .expect("request body"),
                50_001,
            ),
        )
        .await;
        assert_eq!(second.0, StatusCode::OK);

        let client_key = SigningKey::from_slice(&[8_u8; 32]).expect("client key");
        let stale_claim = claim_for(&first, ticket.pairing_id, &client_key);
        let (status, _, error) = exchange(
            &fixture.router,
            request(
                "/api/access/v1/pair/claim",
                serde_json::to_vec(&serde_json::json!({
                    "v": 1,
                    "proof": stale_claim,
                }))
                .expect("request body"),
                50_001,
            ),
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(error["error"]["code"], "pairing_invalid");
        fixture.close().await;
    }

    #[tokio::test]
    async fn claim_challenge_and_status_complete_the_http_contract() {
        let fixture = Fixture::new().await;
        let (ticket, _events) = fixture.pairing.create_pairing().await.expect("pairing");
        let lookup_body = serde_json::to_vec(&serde_json::json!({
            "v": 1,
            "code": ticket.code,
            "requestId": Id128::new([3; 16]),
        }))
        .expect("lookup body");
        let (_, _, lookup) = exchange(
            &fixture.router,
            request("/api/access/v1/pair/lookup", lookup_body, 50_002),
        )
        .await;

        let client_key = SigningKey::from_slice(&[9_u8; 32]).expect("client key");
        let claim = claim_for(&lookup, ticket.pairing_id, &client_key);
        let (status, _, result) = exchange(
            &fixture.router,
            request(
                "/api/access/v1/pair/claim",
                serde_json::to_vec(&serde_json::json!({
                    "v": 1,
                    "proof": claim,
                }))
                .expect("claim body"),
                50_002,
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let host_proof = result["hostProof"].as_str().expect("host proof");
        let claim_result = verify_claim_result(
            host_proof,
            &public_key(&SigningKey::from_slice(&[7_u8; 32]).expect("node key")),
        )
        .expect("node-signed claim result");
        assert_eq!(claim_result.pairing_id, ticket.pairing_id);

        let challenge_body = serde_json::to_vec(&serde_json::json!({
            "v": 1,
            "pairingId": ticket.pairing_id,
            "clientPublicKey": public_key(&client_key),
            "requestId": Id128::new([4; 16]),
        }))
        .expect("challenge body");
        let (status, _, challenge) = exchange(
            &fixture.router,
            request("/api/access/v1/pair/challenge", challenge_body, 50_002),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(challenge.get("clientId").is_none());

        let status_proof = status_for(&challenge, ticket.pairing_id, &client_key);
        let (status, _, result) = exchange(
            &fixture.router,
            request(
                "/api/access/v1/pair/status",
                serde_json::to_vec(&serde_json::json!({
                    "v": 1,
                    "proof": status_proof.clone(),
                }))
                .expect("status body"),
                50_002,
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(result["hostProof"].is_string());

        let (status, headers, error) = exchange(
            &fixture.router,
            request(
                "/api/access/v1/pair/status",
                serde_json::to_vec(&serde_json::json!({
                    "v": 1,
                    "proof": status_proof,
                }))
                .expect("status body"),
                50_002,
            ),
        )
        .await;
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(error["error"]["code"], "rate_limited");
        let retry_after_seconds = error["error"]["retryAfterSeconds"]
            .as_u64()
            .expect("retry seconds");
        assert_eq!(
            headers[RETRY_AFTER].to_str().expect("retry header"),
            retry_after_seconds.to_string()
        );
        fixture.close().await;
    }

    #[tokio::test]
    async fn rejects_transport_version_json_and_size_violations_with_closed_errors() {
        let fixture = Fixture::new().await;
        let (ticket, _events) = fixture.pairing.create_pairing().await.expect("pairing");
        let valid_body = serde_json::to_vec(&serde_json::json!({
            "v": 1,
            "code": ticket.code,
            "requestId": Id128::new([5; 16]),
        }))
        .expect("body");

        let mut missing_origin = Request::builder()
            .method(Method::POST)
            .uri("/api/access/v1/pair/lookup")
            .header(HOST, "node.example.test")
            .header(CONTENT_TYPE, "application/json")
            .extension(PeerAddress(SocketAddr::from(([127, 0, 0, 1], 50_003))))
            .body(Body::from(valid_body.clone()))
            .expect("request");
        missing_origin.headers_mut().remove(ORIGIN);
        let (status, _, error) = exchange(&fixture.router, missing_origin).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(error["error"]["code"], "invalid_request");

        let mut compressed = request("/api/access/v1/pair/lookup", valid_body.clone(), 50_003);
        compressed
            .headers_mut()
            .insert(CONTENT_ENCODING, HeaderValue::from_static("gzip"));
        let (status, _, error) = exchange(&fixture.router, compressed).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(error["error"]["code"], "invalid_request");

        let unsupported = serde_json::to_vec(&serde_json::json!({
            "v": 2,
            "code": ticket.code,
            "requestId": Id128::new([6; 16]),
        }))
        .expect("body");
        let (status, _, error) = exchange(
            &fixture.router,
            request("/api/access/v1/pair/lookup", unsupported, 50_003),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(error["error"]["code"], "unsupported_version");

        let duplicate = format!(
            r#"{{"v":1,"v":1,"code":"{}","requestId":"{}"}}"#,
            ticket.code,
            Id128::new([7; 16]).to_base64url()
        );
        let (status, _, error) = exchange(
            &fixture.router,
            request("/api/access/v1/pair/lookup", duplicate, 50_003),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(error["error"]["code"], "invalid_request");

        let oversized = vec![b' '; MAX_REQUEST_BYTES + 1];
        let (status, _, error) = exchange(
            &fixture.router,
            request("/api/access/v1/pair/lookup", oversized, 50_003),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(error["error"]["code"], "invalid_request");

        let (status, headers, _) = exchange(
            &fixture.router,
            Request::builder()
                .method(Method::GET)
                .uri("/api/access/v1/pair/lookup")
                .body(Body::empty())
                .expect("request"),
        )
        .await;
        assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(headers[CACHE_CONTROL], "no-store");
        fixture.close().await;
    }

    #[tokio::test]
    async fn rejects_host_and_repeated_transport_headers_and_unknown_paths_closed() {
        let fixture = Fixture::new().await;
        let body = serde_json::to_vec(&serde_json::json!({
            "v": 1,
            "code": "ABCD-EFGH",
            "requestId": Id128::new([10; 16]),
        }))
        .expect("lookup body");

        let mut wrong_host = request_with_peer(
            "/api/access/v1/pair/lookup",
            body.clone(),
            SocketAddr::from(([127, 0, 0, 1], 51_001)),
        );
        wrong_host
            .headers_mut()
            .insert(HOST, HeaderValue::from_static("other.example.test"));

        let mut duplicate_host = request_with_peer(
            "/api/access/v1/pair/lookup",
            body.clone(),
            SocketAddr::from(([127, 0, 0, 2], 51_002)),
        );
        duplicate_host
            .headers_mut()
            .append(HOST, HeaderValue::from_static("other.example.test"));

        let mut duplicate_origin = request_with_peer(
            "/api/access/v1/pair/lookup",
            body.clone(),
            SocketAddr::from(([127, 0, 0, 3], 51_003)),
        );
        duplicate_origin.headers_mut().append(
            ORIGIN,
            HeaderValue::from_static("https://other.example.test"),
        );

        let mut duplicate_content_type = request_with_peer(
            "/api/access/v1/pair/lookup",
            body.clone(),
            SocketAddr::from(([127, 0, 0, 4], 51_004)),
        );
        duplicate_content_type
            .headers_mut()
            .append(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        let mut duplicate_content_length = request_with_peer(
            "/api/access/v1/pair/lookup",
            body.clone(),
            SocketAddr::from(([127, 0, 0, 5], 51_005)),
        );
        duplicate_content_length.headers_mut().insert(
            axum::http::header::CONTENT_LENGTH,
            HeaderValue::from_static("1"),
        );
        duplicate_content_length.headers_mut().append(
            axum::http::header::CONTENT_LENGTH,
            HeaderValue::from_static("1"),
        );

        let mut oversized_content_length = request_with_peer(
            "/api/access/v1/pair/lookup",
            body,
            SocketAddr::from(([127, 0, 0, 6], 51_006)),
        );
        oversized_content_length.headers_mut().insert(
            axum::http::header::CONTENT_LENGTH,
            HeaderValue::from_str(&(MAX_REQUEST_BYTES + 1).to_string())
                .expect("content length header"),
        );

        for (name, request) in [
            ("wrong host", wrong_host),
            ("duplicate host", duplicate_host),
            ("duplicate origin", duplicate_origin),
            ("duplicate content type", duplicate_content_type),
            ("duplicate content length", duplicate_content_length),
            ("oversized content length", oversized_content_length),
        ] {
            let (status, headers, error) = exchange(&fixture.router, request).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{name}");
            assert_eq!(error["v"], 1, "{name}");
            assert_eq!(error["error"]["code"], "invalid_request", "{name}");
            assert_eq!(error["error"]["retryAfterSeconds"], Value::Null, "{name}");
            assert_eq!(headers[CACHE_CONTROL], "no-store");
            assert_eq!(headers[REFERRER_POLICY], "no-referrer");
            assert_eq!(headers[X_CONTENT_TYPE_OPTIONS], "nosniff");
        }

        let unknown = Request::builder()
            .method(Method::POST)
            .uri("/api/access/v1/missing")
            .header(HOST, "node.example.test")
            .header(ORIGIN, TEST_ORIGIN)
            .header(CONTENT_TYPE, "application/json")
            .extension(PeerAddress(SocketAddr::from(([127, 0, 0, 1], 51_007))))
            .body(Body::from("{}"))
            .expect("unknown request");
        let (status, headers, error) = exchange(&fixture.router, unknown).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(error["v"], 1);
        assert_eq!(error["error"]["code"], "invalid_request");
        assert_eq!(error["error"]["retryAfterSeconds"], Value::Null);
        assert_eq!(headers[CACHE_CONTROL], "no-store");

        fixture.close().await;
    }

    #[tokio::test]
    async fn claim_retries_return_the_same_result_and_changed_proofs_conflict() {
        let fixture = Fixture::new().await;
        let (ticket, _events) = fixture.pairing.create_pairing().await.expect("pairing");
        let (_, _, lookup) = exchange(
            &fixture.router,
            request(
                "/api/access/v1/pair/lookup",
                serde_json::to_vec(&serde_json::json!({
                    "v": 1,
                    "code": ticket.code,
                    "requestId": Id128::new([11; 16]),
                }))
                .expect("lookup body"),
                51_008,
            ),
        )
        .await;

        let first_key = SigningKey::from_slice(&[12_u8; 32]).expect("first client key");
        let first_proof = claim_for(&lookup, ticket.pairing_id, &first_key);
        let claim_body = serde_json::to_vec(&serde_json::json!({
            "v": 1,
            "proof": first_proof.clone(),
        }))
        .expect("claim body");
        let (status, _, first) = exchange(
            &fixture.router,
            request("/api/access/v1/pair/claim", claim_body.clone(), 51_008),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, _, retry) = exchange(
            &fixture.router,
            request("/api/access/v1/pair/claim", claim_body, 51_008),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(retry["hostProof"], first["hostProof"]);

        let second_key = SigningKey::from_slice(&[13_u8; 32]).expect("second client key");
        let changed_proof = claim_for(&lookup, ticket.pairing_id, &second_key);
        let (status, _, error) = exchange(
            &fixture.router,
            request(
                "/api/access/v1/pair/claim",
                serde_json::to_vec(&serde_json::json!({
                    "v": 1,
                    "proof": changed_proof,
                }))
                .expect("conflicting claim body"),
                51_008,
            ),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(error["error"]["code"], "request_conflict");

        fixture.close().await;
    }

    #[tokio::test]
    async fn rate_limit_is_per_source_and_status_is_limited_per_pairing() {
        let limiter = RateLimiter::default();
        let now = Instant::now();
        let first = SocketAddr::from(([127, 0, 0, 1], 50_004));
        let same_source_new_port = SocketAddr::from(([127, 0, 0, 1], 50_005));
        let other_source = SocketAddr::from(([127, 0, 0, 2], 50_005));
        for _ in 0..5 {
            limiter
                .check_at(first, Endpoint::Lookup, now)
                .expect("within lookup budget");
        }
        let rejected = limiter
            .check_at(first, Endpoint::Lookup, now)
            .expect_err("sixth lookup is rejected");
        assert_eq!(rejected.retry_after_seconds, 60);
        limiter
            .check_at(same_source_new_port, Endpoint::Lookup, now)
            .expect_err("ephemeral source port does not create a new source budget");
        limiter
            .check_at(other_source, Endpoint::Lookup, now)
            .expect("other source has its own budget");

        let pairing_id = Id128::new([8; 16]);
        limiter
            .check_pairing_status_at(pairing_id, now)
            .expect("first status");
        assert_eq!(
            limiter
                .check_pairing_status_at(pairing_id, now + Duration::from_millis(500))
                .expect_err("status is limited"),
            RateLimitRejection {
                retry_after_seconds: 1
            }
        );
        limiter
            .check_pairing_status_at(pairing_id, now + Duration::from_secs(1))
            .expect("status interval elapsed");
    }

    #[tokio::test]
    async fn host_must_parse_to_the_configured_origin() {
        assert!(host_matches("NODE.EXAMPLE.TEST:443", TEST_ORIGIN));
        assert!(host_matches("node.example.test", TEST_ORIGIN));
        for invalid in [
            "other.example.test",
            "node.example.test,evil.example.test",
            "user@node.example.test",
            "node.example.test/path",
            "",
        ] {
            assert!(!host_matches(invalid, TEST_ORIGIN), "{invalid}");
        }
    }

    #[test]
    fn all_endpoint_windows_reset_at_sixty_seconds_without_sliding_on_rejection() {
        for endpoint in [
            Endpoint::Lookup,
            Endpoint::Claim,
            Endpoint::Challenge,
            Endpoint::Status,
        ] {
            let limiter = RateLimiter::default();
            let start = Instant::now();
            let peer = SocketAddr::from(([127, 0, 0, 1], 51000));
            for _ in 0..endpoint.limits().0 {
                limiter
                    .check_at(peer, endpoint, start)
                    .expect("source quota");
            }
            assert_eq!(
                limiter
                    .check_at(peer, endpoint, start)
                    .expect_err("over quota")
                    .retry_after_seconds,
                60
            );
            assert_eq!(
                limiter
                    .check_at(peer, endpoint, start + Duration::from_millis(59999))
                    .expect_err("before boundary")
                    .retry_after_seconds,
                1
            );
            limiter
                .check_at(peer, endpoint, start + Duration::from_secs(60))
                .expect("exact original deadline, rejection did not slide window");
            let state = limiter.state.lock().expect("state");
            assert_eq!(state.sources[&peer.ip()].counts[endpoint.index()], 1);
            assert_eq!(state.global_counts[endpoint.index()], 1);
        }
    }

    #[test]
    fn source_bucket_capacity_cannot_evict_active_sources_and_prunes_at_five_minutes() {
        let limiter = RateLimiter::default();
        let start = Instant::now();
        for source in 0_u16..1024 {
            let [high, low] = source.to_be_bytes();
            // Global quota rejects some requests, but those attempts still create bounded buckets.
            let _ = limiter.check_at(
                SocketAddr::from(([10, 0, high, low], 51000)),
                Endpoint::Lookup,
                start,
            );
        }
        assert_eq!(
            limiter.state.lock().expect("state").sources.len(),
            MAX_SOURCE_BUCKETS
        );
        let new = SocketAddr::from(([10, 1, 0, 0], 51000));
        let minute = start + Duration::from_secs(60);
        limiter
            .check_at(new, Endpoint::Lookup, minute)
            .expect_err("1025th source cannot evict");
        {
            let state = limiter.state.lock().expect("state");
            assert_eq!(
                state.global_counts[Endpoint::Lookup.index()],
                1,
                "global quota not masking bucket rejection"
            );
            assert!(!state.sources.contains_key(&new.ip()));
            drop(state);
        }
        let old = SocketAddr::from(([10, 0, 0, 0], 51000));
        for _ in 0..5 {
            limiter
                .check_at(old, Endpoint::Lookup, minute)
                .expect("old source retained");
        }
        limiter
            .check_at(old, Endpoint::Lookup, minute)
            .expect_err("old quota enforced");
        limiter
            .check_at(new, Endpoint::Lookup, minute)
            .expect_err("still no eviction");
        limiter
            .check_at(old, Endpoint::Lookup, minute)
            .expect_err("new source did not reset old quota");
        limiter
            .check_at(new, Endpoint::Lookup, start + Duration::from_secs(299))
            .expect_err("299 idle seconds retained");
        assert_eq!(
            limiter.state.lock().expect("state").sources.len(),
            MAX_SOURCE_BUCKETS
        );
        limiter
            .check_at(new, Endpoint::Lookup, start + Duration::from_secs(300))
            .expect("exact five-minute idle eviction");
        let state = limiter.state.lock().expect("state");
        assert_eq!(state.sources.len(), 2, "recent old source plus new source");
        assert!(state.sources.contains_key(&old.ip()));
        assert!(state.sources.contains_key(&new.ip()));
        drop(state);
    }
}
