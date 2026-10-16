//! 本地管理配置与 workspace 解析结果（`docs/CORE_PORTS_AND_STORAGE.md` §11.6/§11.9）。
//!
//! 这些值对象只描述本机可读的配置：Agent profile、Provider 引用、workspace 记录、首次初始化标记，
//! 以及 core 解析后的 [`ResolvedWorkspace`]。**凭据值不在此层**：profile 里只有「哪个 Provider 字段
//! 注入哪个环境变量」的绑定，凭据本身只经平台 keystore 端口的 [`SecretValue`]
//! （`SECURITY_DESIGN.md` §13.1）。

use std::fmt;
use std::path::Path;
use std::str::FromStr;

use super::error::InvalidValue;
use super::ids::{AgentId, WorkspaceAlias, is_spec_id, require_bounded};
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

/// 启动子进程时把某个 Provider 凭据字段注入哪个环境变量（§11.6）。
///
/// 不变式：`name` 必须同时出现在同一条 profile 的 `env_allowlist` 里（白名单是上限，绑定不能越过它）；
/// `(provider_id, field)` 在同一条 profile 内不得重复；`name` 不得是保留名。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderEnvBinding {
    provider_id: String,
    field: String,
    name: String,
}

impl ProviderEnvBinding {
    /// 构造。`provider_id` 必须匹配 `^[A-Za-z0-9._-]{1,64}$`，`field` 1..=128，`name` 是合法且非保留的
    /// 环境变量名。
    pub fn try_new(provider_id: &str, field: &str, name: &str) -> Result<Self, InvalidValue> {
        if !is_provider_id(provider_id) || !is_spec_id(field) {
            return Err(InvalidValue::Field);
        }
        if !is_env_name(name) || is_reserved_env_name(name) {
            return Err(InvalidValue::Field);
        }
        Ok(Self {
            provider_id: provider_id.to_owned(),
            field: field.to_owned(),
            name: name.to_owned(),
        })
    }

    /// Provider 标识（必须存在于 `owned_provider_ref`）。
    pub fn provider_id(&self) -> &str {
        &self.provider_id
    }

    /// Provider 的字段名（必须在该 Provider 的 `configured_fields` 内）。
    pub fn field(&self) -> &str {
        &self.field
    }

    /// 注入的环境变量名（同时必须在 `env_allowlist` 中）。
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Agent profile（§11.6）：本节点如何启动一个本地 Agent。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentProfile {
    id: AgentId,
    display_name: String,
    command: String,
    args: Vec<String>,
    env_allowlist: Vec<String>,
    env: Vec<ProviderEnvBinding>,
    default: bool,
    created_at: Timestamp,
    updated_at: Timestamp,
}

impl AgentProfile {
    /// 构造。校验：显示名 1..=128；命令 1..=1024 且不含 NUL；参数逐个 ≤4096 且不含 NUL；
    /// 白名单项为合法非保留环境变量名且不重复；每个绑定的 `name` 必须在白名单内，
    /// `(provider_id, field)` 不重复。
    #[allow(clippy::too_many_arguments)] // 与 §7.3 的 `owned_agent_profile` 列一一对应
    pub fn try_new(
        id: AgentId,
        display_name: &str,
        command: &str,
        args: Vec<String>,
        env_allowlist: Vec<String>,
        env: Vec<ProviderEnvBinding>,
        default: bool,
        created_at: Timestamp,
        updated_at: Timestamp,
    ) -> Result<Self, InvalidValue> {
        require_bounded(display_name, 1, 128)?;
        require_bounded(command, 1, 1024)?;
        if !no_nul(command) {
            return Err(InvalidValue::Field);
        }
        for arg in &args {
            require_bounded(arg, 0, 4096)?;
            if !no_nul(arg) {
                return Err(InvalidValue::Field);
            }
        }
        let mut seen_names: Vec<&str> = Vec::with_capacity(env_allowlist.len());
        for name in &env_allowlist {
            if !is_env_name(name) || is_reserved_env_name(name) {
                return Err(InvalidValue::Field);
            }
            if seen_names.contains(&name.as_str()) {
                return Err(InvalidValue::Field);
            }
            seen_names.push(name);
        }
        let mut seen_bindings: Vec<(&str, &str)> = Vec::with_capacity(env.len());
        for binding in &env {
            if !seen_names.contains(&binding.name()) {
                return Err(InvalidValue::Field);
            }
            let key = (binding.provider_id(), binding.field());
            if seen_bindings.contains(&key) {
                return Err(InvalidValue::Field);
            }
            seen_bindings.push(key);
        }
        Ok(Self {
            id,
            display_name: display_name.to_owned(),
            command: command.to_owned(),
            args,
            env_allowlist,
            env,
            default,
            created_at,
            updated_at,
        })
    }

