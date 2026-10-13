//! 一条本地通道连接的用途绑定、请求分派与关闭规则。
//!
//! 逐条对应 `docs/LOCAL_ADMIN_PROTOCOL.md`：
//!
//! - §3：首帧的 channel 决定连接用途；同一连接上出现不同 channel、channel 未知、`length == 0`、超上限
//!   一律关闭连接且**不返回错误帧**。
//! - §4 规则 1：信封 `v != 1` → 关闭连接（版本未知时对端语义不可判定，不能用错误帧回答）。
//! - §4 规则 2：同一连接同时最多 [`MAX_IN_FLIGHT_REQUESTS`] 条未完成请求，超出属协议滥用 → 关闭连接；
//!   同一连接上未完成请求的 `id` 必须唯一，重复按信封非法回答（§6 的 `local.invalid_request`）。
//! - §5 规则 5：每个请求恰好一个响应。响应由 `JoinSet` 回收后写回，因此同一连接上可以并存多条未完成请求
//!   （pipelining），响应按 `id` 关联而不是按到达顺序。
//! - §3.1 实现状态注记（本切片）：`0x02` 连接在完成 framing 校验后立即关闭并记结构化警告，不分配也不消耗
//!   `FacadeAttachmentId`、不转发任何字节、不伪造 ACP 响应。
//! - `daemon.stop` 被接受后：本连接在写回响应后由服务端结束（[`ConnectionShutdown`]），不再等待对端。
//!   关闭序列因此不把这条连接当作在途客户端连接排空——它的语义是「请求关闭」而不是「正在进行业务」。
//!
//! 连接结束时 `JoinSet` 被 drop，从而中止尚未完成的处理器任务：链路已经结束，响应无法送达，也不允许处理器
//! 在连接之外继续持有该连接的语义。处理器实现必须在被取消时不留下半提交状态（core 的写集是单事务，
//! 取消即回滚）。

use std::collections::HashSet;
use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio::task::JoinSet;

use crate::local_admin::{self, AdminResponse, LocalAdminHandler, RequestDecodeOutcome, RequestId};
use crate::transport::local::MAX_IN_FLIGHT_REQUESTS;
use crate::transport::local::framing::{ChannelKind, FrameError, FrameReader, encode_frame};

/// 一条连接要消费的处理器。
///
/// 切片 6 接入 `server::acp_facade` 时在这里增加 `0x02` 的分发目标；本切片只有管理面。
pub struct LocalConnectionHandlers {
    /// 管理请求（channel `0x01`）的处理器；WP3b 的 `LocalAdminRouter` 实现它。
    pub admin: Arc<dyn LocalAdminHandler>,
    /// 本连接的「已请求关闭」标记：管理面在**接受**了 `daemon.stop` 之后置位，[`serve_connection`]
    /// 据此在写回响应后结束本连接（见 [`ConnectionShutdown`]）。
    pub shutdown: Arc<ConnectionShutdown>,
}

impl LocalConnectionHandlers {
    /// 只装配管理处理器的构造入口：自带一个永不置位的独立标记（测试与「无关闭语义」的连接用它）。
    pub fn new(admin: Arc<dyn LocalAdminHandler>) -> Self {
        Self {
            admin,
            shutdown: Arc::new(ConnectionShutdown::new()),
        }
    }

    /// 装配管理处理器，并绑定**由接受循环持有**的关闭请求标记。
    ///
    /// 管理面（置位方）与 [`serve_connection`]（读方）必须看到同一个标记，因此这里由调用方把两者成对
    /// 装配，而不是让传输层自己新建。
    pub fn with_shutdown(
        admin: Arc<dyn LocalAdminHandler>,
        shutdown: Arc<ConnectionShutdown>,
    ) -> Self {
        Self { admin, shutdown }
    }
}

