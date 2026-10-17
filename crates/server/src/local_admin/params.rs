//! 方法参数校验与 `PortError` → `local.*` 映射
//! （`docs/LOCAL_ADMIN_PROTOCOL.md` §1.1、§5.2–§5.6、§6）。
//!
//! 本模块是路由的**唯一**参数形状知识所在：`params` 与嵌套对象都是 closed object（§1.1），
//! 缺字段、类型不符或出现未知字段一律 `local.invalid_params`（§4 规则 4）。校验函数是纯函数：
//! 不接触存储、时钟、keystore 与日志；只有 §5.2 的 `rootPath` 与 §5.6 的 `outputPath` 需要按合同
//! 判定文件系统事实（已存在的绝对目录 / 绝对路径），那是可判定规则本身的一部分。
//!
//! 两条口径（在报告里登记）：
//!
//! - 错误消息里只回显**标识符类**的对端输入（参数名、方法名、audit 类别名），不回显自由文本值
//!   （路径、命令、显示名、凭据值）——§4 规则 6 禁止 secret、堆栈与完整敏感路径。
//! - 语义级词表（环境变量名与其保留前缀、`grant.*`、时间戳格式）**只有 core 一份定义**：本层只做
//!   类型与结构校验，语义拒绝由 core 的构造器返回 `InvalidRequest`，再由 [`map_port_error`] 收敛成
//!   `local.invalid_params`，避免出现第二份词表。

use std::path::{Component, Path, PathBuf};

use acp_core::model::{
    AgentId, AuditAction, ConflictKind, DeviceId, NodeId, PairingId, PairingRecord, PairingState,
    PortError, Timestamp,
};
use identity_auth::PairingError;
use serde_json::Value;

use crate::local_admin::daemon::MAX_STOP_GRACE_MS;
use crate::local_admin::envelope::JsonObject;
use crate::local_admin::error::{AdminError, LocalErrorCode};

/// `displayName` 的长度上界（§5.2/§5.5 的 `≤128`）。
const MAX_DISPLAY_NAME_CHARS: usize = 128;

/// `device.pair.reject`/`node.pair.reject` 的 `reason` 长度上界：与 `core::model` 的
/// `PairingSettlement::rejected` 是同一边界（§5.3/§5.4 只登记 `reason: string | null`，长度由核心
/// 值对象决定）。本层提前判，使超长回 `local.invalid_params` 而不是端口错误。
const MAX_REJECT_REASON_CHARS: usize = 256;

/// `local.invalid_params`（§6）。
pub fn invalid_params(message: impl AsRef<str>) -> AdminError {
    AdminError::new(LocalErrorCode::InvalidParams, message)
}

// ---------------------------------------------------------------------------------------------
// 通用取值助手（closed object + 类型校验）
// ---------------------------------------------------------------------------------------------

/// 拒绝本文档未登记的 `params` 字段（§1.1：`params` 是 closed object）。
pub(crate) fn reject_unknown_fields(
    params: &JsonObject,
    allowed: &[&str],
) -> Result<(), AdminError> {
    match params.keys().find(|key| !allowed.contains(&key.as_str())) {
        Some(key) => Err(invalid_params(format!("unknown parameter `{key}`"))),
        None => Ok(()),
    }
}

/// 取一个必需的字段（缺失即 `local.invalid_params`）。
fn value<'a>(params: &'a JsonObject, key: &str) -> Result<&'a Value, AdminError> {
    params
        .get(key)
        .ok_or_else(|| invalid_params(format!("missing parameter `{key}`")))
}

fn string(params: &JsonObject, key: &str) -> Result<String, AdminError> {
    value(params, key)?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| invalid_params(format!("parameter `{key}` must be a string")))
}

/// `null` 与非 `null` 两态的可选字符串（§5.2 的 `graceMs`/§5.6 的 `since`/`until` 风格）。
fn optional_string(params: &JsonObject, key: &str) -> Result<Option<String>, AdminError> {
    match value(params, key)? {
        Value::Null => Ok(None),
        Value::String(text) => Ok(Some(text.clone())),
        _ => Err(invalid_params(format!(
            "parameter `{key}` must be a string or null"
        ))),
    }
}

