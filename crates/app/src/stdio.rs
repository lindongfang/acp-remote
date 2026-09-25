//! `acp-remote acp-stdio`：stdin/stdout ↔ 本地通道 channel `0x02` 的字节泵
//! （`docs/LOCAL_ADMIN_PROTOCOL.md` §3/§3.1/§5.8、`specs/cli-commands` 的「`doctor` 与 `acp-stdio`」）。
//!
//! 职责只有一件：把 stdin 的字节按 §3 组帧（`u32be length | 0x02 | payload`）写给 Daemon，把 Daemon 回来的
//! `0x02` 帧的 payload 原样写到 stdout。因此本模块：
//!
//! - **不进管理信封**：不解析也不构造 `v`/`id`/`method`/`params`，不调用 `server::local_admin` 的任何类型；
//! - **不内嵌核心**：不依赖 `core`/`storage-sqlite`/`agent-host`（那些只在 `daemon start` 里装配），
//!   也绝不打开数据库或启动第二套核心；
//! - **stdout 只承载 ACP 字节流**：本模块不打印任何提示（错误由 `app::cli` 的统一出口写 stderr）；
//! - **二进制安全**：按块转发，不做行缓冲、不假设 UTF-8（含 NUL 的字节原样透传）。
//!
//! 失败关闭（§3.1 的实现状态注记：facade 未装配时 Daemon 在 framing 校验后立即关闭 `0x02` 连接）：
//! 连接建立后**一个** ACP 字节都没收到就被关闭 → [`StdioError::NoStream`]（「daemon 未提供 ACP 流」）；
//! stdin 在任何 ACP 字节传出前结束 → [`StdioError::NoInput`]。两种情况都不静默按成功退出。

use std::io;

use server::transport::local::{
    CHANNEL_ACP_STREAM_BYTE, ChannelKind, FrameError, FrameReader, MAX_FRAME_PAYLOAD_BYTES,
};
use tokio::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};

/// 每次读 stdin 的字节数：含 channel 字节后仍远小于 1 MiB 的帧上限，且不依赖行边界。
const CHUNK_BYTES: usize = 8 * 1024;

/// 字节泵的失败。全部由 `app::cli` 映射成非零退出码 + 一行结构化 stderr。
#[derive(Debug, thiserror::Error)]
pub enum StdioError {
    /// 无法连接本地通道（Daemon 未运行或正在关闭）。
    #[error("无法连接本地管理通道（Daemon 未运行或正在关闭），无法提供 ACP 流")]
    Connect,
    /// 连接建立后的读写失败。
    #[error("本地管理通道在传输中失败")]
    Transport,
    /// Daemon 在提供任何 ACP 字节前关闭了连接（facade 缺席期的既知行为，§3.1）。
    #[error("daemon 未提供 ACP 流（连接在收到任何 ACP 字节前被关闭）")]
    NoStream,
    /// stdin 在任何 ACP 字节传出前结束：无法建立 ACP 流。
    #[error("stdin 在传出任何 ACP 字节前结束，未建立 ACP 流")]
    NoInput,
    /// 帧层失败（channel 不是 `0x02`、帧超长等）。
    #[error("ACP 流帧非法")]
    Frame,
    /// stdout 写失败。
    #[error("写出 ACP 流失败")]
    Output,
    /// 无法创建异步 runtime。
    #[error("无法创建异步 runtime")]
    Runtime,
}

/// 泵的收发统计（诊断用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Summary {
    /// 转发给 Daemon 的 ACP 字节数（不含帧头与 channel 字节）。
    pub to_daemon: u64,
    /// 从 Daemon 转发到 stdout 的 ACP 字节数。
    pub from_daemon: u64,
}

/// 连接 endpoint 并泵 stdin/stdout（CLI 侧的完整入口）。
pub fn pump(endpoint: &str) -> Result<Summary, StdioError> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| StdioError::Runtime)?;
    runtime.block_on(async {
        let stream = open_stream(endpoint).await?;
        pump_stream(stream, tokio::io::stdin(), tokio::io::stdout()).await
    })
}

