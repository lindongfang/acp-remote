//! WebSocket 连接值对象与上层的会话处理器（`NODE_LINK_PROTOCOL.md` §2.1、§2.5）。
//!
//! [`WsConnection`] 只做帧级工作：把 text/binary 帧原样交给上层（是否拒绝 binary 由 `server::node_link`
//! 按 §2.1 决定），按 `max_message_bytes` 在**分配大缓冲前**以 `1009` 拒绝超限消息，并提供 `close(code, reason)`。
//!
//! 一条连接可以经 [`WsConnection::split`] 拆成 [`WsReader`] 与 [`WsWriter`] 两半，**读半边只判定、写半边
//! 只动作**。拆分的动机在会话侧（`node_link::conn::session`）：写路径要有期限、且不能让心跳/静默/慢消费者
//! 看门狗跟着写一起停摆，因此写半边必须能交给一个独立的写任务；共享一个 `WebSocket` 的另一条路是
//! `Arc<Mutex<WebSocket>>`，那会让 `recv` 与写互相阻塞，比要修的缺口更糟。超限就是这条分离的直接后果：
//! [`WsReader::recv`] 仍**判定**超限（返回 [`WsError::MessageTooLarge`]），但 1009 close 帧改由调用方经
//! [`WsWriter::close`] 发出（[`MESSAGE_TOO_LARGE_CLOSE`] 给出 code 与原因）——读半边没有写能力，而
//! 「超限按 1009 关闭」是既有契约。未拆分的 [`WsConnection::recv`] 保留原有的「自己发 1009」行为。
//!
//! 「分配前拒绝」由底层 tungstenite 的 `max_frame_size` 保证：它解析帧头后就拒绝，早于任何 payload 分配
//! （升级响应在 [`crate::transport::net::route`] 里把 `max_frame_size`/`max_message_size` 设为同一上限）。
//! 该错误从 `axum` 冒泡出来时只带文本，因此判定集中在 [`message_too_large`] 一处，并有回归用例固定
//! 依赖的文本形态——一旦上游措辞变化，用例会失败，而不是静默把 1009 降级成「无 code 的断开」。

use std::sync::Arc;

use axum::Error;
use axum::body::Bytes;
use axum::extract::ws::CloseFrame;
use axum::extract::ws::Message;
use axum::extract::ws::Utf8Bytes;
use axum::extract::ws::WebSocket;
use axum::extract::ws::close_code;
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt as _, StreamExt as _};

/// 超限关闭时随 `1009` 给出的原因（协议未规定文本，这里只用于诊断，不含任何业务数据）。
const MESSAGE_TOO_LARGE_REASON: &str = "message too large";

/// 超限消息的关闭参数（`1009` + 原因）。
///
/// [`WsReader::recv`] 只判定超限，close 帧由调用方发出（读半边没有写能力）：调用方拿本常量调用
/// [`WsWriter::close`]，与未拆分连接里 [`WsConnection::recv`] 自己发的那一帧同值。
pub const MESSAGE_TOO_LARGE_CLOSE: (u16, &str) = (close_code::SIZE, MESSAGE_TOO_LARGE_REASON);

/// tungstenite 对「帧/消息超过配置上限」的错误文本片段（`CapacityError::MessageTooLong`，0.29）。
///
/// 完整形态是 `Space limit exceeded: Message too long: {大小} > {上限}`（外层是 `Error::Capacity` 的
/// 前缀），因此取最后一次出现之后的部分再解析，不依赖外层前缀的措辞。
const MESSAGE_TOO_LONG_MARKER: &str = "Message too long: ";

/// 上层收到的帧。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WsMessage {
    /// 文本帧（UTF-8 已校验）。
    Text(String),
    /// binary 帧原样上送（是否拒绝见 `NODE_LINK_PROTOCOL.md` §2.1，由 `server::node_link` 判定）。
    Binary(Bytes),
}

/// 帧级错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WsError {
    /// 单条消息超过 `max_message_bytes`：这条连接必须按 `1009` 关闭。
    ///
    /// 未拆分的 [`WsConnection::recv`] 自己发这一帧；拆分后的 [`WsReader::recv`] 只判定，close 帧由
    /// 调用方经 [`WsWriter::close`] 发出（参数见 [`MESSAGE_TOO_LARGE_CLOSE`]）。
    MessageTooLarge,
    /// 传输/协议层错误；文本只用于诊断，不含业务数据。
    Transport(String),
}