fn object<'a>(params: &'a JsonObject, key: &str) -> Result<&'a JsonObject, AdminError> {
    match value(params, key)? {
        Value::Object(map) => Ok(map),
        _ => Err(invalid_params(format!(
            "parameter `{key}` must be an object"
        ))),
    }
}

fn array<'a>(params: &'a JsonObject, key: &str) -> Result<&'a Vec<Value>, AdminError> {
    match value(params, key)? {
        Value::Array(items) => Ok(items),
        _ => Err(invalid_params(format!(
            "parameter `{key}` must be an array"
        ))),
    }
}

fn strings(params: &JsonObject, key: &str) -> Result<Vec<String>, AdminError> {
    array(params, key)?
        .iter()
        .map(|item| {
            item.as_str()
                .map(str::to_owned)
                .ok_or_else(|| invalid_params(format!("every item of `{key}` must be a string")))
        })
        .collect()
}

/// 布尔字段。
fn boolean(params: &JsonObject, key: &str) -> Result<bool, AdminError> {
    value(params, key)?
        .as_bool()
        .ok_or_else(|| invalid_params(format!("parameter `{key}` must be a boolean")))
}

/// `displayName` 形态：1..=128 个字符（Unicode code point，与 JSON Schema 的 `maxLength` 同口径）。
fn display_name(params: &JsonObject, key: &str) -> Result<String, AdminError> {
    let text = string(params, key)?;
    let count = text.chars().count();
    if count == 0 || count > MAX_DISPLAY_NAME_CHARS {
        return Err(invalid_params(format!(
            "parameter `{key}` must be 1..={MAX_DISPLAY_NAME_CHARS} characters"
        )));
    }
    Ok(text)
}

/// `^[A-Za-z0-9._-]{1,max}$`（`providerId` 与 `values` 的字段名；core 未导出这两个谓词）。
fn is_spec_name(text: &str, max: usize) -> bool {
    let bytes = text.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= max
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn spec_name(params: &JsonObject, key: &str, max: usize) -> Result<String, AdminError> {
    let text = string(params, key)?;
    if !is_spec_name(&text, max) {
        return Err(invalid_params(format!(
            "parameter `{key}` must match ^[A-Za-z0-9._-]{{1,{max}}}$"
        )));
    }
    Ok(text)
}

/// `^[A-Za-z0-9._-]{1,max}$` 形态判定（供嵌套字段复用）。
fn require_spec_name(text: &str, max: usize, what: &str) -> Result<(), AdminError> {
    if is_spec_name(text, max) {
        Ok(())
    } else {
        Err(invalid_params(format!(
            "{what} must match ^[A-Za-z0-9._-]{{1,{max}}}$"
        )))
    }
}

/// §1.1 的时间戳文本（UTC、毫秒、`Z`）。
fn timestamp(params: &JsonObject, key: &str) -> Result<Option<Timestamp>, AdminError> {
    match optional_string(params, key)? {
        Some(text) => Timestamp::new(&text).map(Some).map_err(|_| {
            invalid_params(format!(
                "parameter `{key}` must be a UTC RFC 3339 timestamp"
            ))
        }),
        None => Ok(None),
    }
}

/// 由 core 的具名构造错误生成 `local.invalid_params`（词表在 core，本层只翻译错误码）。
fn from_invalid(what: &str, error: acp_core::model::InvalidValue) -> AdminError {
    invalid_params(format!("{what} is invalid: {error}"))
}

// ---------------------------------------------------------------------------------------------
// daemon.*（§5.2）
// ---------------------------------------------------------------------------------------------

/// `daemon.status`：无参数（§1.1：无参数方法必须发送 `{}`）。
pub fn daemon_status(params: &JsonObject) -> Result<(), AdminError> {
    reject_unknown_fields(params, &[])
}

/// `daemon.stop` 的 `graceMs`：`null` = 使用配置值，非空时 `0..=60000`（§5.2）。
pub fn daemon_stop(params: &JsonObject) -> Result<Option<u64>, AdminError> {
    reject_unknown_fields(params, &["graceMs"])?;
    match value(params, "graceMs")? {
        Value::Null => Ok(None),
        Value::Number(number) => {
            let grace_ms = number.as_u64().ok_or_else(|| {
                invalid_params("parameter `graceMs` must be an integer in 0..=60000")
            })?;
            if grace_ms > MAX_STOP_GRACE_MS {
                return Err(invalid_params(format!(
                    "parameter `graceMs` must be at most {MAX_STOP_GRACE_MS}"
                )));
            }
            Ok(Some(grace_ms))
        }
        _ => Err(invalid_params(
            "parameter `graceMs` must be an integer or null",
        )),
    }
}

// ---------------------------------------------------------------------------------------------
// workspace.select / agent.configure / provider.configure（§5.2）
// ---------------------------------------------------------------------------------------------
// audit.export（§5.6）
// ---------------------------------------------------------------------------------------------

/// 审计导出的文件格式（§5.6）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditFormat {
    /// `jsonl`：每行一个 JSON 对象。
    Jsonl,
    /// `csv`：首行为列名。
    Csv,
}

