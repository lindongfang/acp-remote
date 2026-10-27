//! 空会话内容分类、删除写集与重启后的保留截止时间。

use super::CommandRecord;
use super::CommittedEvent;
use super::error::InvalidValue;
use super::identity::Actor;
use super::ids::{RequestId, SessionId};
use super::scalars::{Digest, ResourceGeneration, Timestamp};

/// 元数据事件家族；未列入这里的事件按内容保守处理。
pub const SESSION_METADATA_EVENT_TYPES: &[&str] = &[
    "session.created",
    "session.info.changed",
    "session.mode.changed",
    "session.config.changed",
    "session.activity.changed",
    "session.updated",
    "session.commands.changed",
    "session.usage.changed",
    "session.origin.online_changed",
    "session.deleted",
    "command.accepted",
    "command.completed",
    "command.failed",
    "command.rejected",
    "command.uncertain",
];

/// 单个事件的内容分类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionContentClass {
    Content,
    MetadataOnly,
}

/// 未知事件默认属于内容，避免新增内容事件被错误回收。
pub fn classify_session_event(event_type: &str) -> SessionContentClass {
    if SESSION_METADATA_EVENT_TYPES.contains(&event_type) {
        SessionContentClass::MetadataOnly
    } else {
        SessionContentClass::Content
    }
}

/// 删除事务的完整写入身份。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionDeletionWrite {
    session: SessionId,
    actor: Actor,
    request: RequestId,
    request_fingerprint: Digest,
    at: Timestamp,
    expected_generation: Option<ResourceGeneration>,
}

impl SessionDeletionWrite {
    pub fn new(
        session: SessionId,
        actor: Actor,
        request: RequestId,
        request_fingerprint: Digest,
        at: Timestamp,
    ) -> Self {
        Self {
            session,
            actor,
            request,
            request_fingerprint,
            at,
            expected_generation: None,
        }
    }

    pub fn session(&self) -> &SessionId {
        &self.session
    }

    pub fn actor(&self) -> &Actor {
        &self.actor
    }

    pub fn request(&self) -> &RequestId {
        &self.request
    }

    pub fn request_fingerprint(&self) -> &Digest {
        &self.request_fingerprint
    }

    pub fn at(&self) -> &Timestamp {
        &self.at
    }

    pub fn expected_generation(&self) -> Option<ResourceGeneration> {
        self.expected_generation
    }

    /// Records the runtime resource generation proven by the lifecycle gate. Only the
    /// delivery layer can set this value; it is never accepted from wire input.
    pub fn with_expected_generation(mut self, generation: ResourceGeneration) -> Self {
        self.expected_generation = Some(generation);
        self
    }
}

/// 删除事务的最终资格分类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionDeletionBlocker {
    NotFound,
    NotEmpty,
    Protected,
}

impl SessionDeletionBlocker {
    pub const ALL: [Self; 3] = [Self::NotFound, Self::NotEmpty, Self::Protected];
}

/// 删除端口在真实释放资源之前的只读资格结果。
///
/// 该结果同时覆盖会话可见性、内容事实和命令执行保护；这些原因保持分离，
/// 调用方不得把 `Protected` 误报成 `NotEmpty`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionDeletionEligibility {
    Eligible,
    Blocked(SessionDeletionBlocker),
}

/// 删除事务结果。
#[derive(Debug, Clone, PartialEq)]
pub enum SessionDeletionOutcome {
    Deleted {
        deletion_event: CommittedEvent,
        terminal_event: CommittedEvent,
    },
    Replayed(CommandRecord),
    Failed {
        blocker: SessionDeletionBlocker,
        terminal_event: CommittedEvent,
    },
}

/// 启动时重新播种的空会话截止时间。
///
/// 构造只读取本次启动传入的时间；它不会接受或恢复上一次进程的截止时间。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestartEmptySessionDeadline {
    started_at: Timestamp,
    expires_at: Option<Timestamp>,
}

impl RestartEmptySessionDeadline {
    /// 从本次启动发现时刻开始一个完整窗口；`0` 表示关闭按时间回收。
    pub fn from_restart(started_at: Timestamp, idle_timeout_ms: u64) -> Result<Self, InvalidValue> {
        let expires_at = if idle_timeout_ms == 0 {
            None
        } else {
            Some(
                started_at
                    .checked_add_millis(idle_timeout_ms)
                    .ok_or(InvalidValue::Field)?,
            )
        };
        Ok(Self {
            started_at,
            expires_at,
        })
    }

    pub fn started_at(&self) -> &Timestamp {
        &self.started_at
    }

    pub fn expires_at(&self) -> Option<&Timestamp> {
        self.expires_at.as_ref()
    }