impl std::fmt::Display for WsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MessageTooLarge => f.write_str("WebSocket 消息超过单条上限（已按 1009 关闭）"),
            Self::Transport(detail) => write!(f, "WebSocket 传输错误：{detail}"),
        }
    }
}

impl std::error::Error for WsError {}

/// 上层的连接处理器：由 `server::node_link` 实现并注入
/// （[`crate::transport::net::NetListener::register_ws`]）。
///
/// 实现者拥有连接的全部协议语义（握手、信封、心跳、发送队列）；本模块只保证连接已升级、已通过
/// Host/代理头边界、已取到请求来源，并把 `max_message_bytes` 的上限与关闭信号一并交给它。处理器返回即
/// 会话结束。
///
/// [`handle`](WsHandler::handle) 的 `origin` 是升级请求里**已取的来源**（`docs/SYNC_PROTOCOL.md` §3.2 的
/// canonical origin，按其归一化口径取值）：本层只如实取得，**不**判定它与设备记录中的绑定是否一致
/// （那是 `identity-auth` 绑定检查的职责），也不把来源与 [`crate::transport::net::PeerInfo`] 混在一起
/// （后者的语义是对端网络地址与转发头的可信度）。不依赖来源的端点（如 Node Link）忽略该参数即可。
///
/// 接收者是 `self: Arc<Self>` 而不是 `&self`：会话 future 必须能被接入选层放进 `'static` 的任务集
/// （`axum` 的升级回调自身是 `tokio::spawn` 出来的），借用式接收者会让会话 future 绑在栈上。
#[async_trait::async_trait]
pub trait WsHandler: Send + Sync + 'static {
    /// 处理一条已升级的连接。
    ///
    /// `origin` 为 [`crate::transport::net::RequestOrigin::Absent`] 表示升级请求没有 `Origin` 头
    /// （非浏览器原生客户端）：按 §3.2，transcript 的来源字段此时取**空字节串**，授权仍按设备与 `Host`
    /// 身份判定，不得因此降级为无认证。
    async fn handle(
        self: Arc<Self>,
        connection: WsConnection,
        peer: crate::transport::net::PeerInfo,
        origin: crate::transport::net::RequestOrigin,
    );
}

/// 一条已升级的 WebSocket 连接。
pub struct WsConnection {
    inner: WebSocket,
    max_message_bytes: usize,
}

impl WsConnection {
    /// 由路由层构造；`max_message_bytes` 必须与升级响应上的帧/消息上限一致（[`crate::transport::net::route`] 负责）。
    pub(crate) fn new(inner: WebSocket, max_message_bytes: usize) -> Self {
        Self {
            inner,
            max_message_bytes,
        }
    }

    /// 协商成功的 subprotocol（未协商时 `None`）。
    pub fn negotiated_subprotocol(&self) -> Option<&str> {
        self.inner.protocol().and_then(|value| value.to_str().ok())
    }

    /// 接收下一条帧。
    ///
    /// 返回 `None` 表示连接已结束（对端关闭或流终止）；返回 `Some(Err(WsError::MessageTooLarge))` 表示
    /// 对端发来的消息超过上限，连接**已由本方法**按 `1009` 关闭。ping/pong 由底层自动应答，不上交。
    ///
    /// 拆分后的读半边是 [`WsReader`]，它只判定超限、不自己发 1009（见 [`WsReader::recv`]）。
    pub async fn recv(&mut self) -> Option<Result<WsMessage, WsError>> {
        match read_frame(&mut self.inner, self.max_message_bytes).await {
            FrameRead::Message(message) => Some(Ok(message)),
            FrameRead::Closed => None,
            FrameRead::TooLarge => {
                let (code, reason) = MESSAGE_TOO_LARGE_CLOSE;
                self.send_close(code, reason).await;
                Some(Err(WsError::MessageTooLarge))
            }
            FrameRead::Transport(detail) => Some(Err(WsError::Transport(detail))),
        }
    }

    /// 发送一条文本帧。
    pub async fn send_text(&mut self, text: impl Into<String>) -> Result<(), WsError> {
        send_text_frame(&mut self.inner, text.into()).await
    }

