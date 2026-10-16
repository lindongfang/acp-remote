//! 授权词表展开（合同 §6.1）：`grant.*` → 命令级 scope。
//!
//! 唯一机器来源是 [`compatibility/commands/v1/commands.json`](../../../compatibility/commands/v1/commands.json)
//! （路径相对仓库根）；本文件的两张表是它的 Rust 镜像，由 `tests/authorization.rs` 逐项断言（漂移即失败）。
//! 展开结果只允许是**独立命令名**：`grant.*` 不进 wire、不进记录；`local.*` 7 项永不远程授予。
//!
//! 设备方向在这里**没有**任何词汇：设备不再有授权维度（`pairing-model` 删除设备权限层），
//! 因此本模块不再有设备侧的入口，提交设备授权词汇的地方（配对请求集合）由 `RequestedCapabilities`
//! 的定向校验拒绝。

use std::collections::BTreeSet;

use acp_core::model::{GrantSet, ScopeSet};

use crate::error::AuthorizationError;

/// 跨节点导出授权（`grant.*` → 命令名集合；Node Link Export 的 `scopes` 用同一张表）。
pub const GRANTS: &[(&str, &[&str])] = &[
    (
        "grant.observe",
        &[
            "session.list",
            "session.read",
            "command.status",
            "session.mode.list",
            "session.config.list",
            "workspace.browse",
        ],
    ),
    (
        "grant.interact",
        &["session.prompt", "session.cancel", "elicitation.respond"],
    ),
    (
        "grant.configure-session",
        &["session.mode.set", "session.config.set"],
    ),
    ("grant.approve", &["permission.resolve"]),
    ("grant.remote-work", &["session.create", "session.resume"]),
];

/// 7 项本地管理能力：永不远程授予，也不参与任何展开（`SECURITY_DESIGN.md` §10.3）。
pub const LOCAL_CAPABILITIES: &[&str] = &[
    "local.workspace.select",
    "local.agent.configure",
    "local.provider.configure",
    "local.device.manage",
    "local.export.manage",
    "local.node.rotate-key",
    "local.audit.export",
];

/// 节点 `node.pair.begin` 的请求（`requestedGrants`），也是 Export 侧 scope 表达的来源。
///
/// 返回两件东西：记录用的 grant 名集合，以及它展开出的命令级 scope（Export 侧使用）。
/// 未知/拼错名称、以及任何 `local.*` 一律显式拒绝（不做部分展开）。
pub fn expand_node_request(
    requested_grants: &[String],
) -> Result<GrantRequest, AuthorizationError> {
    let mut grants = GrantSet::empty();
    let mut scopes = ScopeSet::empty();
    for grant in requested_grants {
        reject_local(grant)?;
        for command in expand_grant_scopes(grant)? {
            scopes.insert(&command)?;
        }
        grants.insert(grant)?;
    }
    Ok(GrantRequest { grants, scopes })
}

/// 一个导出授权的展开结果（命令级 scope）。
pub fn expand_grant_scopes(name: &str) -> Result<Vec<String>, AuthorizationError> {
    reject_local(name)?;
    let members =
        lookup(GRANTS, name).ok_or_else(|| AuthorizationError::UnknownName(name.to_owned()))?;
    Ok(members.iter().map(|member| (*member).to_owned()).collect())
}

/// 全部命令名（`grants` 的成员集合；与 `commands.json` 的 14 条命令逐项相等，由测试断言）。
pub fn command_names() -> BTreeSet<&'static str> {
    let mut names = BTreeSet::new();
    for (_, members) in GRANTS.iter() {
        for member in *members {
            names.insert(*member);
        }
    }
    names
}

/// 展开后的两件产物：记录用的 grant 名 + 命令级 scope。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrantRequest {
    /// 被请求的 `grant.*` 名称集合。
    pub grants: GrantSet,
    /// 展开出的命令级 scope 集合。
    pub scopes: ScopeSet,
}

fn lookup<'a>(table: &'a [(&'a str, &'a [&'a str])], name: &str) -> Option<&'a [&'a str]> {
    table
        .iter()
        .find(|(entry, _)| *entry == name)
        .map(|(_, members)| *members)
}

/// `local.*` 永不远程授予、也不参与展开：在任何输入位置出现都直接拒绝。
fn reject_local(name: &str) -> Result<(), AuthorizationError> {
    if LOCAL_CAPABILITIES.contains(&name) || name.starts_with("local.") {
        return Err(AuthorizationError::LocalCapability(name.to_owned()));
    }
    Ok(())
}