/// 「本连接已经请求关闭」的标记（`daemon.stop`）。
///
/// 传输层对标记只做机械反应：置位之后，[`serve_connection`] 在**写回当前已完成的全部响应**后结束本连接，
/// 不再等待对端（EOF 或下一条请求）。置位的判定属于管理语义（谁的请求是「请求关闭」），由组合根的处理链
/// 决定；传输层不解析方法名，本地通道的帧形状与协议形状因此不变。
///
/// 为什么需要它：`daemon.stop` 的语义是「请求关闭」，不是「正在进行业务」。关闭序列若把这条连接当作在途
/// 客户端连接来排空，排空时长就取决于该客户端进程何时被调度（Windows 上 Named Pipe 句柄只在驱动它的
/// runtime 被驱动或销毁时才真正释放，实测可复现），而不是取决于服务端有多少在途工作。
#[derive(Debug, Default)]
pub struct ConnectionShutdown {
    requested: AtomicBool,
}

impl ConnectionShutdown {
    /// 新建（未置位）。
    pub fn new() -> Self {
        Self::default()
    }

    /// 置位（幂等）。
    pub fn request(&self) {
        self.requested.store(true, Ordering::SeqCst);
    }

    /// 是否已置位。
    pub fn is_requested(&self) -> bool {
        self.requested.load(Ordering::SeqCst)
    }
}

/// 连接结束的原因（全部按 §3/§4 关闭连接，不发送错误帧）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseReason {
    /// 对端关闭连接（含「半个帧后 EOF」，剩余字节数见结构化日志的 `buffered_bytes`）。
    PeerEof,
    /// 帧 `length == 0`。
    EmptyFrame,
    /// 帧 `length` 超过 1 MiB。
    FrameTooLarge {
        /// 帧头声明的 payload 长度。
        length: u32,
    },
    /// payload 首字节不是已知 channel。
    UnknownChannel {
        /// 未知的 channel 字节。
        byte: u8,
    },
    /// 与首帧确定的连接用途不一致。
    ChannelMismatch {
        /// 首帧确定的用途。
        expected: ChannelKind,
        /// 本帧的 channel。
        found: ChannelKind,
    },
    /// 管理连接未完成请求超过 [`MAX_IN_FLIGHT_REQUESTS`]（协议滥用）。
    TooManyInFlightRequests,
    /// 信封 `v != 1`（或 `v` 缺失/不是整数）：连接级关闭，不返回错误帧。
    ChannelVersionUnsupported,
    /// `0x02` 连接：facade 未装配（§3.1 实现状态注记）。
    FacadeUnavailable,
    /// 本连接接受了 `daemon.stop`：响应已写回，服务端主动结束连接（[`ConnectionShutdown`]）。
    ///
    /// 它不进入关闭序列的排空等待：这条连接的语义是「请求关闭」而不是「正在进行业务」。
    ShutdownRequested,
}

impl CloseReason {
    /// 结构化日志字段里的稳定名字。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PeerEof => "peer_eof",
            Self::EmptyFrame => "empty_frame",
            Self::FrameTooLarge { .. } => "frame_too_large",
            Self::UnknownChannel { .. } => "unknown_channel",
            Self::ChannelMismatch { .. } => "channel_mismatch",
            Self::TooManyInFlightRequests => "too_many_in_flight_requests",
            Self::ChannelVersionUnsupported => "channel_version_unsupported",
            Self::FacadeUnavailable => "facade_unavailable",
            Self::ShutdownRequested => "shutdown_requested",
        }
    }
}