    /// 发送一条 binary 帧。
    pub async fn send_binary(&mut self, bytes: Bytes) -> Result<(), WsError> {
        send_binary_frame(&mut self.inner, bytes).await
    }

    /// 以给定 close code 关闭连接（`code` 必须是协议允许的取值，例如 `1009`/`4400`/`4401`/`4408`/`4410`/`4429`）。
    pub async fn close(mut self, code: u16, reason: &str) {
        self.send_close(code, reason).await;
    }

    /// 发送 close 帧；发送失败表示对端已经断开，此时直接放弃（连接随 `self` 一起关闭）。
    async fn send_close(&mut self, code: u16, reason: &str) {
        send_close_frame(&mut self.inner, code, reason).await;
    }

    /// 把连接拆成读半边与写半边。
    ///
    /// 两半各自被单独 poll：写半边因此可以交给一个独立的写任务（`node_link::conn::session` 的每连接写
    /// 任务），读半边与判据留在会话任务里。`futures_util` 是本 crate 为这一形态新增的直接依赖：它只提供
    /// `StreamExt::split`（`Arc<Mutex<WebSocket>>` 的共享写法会让 `recv` 与写互相阻塞，见模块文档）。
    pub fn split(self) -> (WsReader, WsWriter) {
        let (sink, stream) = self.inner.split();
        (
            WsReader {
                inner: stream,
                max_message_bytes: self.max_message_bytes,
            },
            WsWriter { inner: sink },
        )
    }
}

/// 一条已拆分连接的**读半边**：只做帧级判定，不做任何关闭动作。
///
/// 帧级语义与 [`WsConnection::recv`] 逐条一致，唯一差别是超限时**只判定**（返回
/// [`WsError::MessageTooLarge`]）而**不**发 1009 close 帧——读半边没有写能力。关闭动作由调用方经
/// [`WsWriter::close`] 发出（[`MESSAGE_TOO_LARGE_CLOSE`] 给出 code 与原因）。
pub struct WsReader {
    inner: SplitStream<WebSocket>,
    max_message_bytes: usize,
}

impl WsReader {
    /// 接收下一条帧。
    ///
    /// 返回 `None` 表示连接已结束（对端关闭或流终止）；返回 `Some(Err(WsError::MessageTooLarge))` 表示
    /// 对端发来的消息超过上限（**本方法不发 close 帧**，调用方按 `1009` 关闭）；ping/pong 由底层自动
    /// 应答，不上交。
    pub async fn recv(&mut self) -> Option<Result<WsMessage, WsError>> {
        match read_frame(&mut self.inner, self.max_message_bytes).await {
            FrameRead::Message(message) => Some(Ok(message)),
            FrameRead::Closed => None,
            FrameRead::TooLarge => Some(Err(WsError::MessageTooLarge)),
            FrameRead::Transport(detail) => Some(Err(WsError::Transport(detail))),
        }
    }
}

/// 一条已拆分连接的**写半边**：只做动作（写帧/关闭），不做判定。
pub struct WsWriter {
    inner: SplitSink<WebSocket, Message>,
}

impl WsWriter {
    /// 发送一条文本帧。
    pub async fn send_text(&mut self, text: impl Into<String>) -> Result<(), WsError> {
        send_text_frame(&mut self.inner, text.into()).await
    }

    /// 发送 close 帧（`1009` 与协议级 close 都由调用方经这里发出）。
    ///
    /// 发送失败表示对端已经断开，此时直接放弃——连接随写半边一起关闭。
    pub async fn close(&mut self, code: u16, reason: &str) {
        send_close_frame(&mut self.inner, code, reason).await;
    }
}

/// 一帧的读结果。
///
/// 把「判定」与「动作」分开：超限（[`FrameRead::TooLarge`]）只描述对端违反了上限，是否发 `1009` 由
/// 调用方决定（未拆分连接自己发、拆分后的读半边交给写半边）。
enum FrameRead {
    Message(WsMessage),
    TooLarge,
    Closed,
    Transport(String),
}

