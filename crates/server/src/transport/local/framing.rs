//! 本地通道的帧编解码（`docs/LOCAL_ADMIN_PROTOCOL.md` §3）。
//!
//! ```text
//! u32be length | payload(length bytes)      payload[0] = channel
//! ```
//!
//! - `length` 只计 payload（含 channel 字节），不含 4 字节头；上限 1 MiB，合法取等号。
//! - `length == 0`、超上限、channel 未知一律**关闭连接且不返回错误帧**：此时链路本身不可信，错误帧同样不可信
//!   （§3 末条）。因此本模块只产出关闭原因，绝不生成错误帧。
//! - 读取器自己持有缓冲区：`AsyncReadExt::read` 是取消安全的，所以 [`FrameReader::next_frame`] 可以安全地
//!   参与 `tokio::select!`（被取消时留在缓冲区里的字节不丢），这一点是 [crate::transport::local::serve_connection]
//!   在同一任务里既读帧又写响应的前提。

use std::io;

use tokio::io::{AsyncRead, AsyncReadExt};

/// 长度前缀宽度（§3）。与 `schemas/local-admin/v1/envelope.schema.json#/$defs/framing.lengthPrefixBytes` 同值，
/// 由 `tests/local_admin_schema_drift.rs` 断言。
pub const LENGTH_PREFIX_BYTES: usize = 4;

/// 单帧 payload 上限 1 MiB（§3）。与 schema 的 `framing.maxPayloadBytes` 同值。
pub const MAX_FRAME_PAYLOAD_BYTES: usize = 1_048_576;

/// 每次读取系统调用申请的字节数；只影响内存增长粒度，不影响帧语义。
const READ_CHUNK_BYTES: usize = 8 * 1024;

/// channel `0x01`：管理请求/响应（UTF-8 JSON 信封）。
pub const CHANNEL_LOCAL_ADMIN_BYTE: u8 = 0x01;

/// channel `0x02`：ACP 字节流（对本地管理不透明）。
pub const CHANNEL_ACP_STREAM_BYTE: u8 = 0x02;

/// payload 首字节的 channel 值（§3 的表格）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelKind {
    /// `0x01`：管理载荷，一帧一条完整信封，不跨帧、不合并。
    LocalAdmin,
    /// `0x02`：ACP 字节流，长期双向；本地管理只负责分帧，不解析内容。
    AcpStream,
}

impl ChannelKind {
    /// 由 payload 首字节解析 channel；未知值返回 `None`（调用方关闭连接）。
    pub fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            CHANNEL_LOCAL_ADMIN_BYTE => Some(Self::LocalAdmin),
            CHANNEL_ACP_STREAM_BYTE => Some(Self::AcpStream),
            _ => None,
        }
    }

    /// channel 的 wire 字节值。
    pub fn as_byte(self) -> u8 {
        match self {
            Self::LocalAdmin => CHANNEL_LOCAL_ADMIN_BYTE,
            Self::AcpStream => CHANNEL_ACP_STREAM_BYTE,
        }
    }

    /// 结构化日志与关闭原因里的稳定名字。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LocalAdmin => "local_admin",
            Self::AcpStream => "acp_stream",
        }
    }
}

/// 一个完整帧：已按 §3 校验长度与 channel，`body` 不含 channel 字节。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// 帧所属 channel。
    pub channel: ChannelKind,
    /// payload 去掉 channel 字节后的内容（管理载荷为一条完整信封的 JSON 文本）。
    pub body: Vec<u8>,
}