impl AuditFormat {
    /// 解析 wire 文本（§5.6：`jsonl` | `csv`）。
    fn parse(text: &str) -> Option<Self> {
        match text {
            "jsonl" => Some(Self::Jsonl),
            "csv" => Some(Self::Csv),
            _ => None,
        }
    }
}

/// `audit.export` 的已校验参数（§5.6）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditExport {
    /// 输出文件路径（绝对路径；文件已存在属 `local.conflict`，由写出阶段判定）。
    pub output_path: PathBuf,
    /// 输出格式。
    pub format: AuditFormat,
    /// 时间区间下界（含）。
    pub since: Option<Timestamp>,
    /// 时间区间上界（含）。
    pub until: Option<Timestamp>,
    /// §14.2 的审计类别名；空数组 = 不按类别过滤（必须显式给出该字段）。
    pub categories: Vec<AuditAction>,
}

/// `audit.export`（§5.6）：`outputPath` 必须绝对；`categories` 必须显式给出（可为空数组）。
pub fn audit_export(params: &JsonObject) -> Result<AuditExport, AdminError> {
    reject_unknown_fields(
        params,
        &["outputPath", "format", "since", "until", "categories"],
    )?;

    let output_path = string(params, "outputPath")?;
    if output_path.is_empty() || !Path::new(&output_path).is_absolute() {
        return Err(invalid_params(
            "parameter `outputPath` must be an absolute path",
        ));
    }

    let format_text = string(params, "format")?;
    let format = AuditFormat::parse(&format_text)
        .ok_or_else(|| invalid_params("parameter `format` must be `jsonl` or `csv`"))?;

    let since = timestamp(params, "since")?;
    let until = timestamp(params, "until")?;

    let mut categories = Vec::new();
    for category in strings(params, "categories")? {
        let action = category.parse::<AuditAction>().map_err(|_| {
            invalid_params(format!(
                "parameter `categories` contains `{category}`, which is not a documented audit category"
            ))
        })?;
        if !categories.contains(&action) {
            categories.push(action);
        }
    }

    Ok(AuditExport {
        output_path: PathBuf::from(output_path),
        format,
        since,
        until,
        categories,
    })
}

// ---------------------------------------------------------------------------------------------
// device.pair.* / device.list / device.revoke / node.pair.* / node.list / node.revoke（§5.3/§5.4）
// ---------------------------------------------------------------------------------------------

/// `device.pair.begin` 的已校验参数（§5.3）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DevicePairBegin {
    /// 请求的有效期（毫秒）；`None` = 用满 5 分钟窗口，只能收窄。
    pub expires_in_ms: Option<u64>,
}

/// `device.pair.begin`（§5.3）：只接受 `expiresInMs`（可空）。
///
/// 设备配对**不提交任何授权词汇**（`pairing-model` 删除设备权限层）：配对不授予授权，因此
/// `requestedPacks`/`requestedScopes` 之类的键不再是本方法的输入，出现即 `local.invalid_params`
/// （`params` 是 closed object）。节点配对同样不携带授权集合。
pub fn device_pair_begin(params: &JsonObject) -> Result<DevicePairBegin, AdminError> {
    reject_unknown_fields(params, &["expiresInMs"])?;
    Ok(DevicePairBegin {
        expires_in_ms: pairing_window(params)?,
    })
}

