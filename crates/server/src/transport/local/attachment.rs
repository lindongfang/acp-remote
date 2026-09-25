//! `0x02` ACP 流的 attachment 生命周期（`docs/LOCAL_ADMIN_PROTOCOL.md` §3.1）。
//!
//! 会话语义是落地目标，但 `server::acp_facade` 在本切片尚未装配：`serve_connection` 对 `0x02` 连接在
//! 完成 framing 校验后立即关闭，**不分配也不消耗** attachment（§3.1 的实现状态注记、design.md 决策 5）。
//! 本模块因此只提供 attachment 的标识与注册表本身，切片 6 接入 facade 时替换分发目标、复用这里的生命周期代码。
//!
//! `FacadeAttachmentId` 与 Node Link 的 `attachmentId`/`attachmentGeneration` 是同一概念在不同层的实例
//! （`docs/MODULE_ARCHITECTURE.md` §4.9）：**不共用字段、不跨层传递**，也不与 §2.1 的 `InstanceId`
//! （Daemon 运行期标识）互相转换——三者形状相近只因都是「16 字符小写 hex 的运行期标识」。

use std::collections::HashSet;

use uuid::Uuid;

/// 标识的随机苹节数：8 字节 → 16 字符小写 hex（64 bit CSPRNG 足以在单次 Daemon 运行期内不重复）。
const ID_BYTES: usize = 8;

/// 一条 `0x02` 连接对应的 facade attachment 标识（§3.1）。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FacadeAttachmentId(String);

impl FacadeAttachmentId {
    /// 为新建立的 `0x02` 连接分配一个新的 attachment 标识。
    pub fn generate() -> Self {
        let uuid = Uuid::new_v4();
        let text: String = uuid
            .simple()
            .to_string()
            .chars()
            .take(ID_BYTES * 2)
            .collect();
        Self(text)
    }

    /// 文本形式（16 字符小写 hex）。
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 活跃 attachment 的注册表。
///
/// 语义（§3.1）：连接建立即 `attach`，连接结束即 `detach`；已作废 attachment 的延迟帧必须被丢弃并计入
/// 结构化警告，**不得**命中新 attachment 的会话绑定——因此注册表按标识精确匹配，不做最近一次连接的回落。
#[derive(Debug, Default)]
pub struct FacadeAttachmentRegistry {
    active: HashSet<FacadeAttachmentId>,
}

impl FacadeAttachmentRegistry {
    /// 空注册表。
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册一条新连接并返回其标识。
    pub fn attach(&mut self) -> FacadeAttachmentId {
        let id = FacadeAttachmentId::generate();
        self.active.insert(id.clone());
        id
    }

    /// 作废一条 attachment（连接结束）；返回该标识此前是否活跃。
    pub fn detach(&mut self, id: &FacadeAttachmentId) -> bool {
        self.active.remove(id)
    }

    /// 该 attachment 是否仍然活跃。
    pub fn is_active(&self, id: &FacadeAttachmentId) -> bool {
        self.active.contains(id)
    }

    /// 处理一个指向某 attachment 的帧：活跃则交给 facade；已作废则丢弃并记结构化警告。
    ///
    /// 返回 `true` 表示帧属于活跃 attachment（调用方继续分发），`false` 表示已作废并被丢弃。
    pub fn route(&self, id: &FacadeAttachmentId) -> bool {
        let active = self.is_active(id);
        if !active {
            tracing::warn!(
                event = "acp_facade.stale_attachment_frame",
                attachment_id = id.as_str(),
                "已作废的 attachment 收到延迟帧，丢弃且不命中新绑定（LOCAL_ADMIN_PROTOCOL.md §3.1）"
            );
        }
        active
    }

    /// 活跃 attachment 数量。
    pub fn len(&self) -> usize {
        self.active.len()
    }

    /// 是否没有活跃 attachment。
    pub fn is_empty(&self) -> bool {
        self.active.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_ids_are_16_lowercase_hex_characters() {
        let id = FacadeAttachmentId::generate();
        assert_eq!(id.as_str().len(), 16);
        assert!(
            id.as_str()
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
            "attachmentId 必须是 16 字符小写 hex：{}",
            id.as_str()
        );
    }

    #[test]
    fn two_connections_get_different_attachments() {
        let mut registry = FacadeAttachmentRegistry::new();
        let first = registry.attach();
        let second = registry.attach();
        assert_ne!(first, second);
        assert_eq!(registry.len(), 2);
    }

    #[test]
    fn detached_attachment_is_dropped_and_does_not_hit_a_new_binding() {
        let mut registry = FacadeAttachmentRegistry::new();
        let first = registry.attach();
        assert!(registry.route(&first));
        assert!(registry.detach(&first));
        // 旧 attachment 的延迟帧：丢弃，不命中随后建立的新 attachment。
        let second = registry.attach();
        assert!(!registry.route(&first));
        assert!(registry.route(&second));
        assert_eq!(registry.len(), 1);
        assert!(!registry.detach(&first));
    }
}
