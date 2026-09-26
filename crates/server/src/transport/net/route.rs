//! path 路由与请求边界（`design.md` D2、`specs/node-link-listener/spec.md` 的路由/升级/Host 需求）。
//!
//! 规则：
//!
//! - **只有注册过的 path 有服务**：未注册的 path（含 `/sync/v1*`，`server::sync` 尚未落地）一律 404，
//!   不会进入任何处理器。路由表由 `server::node_link` 在 `app` 组合根里注册，本模块不写死协议路径。
//! - **Host/代理头边界在路由之前**：`axum::Router::layer` 同时覆盖 fallback，因此 Host 不匹配的请求
//!   在未注册 path 上也返回 400，而不会先走 404。
//! - **WebSocket 升级规则**：协商到 `permessage-deflate` 或未声明约定 subprotocol 的升级请求被拒绝；
//!   非升级请求由 `axum` 的升级提取器按 400/405/426 拒绝；单条消息上限在升级响应上固定。
//! - **请求体上限**：配对端点的请求体超过 [`crate::transport::net::NetConfig::max_body_bytes`] 时返回
//!   413（`NODE_LINK_PROTOCOL.md` §13.4），处理器不会被调用。

use std::collections::BTreeMap;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;

use axum::Router;
use axum::body::Bytes;
use axum::extract::ConnectInfo;
use axum::extract::FromRequestParts as _;
use axum::extract::Request;
use axum::extract::State;
use axum::extract::ws::WebSocketUpgrade;
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::http::header;
use axum::middleware::Next;
use axum::middleware::from_fn_with_state;
use axum::response::IntoResponse as _;
use axum::response::Response;
use axum::routing::get;
use axum::routing::post;
use tokio::sync::mpsc;

use crate::transport::net::HostPolicy;
use crate::transport::net::PeerInfo;
use crate::transport::net::ProxyPolicy;
use crate::transport::net::Shutdown;
use crate::transport::net::http::HttpRequest;
use crate::transport::net::http::HttpResponse;
use crate::transport::net::ws::WsConnection;
use crate::transport::net::ws::WsHandler;

/// 一条已升级会话在接入层的持有形式（由 supervisor 任务放进 `JoinSet`）。
pub(crate) type SessionJob = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

/// 路由与请求处理共享的状态。
pub(crate) struct NetState {
    pub(crate) host_policy: HostPolicy,
    pub(crate) proxy_policy: ProxyPolicy,
    pub(crate) max_message_bytes: usize,
    pub(crate) max_body_bytes: usize,
    pub(crate) tls_terminated: bool,
    pub(crate) shutdown: Shutdown,
    pub(crate) sessions: mpsc::Sender<SessionJob>,
}

/// 配对 HTTP 端点的处理器：由 `server::node_link` 实现并注入。
#[async_trait::async_trait]
pub trait HttpHandler: Send + Sync + 'static {
    /// 处理一条已通过 Host/代理头边界、请求体在上限内的请求。
    async fn handle(&self, request: HttpRequest) -> HttpResponse;
}

/// 路由注册失败。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RouteError {
    /// path 不是本模块接受的形态（必须以 `/` 开头，不含路径参数、query、fragment 或空白，长度 ≤ 256）。
    #[error("路由 path 非法：`{path}`")]
    InvalidPath {
        /// 被拒绝的 path。
        path: String,
    },
    /// 约定的 subprotocol 为空或不是合法 token。
    #[error("要求的 WebSocket subprotocol 非法：`{subprotocol}`")]
    InvalidSubprotocol {
        /// 被拒绝的 subprotocol。
        subprotocol: String,
    },
    /// 同一个 path 被注册了两次。
    #[error("路由 path 已注册：`{path}`")]
    DuplicatePath {
        /// 重复的 path。
        path: String,
    },
}

/// 一个 WebSocket 端点的注册项。
struct WsRoute {
    required_subprotocol: String,
    handler: Arc<dyn WsHandler>,
}

/// 一个 HTTP POST 端点的注册项。
struct PostRoute {
    handler: Arc<dyn HttpHandler>,
}

/// path → 处理器的注册表。
#[derive(Default)]
pub(crate) struct RouteTable {
    ws: BTreeMap<String, WsRoute>,
    post: BTreeMap<String, PostRoute>,
}

