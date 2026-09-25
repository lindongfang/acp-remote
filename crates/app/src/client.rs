//! 本地管理通道的客户端（`docs/LOCAL_ADMIN_PROTOCOL.md` §3/§4）。
//!
//! WP4b 的 CLI 子命令与集成测试共用本模块：**不复制**信封定义（响应解码直接用
//! `server::local_admin::AdminResponse`），只负责三件传输层的事：
//!
//! 1. 按 endpoint 定位串连接（Windows Named Pipe / Unix socket）；
//! 2. 按 §3 组帧（`u32be length | channel 0x01 | payload`）——`server` 侧的 `encode_frame` 是
//!    `pub(crate)`，客户端因此在这里独立实现**同一**规则（唯一的一处重复，且只有三行）；
//! 3. 读回一帧并交给 `AdminResponse::decode`（长度前缀与 channel 校验复用 `FrameReader`）。
//!
//! 约束（`cli-commands` 规格）：CLI **不**自动重试 mutation、不直连 SQLite、不启动第二套核心；连接失败
//! 就是失败，由调用方报错退出。

use std::io;

use serde_json::Value;
use server::local_admin::{
    AdminResponse, CHANNEL_VERSION, JsonObject, Method, RequestId, decode_response,
};
use server::transport::local::{CHANNEL_LOCAL_ADMIN_BYTE, FrameReader};
use tokio::io::AsyncWriteExt as _;
use uuid::Uuid;

/// 客户端失败。全部是「连接/传输/协议」级失败：调用方据此以非零退出码结束。
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    /// 无法连接 endpoint（Daemon 未运行、正在关闭或没有管理通道）。
    #[error("连接本地管理通道失败（{source}）")]
    Connect {
        /// 底层错误。
        #[source]
        source: io::Error,
    },
    /// 连接已建立但写入或读取失败。
    #[error("本地管理通道传输失败（{source}）")]
    Transport {
        /// 底层错误。
        #[source]
        source: io::Error,
    },
    /// 对端在响应前关闭连接（例如 Daemon 正在关闭）。
    #[error("本地管理通道在返回响应前关闭")]
    Closed,
    /// 帧层失败（channel 未知、帧超长等）。
    #[error("本地管理通道帧非法（{detail}）")]
    Frame {
        /// 简短说明（不含 payload）。
        detail: String,
    },
    /// 响应信封非法。
    #[error("本地管理响应非法（{detail}）")]
    Response {
        /// 简短说明。
        detail: String,
    },
}

/// 与 Daemon 的一条本地管理连接（一次连接可发多条请求，响应按 `id` 关联）。
pub struct LocalAdminClient {
    stream: ClientStream,
    frames: FrameReader,
    endpoint: String,
}

impl LocalAdminClient {
    /// 连接 endpoint 定位串（锁文件的 `endpoint` 字段）。
    pub async fn connect(endpoint: &str) -> Result<Self, ClientError> {
        let stream = open_stream(endpoint).await?;
        Ok(Self {
            stream,
            frames: FrameReader::new(),
            endpoint: endpoint.to_owned(),
        })
    }

    /// endpoint 定位串。
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// 发一条管理请求并读回**唯一**响应（§4 规则 5）。
    pub async fn call(
        &mut self,
        method: Method,
        params: JsonObject,
    ) -> Result<AdminResponse, ClientError> {
        let id = RequestId::parse(&Uuid::new_v4().hyphenated().to_string()).ok_or_else(|| {
            ClientError::Response {
                detail: "generated request id is not canonical".to_owned(),
            }
        })?;
        let payload = request_payload(&id, method, params)?;
        self.send_frame(&payload).await?;
        self.read_response(&id).await
    }

    /// 发送一段已编码的请求载荷（测试与 `acp-stdio` 之外的调用方用它复用同一条连接）。
    pub async fn send_frame(&mut self, payload: &[u8]) -> Result<(), ClientError> {
        let length = u32::try_from(payload.len() + 1).map_err(|_| ClientError::Frame {
            detail: "request payload is too large".to_owned(),
        })?;
        let mut frame = Vec::with_capacity(4 + payload.len() + 1);
        frame.extend_from_slice(&length.to_be_bytes());
        frame.push(CHANNEL_LOCAL_ADMIN_BYTE);
        frame.extend_from_slice(payload);
        self.stream
            .write_all(&frame)
            .await
            .map_err(|source| ClientError::Transport { source })?;
        self.stream
            .flush()
            .await
            .map_err(|source| ClientError::Transport { source })
    }

