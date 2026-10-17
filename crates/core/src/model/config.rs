//! 本地配置与 workspace 解析结果（`docs/CORE_PORTS_AND_STORAGE.md` §11.6/§11.9）。
//!
//! 这些值对象只描述本机可读的配置：目录记录、Agent 声明（`[agent]` 配置表）与 core 解析后的
//! [`ResolvedWorkspace`]。**凭据值不在此层**：Provider 凭据由各 Agent 自己的配置面持有，本系统不再代管。

use std::path::Path;

use super::error::InvalidValue;
use super::ids::{AgentId, DirectoryId, derive_directory_id, is_spec_id, require_bounded};
use super::scalars::Timestamp;

/// 环境变量名：`^[A-Za-z_][A-Za-z0-9_]{0,127}$`。
fn is_env_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphabetic() || first == '_' => {}
        _ => return false,
    }
    name.len() <= 128 && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// daemon 自己的环境变量前缀；profile 的绑定不得占用（§11.6）。
fn is_reserved_env_name(name: &str) -> bool {
    name.to_ascii_uppercase().starts_with("ACP_REMOTE_")
}

/// Provider 标识：`^[A-Za-z0-9._-]{1,64}$`（`LOCAL_ADMIN_PROTOCOL.md` §5.2）。
fn is_provider_id(text: &str) -> bool {
    text.len() <= 64 && is_spec_id(text)
}

pub(crate) fn no_nul(text: &str) -> bool {
    !text.contains('\0')
}

/// Agent 声明（§11.6）：本节点如何启动一个本地 Agent。来源是启动配置的 `[agent]` 表。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentProfile {
    id: AgentId,
    command: String,
    args: Vec<String>,
    default: bool,
}

impl AgentProfile {
    /// 构造。命令 1..=1024 且不含 NUL；argv 逐项 1..=4096、不含 NUL、不得为空串。
    pub fn try_new(
        id: AgentId,
        command: &str,
        args: Vec<String>,
        default: bool,
    ) -> Result<Self, InvalidValue> {
        require_bounded(command, 1, 1024)?;
        if !no_nul(command) {
            return Err(InvalidValue::Field);
        }
        for arg in &args {
            require_bounded(arg, 1, 4096)?;
            if !no_nul(arg) {
                return Err(InvalidValue::Field);
            }
        }
        Ok(Self {
            id,
            command: command.to_owned(),
            args,
            default,
        })
    }

    /// 声明标识。
    pub fn id(&self) -> &AgentId {
        &self.id
    }

    /// 展示名：配置不引入第二个字段，展示名就是 `agentId`。
    pub fn display_name(&self) -> &str {
        self.id.as_str()
    }

    /// 可执行文件路径或名字（不经 shell 拼接）。
    pub fn command(&self) -> &str {
        &self.command
    }

    /// 参数（逐项传给子进程，不拼接）。
    pub fn args(&self) -> &[String] {
        &self.args
    }

    /// 是否为默认 Agent。
    pub fn is_default(&self) -> bool {
        self.default
    }

    /// 标记（或取消）默认；默认唯一性由配置解析保证。
    pub fn with_default(mut self, default: bool) -> Self {
        self.default = default;
        self
    }
}


/// 本机目录记录（§11.6、`design.md` D7）。路径只在本节点可读，不进入 Node Link catalog。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryRecord {
    directory_id: DirectoryId,
    canonical_path: String,
    created_at: Timestamp,
    last_used_at: Timestamp,
}

impl DirectoryRecord {
    /// 构造。路径 1..=4096、不含 NUL 且必须是绝对路径形状；标识 MUST 等于由该路径派生的值。
    pub fn try_new(
        directory_id: DirectoryId,
        canonical_path: &str,
        created_at: Timestamp,
        last_used_at: Timestamp,
    ) -> Result<Self, InvalidValue> {
        require_bounded(canonical_path, 1, 4096)?;
        if !no_nul(canonical_path) || !Path::new(canonical_path).is_absolute() {
            return Err(InvalidValue::Field);
        }
        Ok(Self {
            directory_id,
            canonical_path: canonical_path.to_owned(),
            created_at,
            last_used_at,
        })
    }

    /// 目录标识（派生值，不是路径）。
    pub fn directory_id(&self) -> &DirectoryId {
        &self.directory_id
    }

    /// 规范化绝对路径。
    pub fn canonical_path(&self) -> &str {
        &self.canonical_path
    }

    /// 创建时间。
    pub fn created_at(&self) -> &Timestamp {
        &self.created_at
    }

    /// 最近一次被会话使用的时间。
    pub fn last_used_at(&self) -> &Timestamp {
        &self.last_used_at
    }
}