/// `node.pair.begin` 的已校验参数（§5.4）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodePairBegin {
    /// `mode = "owner"`：本节点创建一次性配对并返回二维码 URL。
    Owner,
    /// `mode = "access"`：本切片不支持（路由层回 `local.unsupported`，不发任何出站请求）。
    Access,
}

/// `node.pair.begin`（§5.4）：`mode` 决定分支；`owner` 分支要求 `pairingUrl` 为 `null`。
///
/// `mode = "access"` 时**不继续解析**其余字段：本切片不实现该模式，把「不支持」报成「参数非法」
/// 会误导调用方（§5.4 的 `mode = "access"` 分支与 `node.rotate-key.begin` 同类）。
///
pub fn node_pair_begin(params: &JsonObject) -> Result<NodePairBegin, AdminError> {
    reject_unknown_fields(params, &["mode", "pairingUrl"])?;
    let mode = string(params, "mode")?;
    match mode.as_str() {
        "access" => Ok(NodePairBegin::Access),
        "owner" => {
            if optional_string(params, "pairingUrl")?.is_some() {
                return Err(invalid_params(
                    "node.pair.begin: `pairingUrl` must be null when `mode` is `owner`",
                ));
            }
            Ok(NodePairBegin::Owner)
        }
        _ => Err(invalid_params(
            "parameter `mode` must be `owner` or `access`",
        )),
    }
}

/// `device.pair.confirm` 的已校验参数（§5.3）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DevicePairConfirm {
    /// 配对目标。
    pub pairing_id: PairingId,
}

/// `device.pair.confirm`（§5.3）：只接受 `{ pairingId }`。
///
/// 设备配对不授予授权，因此确认**不接受**任何授权集合参数（`scopes` 之类出现即
/// `local.invalid_params`）；节点确认同样不接受任何授权集合。
pub fn device_pair_confirm(params: &JsonObject) -> Result<DevicePairConfirm, AdminError> {
    reject_unknown_fields(params, &["pairingId"])?;
    let pairing_id = pairing_id_field(params)?;
    Ok(DevicePairConfirm { pairing_id })
}

/// `node.pair.confirm` 的已校验参数（§5.4）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodePairConfirm {
    /// 配对目标。
    pub pairing_id: PairingId,
}

/// `node.pair.confirm`（§5.4）：只接受 `{ pairingId }`；任何授权集合字段都按未知字段拒绝。
pub fn node_pair_confirm(params: &JsonObject) -> Result<NodePairConfirm, AdminError> {
    reject_unknown_fields(params, &["pairingId"])?;
    let pairing_id = pairing_id_field(params)?;
    Ok(NodePairConfirm { pairing_id })
}

/// `device.pair.reject` / `node.pair.reject` 的已校验参数（§5.3/§5.4）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingReject {
    /// 配对目标。
    pub pairing_id: PairingId,
    /// 可选简短原因（≤150 字符；不进审计正文）。
    pub reason: Option<String>,
}

/// `device.pair.reject` / `node.pair.reject`（§5.3/§5.4）：`{ pairingId, reason }`。
pub fn pairing_reject(params: &JsonObject) -> Result<PairingReject, AdminError> {
    reject_unknown_fields(params, &["pairingId", "reason"])?;
    let pairing_id = pairing_id_field(params)?;
    let reason = optional_string(params, "reason")?;
    if let Some(reason) = &reason {
        if reason.chars().count() > MAX_REJECT_REASON_CHARS {
            return Err(invalid_params(format!(
                "parameter `reason` must be at most {MAX_REJECT_REASON_CHARS} characters"
            )));
        }
    }
    Ok(PairingReject { pairing_id, reason })
}

/// `device.pair.status` / `device.pair.reject` / `node.pair.status` / `node.pair.reject` 的 `pairingId`。
pub fn pairing_id(params: &JsonObject) -> Result<PairingId, AdminError> {
    reject_unknown_fields(params, &["pairingId"])?;
    pairing_id_field(params)
}