/// 泵一条已建立的双向字节流（单元测试注入内存流，覆盖二进制与两种失败形态）。
pub(crate) async fn pump_stream<D, I, O>(
    stream: D,
    mut stdin: I,
    mut stdout: O,
) -> Result<Summary, StdioError>
where
    D: AsyncRead + AsyncWrite,
    I: AsyncRead + Unpin,
    O: AsyncWrite + Unpin,
{
    let (mut from_daemon_reader, mut to_daemon_writer) = tokio::io::split(stream);
    let mut frames = FrameReader::new();
    let mut buffer = vec![0u8; CHUNK_BYTES];
    let mut summary = Summary::default();
    let mut stdin_open = true;

    loop {
        tokio::select! {
            // stdin → Daemon：每个块一帧（channel 固定 `0x02`；§3 要求首帧定连接用途）。
            result = stdin.read(&mut buffer), if stdin_open => {
                match result {
                    Ok(0) => {
                        if summary.to_daemon == 0 {
                            // 一个 ACP 字节都没传出去：channel 从未建立，不能静默退出。
                            return Err(StdioError::NoInput);
                        }
                        stdin_open = false;
                        // 半关闭写端：Daemon 侧看到流结束，读方向继续转发到对端关闭为止。
                        if to_daemon_writer.shutdown().await.is_err() {
                            return Err(StdioError::Transport);
                        }
                    }
                    Ok(read) => {
                        write_frame(&mut to_daemon_writer, &buffer[..read]).await?;
                        summary.to_daemon += read as u64;
                    }
                    Err(_) => {
                        // stdin 读失败：与 EOF 同样的收尾（已经传出的字节仍然有效）。
                        if summary.to_daemon == 0 {
                            return Err(StdioError::NoInput);
                        }
                        stdin_open = false;
                    }
                }
            }
            // Daemon → stdout：帧 payload 原样写出，不做文本转换。
            frame = frames.next_frame(&mut from_daemon_reader) => {
                match frame {
                    Ok(Some(frame)) => {
                        if frame.channel != ChannelKind::AcpStream {
                            return Err(StdioError::Frame);
                        }
                        stdout.write_all(&frame.body).await.map_err(|_| StdioError::Output)?;
                        stdout.flush().await.map_err(|_| StdioError::Output)?;
                        summary.from_daemon += frame.body.len() as u64;
                    }
                    Ok(None) => return closed(summary),
                    Err(FrameError::Io(error)) if is_closed(&error) => return closed(summary),
                    Err(FrameError::Io(_)) => return Err(StdioError::Transport),
                    Err(_) => return Err(StdioError::Frame),
                }
            }
        }
    }
}

/// Daemon 侧关闭连接：从未收到 ACP 字节即明确失败（facade 缺席期的既知行为），否则是正常结束。
fn closed(summary: Summary) -> Result<Summary, StdioError> {
    if summary.from_daemon == 0 {
        Err(StdioError::NoStream)
    } else {
        Ok(summary)
    }
}

/// 按 §3 组帧并写出：`u32be length | 0x02 | payload`（`length` 含 channel 字节）。
async fn write_frame<W>(writer: &mut W, payload: &[u8]) -> Result<(), StdioError>
where
    W: AsyncWrite + Unpin,
{
    let length = payload.len() + 1;
    if length > MAX_FRAME_PAYLOAD_BYTES {
        return Err(StdioError::Frame);
    }
    let length = u32::try_from(length).map_err(|_| StdioError::Frame)?;
    let mut frame = Vec::with_capacity(4 + length as usize);
    frame.extend_from_slice(&length.to_be_bytes());
    frame.push(CHANNEL_ACP_STREAM_BYTE);
    frame.extend_from_slice(payload);
    writer
        .write_all(&frame)
        .await
        .map_err(|_| StdioError::Transport)?;
    writer.flush().await.map_err(|_| StdioError::Transport)
}

/// 对端关闭连接时的读错误（Windows 的 pipe 关闭多为 `BrokenPipe`，Unix 多为 EOF）。
fn is_closed(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::BrokenPipe
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::UnexpectedEof
    )
}

/// 连接 endpoint（Windows Named Pipe / Unix socket；定位串来自运行记录）。
async fn open_stream(endpoint: &str) -> Result<LocalStream, StdioError> {
    #[cfg(windows)]
    {
        tokio::net::windows::named_pipe::ClientOptions::new()
            .open(endpoint)
            .map_err(|_| StdioError::Connect)
    }
    #[cfg(unix)]
    {
        tokio::net::UnixStream::connect(endpoint)
            .await
            .map_err(|_| StdioError::Connect)
    }
}

#[cfg(windows)]
type LocalStream = tokio::net::windows::named_pipe::NamedPipeClient;
#[cfg(unix)]
type LocalStream = tokio::net::UnixStream;

#[cfg(test)]
mod tests {
    use super::*;
    use server::transport::local::CHANNEL_LOCAL_ADMIN_BYTE;