/// 从流里取下一帧：text/binary 上交、ping/pong 由底层自动应答不上交、对端关闭返回
/// [`FrameRead::Closed`]、超限返回 [`FrameRead::TooLarge`]（**不**在此发 close 帧）。
///
/// 拆分成两半之前（[`WebSocket`]）与之后（[`SplitStream<WebSocket>`]）走同一条路径，语义因此不会漂移。
async fn read_frame<S>(stream: &mut S, max_message_bytes: usize) -> FrameRead
where
    S: futures_util::Stream<Item = Result<Message, Error>> + Unpin,
{
    loop {
        match stream.next().await {
            None => return FrameRead::Closed,
            Some(Ok(Message::Text(text))) => {
                return FrameRead::Message(WsMessage::Text(text.as_str().to_owned()));
            }
            Some(Ok(Message::Binary(bytes))) => {
                return FrameRead::Message(WsMessage::Binary(bytes));
            }
            Some(Ok(Message::Ping(_) | Message::Pong(_))) => continue,
            Some(Ok(Message::Close(_))) => return FrameRead::Closed,
            Some(Err(error)) => {
                if message_too_large(&error, max_message_bytes) {
                    return FrameRead::TooLarge;
                }
                return FrameRead::Transport(error.to_string());
            }
        }
    }
}

/// 发送一条文本帧（未拆分的连接与写半边共用）。
async fn send_text_frame<S>(sink: &mut S, text: String) -> Result<(), WsError>
where
    S: futures_util::Sink<Message, Error = Error> + Unpin,
{
    sink.send(Message::Text(Utf8Bytes::from(text)))
        .await
        .map_err(|error| WsError::Transport(error.to_string()))
}

/// 发送一条 binary 帧（未拆分的连接与写半边共用）。
async fn send_binary_frame<S>(sink: &mut S, bytes: Bytes) -> Result<(), WsError>
where
    S: futures_util::Sink<Message, Error = Error> + Unpin,
{
    sink.send(Message::Binary(bytes))
        .await
        .map_err(|error| WsError::Transport(error.to_string()))
}

/// 发送 close 帧；发送失败表示对端已经断开，此时直接放弃（连接随写半边一起关闭）。
async fn send_close_frame<S>(sink: &mut S, code: u16, reason: &str)
where
    S: futures_util::Sink<Message, Error = Error> + Unpin,
{
    let frame = CloseFrame {
        code,
        reason: Utf8Bytes::from(reason),
    };
    let _ = sink.send(Message::Close(Some(frame))).await;
}

/// 判定一个帧级错误是否由「超过本连接配置的单条上限」触发。
///
/// 这里同时核对上限数值等于本连接的配置值，因此不会把别的原因（例如对端协商出的另一套上限）误判成超限。
fn message_too_large(error: &Error, limit: usize) -> bool {
    let text = error.to_string();
    let Some((_, rest)) = text.rsplit_once(MESSAGE_TOO_LONG_MARKER) else {
        return false;
    };
    let Some((_, max)) = rest.split_once('>') else {
        return false;
    };
    max.trim().parse::<usize>() == Ok(limit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_the_capacity_error_for_this_connection() {
        // 文本形态与 tungstenite 0.29 的实际输出一致（外层带 `Space limit exceeded: ` 前缀）。
        let error =
            Error::new("Space limit exceeded: Message too long: 1500000 > 1048576".to_owned());
        assert!(message_too_large(&error, 1024 * 1024));
        // 上限不匹配（别的连接/别的来源）时不判定为超限。
        assert!(!message_too_large(&error, 4096));
        // 内层文本单独出现（上游去掉外层前缀）时同样能识别。
        let error = Error::new("Message too long: 1500000 > 1048576".to_owned());
        assert!(message_too_large(&error, 1024 * 1024));
    }

    #[test]
    fn other_errors_are_not_message_too_large() {
        for text in [
            "Connection reset without closing handshake",
            "Invalid UTF-8",
            "Space limit exceeded: Message too long: 1500000 > ",
            "Space limit exceeded: Message too long: nonsense",
            "Received after closing",
        ] {
            assert!(
                !message_too_large(&Error::new(text.to_owned()), 1024),
                "{text} 不得判为超限"
            );
        }
    }

    #[test]
    fn error_display_does_not_include_business_data() {
        let error = WsError::MessageTooLarge;
        assert_eq!(
            error.to_string(),
            "WebSocket 消息超过单条上限（已按 1009 关闭）"
        );
    }
}