    /// profile 标识。
    pub fn id(&self) -> &AgentId {
        &self.id
    }

    /// 显示名。
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    /// 可执行文件路径或名字（不经 shell 拼接）。
    pub fn command(&self) -> &str {
        &self.command
    }

    /// 参数（逐项传给子进程，不拼接）。
    pub fn args(&self) -> &[String] {
        &self.args
    }

    /// 环境变量名白名单（是上限而不是提示）。
    pub fn env_allowlist(&self) -> &[String] {
        &self.env_allowlist
    }

    /// 凭据到环境变量的显式绑定；空表示不注入任何凭据。
    pub fn env(&self) -> &[ProviderEnvBinding] {
        &self.env
    }

    /// 是否为默认 profile（全库至多一个）。
    pub fn is_default(&self) -> bool {
        self.default
    }

    /// 创建时间。
    pub fn created_at(&self) -> &Timestamp {
        &self.created_at
    }

    /// 最近更新时间。
    pub fn updated_at(&self) -> &Timestamp {
        &self.updated_at
    }
}

/// 本机 workspace 记录（§11.6）。路径只在本节点可读，不进入 Node Link catalog。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceRecord {
    alias: WorkspaceAlias,
    display_name: String,
    canonical_path: String,
    created_at: Timestamp,
    updated_at: Timestamp,
}

impl WorkspaceRecord {
    /// 构造。显示名 1..=128；路径 1..=4096、不含 NUL 且必须是绝对路径形状
    /// （真正的 `canonicalize`/存在性校验由 [`ResolvedWorkspace`] 的解析路径负责）。
    pub fn try_new(
        alias: WorkspaceAlias,
        display_name: &str,
        canonical_path: &str,
        created_at: Timestamp,
        updated_at: Timestamp,
    ) -> Result<Self, InvalidValue> {
        require_bounded(display_name, 1, 128)?;
        require_bounded(canonical_path, 1, 4096)?;
        if !no_nul(canonical_path) || !Path::new(canonical_path).is_absolute() {
            return Err(InvalidValue::Field);
        }
        Ok(Self {
            alias,
            display_name: display_name.to_owned(),
            canonical_path: canonical_path.to_owned(),
            created_at,
            updated_at,
        })
    }

    /// 别名（与 Export 的 `workspace_aliases[].alias` 同一命名空间）。
    pub fn alias(&self) -> &WorkspaceAlias {
        &self.alias
    }

    /// 显示名。
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    /// 规范化绝对路径。
    pub fn canonical_path(&self) -> &str {
        &self.canonical_path
    }

    /// 创建时间。
    pub fn created_at(&self) -> &Timestamp {
        &self.created_at
    }

    /// 最近更新时间。
    pub fn updated_at(&self) -> &Timestamp {
        &self.updated_at
    }
}

/// Sync 快照 `workspaces` 资源的条目（`SYNC_PROTOCOL.md` §9.4；§3.7）。
///
/// 它是 [`WorkspaceRecord`] 的**投影**：`alias`/`display_name` 逐字取自记录，`path` 只在客户端协商了
/// feature `core.workspace-path.v1` 时非空。`None` 的语义是「该键**缺席**」（不是空串、不是 `null`）
/// ——已协商 `core.local-catalog.v1` 的旧客户端在封闭对象上多收到一个键会按 `protocol.schema_invalid`
/// 拒收整条快照，因此路径必须由协商结果门控（`design.md` D1/D7）。
///
/// `path` 的取值是已登记目录的规范化路径**剥掉平台 verbatim 前缀**后的形式（`design.md` D2）：客户端
/// 回传该形式时本机重新 `canonicalize` 并以其结果为权威值，因此显示形式与权威值不会分叉。路径只出现在
/// 本类型的 `path` 上——会话摘要的目录引用仍只有 `{alias, displayName}` 两个键。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceCatalogEntry {
    alias: WorkspaceAlias,
    display_name: String,
    path: Option<String>,
}

impl WorkspaceCatalogEntry {
    /// 构造。显示名与路径（若在场）的约束与 [`WorkspaceRecord`] 逐条相同：1..=4096 字符、不含 NUL、
    /// 绝对路径形状（真正的 `canonicalize` 由解析路径负责）。
    pub fn try_new(
        alias: WorkspaceAlias,
        display_name: &str,
        path: Option<&str>,
    ) -> Result<Self, InvalidValue> {
        require_bounded(display_name, 1, 128)?;
        if let Some(path) = path {
            require_bounded(path, 1, 4096)?;
            if !no_nul(path) || !Path::new(path).is_absolute() {
                return Err(InvalidValue::Field);
            }
        }
        Ok(Self {
            alias,
            display_name: display_name.to_owned(),
            path: path.map(str::to_owned),
        })
    }