    /// 读回一条响应并校验它关联的 `id`。
    async fn read_response(&mut self, expected: &RequestId) -> Result<AdminResponse, ClientError> {
        let frame = match self.frames.next_frame(&mut self.stream).await {
            Ok(Some(frame)) => frame,
            Ok(None) => return Err(ClientError::Closed),
            Err(error) => {
                return Err(match error {
                    server::transport::local::FrameError::Io(source) => {
                        ClientError::Transport { source }
                    }
                    other => ClientError::Frame {
                        detail: other.to_string(),
                    },
                });
            }
        };
        if frame.channel != server::transport::local::ChannelKind::LocalAdmin {
            return Err(ClientError::Frame {
                detail: format!("unexpected channel {}", frame.channel.as_str()),
            });
        }
        let response = decode_response(&frame.body).map_err(|error| ClientError::Response {
            detail: format!("{error:?}"),
        })?;
        if response.id() != expected {
            return Err(ClientError::Response {
                detail: "response id does not match the request id".to_owned(),
            });
        }
        Ok(response)
    }
}

impl std::fmt::Debug for LocalAdminClient {
    /// 不打印 endpoint 内容（可能含内网主机名或用户名派生片段）与连接句柄。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LocalAdminClient")
            .field("connected", &true)
            .finish_non_exhaustive()
    }
}

/// 一次性调用：连接 → 一条请求 → 关闭（CLI 子命令的常规形态）。
pub async fn call_once(
    endpoint: &str,
    method: Method,
    params: JsonObject,
) -> Result<AdminResponse, ClientError> {
    let mut client = LocalAdminClient::connect(endpoint).await?;
    client.call(method, params).await
}

/// 响应的两种读法（CLI 与测试都只需这两种；`result` 是开放容器，保持原样不解读语义）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientOutcome {
    /// `ok = true`。
    Success(JsonObject),
    /// `ok = false`：本地错误码（§6 词表）与消息。
    Failure {
        /// 错误码文本（`local.*`）。
        code: String,
        /// 简短英文消息（**不是**稳定契约）。
        message: String,
    },
}

impl ClientOutcome {
    /// 是否成功。
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Success(_))
    }
}

/// 拆解一条响应（不解读 `result` 的业务语义）。
pub fn outcome_of(response: &AdminResponse) -> ClientOutcome {
    match response.outcome() {
        server::local_admin::AdminOutcome::Success { result } => {
            ClientOutcome::Success(result.clone())
        }
        server::local_admin::AdminOutcome::Failure { error } => ClientOutcome::Failure {
            code: error.code().as_str().to_owned(),
            message: error.message().to_owned(),
        },
    }
}

/// 按 §4 的信封组请求载荷：`{ v, id, method, params }`，closed object。
///
/// 请求方向的编码是本 crate 唯一一处构造信封的地方；响应方向复用 `server::local_admin` 的类型。
pub fn request_payload(
    id: &RequestId,
    method: Method,
    params: JsonObject,
) -> Result<Vec<u8>, ClientError> {
    let mut object = serde_json::Map::with_capacity(4);
    object.insert("v".to_owned(), Value::from(CHANNEL_VERSION));
    object.insert("id".to_owned(), Value::from(id.as_str()));
    object.insert("method".to_owned(), Value::from(method.as_str()));
    object.insert("params".to_owned(), Value::Object(params));
    serde_json::to_vec(&Value::Object(object)).map_err(|error| ClientError::Response {
        detail: error.to_string(),
    })
}

/// 便捷构造：无参数的请求（`params = {}`）。
pub fn no_params() -> JsonObject {
    JsonObject::new()
}