/// Sync 快照 `workspaces` 资源的条目：目录标识与规范化路径始终成对下发。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceCatalogEntry {
    directory_id: DirectoryId,
    path: String,
}

impl WorkspaceCatalogEntry {
    /// 构造。路径约束与 [`DirectoryRecord`] 相同。
    pub fn try_new(directory_id: DirectoryId, path: &str) -> Result<Self, InvalidValue> {
        require_bounded(path, 1, 4096)?;
        if !no_nul(path) || !Path::new(path).is_absolute() {
            return Err(InvalidValue::Field);
        }
        Ok(Self {
            directory_id,
            path: path.to_owned(),
        })
    }

    /// 目录标识。
    pub fn directory_id(&self) -> &DirectoryId {
        &self.directory_id
    }

    /// 规范化路径。
    pub fn path(&self) -> &str {
        &self.path
    }
}

/// 已解析的 workspace（§11.9）：目录标识 + 规范化后的本机绝对路径。
///
/// 标识总是由规范化路径派生（`design.md` D1），因此客户端提供的目录在解析成功后也有标识；目录记录
/// 本身只在**会话创建成功**的那次提交里落库（`design.md` D2）。两者不得由任一项推导补齐。
///
/// 路径只能存进 [`super::CreateSessionRequest`] 并交给后端，不得进事件、不得随 Node Link/Sync 下发。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedWorkspace {
    directory_id: DirectoryId,
    canonical_path: String,
}

impl ResolvedWorkspace {
    /// 按**已记录目录**构造。路径必须是绝对路径形状，且标识必须等于由该路径派生的值
    /// （`canonicalize` 与目录判定由用例层的解析路径完成）。
    pub fn try_new(directory_id: DirectoryId, canonical_path: String) -> Result<Self, InvalidValue> {
        Self::checked(directory_id, canonical_path)
    }

    /// 按**客户端提供的目录**构造：标识由路径派生，目录记录在创建成功时才写入。
    pub fn at_path(canonical_path: String) -> Result<Self, InvalidValue> {
        let directory_id = derive_directory_id(&canonical_path);
        Self::checked(directory_id, canonical_path)
    }

    fn checked(
        directory_id: DirectoryId,
        canonical_path: String,
    ) -> Result<Self, InvalidValue> {
        require_bounded(&canonical_path, 1, 4096)?;
        if !no_nul(&canonical_path) || !Path::new(&canonical_path).is_absolute() {
            return Err(InvalidValue::Field);
        }
        Ok(Self {
            directory_id,
            canonical_path,
        })
    }

    /// 目录标识（会话归属与命令引用的稳定值）。
    pub fn directory_id(&self) -> &DirectoryId {
        &self.directory_id
    }

    /// 规范化后的本机绝对路径。
    pub fn canonical_path(&self) -> &str {
        &self.canonical_path
    }
}

/// `session.create` 的目录来源（`workspace-resolution` 的「解析归属与后端输入」）。
///
/// 三种来源的失败分类不同（见 `UseCases::create_session`）：**目录标识未在本机记录**是参数类错误
/// （客户端可修正），**目录形状非法**（相对路径、含 `..`）同样是参数类错误且不触碰文件系统，
/// 而**形式合法却解析不了**（目录被删除、被换成文件、`canonicalize` 失败、权限不足）是服务端
/// 不可用类；**置了建目录开关**时目录可以不存在，由本层先创建（连同缺失的父层），创建失败或路径上
/// 已存在非目录成分同样按服务端不可用类拒绝（`sync-session-create` 的「目录校验与失败分类」）。
/// 目录来源只经 Sync 面出现（Node Link 面的请求不得携带任意 Owner 路径）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceSource {
    /// 本机已记录的目录标识（Node Link 与 Sync 的 `directoryId`）。
    Directory(DirectoryId),
    /// 客户端提供的目录原文（仅 Sync 面的 `workspacePath`）：本层只校验形状与文件系统事实，
    /// **不查**目录表、也**不在解析阶段**记录新目录。默认（未置建目录开关）时该目录必须**已存在**
    /// 且是目录。
    Path(String),
    /// 客户端提供的目录原文 + 显式的「目录不存在则先创建」开关（仅 Sync 面的 `workspacePath` +
    /// `createIfMissing: true`）：该目录**可以不存在**，系统先创建它（连同缺失的父层）再启动 Agent。
    ///
    /// 开关与目录标识**互斥**（已记录的目录必然已存在），该互斥在类型上成立，不依赖运行时校验。
    /// 被创建的目录与成功的会话在同一次提交里记录。
    PathWithCreate(String),
}
