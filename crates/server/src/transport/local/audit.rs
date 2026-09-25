//! 本地通道的审计接线点（`docs/SECURITY_DESIGN.md` §14.2 的 `authorization.denied`）。
//!
//! 拒绝连接、方法失败与撤销都要记审计事件。审计的**写入**属于 `core` 的审计写集，由 `server::local_admin`
//! 的方法路由在 WP3b 接线（`AuditStore` 端口经组合根注入）；本模块只定义连接级拒绝事件的形状与一个小 trait，
//! 让 [crate::transport::local::LocalEndpoint] 在没有存储依赖的情况下也能把事件交出去。
//!
//! 事件字段只有审计元数据：传输种类、本次运行期的实例标识与拒绝原因，外加对端操作系统的用户标识
//! （SID 字符串或十进制 uid）。它们都不是 secret，也不含路径、prompt 或凭据值。

/// 对端被拒绝的原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeniedReason {
    /// 无法确认对端 OS 用户（Windows 拿不到对端进程 token；非 Linux/Android 的 Unix 目标没有 `SO_PEERCRED`）。
    PeerIdentityUnavailable,
    /// 对端 OS 用户与 Daemon 进程不同。
    DifferentOsUser,
}

impl DeniedReason {
    /// 结构化日志与审计记录里的稳定名字。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PeerIdentityUnavailable => "peer_identity_unavailable",
            Self::DifferentOsUser => "different_os_user",
        }
    }
}

/// 一条 `authorization.denied` 事件（§2.2、§14.2）。
#[derive(Debug, Clone)]
pub struct AuthorizationDenied<'a> {
    /// 传输种类：`named_pipe` / `unix_socket`。
    pub transport: &'a str,
    /// §2.1 的 `<instanceId>`（Daemon 运行期标识；不是路径）。
    pub instance_id: &'a str,
    /// 拒绝原因。
    pub reason: DeniedReason,
    /// 对端 OS 用户标识；无法取得时为 `None`。
    pub peer_identity: Option<&'a str>,
}

/// 审计事件的接收端。
///
/// WP3b 在组合根注入一个写 `core` 审计写集的实现（与同事务提交语义一致，`CORE_PORTS_AND_STORAGE.md` §11.2）；
/// 本切片只需要接线点，[`LoggingAuditHook`] 只把事件记成结构化日志。
pub trait AuditHook: Send + Sync {
    /// 记录一条拒绝事件。实现必须是同步、快速、不阻塞的。
    fn authorization_denied(&self, event: AuthorizationDenied<'_>);
}

/// 平台模块共用的拒绝路径：关闭连接之前先把 `authorization.denied` 交出去（§2.2）。
pub(crate) fn deny_connection(
    audit: &dyn AuditHook,
    transport: &'static str,
    instance_id: &str,
    reason: DeniedReason,
    peer_identity: Option<&str>,
) {
    audit.authorization_denied(AuthorizationDenied {
        transport,
        instance_id,
        reason,
        peer_identity,
    });
}

/// 只写结构化日志的审计实现（本切片默认；WP3b 换成写库的实现）。
#[derive(Debug, Default, Clone, Copy)]
pub struct LoggingAuditHook;

impl AuditHook for LoggingAuditHook {
    fn authorization_denied(&self, event: AuthorizationDenied<'_>) {
        tracing::warn!(
            event = "authorization.denied",
            transport = event.transport,
            instance_id = event.instance_id,
            reason = event.reason.as_str(),
            peer_identity = event.peer_identity.unwrap_or("unavailable"),
            "本地通道拒绝了来自其他 OS 用户的连接（LOCAL_ADMIN_PROTOCOL.md §2.2）"
        );
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    /// 记录事件的审计实现（用例自己就是「组合根注入」的接法）。
    #[derive(Default)]
    struct Recording {
        events: Mutex<Vec<RecordedDenial>>,
    }

    /// 一条被记录下来的拒绝事件（传输种类、实例标识、原因、对端标识）。
    struct RecordedDenial {
        transport: String,
        instance_id: String,
        reason: DeniedReason,
        peer_identity: Option<String>,
    }

    impl AuditHook for Recording {
        fn authorization_denied(&self, event: AuthorizationDenied<'_>) {
            self.events.lock().expect("未中毒").push(RecordedDenial {
                transport: event.transport.to_string(),
                instance_id: event.instance_id.to_string(),
                reason: event.reason,
                peer_identity: event.peer_identity.map(str::to_string),
            });
        }
    }

    /// 拒绝路径必须把 `authorization.denied` 交给注入的实现，并带上传输种类、实例标识、原因与对端标识。
    /// （真实跨用户拒绝在本机单账号 / Linux CI 上都构造不出，见交付报告的已知限制。）
    #[test]
    fn denial_events_reach_the_injected_hook_with_metadata() {
        let hook = Recording::default();
        deny_connection(
            &hook,
            "named_pipe",
            "0123456789abcdef",
            DeniedReason::DifferentOsUser,
            Some("S-1-5-21-1"),
        );
        deny_connection(
            &hook,
            "unix_socket",
            "fedcba9876543210",
            DeniedReason::PeerIdentityUnavailable,
            None,
        );
        let events = hook.events.lock().expect("未中毒");
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].transport, "named_pipe");
        assert_eq!(events[0].instance_id, "0123456789abcdef");
        assert_eq!(events[0].reason, DeniedReason::DifferentOsUser);
        assert_eq!(events[0].peer_identity.as_deref(), Some("S-1-5-21-1"));
        assert_eq!(events[1].reason, DeniedReason::PeerIdentityUnavailable);
        assert_eq!(events[1].peer_identity, None);
        assert_eq!(DeniedReason::DifferentOsUser.as_str(), "different_os_user");
        assert_eq!(
            DeniedReason::PeerIdentityUnavailable.as_str(),
            "peer_identity_unavailable"
        );
    }
}
