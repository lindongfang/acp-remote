//! path 路由与请求边界（`design.md` D2、`specs/node-link-listener/spec.md` 的路由/升级/Host 需求）。
//!
//! 规则：
//!
//! - **只有注册过的 path 有服务，另加一条显式注册的静态回退**：未注册的 path 默认 404，不会进入任何
//!   处理器。注册了静态端点（[`RouteTable::insert_static`]）后，未被协议端点命中的请求由静态面接住——
//!   协议族保留前缀（每条已注册 path 的前两段）之下的路径仍 404，其余读取方法回退到前端入口文档、
//!   非读取方法 405（解析顺序见 `static_ui` 模块文档）。静态面是**显式**注册的，不是无条件的通配：
//!   没注册它时 `/` 依旧 404。路由表由 `server::node_link` 与 `server::sync` 各自的注册函数在 `app`
//!   组合根里填入，本模块不写死协议路径。
//! - **Host/代理头边界在路由之前**：`axum::Router::layer` 同时覆盖 fallback，因此 Host 不匹配的请求
//!   在未注册 path 上也返回 400，而不会先走 404。
//! - **WebSocket 升级规则**：声明了约定 subprotocol 的升级请求照常接受；**提出** `permessage-deflate`
//!   的升级**同样接受**——`Sec-WebSocket-Extensions` 是**协商**请求（RFC 7692），服务端只要不选择任何
//!   扩展（101 响应不回显 `Sec-WebSocket-Extensions`，本层不开启 axum 的压缩特性）就等于「不协商压缩」
//!   （`NODE_LINK_PROTOCOL.md` §2.1 与 `SYNC_PROTOCOL.md` §3.1 都是「不协商」口径）。**拒绝提出扩展的
//!   请求是错的**：Chromium/Firefox 的 `WebSocket` 构造函数**一律**带上这个头且页面侧无法关闭，按提出
//!   即拒会让任何浏览器客户端都连不上（实测：400「本端点不接受 permessage-deflate」）；
//!   未声明约定 subprotocol 的升级请求仍被拒绝。
//!   非升级请求由 `axum` 的升级提取器按 400/405/426 拒绝；单条消息上限在升级响应上固定。
//! - **来源（`Origin`）与升级一起取值**：升级请求带 `Origin` 头时，本模块按 `docs/SYNC_PROTOCOL.md`
//!   §3.2 的口径归一化后交给处理器（见 [`crate::transport::net::origin`]）；头缺席也是合法形态（非浏览器
//!   对端）；头存在但不可归一化则失败关闭（400，不降级成「无来源」）。这条判定对所有已注册的 WS path
//!   一致（来源是请求头边界，与 path 无关；非浏览器客户端本来就按 §3.2 不带该头）。**与 `Host` 无关**：
//!   `Host` 是虚拟主机概念，来源是协议层概念，两者各自判定、不互为替代。
//! - **请求体上限**：配对端点的请求体超过 [`crate::transport::net::NetConfig::max_body_bytes`] 时返回
//!   413（`NODE_LINK_PROTOCOL.md` §13.4、`SYNC_PROTOCOL.md` §14），处理器不会被调用。上限是**每 listener
//!   一个值**（不是每 path），因此两面共用同一个 16 KiB 默认值（见 `DEFAULT_MAX_BODY_BYTES` 的注记）。
//! - **每路径默认响应头**：`register_post` 可为一条 path 声明一组默认响应头，该 path 上的**所有**响应都会
//!   补齐它们——处理器产生的响应、接入层在调用处理器前产生的 413/405，以及 Host 边界在这个 path 上的 400。
//!   响应已带同名头时保留响应自己的值（默认头是补齐语义，不会变成重复头）：配对端点因此能声明
//!   「无论成功或失败都带四个安全头」（`NODE_LINK_PROTOCOL.md` §13.1）。静态端点走**同一条缝**，
//!   声明自己那份（CSP 与四个安全头，取值理由见 `static_ui`），因此 Host 边界在静态 path 上的 400
//!   也带它们。

use std::collections::BTreeMap;
use std::collections::BTreeSet;
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
use axum::http::HeaderName;
use axum::http::HeaderValue;
use axum::http::StatusCode;
use axum::http::header;
use axum::middleware::Next;
use axum::middleware::from_fn;
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
use crate::transport::net::origin::request_origin;
use crate::transport::net::static_ui;
use crate::transport::net::static_ui::StaticEndpoint;
use crate::transport::net::static_ui::UiAssetSource;
use crate::transport::net::ws::WsConnection;
use crate::transport::net::ws::WsHandler;