/// 连接级错误：读写失败、处理器任务异常、响应无法编码。
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    /// 读失败。
    #[error("本地通道读失败：{0}")]
    Read(#[source] io::Error),
    /// 写失败。
    #[error("本地通道写失败：{0}")]
    Write(#[source] io::Error),
    /// 管理请求的处理任务异常（panic）。此时无法回带请求 `id`，因此不能伪造响应，只能结束连接。
    #[error("管理请求任务异常：{0}")]
    RequestTask(#[source] tokio::task::JoinError),
    /// 响应编码失败（属实现缺陷，不静默丢弃）。
    #[error("管理响应编码失败：{0}")]
    Encode(#[source] local_admin::EnvelopeEncodeError),
    /// 响应超过帧上限：不能编码成一个合法帧（§3）。
    #[error("管理响应超过帧上限（{length} 字节 payload）")]
    ResponseTooLarge {
        /// 响应 payload 字节数。
        length: usize,
    },
}

/// 处理一条**已通过 endpoint 权限与对端凭据校验**的连接。
///
/// 返回关闭原因（正常路径）或连接级错误。`stream` 需为双向流：Windows 的 `NamedPipeServer`、Unix 的
/// `UnixStream`，测试里用 `tokio::io::duplex`。
pub async fn serve_connection<S>(
    stream: S,
    handlers: Arc<LocalConnectionHandlers>,
) -> Result<CloseReason, TransportError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let (mut reader, mut writer) = tokio::io::split(stream);
    let mut frames = FrameReader::new();
    let mut bound: Option<ChannelKind> = None;
    let mut pending: JoinSet<AdminResponse> = JoinSet::new();
    let mut in_flight: HashSet<RequestId> = HashSet::new();

    let close = loop {
        // 关闭请求者：管理面已接受 `daemon.stop`（本连接发出），因此不再等待对端。`pending` 为空即
        // 「已接受的请求都已写回响应」——写回就在 `pending.join_next()` 分支里紧随回收完成，所以这里
        // 不会在响应落地前结束连接（§4 规则 5：每个请求恰好一个响应）。
        //
        // 为什么不等对端：这条连接的语义是「请求关闭」，不是「正在进行业务」；等它会使关闭序列的排空
        // 时长取决于客户端进程何时被调度（见 `ConnectionShutdown`）。
        if handlers.shutdown.is_requested() && pending.is_empty() {
            break CloseReason::ShutdownRequested;
        }
        tokio::select! {
            biased;
            // 已完成的请求先写回响应；`pending` 为空时禁用该分支，避免 select! 空转。
            // `JoinSet::join_next` 与 `FrameReader::next_frame` 都是取消安全的。
            Some(finished) = pending.join_next(), if !pending.is_empty() => {
                let response = finished.map_err(TransportError::RequestTask)?;
                in_flight.remove(response.id());
                write_response(&mut writer, &response).await?;
            }
            frame = frames.next_frame(&mut reader) => {
                let frame = match frame {
                    Ok(Some(frame)) => frame,
                    Ok(None) => {
                        if frames.buffered_bytes() > 0 {
                            tracing::warn!(
                                event = "local_admin.truncated_frame",
                                buffered_bytes = frames.buffered_bytes(),
                                "对端在半个帧上关闭了连接"
                            );
                        }
                        break CloseReason::PeerEof;
                    }
                    Err(FrameError::Io(source)) => return Err(TransportError::Read(source)),
                    Err(error) => break frame_failure(error)?,
                };

                // §3：首帧定连接用途，后续帧的 channel 必须一致。
                match bound {
                    None => {
                        if frame.channel == ChannelKind::AcpStream {
                            // §3.1 实现状态注记（design.md 决策 5）：facade 未装配。framing 已经校验通过，
                            // 这里立即关闭：不分配 FacadeAttachmentId、不转发任何字节、不伪造 ACP 响应。
                            // 切片 6 在此把分发目标换成 server::acp_facade。
                            tracing::warn!(
                                event = "local_admin.acp_facade_unavailable",
                                "server::acp_facade 未装配：0x02 连接在 framing 校验后立即关闭（LOCAL_ADMIN_PROTOCOL.md §3.1）"
                            );
                            break CloseReason::FacadeUnavailable;
                        }
                        bound = Some(ChannelKind::LocalAdmin);
                    }
                    Some(expected) if expected != frame.channel => {
                        break CloseReason::ChannelMismatch {
                            expected,
                            found: frame.channel,
                        };
                    }
                    Some(_) => {}
                }

                // 管理载荷：信封规则（§4）与同一连接的未完成请求上限（§4 规则 2）。
                match local_admin::decode_request(&frame.body) {
                    Err(error) => {
                        let reason = error.log_reason();
                        match error.into_outcome() {
                            RequestDecodeOutcome::CloseConnection => {
                                tracing::warn!(
                                    event = "local_admin.connection_closed",
                                    reason,
                                    "信封版本不是 v1：关闭连接且不返回错误帧（LOCAL_ADMIN_PROTOCOL.md §4 规则 1）"
                                );
                                break CloseReason::ChannelVersionUnsupported;
                            }
                            RequestDecodeOutcome::Respond(response) => {
                                tracing::debug!(event = "local_admin.envelope_rejected", reason);
                                write_response(&mut writer, &response).await?;
                            }
                        }
                    }
                    Ok(request) => {
                        if !in_flight.insert(request.id().clone()) {
                            tracing::warn!(
                                event = "local_admin.duplicate_request_id",
                                "同一连接上未完成请求的 id 重复（LOCAL_ADMIN_PROTOCOL.md §4 规则 2）"
                            );
                            let response = AdminResponse::failure(
                                request.id().clone(),
                                local_admin::AdminError::duplicate_request_id(),
                            );
                            write_response(&mut writer, &response).await?;
                        } else {
                            if in_flight.len() > MAX_IN_FLIGHT_REQUESTS {
                                tracing::warn!(
                                    event = "local_admin.in_flight_limit_exceeded",
                                    limit = MAX_IN_FLIGHT_REQUESTS,
                                    "管理连接未完成请求超过上限：关闭连接（LOCAL_ADMIN_PROTOCOL.md §4 规则 2）"
                                );
                                break CloseReason::TooManyInFlightRequests;
                            }
                            let handler = handlers.admin.clone();
                            pending.spawn(async move { handler.handle(request).await });
                        }
                    }
                }
            }
        }
    };

    match close {
        CloseReason::PeerEof => {
            tracing::debug!(
                event = "local_admin.connection_closed",
                reason = close.as_str()
            );
        }
        // 已在发生时记过警告（facade 未装配是本切片设计内的行为）。
        CloseReason::FacadeUnavailable => {}
        CloseReason::ChannelVersionUnsupported => {}
        // 关闭请求自身的连接：这是关闭序列的正常一环（响应已写回，本连接不在排空集合里），因此记 info
        // 而不是警告——它也是「关闭序列没有为该连接等待对端」的可观察证据。
        CloseReason::ShutdownRequested => {
            tracing::info!(
                event = "local_admin.connection_closed",
                reason = close.as_str(),
                "关闭请求自身的连接已写回响应并结束"
            );
        }
        _ => {
            tracing::warn!(
                event = "local_admin.connection_closed",
                reason = close.as_str(),
                "本地通道按 framing 规则关闭连接且不返回错误帧（LOCAL_ADMIN_PROTOCOL.md §3）"
            );
        }
    }
    // `pending` 在此 drop：`JoinSet` 会中止全部未完成的任务（见模块文档）。
    Ok(close)
}

/// 写回一个管理响应（长度前缀 + channel `0x01` + 信封）。
async fn write_response<W>(writer: &mut W, response: &AdminResponse) -> Result<(), TransportError>
where
    W: AsyncWrite + Unpin,
{
    let body = response.encode().map_err(TransportError::Encode)?;
    let frame = encode_frame(ChannelKind::LocalAdmin, &body)
        .ok_or(TransportError::ResponseTooLarge { length: body.len() })?;
    writer
        .write_all(&frame)
        .await
        .map_err(TransportError::Write)
}

/// 把 framing 的失败映射成「关闭原因」或「连接级错误」。
fn frame_failure(error: FrameError) -> Result<CloseReason, TransportError> {
    match error {
        FrameError::Io(source) => Err(TransportError::Read(source)),
        FrameError::EmptyFrame => Ok(CloseReason::EmptyFrame),
        FrameError::PayloadTooLarge { length } => Ok(CloseReason::FrameTooLarge { length }),
        FrameError::UnknownChannel { byte } => Ok(CloseReason::UnknownChannel { byte }),
    }
}