/// `device.revoke`（§5.3）：`{ deviceId }`。
pub fn device_revoke(params: &JsonObject) -> Result<DeviceId, AdminError> {
    reject_unknown_fields(params, &["deviceId"])?;
    let text = string(params, "deviceId")?;
    DeviceId::new(&text).map_err(|error| from_invalid("parameter `deviceId`", error))
}

/// `node.revoke`（§5.4）：`{ nodeId }`。
pub fn node_revoke(params: &JsonObject) -> Result<NodeId, AdminError> {
    reject_unknown_fields(params, &["nodeId"])?;
    let text = string(params, "nodeId")?;
    NodeId::new(&text).map_err(|error| from_invalid("parameter `nodeId`", error))
}

/// 已确认/已拒绝的判定（§5.3/§5.4、spec 「过期与拒绝不创建信任」）：
///
/// - 记录已过期（含时钟判定的过期）或已被拒绝 → `local.expired`（§6：`local.expired` 专服务这一对
///   语义，"`pairingId` 已过期或已被拒绝"）；
/// - 仍未认领，或已批准/已消费 → `local.conflict`（状态不允许该操作）；
/// - 否则放行（`pending_confirmation` 且未过期）。
///
/// 这里**不**替代状态机与存储的判定：放行后仍由 `Authority::settle` 与写集做同一组检查，本层只负责把
/// 「终态」映射成协议文档规定的错误码（存储对 `rejected` 回的是 `ConflictKind::Consumed`，直接透传会
/// 变成 `local.conflict`，与 spec 要求的 `local.expired` 不符）。
pub fn require_pending_confirmation(
    operation: &str,
    record: &PairingRecord,
    now: &Timestamp,
) -> Result<(), AdminError> {
    // 时钟判定的过期与记录状态无关：从未认领的 `created` 配对超过 `expiresAt` 后同样是「已过期」（存储
    // 的过期扫描滞后于调用，路由不能等它才给出正确错误码）。
    let expired_by_clock = now.as_str() >= record.expires_at().as_str();
    match record.state() {
        PairingState::Rejected | PairingState::Expired => Err(expired(operation)),
        PairingState::Created | PairingState::PendingConfirmation if expired_by_clock => {
            Err(expired(operation))
        }
        PairingState::PendingConfirmation => Ok(()),
        _ => Err(AdminError::new(
            LocalErrorCode::Conflict,
            format!(
                "{operation}: pairing state `{}` does not allow this operation",
                record.state().as_str()
            ),
        )),
    }
}

/// `local.expired`（§6：`pairingId` 已过期或已被拒绝）。
fn expired(operation: &str) -> AdminError {
    AdminError::new(
        LocalErrorCode::Expired,
        format!("{operation}: pairing has expired or was rejected"),
    )
}

/// `expiresInMs`（§5.3）：`null` = 用满 5 分钟；非空时 `1..=PAIRING_WINDOW_MS`（只能收窄）。
fn pairing_window(params: &JsonObject) -> Result<Option<u64>, AdminError> {
    match value(params, "expiresInMs")? {
        Value::Null => Ok(None),
        Value::Number(number) => {
            let window = number.as_u64().filter(|window| {
                *window > 0 && *window <= crate::local_admin::pairing::PAIRING_WINDOW_MS
            });
            window.map(Some).ok_or_else(|| {
                invalid_params(format!(
                    "parameter `expiresInMs` must be an integer in 1..={}",
                    crate::local_admin::pairing::PAIRING_WINDOW_MS
                ))
            })
        }
        _ => Err(invalid_params(
            "parameter `expiresInMs` must be an integer or null",
        )),
    }
}

/// `pairingId`：canonical 小写 UUID（core 的 [`PairingId`] 是唯一校验入口）。
fn pairing_id_field(params: &JsonObject) -> Result<PairingId, AdminError> {
    let text = string(params, "pairingId")?;
    PairingId::new(&text).map_err(|error| from_invalid("parameter `pairingId`", error))
}

// ---------------------------------------------------------------------------------------------
// PortError → local.*（§6）
// ---------------------------------------------------------------------------------------------

