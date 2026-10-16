//! 前端静态资源托管：资源源缝、解析顺序、内容类型映射与安全响应头
//! （`design.md` D1/D4，`specs/ui-asset-hosting/spec.md`）。
//!
//! 本模块是路由表的**第三类端点**（静态面）：协议端点全部未命中时，`route::RouteTable` 组装出来的
//! router 的 fallback 调用 [`serve`]。「协议端点优先」由 `axum` 的精确路由结构保证——命中已注册
//! ws/post path 的请求根本到不了这里，因此静态面不需要（也不得）自己判断协议优先级；D1 否决的通配
//! 中间件方案正是要手写这套判定。
//!
//! 解析顺序（[`serve`] 的全部判定都在 [`resolve`]，便于单测锁定；下面是**判定顺序**，与代码逐条对应）：
//!
//! 1. 已注册的 ws/post path → 各自的协议处理逻辑（精确路由，天然优先，本模块看不到它们）；
//! 2. 协议族**保留前缀**之下（含前缀本身）→ `404`，与未注册 path 的既有语义一致，不被静态面接管；
//!    这一条**先于**方法判定：保留前缀下的非读取方法同样是 404，而不是静态面的 405；
//! 3. 非读取方法（非 `GET`/`HEAD`）→ `405` + `Allow: GET, HEAD`（明确拒绝，不返回入口文档冒充成功）；
//! 4. 注册的静态 path（生产形态是根路径）→ 入口文档；其它 path 命中资源表 → 该资源本身；
//! 5. 其余读取方法（`GET`/`HEAD`）→ 入口文档（单页应用回退）。
//!
//! **保留前缀为什么取「前两个 path 段」**：本模块路由表里的协议 path 全部在第 2 段带版本号
//! （`/sync/v1`、`/node-link/v1`，`docs/SYNC_PROTOCOL.md` §3 与 `docs/NODE_LINK_PROTOCOL.md` §2.1），
//! 因此前两段正是版本化协议族的边界（[`protocol_prefix`]）。注册的 path 本身由精确路由拦下，但它们
//! **未注册的子路径**（例如 `/sync/v1/pairing/other`）会掉进这里，必须按协议面语义拒绝。
//!
//! **依赖方向**：本模块不得 import `server::sync`/`server::node_link`（接入层不依赖业务适配器），因此
//! 配对端点声明的那四个安全头在这里按同一取值**独立**声明一份（[`STATIC_HEADERS`]），理由见该常量。

use std::collections::BTreeSet;
use std::sync::Arc;

use axum::body::Body;
use axum::body::Bytes;
use axum::extract::Request;
use axum::http::HeaderValue;
use axum::http::Method;
use axum::http::StatusCode;
use axum::http::header;
use axum::response::IntoResponse as _;
use axum::response::Response;

use crate::transport::net::route::PathDefaultHeaders;
use crate::transport::net::route::apply_default_headers;

/// 内容安全策略基线（`docs/FRONTEND_DESIGN.md` §4.5 的原文，逐字；`docs/SECURITY_DESIGN.md` §11.1）。
///
/// 不得由本层放宽：前端的内联 `<style>` 形态已由前端侧移除（入口文档改用同源外部样式表，
/// `ui-csp-inline-styles`），CSP 取值不变；放行 `unsafe-inline` 只会掩盖这类冲突，并同时打开
/// XSS 面（Agent 产生的内容按不可信输入渲染）。
pub(crate) const CONTENT_SECURITY_POLICY: &str = "default-src 'self'; script-src 'self'; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'";