/// 读取一帧时的协议级错误：全部对应「关闭连接且不发错误帧」。
#[derive(Debug, thiserror::Error)]
pub enum FrameError {
    /// 底层读写失败；连接无法继续。
    #[error("本地通道读失败：{0}")]
    Io(#[source] io::Error),
    /// `length == 0`：channel 字节是强制的（§3）。
    #[error("帧长度为 0（channel 字节缺失）")]
    EmptyFrame,
    /// `length > 1 MiB`（§3）。
    #[error("帧长度 {length} 超过上限 {MAX_FRAME_PAYLOAD_BYTES}")]
    PayloadTooLarge {
        /// 帧头声明的 payload 长度。
        length: u32,
    },
    /// 首字节不是 `0x01` / `0x02`（§3）。
    #[error("未知 channel 字节 0x{byte:02x}")]
    UnknownChannel {
        /// 未知的 channel 字节。
        byte: u8,
    },
}

/// 有缓冲的帧读取器：状态（缓冲与位置）随连接存活，读取本身可被取消。
#[derive(Debug, Default)]
pub struct FrameReader {
    buffer: Vec<u8>,
}

impl FrameReader {
    /// 新建一个空读取器。
    pub fn new() -> Self {
        Self::default()
    }

    /// 缓冲里尚未构成完整帧的字节数（对端在半个帧上关闭时非 0，用于诊断）。
    pub fn buffered_bytes(&self) -> usize {
        self.buffer.len()
    }

    /// 读取下一帧。
    ///
    /// 返回 `Ok(None)` 表示对端干净关闭（读回 0 字节）：调用方按 §3 关闭连接，不生成错误帧。
    /// 返回 `Err(FrameError::Io)` 表示链路已不可用。
    ///
    /// 取消安全：任何一次未完成的 `read` 被取消时都不会丢弃已读字节（它们仍在 `buffer` 里），
    /// 而一次完整的帧在成为完整帧的同一个轮次里返回，不存在「取走但不返回」的中间态。
    pub async fn next_frame<R>(&mut self, reader: &mut R) -> Result<Option<Frame>, FrameError>
    where
        R: AsyncRead + Unpin,
    {
        loop {
            if let Some(frame) = self.take_frame()? {
                return Ok(Some(frame));
            }
            let mut chunk = [0u8; READ_CHUNK_BYTES];
            let read = reader.read(&mut chunk).await.map_err(FrameError::Io)?;
            if read == 0 {
                // 半个帧之后 EOF 与「干净的连接结束」都按同一原因关闭；剩余字节数由调用方记入诊断字段。
                return Ok(None);
            }
            self.buffer.extend_from_slice(&chunk[..read]);
        }
    }

