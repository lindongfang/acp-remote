//! Only the original production factory can assemble this read-only handle.
use super::ProductQueue;
use crate::{
    application::{
        queue_owner::{ProcessingPhase, QueueRead, QueueReadFailure, QueueReadReceipt},
        shared_views::socket_authority::{SocketViewAuthority, SocketViewClient},
    },
    domain::{
        EntityId,
        input_queue::QueueEntry,
        request_registry::{RegistrationId, RegistryError, RequestFact},
    },
};
use serde::Serialize;
use std::sync::Arc;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct QueueEntryMetadata {
    item_id: String,
    revision: String,
    submitter_id: String,
    editor_id: String,
    suspended: bool,
}
impl QueueEntryMetadata {
    fn of<T>(entry: &QueueEntry<T>) -> Self {
        Self {
            item_id: entry.id().encode(),
            revision: entry.revision().to_string(),
            submitter_id: entry.input().submitter.encode(),
            editor_id: entry.editor().encode(),
            suspended: entry.suspended(),
        }
    }
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct QueueProcessingMetadata {
    phase: &'static str,
    applied: bool,
    registration_id: String,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueMetadata {
    target_id: String,
    failure_suspended: bool,
    stop_reason: Option<&'static str>,
    priority_failure: Option<&'static str>,
    waiting: Vec<QueueEntryMetadata>,
    active: Option<QueueEntryMetadata>,
    priority: Option<String>,
    processing: Option<QueueProcessingMetadata>,
}
const fn phase(value: ProcessingPhase) -> &'static str {
    match value {
        ProcessingPhase::AwaitBegin => "await_begin",
        ProcessingPhase::BeginUnconfirmed => "begin_unconfirmed",
        ProcessingPhase::ReadyToApply => "ready_to_apply",
        ProcessingPhase::AwaitSettle => "await_settle",
        ProcessingPhase::SettleUnconfirmed => "settle_unconfirmed",
        ProcessingPhase::Faulted => "faulted",
    }
}
fn metadata<T>(target: EntityId, read: &QueueRead<'_, T>) -> QueueMetadata {
    let (queue, processing) = match read {
        QueueRead::Stable { queue } => (queue, None),
        QueueRead::Processing {
            phase: current,
            applied,
            request,
            queue,
        } => (
            queue,
            Some(QueueProcessingMetadata {
                phase: phase(*current),
                applied: *applied,
                registration_id: request.encode(),
            }),
        ),
    };
    QueueMetadata {
        target_id: target.encode(),
        failure_suspended: queue.failure_suspended(),
        stop_reason: queue
            .last_stop_reason()
            .map(crate::domain::native_dispatch::KnownStopReason::label),
        priority_failure: queue
            .priority_failure()
            .map(crate::domain::input_queue::PriorityFailure::label),
        waiting: queue.waiting().map(QueueEntryMetadata::of).collect(),
        active: queue.active().map(QueueEntryMetadata::of),
        priority: queue.priority().map(EntityId::encode),
        processing,
    }
}

#[derive(Clone)]
pub struct NodeQueueReadService {
    queue: ProductQueue,
    authority: Arc<SocketViewAuthority>,
}

impl NodeQueueReadService {
    pub(super) const fn new(queue: ProductQueue, authority: Arc<SocketViewAuthority>) -> Self {
        Self { queue, authority }
    }
    pub(crate) fn belongs_to(&self, authority: &Arc<SocketViewAuthority>) -> bool {
        Arc::ptr_eq(&self.authority, authority)
    }
    fn current_socket(&self, client: &SocketViewClient) -> Result<(), RegistryError> {
        if !self.belongs_to(client.context.authority()) {
            return Err(RegistryError::InvalidActor);
        }
        client
            .context
            .lifetime
            .with_live(|| Ok(()))
            .map_err(|error| match error {
                crate::domain::shared_view::ViewError::NodeStopping => RegistryError::NodeStopping,
                _ => RegistryError::InvalidActor,
            })
    }
    pub(crate) fn inspect(
        &self,
        client: &SocketViewClient,
        target: EntityId,
    ) -> Result<QueueReadReceipt<QueueMetadata>, QueueReadFailure> {
        self.current_socket(client)
            .map_err(QueueReadFailure::Registry)?;
        self.queue
            .begin_inspect_queue(client.context.actor().clone(), target, move |read| {
                metadata(target, &read)
            })
    }
    pub(crate) fn query(
        &self,
        client: &SocketViewClient,
        target: EntityId,
        registration: RegistrationId,
    ) -> Result<QueueReadReceipt<RequestFact>, QueueReadFailure> {
        self.current_socket(client)
            .map_err(QueueReadFailure::Registry)?;
        self.queue
            .begin_query_request(client.context.actor().clone(), target, registration)
    }
}

#[cfg(test)]
mod projection_development {
    use super::*;
    use crate::{
        application::queue_owner::{QueueRead, ValidatedBody},
        domain::{
            EntityId,
            input_queue::{DispatchReadiness, InputQueue, PriorityFailure, QueueInput},
            native::NativeSessionKey,
            native_dispatch::KnownStopReason,
        },
    };