/// `PortError` → `local.*`（§6；映射表逐条见报告与 `LOCAL_ADMIN_PROTOCOL.md` §6）。
///
/// 口径：
///
/// - `NotFound` → `local.not_found`；`Conflict` → `local.conflict`，但 `ConflictKind::Expired`
///   （`pairing.expired`）按 §6 映射为 `local.expired`；
/// - `Unavailable(_)`（含 `KeystoreUnavailable`）→ `local.unavailable`；
/// - `InvalidRequest`（含 `require_local` 的 `authorization.scope_denied`）→ `local.invalid_params`；
/// - `Corrupt`/`Backend` → `local.internal`，且**不转述**内层文本（适配器文本可能含 SQL、路径或 prompt）。
///
/// `operation` 只允许传方法名常量：错误消息与日志里不得出现对端给的自由文本值。
pub fn map_port_error(operation: &str, error: PortError) -> AdminError {
    match error {
        PortError::NotFound(target) => AdminError::new(
            LocalErrorCode::NotFound,
            format!("{operation}: {target} does not exist"),
        ),
        PortError::Conflict(ConflictKind::Expired) => AdminError::new(
            LocalErrorCode::Expired,
            format!("{operation}: pairing has expired or was rejected"),
        ),
        PortError::Conflict(kind) => AdminError::new(
            LocalErrorCode::Conflict,
            format!("{operation}: {kind} conflicts with the current state"),
        ),
        PortError::Unavailable(kind) => {
            AdminError::new(LocalErrorCode::Unavailable, format!("{operation}: {kind}"))
        }
        PortError::InvalidRequest(reason) => AdminError::new(
            LocalErrorCode::InvalidParams,
            format!("{operation}: {reason}"),
        ),
        PortError::Corrupt(_) | PortError::Backend(_) => AdminError::new(
            LocalErrorCode::Internal,
            format!("{operation}: unexpected internal error"),
        ),
    }
}

/// `identity_auth::PairingError` → `local.*`（§5.3/§5.4、§6）。
///
/// 映射口径（与 [`map_port_error`] 同源）：
///
/// - `Expired` → `local.expired`；
/// - `WrongState`/`NotClaimable` → `local.conflict`（状态不允许该操作）；
/// - 集合越界/窗口非法/目标族不匹配 → `local.invalid_params`（参数越界）；
/// - `SecretUnavailable`/`EntropyUnavailable` → `local.unavailable`（重启或熵源不可用时重试语义）；
/// - 其余（transcript/证明/绑定/标识不一致）→ `local.internal`，且**不转述**内层文本：
///   `error.message` 必须是简短英文描述，内层的诊断是中文且可能回显对端输入（§4 规则 6）。
pub fn map_pairing_error(operation: &str, error: PairingError) -> AdminError {
    match error {
        PairingError::Expired => AdminError::new(
            LocalErrorCode::Expired,
            format!("{operation}: pairing has expired or was rejected"),
        ),
        PairingError::WrongState | PairingError::NotClaimable => AdminError::new(
            LocalErrorCode::Conflict,
            format!("{operation}: pairing state does not allow this operation"),
        ),
        PairingError::CapabilityKindMismatch
        | PairingError::InvalidWindow
        | PairingError::TargetMismatch
        | PairingError::CapabilitiesExceedRegistered
        | PairingError::CapabilitiesExceedRequested
        | PairingError::Invalid(_) => invalid_params(format!(
            "{operation}: pairing parameters or sets are out of the allowed range"
        )),
        PairingError::SecretUnavailable => AdminError::new(
            LocalErrorCode::Unavailable,
            format!("{operation}: pairing secret is no longer held in memory"),
        ),
        PairingError::EntropyUnavailable => AdminError::new(
            LocalErrorCode::Unavailable,
            format!("{operation}: system entropy is not available"),
        ),
        PairingError::ClaimMismatch
        | PairingError::BindingMismatch
        | PairingError::Proof(_)
        | PairingError::Transcript(_) => AdminError::new(
            LocalErrorCode::Internal,
            format!("{operation}: pairing state machine rejected the stored facts"),
        ),
    }
}