    /// 别名（与 Export 的 `workspace_aliases[].alias` 同一命名空间）。
    pub fn alias(&self) -> &WorkspaceAlias {
        &self.alias
    }

    /// 显示名。
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    /// 规范化路径；`None` = 客户端未协商 `core.workspace-path.v1`（该键缺席）。
    pub fn path(&self) -> Option<&str> {
        self.path.as_deref()
    }
}

token_enum!(
    /// Provider 引用的类别（§11.6/§11.7）。
    ProviderRefKind {
        Provider => "provider",
        Mcp => "mcp",
    }
);

/// Provider 引用（§11.6）：**只记字段名、引用与版本，永不记值**。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderRef {
    id: String,
    kind: ProviderRefKind,
    display_name: String,
    configured_fields: Vec<String>,
    keystore_ref: String,
    version: u64,
    updated_at: Timestamp,
}

impl ProviderRef {
    /// 构造。`id` 匹配 `^[A-Za-z0-9._-]{1,64}$`；显示名 1..=128；字段名逐个 1..=128 且不重复；
    /// `keystore_ref` 1..=256；`version ≥ 1`。
    #[allow(clippy::too_many_arguments)] // 与 §7.3 的 `owned_provider_ref` 列一一对应
    pub fn try_new(
        id: &str,
        kind: ProviderRefKind,
        display_name: &str,
        configured_fields: Vec<String>,
        keystore_ref: &str,
        version: u64,
        updated_at: Timestamp,
    ) -> Result<Self, InvalidValue> {
        if !is_provider_id(id) {
            return Err(InvalidValue::Field);
        }
        require_bounded(display_name, 1, 128)?;
        require_bounded(keystore_ref, 1, 256)?;
        if version == 0 {
            return Err(InvalidValue::Field);
        }
        let mut seen: Vec<&str> = Vec::with_capacity(configured_fields.len());
        for field in &configured_fields {
            if !is_spec_id(field) {
                return Err(InvalidValue::Field);
            }
            if seen.contains(&field.as_str()) {
                return Err(InvalidValue::Field);
            }
            seen.push(field);
        }
        Ok(Self {
            id: id.to_owned(),
            kind,
            display_name: display_name.to_owned(),
            configured_fields,
            keystore_ref: keystore_ref.to_owned(),
            version,
            updated_at,
        })
    }

    /// Provider 标识。
    pub fn id(&self) -> &str {
        &self.id
    }

    /// 类别。
    pub fn kind(&self) -> ProviderRefKind {
        self.kind
    }

    /// 显示名。
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    /// 已配置字段名（只有名字，没有值）。
    pub fn configured_fields(&self) -> &[String] {
        &self.configured_fields
    }

    /// 平台 keystore 条目引用（不是凭据本身）。
    pub fn keystore_ref(&self) -> &str {
        &self.keystore_ref
    }

    /// 引用版本；换绑必须递增（§11.2 第 7 条）。
    pub fn version(&self) -> u64 {
        self.version
    }

    /// 最近更新时间。
    pub fn updated_at(&self) -> &Timestamp {
        &self.updated_at
    }
}

/// 首次初始化状态（§11.6）：`seeded = false` 时启动流程才能导入种子。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeedState {
    seeded: bool,
    seeded_at: Option<Timestamp>,
}

impl SeedState {
    /// 构造。`seeded_at` 非空 ⟺ `seeded`。
    pub fn try_new(seeded: bool, seeded_at: Option<Timestamp>) -> Result<Self, InvalidValue> {
        if seeded_at.is_some() != seeded {
            return Err(InvalidValue::Field);
        }
        Ok(Self { seeded, seeded_at })
    }

    /// 尚未初始化。
    pub fn unseeded() -> Self {
        Self {
            seeded: false,
            seeded_at: None,
        }
    }

    /// 是否已初始化。
    pub fn is_seeded(&self) -> bool {
        self.seeded
    }

    /// 初始化时间。
    pub fn seeded_at(&self) -> Option<&Timestamp> {
        self.seeded_at.as_ref()
    }
}

/// 不透明凭据值：只经 [`super::super::ports::CredentialResolver`] 进出。
///
/// 刻意**不**实现 `Debug`/`Serialize`，也不提供 `Display`：它不得落盘、进事件、进日志或进错误消息。
pub struct SecretValue(String);