/// 静态响应的安全响应头。
///
/// 取值与两个配对适配器（`server::sync::pairing`、`server::node_link::pairing`）声明的那份**一致但独立**：
/// 依赖方向不允许接入层 import 业务适配器，所以这里重新声明一份，逐条理由：
///
/// - `content-security-policy`：见 [`CONTENT_SECURITY_POLICY`]，**必须由响应头下发**（文档内的 `<meta>`
///   约束不了 `frame-ancestors` 之类的指令）；
/// - `cache-control: no-store` / `pragma: no-cache`：保守地不做客户端缓存。**不是**因为资源会变——
///   脚本等产物是内容哈希命名的（`/_expo/static/js/web/entry-*.js`），哈希本身已把构建绑进入口文档
///   （`design.md` D3）；真实取舍是避免构建切换窗口内新旧产物混用（入口文档与它引用的哈希资源分属
///   两次构建时，缓存会把窗口拉长），代价是每次刷新重新取一遍；
/// - `referrer-policy: no-referrer`：页面地址可能带配对深链（`/pair#data=...`），不得随外链泄露；
/// - `x-content-type-options: nosniff`：与内容类型映射一起阻止浏览器嗅探执行——未知扩展名回落
///   [`FALLBACK_CONTENT_TYPE`] 时，这条是「不被当脚本/HTML 执行」的另一半。
pub(crate) const STATIC_HEADERS: [(&str, &str); 5] = [
    ("content-security-policy", CONTENT_SECURITY_POLICY),
    ("cache-control", "no-store"),
    ("pragma", "no-cache"),
    ("referrer-policy", "no-referrer"),
    ("x-content-type-options", "nosniff"),
];

/// 非读取方法的 `Allow` 头取值（只读静态面：`GET` 与 `HEAD`）。
pub(crate) const ALLOWED_METHODS: &str = "GET, HEAD";

/// 入口文档的内容类型。单页回退的落点固定是 HTML，**不能**按请求路径的扩展名推导（深链没有扩展名，
/// 甚至有像 `/pair` 这样的「无扩展名」路径），因此它是独立常量，并与 [`MIME_TYPES`] 里的 `html` 共用同一个值。
pub(crate) const ENTRY_CONTENT_TYPE: &str = "text/html; charset=utf-8";

/// 未知扩展名的内容类型。
///
/// 取 `application/octet-stream` 而不是 `text/html`：后者会让任意字节被当页面渲染（配合 `nosniff` 也白搭，
/// 因为类型**确实**是 HTML）。通用二进制 + `nosniff` 是「不误导浏览器执行」的形态。
pub(crate) const FALLBACK_CONTENT_TYPE: &str = "application/octet-stream";

/// 构建产物扩展名 → `Content-Type` 的**唯一**一份映射（本模块持有，静态面与后续的嵌入资源表共用，
/// 不在第二处再写一张）。覆盖前端构建产物实际出现的类型。
///
/// - 脚本用 `text/javascript`（RFC 9239）而不是通用二进制：`nosniff` + `octet-stream` 会让浏览器直接
///   拒绝执行 `<script>`，页面会白屏；
/// - 文本类型都带 `charset=utf-8`：产物与 Agent 内容可能含非 ASCII，缺 charset 会被按 latin-1 解出乱码；
/// - `.map` 是 source map，本身是 JSON；
/// - `webmanifest` 是 PWA 清单（Expo web 产物会带）。
pub(crate) const MIME_TYPES: &[(&str, &str)] = &[
    ("html", ENTRY_CONTENT_TYPE),
    ("js", "text/javascript; charset=utf-8"),
    ("mjs", "text/javascript; charset=utf-8"),
    ("css", "text/css; charset=utf-8"),
    ("json", "application/json; charset=utf-8"),
    ("map", "application/json; charset=utf-8"),
    ("webmanifest", "application/manifest+json; charset=utf-8"),
    ("txt", "text/plain; charset=utf-8"),
    ("svg", "image/svg+xml"),
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("webp", "image/webp"),
    ("ico", "image/x-icon"),
    ("woff", "font/woff"),
    ("woff2", "font/woff2"),
    ("ttf", "font/ttf"),
];

/// 按构建产物路径的扩展名取内容类型；未知扩展名回落 [`FALLBACK_CONTENT_TYPE`]。
///
/// 只看**最后一段**的最后一个 `.` 之后（query 已在 `uri.path()` 处被去掉）：`/a/b.js` → `js`，
/// `/a.b/c`（目录名带点）→ 无扩展名 → 回落。
pub(crate) fn content_type_for_path(path: &str) -> &'static str {
    let extension = path
        .rsplit('/')
        .next()
        .and_then(|segment| segment.rsplit_once('.'))
        .map(|(_, extension)| extension);
    extension
        .and_then(|extension| {
            MIME_TYPES
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(extension))
                .map(|(_, content_type)| *content_type)
        })
        .unwrap_or(FALLBACK_CONTENT_TYPE)
}

