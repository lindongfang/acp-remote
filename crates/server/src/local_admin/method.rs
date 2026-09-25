//! v1 方法集（`docs/LOCAL_ADMIN_PROTOCOL.md` §5、`schemas/local-admin/v1/envelope.schema.json#/$defs/methodName`）。
//!
//! 方法名有两层校验：
//!
//! 1. **语法**：匹配 `^[a-z][a-z0-9]*(\.[a-z0-9]+(-[a-z0-9]+)*)*$`（§4 的表格）。不匹配属信封非法 → `local.invalid_request`。
//! 2. **在集内**：必须是本枚举的 25 个方法之一。语法合法但不在集内 → `local.unsupported`
//!    （§4 规则 3：「未知 method → local.unsupported；这使新增方法成为向后兼容变更」）。
//!
//! 枚举与 schema 的 `methodName.enum` 由常驻漂移测试断言逐项相等，本文件不复制第二份词表。

/// v1 的管理方法。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Method {
    /// `daemon.status`
    DaemonStatus,
    /// `daemon.stop`
    DaemonStop,
    /// `workspace.select`（`local.workspace.select`）
    WorkspaceSelect,
    /// `agent.configure`（`local.agent.configure`）
    AgentConfigure,
    /// `provider.configure`（`local.provider.configure`）
    ProviderConfigure,
    /// `device.pair.begin`（`local.device.manage`）
    DevicePairBegin,
    /// `device.pair.status`（`local.device.manage`）
    DevicePairStatus,
    /// `device.pair.confirm`（`local.device.manage`）
    DevicePairConfirm,
    /// `device.pair.reject`（`local.device.manage`）
    DevicePairReject,
    /// `device.list`（`local.device.manage`）
    DeviceList,
    /// `device.revoke`（`local.device.manage`）
    DeviceRevoke,
    /// `node.pair.begin`（`local.device.manage`）
    NodePairBegin,
    /// `node.pair.status`（`local.device.manage`）
    NodePairStatus,
    /// `node.pair.confirm`（`local.device.manage`）
    NodePairConfirm,
    /// `node.pair.reject`（`local.device.manage`）
    NodePairReject,
    /// `node.list`（`local.device.manage`）
    NodeList,
    /// `node.revoke`（`local.device.manage`）
    NodeRevoke,
    /// `export.create`（`local.export.manage`）
    ExportCreate,
    /// `export.list`（`local.export.manage`）
    ExportList,
    /// `export.revoke`（`local.export.manage`）
    ExportRevoke,
    /// `import.add`（`local.export.manage`）
    ImportAdd,
    /// `import.list`（`local.export.manage`）
    ImportList,
    /// `import.remove`（`local.export.manage`）
    ImportRemove,
    /// `node.rotate-key.begin`（`local.node.rotate-key`，`post_mvp`；§5.7：字段定义落地前调用回
    /// `local.unsupported`，不得自行填充 `params`/`result`）
    NodeRotateKeyBegin,
    /// `audit.export`（`local.audit.export`）
    AuditExport,
}

impl Method {
    /// 全部 25 个方法，顺序与 §5.1 的能力对应表一致（`node.rotate-key.begin` 是 §5.1 第 6 行
    /// `local.node.rotate-key` 的方法，因此排在 `import.remove` 与 `audit.export` 之间）。
    pub const ALL: [Self; 25] = [
        Self::DaemonStatus,
        Self::DaemonStop,
        Self::WorkspaceSelect,
        Self::AgentConfigure,
        Self::ProviderConfigure,
        Self::DevicePairBegin,
        Self::DevicePairStatus,
        Self::DevicePairConfirm,
        Self::DevicePairReject,
        Self::DeviceList,
        Self::DeviceRevoke,
        Self::NodePairBegin,
        Self::NodePairStatus,
        Self::NodePairConfirm,
        Self::NodePairReject,
        Self::NodeList,
        Self::NodeRevoke,
        Self::ExportCreate,
        Self::ExportList,
        Self::ExportRevoke,
        Self::ImportAdd,
        Self::ImportList,
        Self::ImportRemove,
        Self::NodeRotateKeyBegin,
        Self::AuditExport,
    ];