/// 请求编码后的信封（测试与 CLI 的参数构造）可直接交给 [`LocalAdminClient::send_frame`]。
pub fn encoded_request(
    id: &str,
    method: Method,
    params: JsonObject,
) -> Result<Vec<u8>, ClientError> {
    let id = RequestId::parse(id).ok_or_else(|| ClientError::Response {
        detail: "request id is not a canonical uuid".to_owned(),
    })?;
    request_payload(&id, method, params)
}

/// 连接 endpoint：Windows 的 `\\.\pipe\...` 走 Named Pipe 客户端，其余按 Unix socket 路径。
async fn open_stream(endpoint: &str) -> Result<ClientStream, ClientError> {
    #[cfg(windows)]
    {
        tokio::net::windows::named_pipe::ClientOptions::new()
            .open(endpoint)
            .map_err(|source| ClientError::Connect { source })
    }
    #[cfg(unix)]
    {
        tokio::net::UnixStream::connect(endpoint)
            .await
            .map_err(|source| ClientError::Connect { source })
    }
}

#[cfg(windows)]
type ClientStream = tokio::net::windows::named_pipe::NamedPipeClient;
#[cfg(unix)]
type ClientStream = tokio::net::UnixStream;

#[cfg(test)]
mod tests {
    use super::*;
    use server::local_admin::AdminRequest;

    /// 请求载荷的字段集合与拼写是 §4 的 closed object（顺序不构成契约）。
    #[test]
    fn the_request_payload_matches_the_envelope() {
        let id = RequestId::parse("2ae1c07c-0000-4000-8000-000000000001").expect("id");
        let mut params = JsonObject::new();
        params.insert("graceMs".to_owned(), Value::Null);
        let payload = request_payload(&id, Method::DaemonStop, params).expect("可编码");
        let parsed: Value = serde_json::from_slice(&payload).expect("合法 JSON");
        assert_eq!(parsed["v"], Value::from(CHANNEL_VERSION));
        assert_eq!(
            parsed["id"],
            Value::from("2ae1c07c-0000-4000-8000-000000000001")
        );
        assert_eq!(parsed["method"], Value::from("daemon.stop"));
        assert_eq!(parsed["params"]["graceMs"], Value::Null);
        assert_eq!(
            parsed.as_object().expect("object").len(),
            4,
            "closed object：只有 v/id/method/params"
        );
    }

    #[test]
    fn a_non_canonical_request_id_is_rejected() {
        assert!(matches!(
            encoded_request(
                "2AE1C07C-0000-4000-8000-000000000001",
                Method::DaemonStatus,
                no_params()
            ),
            Err(ClientError::Response { .. })
        ));
        assert!(
            encoded_request(
                "2ae1c07c-0000-4000-8000-000000000001",
                Method::DaemonStatus,
                no_params()
            )
            .is_ok()
        );
    }

    /// 客户端与 Daemon 之间的组帧互为逆：服务端读取器的形状即客户端写出的形状（§3）。
    #[tokio::test]
    async fn the_client_frame_is_readable_by_the_server_reader() {
        let id = RequestId::parse("2ae1c07c-0000-4000-8000-000000000001").expect("id");
        let payload = request_payload(&id, Method::DaemonStatus, no_params()).expect("可编码");
        let length = u32::try_from(payload.len() + 1).expect("长度");
        let mut frame = Vec::new();
        frame.extend_from_slice(&length.to_be_bytes());
        frame.push(CHANNEL_LOCAL_ADMIN_BYTE);
        frame.extend_from_slice(&payload);

        let (mut client, mut server) = tokio::io::duplex(4096);
        client.write_all(&frame).await.expect("写出");
        drop(client);
        let mut reader = FrameReader::new();
        let read = reader
            .next_frame(&mut server)
            .await
            .expect("可读")
            .expect("一帧");
        assert_eq!(
            read.channel,
            server::transport::local::ChannelKind::LocalAdmin
        );
        assert_eq!(read.body, payload);
        let request: AdminRequest =
            server::local_admin::decode_request(&read.body).expect("信封合法");
        assert_eq!(request.method(), Method::DaemonStatus);
        assert_eq!(request.id(), &id);
    }
}