    /// 用 `duplex` 充当 Daemon 端与 stdin/stdout，断言二进制（含 NUL）原样双向透传。
    #[tokio::test]
    async fn binary_payloads_round_trip_through_both_directions() {
        let (daemon, pump_side) = tokio::io::duplex(64 * 1024);
        let (mut stdin_writer, stdin_reader) = tokio::io::duplex(64 * 1024);
        let (stdout_writer, mut stdout_reader) = tokio::io::duplex(64 * 1024);
        let (mut daemon_read, mut daemon_write) = tokio::io::split(daemon);

        let pump = tokio::spawn(pump_stream(pump_side, stdin_reader, stdout_writer));
        // stdin → Daemon：含 NUL 与非 UTF-8 字节的块必须按 §3 组帧原样送出。
        let request = [0x00u8, 0x7f, 0x80, 0xff, b'{', b'}'];
        stdin_writer.write_all(&request).await.expect("写 stdin");
        stdin_writer.flush().await.expect("刷新 stdin");

        let mut frames = FrameReader::new();
        let frame = frames
            .next_frame(&mut daemon_read)
            .await
            .expect("可读")
            .expect("一帧");
        assert_eq!(frame.channel, ChannelKind::AcpStream);
        assert_eq!(frame.body, request, "二进制字节必须原样透传");

        // Daemon → stdout：帧 payload 原样写出。
        let response = [0x01u8, 0x00, 0xfe];
        let mut out = Vec::new();
        out.extend_from_slice(
            &u32::try_from(response.len() + 1)
                .expect("长度")
                .to_be_bytes(),
        );
        out.push(CHANNEL_ACP_STREAM_BYTE);
        out.extend_from_slice(&response);
        daemon_write.write_all(&out).await.expect("写响应");
        daemon_write.flush().await.expect("刷新响应");
        let mut received = [0u8; 3];
        stdout_reader.read_exact(&mut received).await.expect("读出");
        assert_eq!(received, response);

        // 两侧都结束后：已有 ACP 字节流过，因此是正常结束（退出码 0）。
        drop(stdin_writer);
        drop(daemon_write);
        drop(daemon_read);
        let summary = pump.await.expect("泵任务").expect("已有字节流过即成功");
        assert_eq!(summary.to_daemon, request.len() as u64);
        assert_eq!(summary.from_daemon, response.len() as u64);
    }

    /// stdin 一个字节都没传出去就结束：明确失败（不与「Daemon 没提供流」混淆）。
    #[tokio::test]
    async fn an_empty_stdin_is_an_explicit_failure() {
        let (_daemon, pump_side) = tokio::io::duplex(4096);
        let (stdin_writer, stdin_reader) = tokio::io::duplex(4096);
        drop(stdin_writer);
        let (_stdout_writer, stdout_reader) = tokio::io::duplex(4096);
        let error = pump_stream(pump_side, stdin_reader, stdout_reader)
            .await
            .expect_err("空 stdin 必须明确失败");
        assert!(matches!(error, StdioError::NoInput), "{error}");
    }

    /// 即连即关（facade 缺席期）：收到任何 ACP 字节前连接被关闭 ⇒ 明确失败，且 stdout 未被写入。
    #[tokio::test]
    async fn an_immediately_closed_connection_means_no_acp_stream() {
        let (daemon, pump_side) = tokio::io::duplex(4096);
        let (mut stdin_writer, stdin_reader) = tokio::io::duplex(4096);
        let (stdout_writer, mut stdout_reader) = tokio::io::duplex(4096);
        // 写半边立即丢弃（只保留读半边）：`duplex` 的两个半边共享同一条流，两个都 drop 才是对端关闭。
        let (mut daemon_read, daemon_write) = tokio::io::split(daemon);
        drop(daemon_write);

        let pump = tokio::spawn(pump_stream(pump_side, stdin_reader, stdout_writer));
        stdin_writer
            .write_all(&[0x00, 0x01, 0x02])
            .await
            .expect("写 stdin");
        stdin_writer.flush().await.expect("刷新");
        // 先证明请求那一帧确实发出（首帧定用途），再模拟服务端立即关闭。
        let mut frames = FrameReader::new();
        frames
            .next_frame(&mut daemon_read)
            .await
            .expect("可读")
            .expect("一帧");
        drop(daemon_read);
        drop(stdin_writer);

        let error = pump
            .await
            .expect("泵任务")
            .expect_err("即连即关必须明确失败");
        assert!(matches!(error, StdioError::NoStream), "{error}");
        // stdout 不得被污染（既没有 ACP 字节，也没有把输入回写成输出）。
        let mut leftover = [0u8; 16];
        assert_eq!(
            stdout_reader.read(&mut leftover).await.expect("读 stdout"),
            0,
            "stdout 必须保持为空"
        );
    }

    /// 组帧的字节形态与 §3 一致（`u32be length | 0x02 | payload`，`length` 含 channel 字节）。
    #[tokio::test]
    async fn frames_match_the_documented_shape() {
        let (mut writer, mut reader) = tokio::io::duplex(4096);
        write_frame(&mut writer, b"abc").await.expect("写出");
        let mut frame = [0u8; 8];
        reader.read_exact(&mut frame).await.expect("读出");
        assert_eq!(&frame[..4], &4u32.to_be_bytes());
        assert_eq!(frame[4], CHANNEL_ACP_STREAM_BYTE);
        assert_ne!(frame[4], CHANNEL_LOCAL_ADMIN_BYTE, "不得进管理信封");
        assert_eq!(&frame[5..], b"abc");
    }
}
