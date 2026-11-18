//! Standard ACP updates projected for resource policy, preserving the original raw message.
use super::{Arc, Incoming, SessionEvent, UpdateOrigin, Version, d, n};
use crate::{
    adapters::acp::{
        AcpError, Received, UpdateFreshness,
        model::{field, text},
        raw_json::RawValueRef,
    },
    domain::session_capacity::Coverage,
};

pub(super) fn project(
    received: &Received,
    wire: Version,
) -> Result<Option<n::CapacityObservation>, d::DispatchError> {
    let freshness = match received.update_freshness() {
        Some(UpdateFreshness::Current) => n::CapacityFreshness::Current,
        Some(UpdateFreshness::History) => n::CapacityFreshness::History,
        Some(UpdateFreshness::Unconfirmed) | None => n::CapacityFreshness::Unconfirmed,
    };
    project_update(received.incoming(), wire, freshness)
}

fn project_update(
    incoming: &Incoming,
    wire: Version,
    freshness: n::CapacityFreshness,
) -> Result<Option<n::CapacityObservation>, d::DispatchError> {
    let Incoming::Update { event, origin, raw } = incoming else {
        return Ok(None);
    };
    let origin = match origin {
        UpdateOrigin::Live => n::ObservationOrigin::Live,
        UpdateOrigin::DuringResume => n::ObservationOrigin::DuringResume,
        UpdateOrigin::DuringReplay => n::ObservationOrigin::DuringReplay,
    };
    let params = raw
        .field("params")
        .map_err(super::error)?
        .ok_or(d::DispatchError::ProtocolRejected)?;
    let value = field(params, "update").map_err(super::error)?;
    let update = match event {
        SessionEvent::State { state, .. } => n::CapacityUpdate::State(n::CapacityState {
            foreground: foreground(state),
            background: if wire == Version::V2 {
                background(value).map_err(super::error)?
            } else {
                None
            },
        }),
        SessionEvent::Tool { tool, .. } => {
            let status = optional(value, "status").map_err(super::error)?;
            let active = if wire == Version::V2 && status.is_some_and(RawValueRef::is_null) {
                Some(true)
            } else {
                status
                    .and_then(|status| text(status).ok())
                    .map(|status| match status.as_ref() {
                        "completed" | "failed" => false,
                        "cancelled" if wire == Version::V2 => false,
                        _ => true,
                    })
            };
            n::CapacityUpdate::Tool {
                id: Arc::from(tool.as_str()),
                active,
            }
        }
        SessionEvent::Other { kind, .. }
            if wire == Version::V2 && kind.as_ref() == "subagent_update" =>
        {
            // The union's child association fields are on `update`, not under a child object.
            let session =
                text(field(value, "sessionId").map_err(super::error)?).map_err(super::error)?;
            if session.is_empty() || session.len() > 4096 {
                return Err(d::DispatchError::InvalidTarget);
            }
            let state = optional(value, "state")
                .map_err(super::error)?
                .and_then(|state| {
                    if state.is_null() {
                        Some(None)
                    } else {
                        let current = field(state, "state").and_then(text).ok()?;
                        Some(Some(n::CapacityState {
                            foreground: foreground(&current),
                            background: background(state).ok()?,
                        }))
                    }
                });
            n::CapacityUpdate::Child { session, state }
        }
        _ => n::CapacityUpdate::Activity,
    };
    Ok(Some(n::CapacityObservation {
        session: Arc::from(event.session().as_str()),
        origin,
        freshness,
        update,
    }))
}

fn foreground(state: &str) -> Coverage {
    match state {
        "idle" => Coverage::Idle,
        "running" | "requires_action" => Coverage::Active,
        _ => Coverage::Unknown,
    }
}

fn optional<'a>(value: RawValueRef<'a>, key: &str) -> Result<Option<RawValueRef<'a>>, AcpError> {
    Ok(value.object().ok_or(AcpError::InvalidMessage)?.get(key)?)
}