impl RouteTable {
    /// 注册一个 WebSocket 端点：只有声明了 `required_subprotocol` 的升级请求才会成功。
    pub(crate) fn insert_ws(
        &mut self,
        path: &str,
        required_subprotocol: &str,
        handler: Arc<dyn WsHandler>,
    ) -> Result<(), RouteError> {
        validate_path(path)?;
        validate_subprotocol(required_subprotocol)?;
        if self.ws.contains_key(path) || self.post.contains_key(path) {
            return Err(RouteError::DuplicatePath {
                path: path.to_owned(),
            });
        }
        self.ws.insert(
            path.to_owned(),
            WsRoute {
                required_subprotocol: required_subprotocol.to_owned(),
                handler,
            },
        );
        Ok(())
    }

    /// 注册一个只接受 `POST` 的 HTTP 端点。
    pub(crate) fn insert_post(
        &mut self,
        path: &str,
        handler: Arc<dyn HttpHandler>,
    ) -> Result<(), RouteError> {
        validate_path(path)?;
        if self.ws.contains_key(path) || self.post.contains_key(path) {
            return Err(RouteError::DuplicatePath {
                path: path.to_owned(),
            });
        }
        self.post.insert(path.to_owned(), PostRoute { handler });
        Ok(())
    }

    /// 组装 `axum` 路由：注册 path 按种类分派，其余 path 一律 404；边界中间件覆盖含 fallback 的全部请求。
    pub(crate) fn into_router(self, state: Arc<NetState>) -> Router {
        let mut router = Router::new();
        for (path, route) in self.ws {
            let state = Arc::clone(&state);
            let handler = route.handler;
            let required_subprotocol = route.required_subprotocol;
            router =
                router.route(
                    &path,
                    get(move |request: Request| {
                        let state = Arc::clone(&state);
                        let handler = Arc::clone(&handler);
                        let required_subprotocol = required_subprotocol.clone();
                        async move {
                            upgrade_websocket(state, handler, required_subprotocol, request).await
                        }
                    }),
                );
        }
        for (path, route) in self.post {
            let state = Arc::clone(&state);
            let handler = route.handler;
            router = router.route(
                &path,
                post(move |request: Request| {
                    let state = Arc::clone(&state);
                    let handler = Arc::clone(&handler);
                    async move { serve_post(state, handler, request).await }
                }),
            );
        }
        // 路由表为空时也要经过边界中间件（未注册 path 的 404 因此与已注册 path 同一条路径）。
        router
            .fallback(not_found)
            .layer(from_fn_with_state(state, boundary))
    }
}

/// 未注册 path 的响应（`/sync/v1*` 与其余未知 path）。
async fn not_found() -> StatusCode {
    StatusCode::NOT_FOUND
}

/// 注册 path 的形态校验（不做路径参数匹配：本层的 path 全是固定字面量）。
fn validate_path(path: &str) -> Result<(), RouteError> {
    let invalid = !path.starts_with('/')
        || path.len() > 256
        || path.contains(['{', '}', '?', '#', ' '])
        || path.contains('\t');
    if invalid {
        return Err(RouteError::InvalidPath {
            path: path.to_owned(),
        });
    }
    Ok(())
}

/// subprotocol 必须是 RFC 7230 token（`acp-remote.nodelink.v1.json` 是其中一例）。
fn validate_subprotocol(subprotocol: &str) -> Result<(), RouteError> {
    let valid = !subprotocol.is_empty()
        && subprotocol.len() <= 128
        && subprotocol.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'!' | b'#'
                        | b'$'
                        | b'%'
                        | b'&'
                        | b'\''
                        | b'*'
                        | b'+'
                        | b'-'
                        | b'.'
                        | b'^'
                        | b'_'
                        | b'`'
                        | b'|'
                        | b'~'
                )
        });
    if valid {
        Ok(())
    } else {
        Err(RouteError::InvalidSubprotocol {
            subprotocol: subprotocol.to_owned(),
        })
    }
}

/// 请求边界中间件：先校验 Host，再解析真实对端地址并注入 [`PeerInfo`]，最后才进入路由。
async fn boundary(
    State(state): State<Arc<NetState>>,
    ConnectInfo(peer_addr): ConnectInfo<SocketAddr>,
    mut request: Request,
    next: Next,
) -> Response {
    if !state
        .host_policy
        .accepts(HostPolicy::request_host(request.headers()))
    {
        // Host 不匹配：不进入任何 path 的处理逻辑（含未注册 path，不先给 404）。
        tracing::debug!(
            event = "net.host_rejected",
            path = %request.uri().path(),
            "Host 不在允许集合内：拒绝请求"
        );
        return host_rejected();
    }
    let resolved = state.proxy_policy.resolve(peer_addr, request.headers());
    request.extensions_mut().insert(PeerInfo {
        peer_addr,
        client_ip: resolved.client_ip,
        forwarded_headers: resolved.forwarded_headers,
        tls_terminated: state.tls_terminated,
        shutdown: state.shutdown.clone(),
    });
    next.run(request).await
}