    /// 方法的 wire 名字。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DaemonStatus => "daemon.status",
            Self::DaemonStop => "daemon.stop",
            Self::WorkspaceSelect => "workspace.select",
            Self::AgentConfigure => "agent.configure",
            Self::ProviderConfigure => "provider.configure",
            Self::DevicePairBegin => "device.pair.begin",
            Self::DevicePairStatus => "device.pair.status",
            Self::DevicePairConfirm => "device.pair.confirm",
            Self::DevicePairReject => "device.pair.reject",
            Self::DeviceList => "device.list",
            Self::DeviceRevoke => "device.revoke",
            Self::NodePairBegin => "node.pair.begin",
            Self::NodePairStatus => "node.pair.status",
            Self::NodePairConfirm => "node.pair.confirm",
            Self::NodePairReject => "node.pair.reject",
            Self::NodeList => "node.list",
            Self::NodeRevoke => "node.revoke",
            Self::ExportCreate => "export.create",
            Self::ExportList => "export.list",
            Self::ExportRevoke => "export.revoke",
            Self::ImportAdd => "import.add",
            Self::ImportList => "import.list",
            Self::ImportRemove => "import.remove",
            Self::NodeRotateKeyBegin => "node.rotate-key.begin",
            Self::AuditExport => "audit.export",
        }
    }

    /// 由 wire 名字精确解析；不在 v1 方法集里返回 `None`（调用方按 §4 规则 3 回 `local.unsupported`）。
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|method| method.as_str() == name)
    }
}

/// 方法名语法（§4 的表格）：`^[a-z][a-z0-9]*(\.[a-z0-9]+(-[a-z0-9]+)*)*$`。
///
/// 首字符必须是 `a-z`（起首不能是数字）；首段只允许 `[a-z0-9]`，后续段允许**段内**连字符
/// （`[a-z0-9]+(-[a-z0-9]+)*`），即连字符不得出现在段的开头或结尾，也不得连续出现；不允许多余或空的点。
pub fn is_method_name(text: &str) -> bool {
    let mut segments = text.split('.');
    let Some(first) = segments.next() else {
        return false;
    };
    let mut first_bytes = first.bytes();
    match first_bytes.next() {
        Some(byte) if byte.is_ascii_lowercase() => {}
        _ => return false,
    }
    if !first_bytes.all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit()) {
        return false;
    }
    segments.all(|segment| {
        !segment.is_empty()
            && segment.split('-').all(|part| {
                !part.is_empty()
                    && part
                        .bytes()
                        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
            })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_methods_round_trip_through_their_wire_name() {
        assert_eq!(Method::ALL.len(), 25);
        for method in Method::ALL {
            assert_eq!(Method::from_name(method.as_str()), Some(method));
            assert!(is_method_name(method.as_str()), "{}", method.as_str());
        }
        assert_eq!(
            Method::from_name("node.rotate-key.begin"),
            Some(Method::NodeRotateKeyBegin)
        );
        assert_eq!(Method::from_name("device.rotate-key.begin"), None);
        assert_eq!(Method::from_name(""), None);
    }

    #[test]
    fn method_name_syntax_rejects_other_shapes() {
        for valid in [
            "daemon.status",
            "a",
            "a.b.c",
            "workspace.select9",
            "x0.y1z",
            "node.rotate-key.begin",
            "a.b-c",
            "x.y-z.w-0",
        ] {
            assert!(is_method_name(valid), "{valid} 应合法");
        }
        for invalid in [
            "",
            "Daemon.status",
            "daemon.Status",
            "daemon..status",
            ".daemon",
            "daemon.",
            "9daemon.status",
            "daemon-status",
            "daemon status",
            "daemon_status",
            // 连字符只允许出现在段的内部（§4 的方法名正则）。
            "node.-rotate-key.begin",
            "node.rotate-key-.begin",
            "node.rotate--key.begin",
            "node.rotate-key..begin",
            "node.rotate-key.",
            "node.rotate-key-",
            "-node.rotate-key.begin",
        ] {
            assert!(!is_method_name(invalid), "{invalid} 应非法");
        }
    }
}