/// 前端构建产物的来源。
///
/// 为什么留这条缝：处理器不引入嵌入依赖，也不能在编译期假设 `clients/app/dist` 存在（产物不入库，
/// 根 `.gitignore` 的 `dist/`），因此处理器只依赖这个 trait——单元测试用内存替身实现它，真实实现
/// （嵌入资源表 + 缺失产物时的占位入口）在 [`crate::transport::net::ui::EmbeddedUi`]。
///
/// 实现必须 `Send + Sync + 'static`：处理器由 router 的 fallback 跨任务共享。
///
/// 路径口径与构建产物一致：**以 `/` 开头的根相对绝对路径**（产物里的
/// `src="/_expo/static/js/web/entry-*.js"` 就是该口径）。处理器不会自己拼资源路径（入口文档之外），
/// 请求的 path 原样交给 [`UiAssetSource::asset`]。
pub trait UiAssetSource: Send + Sync + 'static {
    /// 入口文档：根路径与单页回退的落点。
    fn entry_document(&self) -> Bytes;

    /// 按构建产物路径取资源；产物中没有该文件时返回 `None`（是否回退到入口文档由静态面的解析顺序决定）。
    ///
    /// 返回 [`Bytes`] 而不是 `Vec<u8>`：嵌入实现可以走 `Bytes::from_static`（零拷贝），从磁盘读的
    /// 开发形态只是多一次拷贝，调用方无需区分两者。
    fn asset(&self, path: &str) -> Option<Bytes>;
}

/// 静态端点的运行时状态：注册的 path、资源源、协议族保留前缀与默认响应头。
///
/// 由 `route::RouteTable` 在组装 router 时构造并放进 fallback（[`Arc`] 共享）。
pub(crate) struct StaticEndpoint {
    /// 注册的静态 path（生产形态是根路径）：该 path 上的读取方法固定返回入口文档。
    path: String,
    source: Arc<dyn UiAssetSource>,
    /// 协议族保留前缀：它们之下（含前缀本身）的路径即使在静态面上也不得接管。
    reserved_prefixes: BTreeSet<String>,
    /// 该 path 的默认响应头：与 `register_post` 的每路径默认头**同一条缝**
    /// （[`apply_default_headers`]），因此本 path 上由接入层在调用处理器前产生的响应（Host 边界的 400）
    /// 也带同一组头。处理器自己的响应同样补齐它们。
    default_headers: PathDefaultHeaders,
}

impl StaticEndpoint {
    /// 组装一条静态端点（path 与保留前缀由路由表给出）。
    pub(crate) fn new(
        path: String,
        source: Arc<dyn UiAssetSource>,
        reserved_prefixes: BTreeSet<String>,
        default_headers: PathDefaultHeaders,
    ) -> Self {
        Self {
            path,
            source,
            reserved_prefixes,
            default_headers,
        }
    }

    /// path 是否落在协议族保留前缀之下（含前缀本身）。
    fn is_reserved(&self, path: &str) -> bool {
        self.reserved_prefixes
            .iter()
            .any(|prefix| falls_under(path, prefix))
    }
}

/// 一条落在静态面上的请求的处置结果。
enum Resolution {
    /// 返回这份字节：构建产物里的资源，或入口文档（单页回退）。
    Serve {
        /// 响应体。
        body: Bytes,
        /// 响应内容类型。
        content_type: &'static str,
    },
    /// 明确拒绝：协议族保留前缀之下的未注册路径（接入层既有语义）。
    NotFound,
    /// 明确拒绝：静态语义下的非读取方法。
    MethodNotAllowed,
}