/// Host 边界拒绝的响应（不含业务语义，也不回显收到的 Host）。
pub(crate) fn host_rejected() -> Response {
    (
        StatusCode::BAD_REQUEST,
        "Host 不在允许集合内（CONFIG_REFERENCE.md §1）",
    )
        .into_response()
}

/// 配对 HTTP 端点：请求体上限（413）→ 处理器。
async fn serve_post(
    state: Arc<NetState>,
    handler: Arc<dyn HttpHandler>,
    request: Request,
) -> Response {
    let Some(peer) = request.extensions().get::<PeerInfo>().cloned() else {
        // 中间件必先运行；缺失表示接线错误，失败关闭而不是放行。
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let (parts, body) = request.into_parts();
    let body: Bytes = match axum::body::to_bytes(body, state.max_body_bytes).await {
        Ok(body) => body,
        Err(_) => {
            tracing::debug!(
                event = "net.request_body_too_large",
                path = parts.uri.path(),
                limit = state.max_body_bytes,
                "请求体超过上限：返回 413 且不调用处理器"
            );
            return StatusCode::PAYLOAD_TOO_LARGE.into_response();
        }
    };
    let request = HttpRequest {
        method: parts.method,
        path: parts.uri.path().to_owned(),
        headers: parts.headers,
        body,
        peer,
    };
    handler.handle(request).await.into_response()
}

/// WebSocket 端点：压缩/subprotocol 判定 → 升级 → 把会话交给接入选层持有。
async fn upgrade_websocket(
    state: Arc<NetState>,
    handler: Arc<dyn WsHandler>,
    required_subprotocol: String,
    request: Request,
) -> Response {
    let Some(peer) = request.extensions().get::<PeerInfo>().cloned() else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    if offers_permessage_deflate(request.headers()) {
        // `NODE_LINK_PROTOCOL.md` §2.1：v1 不接受压缩，协商到压缩的 upgrade 必须拒绝。
        return (
            StatusCode::BAD_REQUEST,
            "本端点不接受 permessage-deflate（NODE_LINK_PROTOCOL.md §2.1）",
        )
            .into_response();
    }
    if !offers_subprotocol(request.headers(), &required_subprotocol) {
        return (
            StatusCode::BAD_REQUEST,
            "升级请求未声明约定的 Sec-WebSocket-Protocol（NODE_LINK_PROTOCOL.md §2.1）",
        )
            .into_response();
    }
    let (mut parts, _body) = request.into_parts();
    let upgrade = match WebSocketUpgrade::from_request_parts(&mut parts, &()).await {
        Ok(upgrade) => upgrade,
        // 非升级请求、错误方法等：由 axum 给出 400/405/426 等明确 4xx，不进业务逻辑。
        Err(rejection) => return rejection.into_response(),
    };
    let max_message_bytes = state.max_message_bytes;
    let sessions = state.sessions.clone();
    upgrade
        // 帧与消息上限取同一值：帧上限保证超限在 payload 分配前被拒绝（`ws` 模块文档）。
        .max_message_size(max_message_bytes)
        .max_frame_size(max_message_bytes)
        .protocols([required_subprotocol])
        .on_upgrade(move |socket| async move {
            let connection = WsConnection::new(socket, max_message_bytes);
            if sessions
                .send(Arc::clone(&handler).handle(connection, peer))
                .await
                .is_err()
            {
                // 接入层已停止接受会话（关闭序列中）：直接关闭这条连接。
                tracing::warn!(
                    event = "net.ws_session_rejected",
                    "接入层已停止接受会话：关闭该 WebSocket 连接"
                );
            }
        })
}

/// 客户端是否声明了约定的 subprotocol。
///
/// 比较是**精确**的：RFC 6455 的 subprotocol 是大小写敏感 token，且 `axum` 的 `protocols()` 也按精确值
/// 回填响应头，因此这里必须用同一口径，否则会出现「校验通过但响应没回填」的不一致。
///
/// 同名头出现多次（同一 token 拆成两个 `Sec-WebSocket-Protocol`）与一个头里的逗号列表一视同仁：
/// `axum::extract::ws::WebSocketUpgrade` 同样用 `get_all(...)` 收集全部头值并按逗号切分后 trim，
/// 再用 `contains` 匹配，所以「任一头值里以 token 形式出现」时两边都成立（101 会回填该 token），
/// 两个头都不含该 token 时两边都拒绝。用例 `repeated_subprotocol_headers_...` 锁定了这条口径。
fn offers_subprotocol(headers: &HeaderMap, required: &str) -> bool {
    headers
        .get_all(header::SEC_WEBSOCKET_PROTOCOL)
        .iter()
        .any(|value| {
            value
                .to_str()
                .is_ok_and(|value| value.split(',').any(|token| token.trim() == required))
        })
}

/// 客户端是否要求 `permessage-deflate`。
fn offers_permessage_deflate(headers: &HeaderMap) -> bool {
    headers
        .get_all(header::SEC_WEBSOCKET_EXTENSIONS)
        .iter()
        .any(|value| {
            value.to_str().is_ok_and(|value| {
                value.split(',').any(|extension| {
                    extension
                        .split(';')
                        .next()
                        .is_some_and(|name| name.trim().eq_ignore_ascii_case("permessage-deflate"))
                })
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for (name, value) in pairs {
            headers.append(
                header::HeaderName::from_bytes(name.as_bytes()).expect("合法头名"),
                value.parse().expect("合法头值"),
            );
        }
        headers
    }

    #[test]
    fn path_validation_rejects_ambiguous_shapes() {
        assert!(validate_path("/node-link/v1").is_ok());
        assert!(validate_path("/node-link/v1/pairing/claim").is_ok());
        assert!(validate_path("node-link/v1").is_err());
        assert!(validate_path("/node-link/{id}").is_err());
        assert!(validate_path("/node-link/v1?x=1").is_err());
        assert!(validate_path("/node-link/v1#frag").is_err());
        assert!(validate_path("/node-link/v1 ").is_err());
        assert!(validate_path(&format!("/{}", "a".repeat(300))).is_err());
    }

    #[test]
    fn subprotocol_validation_accepts_the_node_link_token() {
        assert!(validate_subprotocol("acp-remote.nodelink.v1.json").is_ok());
        assert!(validate_subprotocol("").is_err());
        assert!(validate_subprotocol("has space").is_err());
        assert!(validate_subprotocol("has,comma").is_err());
        assert!(validate_subprotocol("全角").is_err());
    }

    #[test]
    fn subprotocol_detection_is_exact_and_token_wise() {
        let required = "acp-remote.nodelink.v1.json";
        assert!(offers_subprotocol(
            &headers(&[("sec-websocket-protocol", required)]),
            required
        ));
        assert!(offers_subprotocol(
            &headers(&[(
                "sec-websocket-protocol",
                "other, acp-remote.nodelink.v1.json"
            )]),
            required
        ));
        assert!(!offers_subprotocol(
            &headers(&[("sec-websocket-protocol", "other")]),
            required
        ));
        // 同一 token 拆成两个头：`HeaderMap` 保留多值，逐个头值按逗号切分判定。
        assert!(offers_subprotocol(
            &headers(&[
                ("sec-websocket-protocol", required),
                ("sec-websocket-protocol", "other")
            ]),
            required
        ));
        assert!(offers_subprotocol(
            &headers(&[
                ("sec-websocket-protocol", "other"),
                ("sec-websocket-protocol", required)
            ]),
            required
        ));
        assert!(!offers_subprotocol(
            &headers(&[
                ("sec-websocket-protocol", "other"),
                ("sec-websocket-protocol", "acp-remote.nodelink.v2.json")
            ]),
            required
        ));
        assert!(!offers_subprotocol(
            &headers(&[("sec-websocket-protocol", "ACP-REMOTE.NODELINK.V1.JSON")]),
            required
        ));
        assert!(!offers_subprotocol(&HeaderMap::new(), required));
    }

    #[test]
    fn compression_detection_covers_parameterised_offers() {
        assert!(offers_permessage_deflate(&headers(&[(
            "sec-websocket-extensions",
            "permessage-deflate; client_max_window_bits"
        )])));
        assert!(offers_permessage_deflate(&headers(&[(
            "sec-websocket-extensions",
            "foo, PerMessage-Deflate"
        )])));
        assert!(!offers_permessage_deflate(&headers(&[(
            "sec-websocket-extensions",
            "x-webkit-deflate-frame"
        )])));
        assert!(!offers_permessage_deflate(&HeaderMap::new()));
    }
}
