//! 交给上层处理器的请求/响应值对象与连接元数据。
//!
//! 这些类型只承载**接入层事实**（方法、path、头、受限请求体、对端地址、TLS 归属、关闭信号），不含
//! 任何 Node Link 语义：`server::node_link` 在处理器内部完成 schema/授权判定，`server::transport::net`
//! 只保证「请求体在上限内」「Host/代理头边界已经过」「path 已注册」这三件事。

use std::net::IpAddr;
use std::net::SocketAddr;

use axum::body::Body;
use axum::body::Bytes;
use axum::http::HeaderMap;
use axum::http::HeaderName;
use axum::http::HeaderValue;
use axum::http::Method;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::response::Response;

use crate::transport::net::shutdown::Shutdown;

/// 连接级元数据：处理器据此限流、审计与取消。
#[derive(Debug, Clone)]
pub struct PeerInfo {
    /// TCP 对端地址（未经任何头部改写）。
    pub peer_addr: SocketAddr,
    /// 用于限流与日志的对端地址；仅当 `peer_addr` 在 `daemon.trusted_proxies` 内且转发头可用时才取自头部。
    pub client_ip: IpAddr,
    /// `client_ip` 是否来自被采信的转发头。
    pub forwarded_headers: bool,
    /// 连接是否已被本进程终止 TLS（`daemon.tls.mode = "direct"`）。
    pub tls_terminated: bool,
    /// 接入层的关闭信号：处理器可据此结束会话（所有异步任务都要有取消路径）。
    pub shutdown: Shutdown,
}

/// 一条配对 HTTP 请求。
#[derive(Debug)]
pub struct HttpRequest {
    /// 请求方法。
    pub method: Method,
    /// 请求 path（不含 query）。
    pub path: String,
    /// 请求头。
    pub headers: HeaderMap,
    /// 请求体；已按 [`crate::transport::net::NetConfig::max_body_bytes`] 限制（超限在调用处理器前返回 413）。
    pub body: Bytes,
    /// 连接元数据。
    pub peer: PeerInfo,
}

/// 上层的 HTTP 响应。
#[derive(Debug)]
pub struct HttpResponse {
    /// 状态码。
    pub status: StatusCode,
    /// 响应头。
    pub headers: HeaderMap,
    /// 响应体。
    pub body: Bytes,
}

impl HttpResponse {
    /// 空体响应。
    pub fn new(status: StatusCode) -> Self {
        Self {
            status,
            headers: HeaderMap::new(),
            body: Bytes::new(),
        }
    }

    /// `application/json` 响应（配对 HTTP 的唯一成功形态）。
    pub fn json(status: StatusCode, body: Bytes) -> Self {
        Self::new(status)
            .with_header(
                HeaderName::from_static("content-type"),
                HeaderValue::from_static("application/json"),
            )
            .with_body(body)
    }

    /// 追加一个响应头。
    pub fn with_header(mut self, name: HeaderName, value: HeaderValue) -> Self {
        self.headers.append(name, value);
        self
    }

    /// 设置响应体。
    pub fn with_body(mut self, body: impl Into<Bytes>) -> Self {
        self.body = body.into();
        self
    }
}

impl IntoResponse for HttpResponse {
    fn into_response(self) -> Response {
        let mut response = Response::new(Body::from(self.body));
        *response.status_mut() = self.status;
        *response.headers_mut() = self.headers;
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_response_sets_content_type_and_status() {
        let response = HttpResponse::json(StatusCode::CREATED, Bytes::from_static(b"{}"));
        assert_eq!(response.status, StatusCode::CREATED);
        assert_eq!(
            response.headers.get("content-type").expect("已设置"),
            "application/json"
        );
        assert_eq!(response.body, Bytes::from_static(b"{}"));
    }

    #[test]
    fn headers_are_appended_not_replaced() {
        let response = HttpResponse::new(StatusCode::OK)
            .with_header(
                HeaderName::from_static("cache-control"),
                HeaderValue::from_static("no-store"),
            )
            .with_header(
                HeaderName::from_static("cache-control"),
                HeaderValue::from_static("no-cache"),
            );
        assert_eq!(response.headers.get_all("cache-control").iter().count(), 2);
    }

    #[test]
    fn conversion_keeps_status_and_body() {
        let response = HttpResponse::new(StatusCode::NOT_FOUND)
            .with_body(Bytes::from_static(b"x"))
            .into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