/// 一条已升级会话在接入层的持有形式（由 supervisor 任务放进 `JoinSet`）。
pub(crate) type SessionJob = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

/// 一条 POST path 的默认响应头（名字与值都已按 `http` 的规则解析）。
pub(crate) type PathDefaultHeaders = Vec<(HeaderName, HeaderValue)>;

/// 路由与请求处理共享的状态。
pub(crate) struct NetState {
    pub(crate) host_policy: HostPolicy,
    pub(crate) proxy_policy: ProxyPolicy,
    pub(crate) max_message_bytes: usize,
    pub(crate) max_body_bytes: usize,
    pub(crate) tls_terminated: bool,
    pub(crate) shutdown: Shutdown,
    pub(crate) sessions: mpsc::Sender<SessionJob>,
    /// 已注册 path 的默认响应头（POST 与静态端点；Host 边界在路由之前拒绝时按 path 补齐）。
    pub(crate) path_default_headers: BTreeMap<String, PathDefaultHeaders>,
}

/// 把一条 POST path 的默认头补齐到响应上。
///
/// 语义是**默认**：响应已经带了同名头时保留响应自己的值（处理器可以覆盖默认值），缺失才补上——
/// 因此处理器自带的头不会变成重复头。
pub(crate) fn apply_default_headers(headers: &mut HeaderMap, defaults: &PathDefaultHeaders) {
    for (name, value) in defaults {
        if !headers.contains_key(name) {
            headers.insert(name.clone(), value.clone());
        }
    }
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
    /// `register_post` 声明的默认响应头不是合法的 header 名或值。
    #[error("默认响应头非法：`{name}: {value}`")]
    InvalidDefaultHeader {
        /// 被拒绝的头名。
        name: String,
        /// 被拒绝的头值。
        value: String,
    },
    /// 静态端点已经注册过。
    #[error("静态端点已注册：至多一条（它服务根路径，并作为未命中路径的回退）")]
    StaticEndpointAlreadyRegistered,
}

/// 一个 WebSocket 端点的注册项。
struct WsRoute {
    required_subprotocol: String,
    handler: Arc<dyn WsHandler>,
}

/// 一个 HTTP POST 端点的注册项。
struct PostRoute {
    handler: Arc<dyn HttpHandler>,
    default_headers: PathDefaultHeaders,
}

/// 一个静态端点的注册项（路由表的第三类端点，见 `static_ui`）。
///
/// 至多一条：它服务注册的 path（生产形态是根路径），并作为未被协议端点命中的路径的回退。
struct StaticRoute {
    path: String,
    source: Arc<dyn UiAssetSource>,
    default_headers: PathDefaultHeaders,
}

