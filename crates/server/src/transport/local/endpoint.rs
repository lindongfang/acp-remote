//! 本地通道 endpoint 的平台无关部分：命名规则（§2.1）、配置与错误类型。
//!
//! 平台差异（Named Pipe / Unix socket 的创建、权限与对端凭据校验）在 `platform` 子模块里；本模块只放
//! 两个平台都要遵守的**命名算法**与共同的类型，因此命名规则可以在任何平台上被独立测试。

use std::path::{Path, PathBuf};

use sha2::{Digest as _, Sha256};
use uuid::Uuid;

/// §2.1 的实例标识：16 字符小写 hex（64 bit CSPRNG），同一 Daemon 运行期内不变，重启后重新生成。
///
/// 由单实例锁持有者在获取锁时生成并写入锁文件供 CLI 读取（`app` 的职责，WP4）；本 crate 只做形状校验、
/// 命名与 `generate()` 这个唯一随机来源，避免第二处 hex 随机实现。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct InstanceId(String);

/// [`InstanceId`] 的形状校验失败。
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("instance id 必须是 16 字符小写十六进制")]
pub struct InvalidInstanceId;

impl InstanceId {
    /// 由文本构造并校验形状。
    pub fn new(text: impl Into<String>) -> Result<Self, InvalidInstanceId> {
        let text = text.into();
        if text.len() == 16
            && text
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            Ok(Self(text))
        } else {
            Err(InvalidInstanceId)
        }
    }

    /// 生成一个新的实例标识（8 字节 CSPRNG → 16 字符小写 hex）。
    pub fn generate() -> Self {
        let uuid = Uuid::new_v4();
        let text: String = uuid.simple().to_string().chars().take(16).collect();
        Self(text)
    }

    /// 文本形式。
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// endpoint 的创建参数。
#[derive(Debug, Clone)]
pub struct LocalEndpointConfig {
    /// `daemon.data_dir`：Unix 在 `XDG_RUNTIME_DIR` 未设置时回落到 `<data_dir>/run/`（§2.1）。
    pub data_dir: PathBuf,
    /// 单实例锁生成的 `<instanceId>`（§2.1）。
    pub instance_id: InstanceId,
    /// Unix：显式覆盖 `$XDG_RUNTIME_DIR`；`None` 表示按 §2.1 读取环境变量。
    ///
    /// 该字段存在的原因是测试可判定性：Rust 2024 起 `std::env::set_var` 需要显式的不安全块，而 workspace 的
    /// forbid 级 lint（根 `Cargo.toml` 的 `[workspace.lints.rust]`）不允许它；若只能经环境变量指定运行时
    /// 目录，§2.1 的两条路径就无法同时被测试覆盖。
    /// `app` 正常传 `None`。Windows 忽略本字段。
    pub runtime_dir: Option<PathBuf>,
}

/// endpoint 创建与接受连接时的错误。
///
/// 所有变体都表示「失败关闭」：调用方必须拒绝启动或放弃该连接，不得降级为无管理通道或放宽校验
/// （`docs/LOCAL_ADMIN_PROTOCOL.md` §7、ADR-0004 决策 8）。
#[derive(Debug, thiserror::Error)]
pub enum EndpointError {
    /// 无法创建或接受 endpoint（系统调用失败）。
    #[error("本地通道 endpoint 失败：{source}")]
    Os {
        /// 底层错误。
        #[source]
        source: std::io::Error,
    },
    /// endpoint 路径已被另一个存活实例占用（§7「旧 socket 仍存活」）。
    #[error("本地通道 endpoint 已被另一个存活实例占用")]
    AlreadyInUse,
    /// endpoint 路径不是本通道可以安全接管的对象（符号链接、非 socket、非目录）。
    #[error("本地通道 endpoint 路径不安全：{kind:?}")]
    UnsafePath {
        /// 不安全的具体形态。
        kind: UnsafePathKind,
    },
    /// 目录或 socket 的权限无法保证（§2.1「权限无法保证时拒绝启动」）。
    #[error("本地通道 endpoint {kind} 权限无法保证为 {expected:o}")]
    InsecurePermissions {
        /// 对象种类（`directory` / `socket`）。
        kind: &'static str,
        /// 要求的模式位。
        expected: u32,
    },
    /// 本次连接的对端不满足访问控制；连接已被关闭且不发送任何 frame，审计事件已记录。
    ///
    /// 包括「对端是另一个 OS 用户」与「无法确认对端身份」（拿不到对端 SID/uid）两种情况——两者都失败关闭。
    #[error("本地通道拒绝了本次连接（对端 OS 用户不匹配或无法确认）")]
    PeerRejected,
}

/// [`EndpointError::UnsafePath`] 的具体形态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnsafePathKind {
    /// 路径是符号链接：绝不被跟随。
    Symlink,
    /// 目录位置被非目录对象占用。
    NotADirectory,
    /// socket 位置被非 socket 对象占用。
    NotASocket,
}

/// §2.1 的 `<userSidHashHex>`：用户标识字符串 UTF-8 字节的 SHA-256 小写十六进制**前 16 字符**。
///
/// 输入不附加换行或前缀。Windows 传 SID 字符串（`S-1-5-21-…`），Unix 传十进制 UID（`1000`）。
pub fn user_identity_hash(identifier: &str) -> String {
    let digest = Sha256::digest(identifier.as_bytes());
    let mut text = String::with_capacity(16);
    for byte in digest.iter().take(8) {
        text.push_str(&format!("{byte:02x}"));
    }
    text
}

/// Windows 的 pipe 路径（§2.1）：`\\.\pipe\acp-remote-<userSidHashHex>-<instanceId>`。
///
/// pipe 名只用于定位，不是授权手段：授权由创建时的 SDDL 与连接后的凭据校验承担。
pub fn named_pipe_path(user_identity: &str, instance_id: &InstanceId) -> String {
    format!(
        r"\\.\pipe\acp-remote-{}-{}",
        user_identity_hash(user_identity),
        instance_id.as_str()
    )
}

/// Unix 的 socket 路径：`<dir>/<instanceId>.sock`。
pub fn unix_socket_path(directory: &Path, instance_id: &InstanceId) -> PathBuf {
    directory.join(format!("{}.sock", instance_id.as_str()))
}

/// Unix 的运行时目录（§2.1）：`$XDG_RUNTIME_DIR/acp-remote`，未设置时回落 `<data_dir>/run`。
///
/// `xdg_runtime_dir` 由调用方传入环境变量的值（`std::env::var_os("XDG_RUNTIME_DIR")`）——
/// 这样这条回落规则本身可以在不触碰进程环境的前提下被测试（见 [`LocalEndpointConfig::runtime_dir`]）。
pub fn unix_endpoint_directory(
    config: &LocalEndpointConfig,
    xdg_runtime_dir: Option<&Path>,
) -> PathBuf {
    if let Some(explicit) = &config.runtime_dir {
        return explicit.join("acp-remote");
    }
    match xdg_runtime_dir {
        Some(runtime) => runtime.join("acp-remote"),
        None => config.data_dir.join("run"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instance_id_requires_16_lowercase_hex_characters() {
        assert!(InstanceId::new("0123456789abcdef").is_ok());
        assert!(InstanceId::new("0123456789ABCDEF").is_err());
        assert!(InstanceId::new("0123456789abcde").is_err());
        assert!(InstanceId::new("0123456789abcdefg").is_err());
        assert!(InstanceId::new("0123456789abcde.").is_err());
        assert!(InstanceId::new("").is_err());
        assert_eq!(InstanceId::generate().as_str().len(), 16);
    }
}