impl SecretValue {
    /// 包装一个已从 keystore 取出的凭据值。
    pub fn new(value: String) -> Self {
        Self(value)
    }

    /// 取出明文——只在构造子进程环境的边界调用。
    pub fn expose_secret(&self) -> &str {
        &self.0
    }

    /// 值长度（用于日志只记数量/长度的场合，不泄露内容）。
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// 是否为空值。
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// 已解析的 workspace（§11.9）：**可选**符号名 + 规范化后的本机绝对路径。
///
/// 别名只在**解析出别名**时存在（`workspace-resolution` 的「创建时别名的持久化与投影来源」）：
/// 目录来源是本机已登记别名时带上别名原文，来源是客户端提供的目录（仅 Sync 面）时**没有别名**，
/// 该会话的目录归属因此是「未分组」。别名与路径任一缺席都不得由另一项推导补齐——因此这里没有
/// 空串或占位别名，`None` 就是「本次解析没有别名」。
///
/// 路径只能存进 [`super::CreateSessionRequest`] 并交给后端，不得进事件、不得随 Node Link/Sync 下发。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedWorkspace {
    alias: Option<WorkspaceAlias>,
    canonical_path: String,
}

impl ResolvedWorkspace {
    /// 按**本机已登记别名**构造。路径必须是绝对路径形状（`canonicalize` 与目录判定由用例层的
    /// 解析路径完成）。
    pub fn try_new(alias: WorkspaceAlias, canonical_path: String) -> Result<Self, InvalidValue> {
        Self::checked(Some(alias), canonical_path)
    }

    /// 按**客户端提供的目录**构造（没有别名）。形状口径与 [`Self::try_new`] 逐条相同；目录是否
    /// 已登记与本构造器无关——未登记目录不得因此被接受为别名。
    pub fn at_path(canonical_path: String) -> Result<Self, InvalidValue> {
        Self::checked(None, canonical_path)
    }

    fn checked(
        alias: Option<WorkspaceAlias>,
        canonical_path: String,
    ) -> Result<Self, InvalidValue> {
        require_bounded(&canonical_path, 1, 4096)?;
        if !no_nul(&canonical_path) || !Path::new(&canonical_path).is_absolute() {
            return Err(InvalidValue::Field);
        }
        Ok(Self {
            alias,
            canonical_path,
        })
    }

    /// 符号名；`None` = 本次解析没有别名（目录由客户端提供），会话的目录归属是「未分组」。
    pub fn alias(&self) -> Option<&WorkspaceAlias> {
        self.alias.as_ref()
    }

    /// 规范化后的本机绝对路径。
    pub fn canonical_path(&self) -> &str {
        &self.canonical_path
    }
}

/// `session.create` 的目录来源（`workspace-resolution` 的「解析归属与后端输入」）。
///
/// 三种来源的失败分类不同（见 `UseCases::create_session`）：**别名未在本机登记**是参数类错误
/// （客户端可修正），**目录形状非法**（相对路径、含 `..`）同样是参数类错误且不触碰文件系统，
/// 而**形式合法却解析不了**（目录被删除、被换成文件、`canonicalize` 失败、权限不足）是服务端
/// 不可用类；**置了建目录开关**时目录可以不存在，由本层先创建（连同缺失的父层），创建失败或路径上
/// 已存在非目录成分同样按服务端不可用类拒绝（`sync-session-create` 的「目录校验与失败分类」）。
/// 目录来源只经 Sync 面出现（Node Link 面的请求不得携带任意 Owner 路径）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceSource {
    /// 本机已登记的 workspace 别名（Node Link 与 Sync 的 `workspaceAlias`）。
    Alias(WorkspaceAlias),
    /// 客户端提供的目录原文（仅 Sync 面的 `workspacePath`）：本层只校验形状与文件系统事实，
    /// **不查**登记表、也**不登记**新目录。默认（未置建目录开关）时该目录必须**已存在**且是目录。
    Path(String),
    /// 客户端提供的目录原文 + 显式的「目录不存在则先创建」开关（仅 Sync 面的 `workspacePath` +
    /// `createIfMissing: true`）：该目录**可以不存在**，系统先创建它（连同缺失的父层）再启动 Agent。
    ///
    /// 开关只可能是这个变体形态：`sync-session-create` 要求开关与已登记别名**互斥**（别名解析出的目录
    /// 必然已存在），而别名必然已存在是登记的前提——因此这里没有「别名 + 开关」的组合可表达，
    /// 该互斥在类型上成立，不依赖运行时校验。被创建的目录**不登记**为别名：它没有别名，
    /// 该会话的目录归属因此是「未分组」。
    PathWithCreate(String),
}