    /// 缓冲里已有完整帧就取出；否则返回 `None`（需要继续读）。
    fn take_frame(&mut self) -> Result<Option<Frame>, FrameError> {
        if self.buffer.len() < LENGTH_PREFIX_BYTES {
            return Ok(None);
        }
        let header: [u8; LENGTH_PREFIX_BYTES] = match self.buffer[..LENGTH_PREFIX_BYTES].try_into()
        {
            Ok(header) => header,
            Err(_) => return Ok(None),
        };
        let length = u32::from_be_bytes(header);
        if length == 0 {
            return Err(FrameError::EmptyFrame);
        }
        let length = length as usize;
        if length > MAX_FRAME_PAYLOAD_BYTES {
            return Err(FrameError::PayloadTooLarge {
                length: length as u32,
            });
        }
        if self.buffer.len() < LENGTH_PREFIX_BYTES + length {
            return Ok(None);
        }
        let mut frame_bytes: Vec<u8> = self.buffer.drain(..LENGTH_PREFIX_BYTES + length).collect();
        // 丢掉长度前缀（`frame_bytes` 只是缓冲区的一份拷贝，用完即弃）。
        let payload = frame_bytes.split_off(LENGTH_PREFIX_BYTES);
        let channel_byte = match payload.first() {
            Some(byte) => *byte,
            None => return Err(FrameError::EmptyFrame),
        };
        let channel = ChannelKind::from_byte(channel_byte)
            .ok_or(FrameError::UnknownChannel { byte: channel_byte })?;
        let body = payload[1..].to_vec();
        Ok(Some(Frame { channel, body }))
    }
}

/// 把一帧写到连接上（长度前缀 + channel + body）。
///
/// 写侧只服务于本层生成的响应，因此 `body` 超出帧上限属实现缺陷：返回 `None` 让调用方失败关闭，
/// 不做静默截断（§3.1：一旦出现超过帧上限的帧就是 framing 错误）。
pub(crate) fn encode_frame(channel: ChannelKind, body: &[u8]) -> Option<Vec<u8>> {
    let payload_length = body.len().checked_add(1)?;
    let length = u32::try_from(payload_length).ok()?;
    if payload_length > MAX_FRAME_PAYLOAD_BYTES {
        return None;
    }
    let mut frame = Vec::with_capacity(LENGTH_PREFIX_BYTES + payload_length);
    frame.extend_from_slice(&length.to_be_bytes());
    frame.push(channel.as_byte());
    frame.extend_from_slice(body);
    Some(frame)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_kind_round_trips_known_bytes() {
        assert_eq!(ChannelKind::from_byte(0x01), Some(ChannelKind::LocalAdmin));
        assert_eq!(ChannelKind::from_byte(0x02), Some(ChannelKind::AcpStream));
        assert_eq!(ChannelKind::from_byte(0x00), None);
        assert_eq!(ChannelKind::from_byte(0x03), None);
        assert_eq!(ChannelKind::LocalAdmin.as_byte(), 0x01);
        assert_eq!(ChannelKind::AcpStream.as_byte(), 0x02);
    }

    #[test]
    fn encode_frame_uses_big_endian_length_of_payload_only() {
        let frame = encode_frame(ChannelKind::LocalAdmin, b"{}").expect("帧可编码");
        assert_eq!(frame, vec![0x00, 0x00, 0x00, 0x03, 0x01, b'{', b'}']);
    }

    #[test]
    fn encode_frame_rejects_payloads_over_the_frame_limit() {
        let body = vec![0u8; MAX_FRAME_PAYLOAD_BYTES];
        assert!(encode_frame(ChannelKind::LocalAdmin, &body).is_none());
    }

    #[test]
    fn take_frame_waits_for_the_whole_payload() {
        let mut reader = FrameReader::new();
        reader
            .buffer
            .extend_from_slice(&[0x00, 0x00, 0x00, 0x02, 0x01]);
        assert!(reader.take_frame().expect("尚未构成完整帧").is_none());
        reader.buffer.push(b'{');
        let frame = reader.take_frame().expect("完整帧可取出").expect("有帧");
        assert_eq!(frame.channel, ChannelKind::LocalAdmin);
        assert_eq!(frame.body, b"{");
        assert_eq!(reader.buffered_bytes(), 0);
    }

    #[test]
    fn take_frame_rejects_empty_and_oversized_headers() {
        let mut reader = FrameReader::new();
        reader.buffer.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
        assert!(matches!(reader.take_frame(), Err(FrameError::EmptyFrame)));

        let mut reader = FrameReader::new();
        let oversized = (MAX_FRAME_PAYLOAD_BYTES as u32) + 1;
        reader.buffer.extend_from_slice(&oversized.to_be_bytes());
        assert!(matches!(
            reader.take_frame(),
            Err(FrameError::PayloadTooLarge { length }) if length == oversized
        ));
    }

    #[test]
    fn take_frame_accepts_the_exact_frame_limit() {
        let mut reader = FrameReader::new();
        let length = MAX_FRAME_PAYLOAD_BYTES as u32;
        reader.buffer.extend_from_slice(&length.to_be_bytes());
        reader.buffer.push(CHANNEL_LOCAL_ADMIN_BYTE);
        reader
            .buffer
            .extend(std::iter::repeat_n(0u8, MAX_FRAME_PAYLOAD_BYTES - 1));
        let frame = reader
            .take_frame()
            .expect("上限本身合法（§3 合法取等号）")
            .expect("有帧");
        assert_eq!(frame.body.len(), MAX_FRAME_PAYLOAD_BYTES - 1);
    }

    #[test]
    fn take_frame_rejects_unknown_channel_byte() {
        let mut reader = FrameReader::new();
        reader
            .buffer
            .extend_from_slice(&[0x00, 0x00, 0x00, 0x02, 0x07, b'x']);
        assert!(matches!(
            reader.take_frame(),
            Err(FrameError::UnknownChannel { byte: 0x07 })
        ));
    }
}