/// path → 处理器的注册表。
#[derive(Default)]
pub(crate) struct RouteTable {
    ws: BTreeMap<String, WsRoute>,
    post: BTreeMap<String, PostRoute>,
    static_route: Option<StaticRoute>,
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
        if self.is_registered(path) {
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

    /// 注册一个只接受 `POST` 的 HTTP 端点，并声明该 path 的默认响应头。
    pub(crate) fn insert_post(
        &mut self,
        path: &str,
        handler: Arc<dyn HttpHandler>,
        default_headers: &[(&str, &str)],
    ) -> Result<(), RouteError> {
        validate_path(path)?;
        let default_headers = parse_default_headers(default_headers)?;
        if self.is_registered(path) {
            return Err(RouteError::DuplicatePath {
                path: path.to_owned(),
            });
        }
        self.post.insert(
            path.to_owned(),
            PostRoute {
                handler,
                default_headers,
            },
        );
        Ok(())
    }

    /// 注册静态面：构建产物交给浏览器，未被协议端点命中的路径回退到入口文档（`static_ui`）。
    ///
    /// 默认响应头由静态模块自己声明（CSP 基线与四个安全头属于这一面，不交给调用方决定），声明在当前
    /// path 上，因此接入层在调用处理器前产生的响应（Host 边界的 400）也带它们。
    pub(crate) fn insert_static(
        &mut self,
        path: &str,
        source: Arc<dyn UiAssetSource>,
    ) -> Result<(), RouteError> {
        validate_path(path)?;
        if self.is_registered(path) {
            return Err(RouteError::DuplicatePath {
                path: path.to_owned(),
            });
        }
        if self.static_route.is_some() {
            // 只有一条静态端点：它是 fallback 的唯一承担者，「哪条 path 是根」在多条时会变成隐式约定。
            return Err(RouteError::StaticEndpointAlreadyRegistered);
        }
        self.static_route = Some(StaticRoute {
            path: path.to_owned(),
            source,
            default_headers: parse_default_headers(&static_ui::STATIC_HEADERS)?,
        });
        Ok(())
    }

    /// path 是否已被任一类别注册（三类端点共用同一个 path 命名空间）。
    fn is_registered(&self, path: &str) -> bool {
        self.ws.contains_key(path)
            || self.post.contains_key(path)
            || self
                .static_route
                .as_ref()
                .is_some_and(|route| route.path == path)
    }

    /// 已注册 path 的默认响应头（由 [`NetState`] 持有，供 Host 边界按 path 补齐）。
    ///
    /// 两类 path 都收：配对 POST 端点声明的四个安全头，以及静态端点自己那份（CSP 与四个安全头）——
    /// Host 边界在路由之前拒绝时也要带它们。
    pub(crate) fn path_default_headers(&self) -> BTreeMap<String, PathDefaultHeaders> {
        let mut defaults: BTreeMap<String, PathDefaultHeaders> = self
            .post
            .iter()
            .map(|(path, route)| (path.clone(), route.default_headers.clone()))
            .collect();
        if let Some(route) = &self.static_route {
            defaults.insert(route.path.clone(), route.default_headers.clone());
        }
        defaults
    }

    /// 组装 `axum` 路由：注册 path 按种类分派，其余 path 交给静态面（未注册静态端点时一律 404）；
    /// 边界中间件覆盖含 fallback 的全部请求。
    pub(crate) fn into_router(self, state: Arc<NetState>) -> Router {
        // 绑定名**不能**叫 `ws`/`post`：那会遮蔽文件顶部的 `use axum::routing::{get, post};`，让下面的
        // 循环把 `post(...)` 解析成 `BTreeMap` 值而不是路由构造函数（本函数与 `get` 的用法保持对称）。
        let RouteTable {
            ws: ws_routes,
            post: post_routes,
            static_route,
        } = self;
        // 静态回退必须拒绝协议族下未注册的子路径（`static_ui` 模块文档），因此保留前缀从**已注册的协议
        // path** 派生：注册是唯一来源，本模块不写死 `/sync`/`/node-link`。
        let reserved_prefixes: BTreeSet<String> = ws_routes
            .keys()
            .chain(post_routes.keys())
            .map(|path| static_ui::protocol_prefix(path))
            .collect();
        // 静态面是 fallback 的唯一承担者，因此这里只可能有一条。
        let static_endpoint = static_route.map(|route| {
            Arc::new(StaticEndpoint::new(
                route.path,
                route.source,
                reserved_prefixes,
                route.default_headers,
            ))
        });
        let mut router = Router::new();
        for (path, route) in ws_routes {
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
        for (path, route) in post_routes {
            let state = Arc::clone(&state);
            let handler = route.handler;
            let mut method_router = post(move |request: Request| {
                let state = Arc::clone(&state);
                let handler = Arc::clone(&handler);
                async move { serve_post(state, handler, request).await }
            });
            if !route.default_headers.is_empty() {
                // 本 path 上**所有**响应都要补齐默认头，包括接入层在调用处理器前产生的响应
                // （请求体超限的 413、`axum` 的方法不匹配 405），因此层加在方法路由的最外层。
                let defaults = Arc::new(route.default_headers);
                method_router =
                    method_router.layer(from_fn(move |request: Request, next: Next| {
                        let defaults = Arc::clone(&defaults);
                        async move {
                            let mut response = next.run(request).await;
                            apply_default_headers(response.headers_mut(), &defaults);
                            response
                        }
                    }));
            }
            router = router.route(&path, method_router);
        }
        // 静态面接管 fallback：未命中任何协议端点的请求由 `static_ui` 的解析顺序判定。协议端点仍由
        // 精确路由优先命中，因此「协议端点优先」不依赖这里的任何判断。
        let router = match static_endpoint {
            Some(endpoint) => router.fallback(move |request: Request| {
                let endpoint = Arc::clone(&endpoint);
                async move { static_ui::serve(endpoint, request) }
            }),
            // 未注册静态面时保持既有语义：任何未注册 path 一律 404。
            None => router.fallback(not_found),
        };
        // 路由表为空时也要经过边界中间件（未注册 path 的 404 因此与已注册 path 同一条路径）。
        router.layer(from_fn_with_state(state, boundary))
    }
}

/// 未注册 path 的响应（未知 path 与未被任何适配器注册的协议路径）。
///
/// 只在**未注册静态端点**时充当 fallback；注册了静态面时 fallback 换成 `static_ui::serve`，它自带
/// 协议族保留前缀的 404 与静态面的解析顺序。
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

/// 解析注册时声明的默认响应头：名字/值不合法即拒绝注册，而不是留到响应期才失败。
fn parse_default_headers(pairs: &[(&str, &str)]) -> Result<PathDefaultHeaders, RouteError> {
    let mut parsed = Vec::with_capacity(pairs.len());
    for (name, value) in pairs {
        let invalid = || RouteError::InvalidDefaultHeader {
            name: (*name).to_owned(),
            value: (*value).to_owned(),
        };
        let name = HeaderName::from_bytes(name.as_bytes()).map_err(|_| invalid())?;
        let value = HeaderValue::from_str(value).map_err(|_| invalid())?;
        parsed.push((name, value));
    }
    Ok(parsed)
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
        let mut response = host_rejected();
        // 该拒绝发生在路由之前，因此按 path 补齐已注册端点的默认头（未注册 path 没有默认头）。
        // 静态端点的 path 也在表里，所以根路径上的 Host 400 同样带静态面的安全头。
        if let Some(defaults) = state.path_default_headers.get(request.uri().path()) {
            apply_default_headers(response.headers_mut(), defaults);
        }
        return response;
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
    if !offers_subprotocol(request.headers(), &required_subprotocol) {
        return (
            StatusCode::BAD_REQUEST,
            "升级请求未声明约定的 Sec-WebSocket-Protocol（NODE_LINK_PROTOCOL.md §2.1）",
        )
            .into_response();
    }
    // 来源（HTTP `Origin`）是请求边界的一部分：只如实取得并归一化，比较留给上层（`origin` 模块文档）。
    // 头存在但不可归一化时失败关闭，而不是降级成「无来源」——后者在协议里另有含义
    // （`SYNC_PROTOCOL.md` §3.2 的「非浏览器对端」）。不回显收到的取值（与 Host 边界的处置一致）。
    let Ok(origin) = request_origin(request.headers()) else {
        tracing::debug!(
            event = "net.origin_rejected",
            path = %request.uri().path(),
            "Origin 头存在但不可归一化：拒绝升级"
        );
        return (
            StatusCode::BAD_REQUEST,
            "Origin 头不是可校验的来源（SYNC_PROTOCOL.md §3.2）",
        )
            .into_response();
    };
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
                .send(Arc::clone(&handler).handle(connection, peer, origin))
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

    /// 只回固定入口文档的资源源（注册表用例不关心资源内容）。
    struct StubAssets;

    impl UiAssetSource for StubAssets {
        fn entry_document(&self) -> Bytes {
            Bytes::from_static(b"<html></html>")
        }

        fn asset(&self, _path: &str) -> Option<Bytes> {
            None
        }
    }

    /// 固定回 200 的 POST 处理器（只为占住一条 path）。
    struct StubHandler;

    #[async_trait::async_trait]
    impl HttpHandler for StubHandler {
        async fn handle(&self, _request: HttpRequest) -> HttpResponse {
            HttpResponse::new(StatusCode::OK)
        }
    }

    #[test]
    fn static_registration_is_exclusive_and_shares_the_path_namespace() {
        let mut table = RouteTable::default();
        table
            .insert_static("/", Arc::new(StubAssets))
            .expect("第一次注册成功");
        // 三类端点共用同一个 path 命名空间：静态面占住的 path 不能再被 POST 注册。
        assert_eq!(
            table
                .insert_post("/", Arc::new(StubHandler), &[])
                .unwrap_err(),
            RouteError::DuplicatePath {
                path: "/".to_owned()
            }
        );
        // 至多一条静态端点：它是 fallback 的唯一承担者。
        assert_eq!(
            table
                .insert_static("/ui", Arc::new(StubAssets))
                .unwrap_err(),
            RouteError::StaticEndpointAlreadyRegistered
        );
        // 静态端点自带默认响应头（CSP 与四个安全头），Host 边界在根路径上拒绝时也带它们。
        let defaults = table.path_default_headers();
        let static_defaults = defaults.get("/").expect("根路径必须在表里");
        assert!(static_defaults.iter().any(|(name, value)| {
            name.as_str() == "content-security-policy"
                && value
                    .to_str()
                    .is_ok_and(|value| value == static_ui::CONTENT_SECURITY_POLICY)
        }));
        assert_eq!(static_defaults.len(), static_ui::STATIC_HEADERS.len());
    }
}