    #[test]
    fn queue_metadata_exposes_the_stopped_reason_without_content() {
        let target = EntityId::from_bytes([1; 16]);
        let mut queue = InputQueue::<ValidatedBody<String>>::new(
            NativeSessionKey::from_config(Some(EntityId::from_bytes([2; 16])), "native").unwrap(),
        );
        for byte in [3, 4, 5] {
            queue
                .submit(
                    QueueInput {
                        id: EntityId::from_bytes([byte; 16]),
                        submitter: EntityId::from_bytes([6; 16]),
                        body: ValidatedBody::new(
                            "private queue body must not be projected".into(),
                            [byte; 32],
                        ),
                    },
                    DispatchReadiness::Blocked,
                )
                .unwrap();
        }
        queue.suspend_waiting_with_reason(KnownStopReason::MaxTokens);

        let value =
            serde_json::to_value(metadata(target, &QueueRead::Stable { queue: &queue })).unwrap();
        assert_eq!(value["failureSuspended"], true);
        assert_eq!(value["stopReason"], "max_tokens");
        assert_eq!(value["priorityFailure"], serde_json::Value::Null);
        assert_eq!(value["waiting"].as_array().unwrap().len(), 3);
        assert!(value.get("body").is_none());
        assert!(!value.to_string().contains("private queue body"));
    }

    #[test]
    fn queue_metadata_exposes_priority_failure_without_content_or_fallback() {
        let target = EntityId::from_bytes([1; 16]);
        let mut queue = InputQueue::<ValidatedBody<String>>::new(
            NativeSessionKey::from_config(Some(EntityId::from_bytes([2; 16])), "native").unwrap(),
        );
        for byte in [3, 4, 5] {
            queue
                .submit(
                    QueueInput {
                        id: EntityId::from_bytes([byte; 16]),
                        submitter: EntityId::from_bytes([6; 16]),
                        body: ValidatedBody::new(format!("private queue body {byte}"), [byte; 32]),
                    },
                    DispatchReadiness::Blocked,
                )
                .unwrap();
        }
        queue.prioritize(EntityId::from_bytes([4; 16]), 1).unwrap();
        queue
            .edit(
                EntityId::from_bytes([4; 16]),
                1,
                EntityId::from_bytes([7; 16]),
                ValidatedBody::new("private edited body".into(), [8; 32]),
            )
            .unwrap();

        let value =
            serde_json::to_value(metadata(target, &QueueRead::Stable { queue: &queue })).unwrap();
        assert_eq!(value["failureSuspended"], true);
        assert_eq!(value["stopReason"], serde_json::Value::Null);
        assert_eq!(value["priorityFailure"], "selection_changed");
        assert_eq!(value["priority"], serde_json::Value::Null);
        let waiting = value["waiting"].as_array().unwrap();
        assert_eq!(waiting.len(), 3);
        assert!(waiting.iter().all(|entry| entry["suspended"] == true));
        assert_eq!(
            waiting
                .iter()
                .map(|entry| entry["itemId"].as_str().unwrap())
                .collect::<Vec<_>>(),
            [
                EntityId::from_bytes([3; 16]).encode(),
                EntityId::from_bytes([4; 16]).encode(),
                EntityId::from_bytes([5; 16]).encode(),
            ]
        );
        assert_eq!(waiting[1]["revision"], "2");
        assert!(!value.to_string().contains("private"));
    }

    #[test]
    fn queue_metadata_exposes_interrupt_failure_without_content_or_fallback() {
        let target = EntityId::from_bytes([1; 16]);
        let mut queue = InputQueue::<ValidatedBody<String>>::new(
            NativeSessionKey::from_config(Some(EntityId::from_bytes([2; 16])), "native").unwrap(),
        );
        for byte in [3, 4, 5] {
            queue
                .submit(
                    QueueInput {
                        id: EntityId::from_bytes([byte; 16]),
                        submitter: EntityId::from_bytes([6; 16]),
                        body: ValidatedBody::new(
                            format!("private interrupt body {byte}"),
                            [byte; 32],
                        ),
                    },
                    DispatchReadiness::Blocked,
                )
                .unwrap();
        }
        queue.prioritize(EntityId::from_bytes([4; 16]), 1).unwrap();
        queue.fail_priority(PriorityFailure::InterruptUnconfirmed);

        let value =
            serde_json::to_value(metadata(target, &QueueRead::Stable { queue: &queue })).unwrap();
        assert_eq!(value["failureSuspended"], true);
        assert_eq!(value["stopReason"], serde_json::Value::Null);
        assert_eq!(value["priorityFailure"], "interrupt_unconfirmed");
        assert_eq!(value["priority"], serde_json::Value::Null);
        let waiting = value["waiting"].as_array().unwrap();
        assert_eq!(waiting.len(), 3);
        assert!(waiting.iter().all(|entry| entry["suspended"] == true));
        assert!(!value.to_string().contains("private"));
    }
}