fn background(value: RawValueRef<'_>) -> Result<Option<Coverage>, AcpError> {
    let Some(value) = optional(value, "background")?.filter(|value| !value.is_null()) else {
        return Ok(None);
    };
    let Some(items) = value.array() else {
        return Ok(None);
    };
    if items.is_empty() {
        return Ok(Some(Coverage::Idle));
    }
    for item in items.iter() {
        let Ok(kind) = field(item, "type").and_then(text) else {
            return Ok(None);
        };
        let reference = match kind.as_ref() {
            "terminal" => Some("terminalId"),
            "tool_call" => Some("toolCallId"),
            "subagent" => Some("sessionId"),
            _ => None,
        };
        if let Some(reference) = reference
            && field(item, reference).and_then(text).is_err()
        {
            return Ok(None);
        }
    }
    Ok(Some(Coverage::Active))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::acp::{RawMessage, Version, test_support::unit_strategy};

    fn project(input: &Incoming) -> Result<Option<n::CapacityObservation>, d::DispatchError> {
        super::project_update(input, Version::V2, n::CapacityFreshness::Unconfirmed)
    }

    #[test]
    fn v1_extension_does_not_establish_child_ownership() {
        let input = incoming(
            r#"{"sessionUpdate":"subagent_update","sessionId":"child"}"#,
            UpdateOrigin::Live,
        );
        assert_eq!(
            super::project_update(&input, Version::V1, n::CapacityFreshness::Unconfirmed)
                .unwrap()
                .unwrap()
                .update,
            n::CapacityUpdate::Activity
        );
    }

    #[test]
    fn tool_null_is_pending_and_missing_or_malformed_does_not_clear_known_work() {
        for (extra, active) in [
            ("", None),
            (r#","status":17"#, None),
            (r#","status":null"#, Some(true)),
        ] {
            let input = incoming(
                &format!(r#"{{"sessionUpdate":"tool_call_update","toolCallId":"tool"{extra}}}"#),
                UpdateOrigin::Live,
            );
            assert_eq!(
                project(&input).unwrap().unwrap().update,
                n::CapacityUpdate::Tool {
                    id: Arc::from("tool"),
                    active
                }
            );
        }
    }

    #[test]
    fn tool_future_and_paused_status_protect_and_cancelled_is_terminal() {
        for (status, active) in [("_future", true), ("paused", true), ("cancelled", false)] {
            let input = incoming(
                &format!(
                    r#"{{"sessionUpdate":"tool_call_update","toolCallId":"tool","status":"{status}"}}"#
                ),
                UpdateOrigin::Live,
            );
            assert_eq!(
                project(&input).unwrap().unwrap().update,
                n::CapacityUpdate::Tool {
                    id: Arc::from("tool"),
                    active: Some(active)
                }
            );
        }
    }

    fn incoming(update: &str, origin: UpdateOrigin) -> Incoming {
        let raw = RawMessage::parse(
            format!(r#"{{"jsonrpc":"2.0","method":"session/update","params":{{"sessionId":"root","update":{update}}}}}"#).into_bytes(),
        ).unwrap();
        let params = raw.field("params").unwrap().unwrap();
        let strategy = unit_strategy(Version::V2);
        let event = strategy.update(params).unwrap();
        Incoming::Update { event, origin, raw }
    }

    #[test]
    fn missing_null_malformed_and_unknown_background_preserve_scope() {
        for (extra, expected) in [
            ("", None),
            (r#","background":null"#, None),
            (r#","background":{}"#, None),
            (r#","background":[{}]"#, None),
            (r#","background":[{"type":"terminal"}]"#, None),
            (r#","background":[]"#, Some(Coverage::Idle)),
            (
                r#","background":[{"type":"_future","opaque":{"keep":true}}]"#,
                Some(Coverage::Active),
            ),
        ] {
            let input = incoming(
                &format!(r#"{{"sessionUpdate":"state_update","state":"idle"{extra}}}"#),
                UpdateOrigin::Live,
            );
            let Incoming::Update { raw, .. } = &input else {
                unreachable!()
            };
            let before = raw.bytes().to_vec();
            let observation = project(&input).unwrap().unwrap();
            assert_eq!(
                observation.update,
                n::CapacityUpdate::State(n::CapacityState {
                    foreground: Coverage::Idle,
                    background: expected
                })
            );
            assert_eq!(raw.bytes(), before);
        }
    }

    #[test]
    fn child_patch_distinguishes_omitted_null_and_snapshot() {
        for (extra, expected) in [
            ("", None),
            (r#","state":null"#, Some(None)),
            (
                r#","state":{"state":"running","background":[]}"#,
                Some(Some(n::CapacityState {
                    foreground: Coverage::Active,
                    background: Some(Coverage::Idle),
                })),
            ),
        ] {
            let input = incoming(
                &format!(r#"{{"sessionUpdate":"subagent_update","sessionId":"child"{extra}}}"#),
                UpdateOrigin::Live,
            );
            assert_eq!(
                project(&input).unwrap().unwrap().update,
                n::CapacityUpdate::Child {
                    session: Arc::from("child"),
                    state: expected
                }
            );
        }
    }

    #[test]
    fn malformed_optional_child_state_is_unreported_without_rejecting_update() {
        for state in ["17", "{}", r#"{"state":17}"#] {
            let input = incoming(
                &format!(
                    r#"{{"sessionUpdate":"subagent_update","sessionId":"child","state":{state}}}"#
                ),
                UpdateOrigin::Live,
            );
            assert_eq!(
                project(&input).unwrap().unwrap().update,
                n::CapacityUpdate::Child {
                    session: Arc::from("child"),
                    state: None,
                }
            );
        }
    }

    #[test]
    fn replay_and_current_snapshots_remain_distinguishable() {
        for (origin, expected) in [
            (UpdateOrigin::Live, n::ObservationOrigin::Live),
            (
                UpdateOrigin::DuringResume,
                n::ObservationOrigin::DuringResume,
            ),
            (
                UpdateOrigin::DuringReplay,
                n::ObservationOrigin::DuringReplay,
            ),
        ] {
            let input = incoming(
                r#"{"sessionUpdate":"tool_call_update","toolCallId":"tool","status":"in_progress"}"#,
                origin,
            );
            let observation = project(&input).unwrap().unwrap();
            assert_eq!(observation.origin, expected);
            assert_eq!(
                observation.update,
                n::CapacityUpdate::Tool {
                    id: Arc::from("tool"),
                    active: Some(true)
                }
            );
        }
    }
}