    pub fn is_due(&self, at: &Timestamp) -> bool {
        self.expires_at
            .as_ref()
            .is_some_and(|deadline| at >= deadline)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{EventId, EventType, OriginEpoch, Sequence};

    #[test]
    fn every_metadata_family_is_classified_and_unknown_is_content() {
        let frozen_metadata = [
            "session.created",
            "session.info.changed",
            "session.mode.changed",
            "session.config.changed",
            "session.activity.changed",
            "session.updated",
            "session.commands.changed",
            "session.usage.changed",
            "session.origin.online_changed",
            "session.deleted",
            "command.accepted",
            "command.completed",
            "command.failed",
            "command.rejected",
            "command.uncertain",
        ];
        assert_eq!(SESSION_METADATA_EVENT_TYPES, frozen_metadata);
        for event_type in frozen_metadata {
            assert_eq!(
                classify_session_event(event_type),
                SessionContentClass::MetadataOnly,
                "{event_type}"
            );
        }
        assert_eq!(
            classify_session_event("agent.message.delta"),
            SessionContentClass::Content
        );
        assert_eq!(
            classify_session_event("future.content.family"),
            SessionContentClass::Content
        );
    }

    #[test]
    fn every_blocker_is_concrete_and_exhaustive() {
        assert_eq!(
            SessionDeletionBlocker::ALL,
            [
                SessionDeletionBlocker::NotFound,
                SessionDeletionBlocker::NotEmpty,
                SessionDeletionBlocker::Protected,
            ]
        );
    }

    #[test]
    fn every_deletion_outcome_is_representable() {
        let session = SessionId::new("11111111-1111-4111-8111-111111111111").expect("session");
        let request = RequestId::new("22222222-2222-4222-8222-222222222222").expect("request");
        let at = Timestamp::new("2026-01-01T00:00:00.000Z").expect("timestamp");
        let record = CommandRecord::try_new(
            Some(session.clone()),
            request,
            "session.delete",
            super::super::CommandKind::Mutation,
            Actor::LocalCli,
            Some(at.clone()),
            super::super::CommandStatus::Accepted,
            None,
            None,
            None,
            None,
            None,
            Digest::new(&"A".repeat(43)).expect("digest"),
        )
        .expect("command record");
        let sequence = Sequence::new(1).expect("sequence");
        let epoch = OriginEpoch::new("55555555-5555-4555-8555-555555555555").expect("epoch");
        let deletion_event = CommittedEvent {
            id: EventId::new("33333333-3333-4333-8333-333333333333").expect("event"),
            event_type: EventType::new("session.deleted").expect("event type"),
            session: Some(SessionId::new("11111111-1111-4111-8111-111111111111").expect("session")),
            session_sequence: Some(sequence),
            global_sequence: sequence,
            origin_epoch: Some(epoch.clone()),
            origin_sequence: Some(sequence),
            created_at: Timestamp::new("2026-01-01T00:00:00.000Z").expect("timestamp"),
        };
        let terminal_event = CommittedEvent {
            id: EventId::new("44444444-4444-4444-8444-444444444444").expect("event"),
            event_type: EventType::new("command.completed").expect("event type"),
            session: deletion_event.session.clone(),
            session_sequence: Some(Sequence::new(2).expect("sequence")),
            global_sequence: Sequence::new(2).expect("sequence"),
            origin_epoch: Some(epoch),
            origin_sequence: Some(Sequence::new(2).expect("sequence")),
            created_at: Timestamp::new("2026-01-01T00:00:00.000Z").expect("timestamp"),
        };

        assert!(matches!(
            SessionDeletionOutcome::Deleted {
                deletion_event: deletion_event.clone(),
                terminal_event: terminal_event.clone(),
            },
            SessionDeletionOutcome::Deleted { .. }
        ));
        assert_eq!(
            SessionDeletionOutcome::Replayed(record),
            SessionDeletionOutcome::Replayed(
                CommandRecord::try_new(
                    Some(session),
                    RequestId::new("22222222-2222-4222-8222-222222222222").expect("request"),
                    "session.delete",
                    super::super::CommandKind::Mutation,
                    Actor::LocalCli,
                    Some(at),
                    super::super::CommandStatus::Accepted,
                    None,
                    None,
                    None,
                    None,
                    None,
                    Digest::new(&"A".repeat(43)).expect("digest"),
                )
                .expect("command record")
            )
        );
        assert!(matches!(
            SessionDeletionOutcome::Failed {
                blocker: SessionDeletionBlocker::Protected,
                terminal_event,
            },
            SessionDeletionOutcome::Failed { .. }
        ));
    }

    #[test]
    fn restart_seeds_a_new_complete_window() {
        let started = Timestamp::new("2026-01-01T00:00:00.000Z").expect("timestamp");
        let deadline = RestartEmptySessionDeadline::from_restart(started.clone(), 30 * 60 * 1_000)
            .expect("deadline");
        assert_eq!(deadline.started_at(), &started);
        assert_eq!(
            deadline.expires_at().map(Timestamp::as_str),
            Some("2026-01-01T00:30:00.000Z")
        );
        assert!(!deadline.is_due(&Timestamp::new("2026-01-01T00:29:59.999Z").expect("timestamp")));
        assert!(deadline.is_due(&Timestamp::new("2026-01-01T00:30:00.000Z").expect("timestamp")));
    }

    #[test]
    fn restart_constructor_seeds_from_new_start_and_zero_disables_expiry() {
        let first = RestartEmptySessionDeadline::from_restart(
            Timestamp::new("2026-01-01T00:00:00.000Z").expect("timestamp"),
            60_000,
        )
        .expect("deadline");
        let restarted = RestartEmptySessionDeadline::from_restart(
            Timestamp::new("2026-01-01T00:20:00.000Z").expect("timestamp"),
            60_000,
        )
        .expect("deadline");
        assert_ne!(first.expires_at(), restarted.expires_at());
        let disabled = RestartEmptySessionDeadline::from_restart(
            Timestamp::new("2026-01-01T00:00:00.000Z").expect("timestamp"),
            0,
        )
        .expect("deadline");
        assert!(!disabled.is_due(&Timestamp::new("9999-12-31T23:59:59.999Z").expect("timestamp")));
    }
}