/// 静态面的处理器（router 的 fallback 入口）。
///
/// 非 `async`：全部判定都是同步的（资源源接口是同步的），异步只是 fallback 的形状要求。
pub(crate) fn serve(endpoint: Arc<StaticEndpoint>, request: Request) -> Response {
    let head_only = *request.method() == Method::HEAD;
    let readable = head_only || *request.method() == Method::GET;
    let mut response = match resolve(&endpoint, request.uri().path(), readable) {
        // 保留前缀的 404 是**协议面**的既有语义（与未注册 path 的 404 完全同形，也不带静态面的头）：
        // 给协议族 path 贴静态面的响应头会把两个面的边界搅在一起，且没有安全收益。
        Resolution::NotFound => return StatusCode::NOT_FOUND.into_response(),
        Resolution::MethodNotAllowed => method_not_allowed(),
        Resolution::Serve { body, content_type } => content_response(content_type, body, head_only),
    };
    apply_default_headers(response.headers_mut(), &endpoint.default_headers);
    response
}

/// 判定顺序的唯一实现（`serve` 不再做第二个判定）。
fn resolve(endpoint: &StaticEndpoint, path: &str, readable: bool) -> Resolution {
    if endpoint.is_reserved(path) {
        return Resolution::NotFound;
    }
    // 非读取方法先于资源命中判定：即使该 path 真有资源，也**不**用「返回入口文档」冒充成功。
    if !readable {
        return Resolution::MethodNotAllowed;
    }
    if path == endpoint.path.as_str() {
        return entry_document(endpoint);
    }
    match endpoint.source.asset(path) {
        Some(body) => Resolution::Serve {
            body,
            content_type: content_type_for_path(path),
        },
        None => entry_document(endpoint),
    }
}

/// 入口文档的处置（根路径与单页回退共用同一份）。
fn entry_document(endpoint: &StaticEndpoint) -> Resolution {
    Resolution::Serve {
        body: endpoint.source.entry_document(),
        content_type: ENTRY_CONTENT_TYPE,
    }
}

/// 读取方法的响应。
///
/// `HEAD` 只回头部：本层不生成 HEAD 正文，`content-length` 因此为 `0`。浏览器用 HEAD 做存在性与类型
/// 探测，而静态响应一律带 `no-store`，不需要用 HEAD 携带真实长度做缓存元数据。
fn content_response(content_type: &'static str, body: Bytes, head_only: bool) -> Response {
    let body = if head_only {
        Body::empty()
    } else {
        Body::from(body)
    };
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .body(body)
        // 内容类型来自本模块的常量表（有单测锁定每个取值都能解析成头值），构造失败在这里不可能发生；
        // 真发生时失败关闭（500），而不是返回一个类型不明的响应。
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

/// 非读取方法的明确拒绝：`405` + `Allow: GET, HEAD`。
///
/// 比 404 准确——该 path 在静态面上是有资源的，只是方法不被接受；而返回 200 入口文档会让拼错方法
/// （例如用 `POST` 探测）看起来像成功。
fn method_not_allowed() -> Response {
    let mut response = StatusCode::METHOD_NOT_ALLOWED.into_response();
    response
        .headers_mut()
        .insert(header::ALLOW, HeaderValue::from_static(ALLOWED_METHODS));
    response
}

/// path 是否落在 `prefix` 之下（含 `prefix` 本身）。
///
/// 按**段**判定：`/sync/v1extra` 不属于 `/sync/v1`——否则将来出现 `/sync/v10` 这类协议族时会被误拒。
fn falls_under(path: &str, prefix: &str) -> bool {
    path.strip_prefix(prefix)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

/// 一条已注册协议 path 所属的协议族前缀：取前两个 path 段，段数不足时取全部。
///
/// 为什么是前两段：路由表里的协议 path 全部在**第 2 段带版本号**（`/sync/v1`、`/node-link/v1`），
/// 因此前两段正是版本化协议族的边界——`/node-link/v1` 与 `/sync/v1/pairing/claim` 分别归约到
/// `/node-link/v1` 与 `/sync/v1`。取更少会把整棵 `/sync`、`/node-link` 树都保留（会误拒未来的
/// 非版本化 path）；取更多（例如前 3 段）会让 `/sync/v1/pairing/other` 这类未注册子路径重新掉进静态回退。
/// 这条规则成立的前提是两个协议族都带版本段：将来新增**不带**版本段的协议 path 时，此函数必须一并调整。
pub(crate) fn protocol_prefix(path: &str) -> String {
    let mut segments = path.split('/').filter(|segment| !segment.is_empty());
    match (segments.next(), segments.next()) {
        (Some(first), Some(second)) => format!("/{first}/{second}"),
        (Some(first), None) => format!("/{first}"),
        // `validate_path` 要求 path 以 `/` 开头，因此这里只可能是 `/` 本身（无段）：整条保留。
        _ => path.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_prefix_reduces_each_registered_path_to_its_family() {
        // 协议冻结的 path（`SYNC_PROTOCOL.md` §3、`NODE_LINK_PROTOCOL.md` §2.1）都归约到「版本段」为止。
        for (path, expected) in [
            ("/sync/v1", "/sync/v1"),
            ("/sync/v1/pairing/claim", "/sync/v1"),
            ("/sync/v1/pairing/status", "/sync/v1"),
            ("/node-link/v1", "/node-link/v1"),
            ("/node-link/v1/pairing/claim", "/node-link/v1"),
        ] {
            assert_eq!(protocol_prefix(path), expected, "{path}");
        }
        // 段数不足时取全部：单段 path 的协议族就是它自己。
        assert_eq!(protocol_prefix("/single"), "/single");
        assert_eq!(protocol_prefix("/"), "/");
    }

    #[test]
    fn reserved_matching_is_segment_wise() {
        let endpoint = StaticEndpoint::new(
            "/".to_owned(),
            Arc::new(EmptySource),
            BTreeSet::from(["/sync/v1".to_owned()]),
            Vec::new(),
        );
        // 前缀本身与它之下的子路径都保留（未注册子路径不得掉进静态回退）。
        assert!(endpoint.is_reserved("/sync/v1"));
        assert!(endpoint.is_reserved("/sync/v1/pairing/other"));
        assert!(endpoint.is_reserved("/sync/v1/"));
        // 只共享字符串前缀不是「之下」：否则未来的 `/sync/v10` 会被误拒。
        assert!(!endpoint.is_reserved("/sync/v1extra"));
        assert!(!endpoint.is_reserved("/sync"));
        assert!(!endpoint.is_reserved("/"));
    }

    #[test]
    fn content_types_are_extension_wise_and_never_html_by_default() {
        assert_eq!(content_type_for_path("/index.html"), ENTRY_CONTENT_TYPE);
        assert_eq!(
            content_type_for_path("/_expo/static/js/web/entry-abc.js"),
            "text/javascript; charset=utf-8"
        );
        assert_eq!(
            content_type_for_path("/a/b.MJS"),
            "text/javascript; charset=utf-8"
        );
        assert_eq!(
            content_type_for_path("/assets/app.css"),
            "text/css; charset=utf-8"
        );
        assert_eq!(content_type_for_path("/assets/logo.svg"), "image/svg+xml");
        assert_eq!(content_type_for_path("/assets/font.woff2"), "font/woff2");
        assert_eq!(
            content_type_for_path("/_expo/static/js/web/entry-abc.js.map"),
            "application/json; charset=utf-8"
        );
        // 未知扩展名：通用二进制，绝不回落 `text/html`（否则任意字节会被当页面渲染）。
        assert_eq!(
            content_type_for_path("/assets/blob.dat"),
            FALLBACK_CONTENT_TYPE
        );
        assert_eq!(content_type_for_path("/pair"), FALLBACK_CONTENT_TYPE);
        assert_ne!(content_type_for_path("/pair"), ENTRY_CONTENT_TYPE);
        // 目录名里的点不构成扩展名。
        assert_eq!(content_type_for_path("/a.b/c"), FALLBACK_CONTENT_TYPE);
    }

    #[test]
    fn mime_table_entries_are_well_formed() {
        // 表值会被构造成响应头（`content_response` 的 builder）：不可解析的值必须在门禁里失败，而不是
        // 到响应期才回落 500。
        // `MIME_TYPES` 的元素是 `(&str, &str)`，按引用迭代会得到 `&(&str, &str)`；这里解构成 `&str`
        // 让 `is_empty`/`bytes`/`parse` 落在 `&str` 上（与下面 `.iter().any` 的 `*extension` 字型层次一致）。
        for &(extension, content_type) in MIME_TYPES {
            // `str` 上**没有** `is_ascii_lowercase`（它只在 `u8`/`char`/`[u8]` 上，`str` 只有
            // `to_ascii_lowercase`），因此按字节判定。判据是「小写字母或数字」而不是「纯小写字母」：
            // 真实扩展名带数字（表里的 `woff2` 就是），把它判成非法会让用例去否定正确的表值。
            assert!(
                !extension.is_empty()
                    && extension
                        .bytes()
                        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit()),
                "扩展名必须非空且只用小写字母与数字：{extension}"
            );
            assert!(
                content_type.parse::<HeaderValue>().is_ok(),
                "内容类型不是合法头值：{content_type}"
            );
        }
        assert!(
            MIME_TYPES
                .iter()
                .any(|(extension, content_type)| *extension == "html"
                    && *content_type == ENTRY_CONTENT_TYPE),
            "html 与入口文档必须共用同一个取值"
        );
        assert!(FALLBACK_CONTENT_TYPE.parse::<HeaderValue>().is_ok());
    }

    #[test]
    fn csp_baseline_is_exactly_the_documented_directives() {
        // 逐条对上基线（不是「包含某个子串就过」）。
        let directives: Vec<&str> = CONTENT_SECURITY_POLICY.split("; ").collect();
        assert_eq!(
            directives,
            [
                "default-src 'self'",
                "script-src 'self'",
                "connect-src 'self'",
                "object-src 'none'",
                "base-uri 'none'",
                "frame-ancestors 'none'",
            ]
        );
        // 基线本身不得含放宽项：内联/eval、任意来源、明文或任意外部来源。
        for forbidden in [
            "unsafe-inline",
            "unsafe-eval",
            "*",
            "http:",
            "data:",
            "https:",
        ] {
            assert!(
                !CONTENT_SECURITY_POLICY.contains(forbidden),
                "基线不得含 {forbidden}"
            );
        }
        assert_eq!(
            STATIC_HEADERS[0],
            ("content-security-policy", CONTENT_SECURITY_POLICY)
        );
        // 四个既有安全头都在，且取值与配对端点声明的一致。
        for (name, value) in [
            ("cache-control", "no-store"),
            ("pragma", "no-cache"),
            ("referrer-policy", "no-referrer"),
            ("x-content-type-options", "nosniff"),
        ] {
            assert!(
                STATIC_HEADERS.contains(&(name, value)),
                "缺 {name}: {value}"
            );
        }
    }

    #[test]
    fn resolution_order_puts_protocol_rejection_before_readability() {
        let endpoint = StaticEndpoint::new(
            "/".to_owned(),
            Arc::new(EmptySource),
            BTreeSet::from(["/sync/v1".to_owned()]),
            Vec::new(),
        );
        // 保留前缀下的**非读取**方法也是 404（协议面语义），不是 405。
        assert!(matches!(
            resolve(&endpoint, "/sync/v1/pairing/other", false),
            Resolution::NotFound
        ));
        assert!(matches!(
            resolve(&endpoint, "/sync/v1/pairing/other", true),
            Resolution::NotFound
        ));
        // 静态语义下的非读取方法：405。
        assert!(matches!(
            resolve(&endpoint, "/pair", false),
            Resolution::MethodNotAllowed
        ));
        // 根路径与未命中资源都回入口文档。
        assert!(matches!(
            resolve(&endpoint, "/", true),
            Resolution::Serve { .. }
        ));
        assert!(matches!(
            resolve(&endpoint, "/pair", true),
            Resolution::Serve { .. }
        ));
    }

    /// 只有入口文档的资源源（本模块单测不关心具体资源）。
    struct EmptySource;

    impl UiAssetSource for EmptySource {
        fn entry_document(&self) -> Bytes {
            Bytes::from_static(b"<html></html>")
        }

        fn asset(&self, _path: &str) -> Option<Bytes> {
            None
        }
    }
}
