use super::{
    DecimalU64, Id128, Map, Serialize, SessionProtocolError, SourceLaunchSelectionInput, Value,
    bounded_non_empty_text, closed, closed_with_optional, decimal, id, object, source_selection,
    text,
};
use crate::domain::history_page::{Direction, PageSize, cursor::fingerprint};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum HistoryTarget {
    Target {
        target_id: Id128,
    },
    Source {
        selection: SourceLaunchSelectionInput,
        native_session_id: String,
    },
}
impl HistoryTarget {
    #[must_use]
    pub fn fingerprint(&self, child: Option<&str>) -> [u8; 16] {
        match self {
            Self::Target { target_id } => fingerprint("target", &[target_id.as_bytes()], child),
            Self::Source {
                selection: s,
                native_session_id,
            } => fingerprint(
                "source",
                &[
                    s.source_id.as_bytes(),
                    s.harness_id.as_bytes(),
                    s.profile_id.as_bytes(),
                    &s.profile_revision.as_u64().to_be_bytes(),
                    s.workspace_id.as_bytes(),
                    s.launch_path.as_bytes(),
                    native_session_id.as_bytes(),
                ],
                child,
            ),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum HistoryMode {
    First {},
    Continue {
        cursor: String,
        direction: Direction,
    },
    Locate {
        from_turn: DecimalU64,
        direction: Direction,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct HistoryPageInput {
    pub target: HistoryTarget,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub child: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turns: Option<PageSize>,
    pub mode: HistoryMode,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct PageOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turns: Option<PageSize>,
}

fn turns(value: &Value) -> Result<PageSize, SessionProtocolError> {
    PageSize::parse(Some(text(Some(value))?)).map_err(|_| SessionProtocolError::InvalidCommand)
}
pub(super) fn parse_options(value: &Value) -> Result<PageOptions, SessionProtocolError> {
    let o = object(Some(value))?;
    closed_with_optional(o, &[], &["turns"])?;
    Ok(PageOptions {
        turns: o.get("turns").map(turns).transpose()?,
    })
}

pub(super) fn parse_page(p: &Map<String, Value>) -> Result<HistoryPageInput, SessionProtocolError> {
    closed_with_optional(p, &["target", "mode"], &["turns", "child"])?;
    let t = object(p.get("target"))?;
    let target = match text(t.get("kind"))? {
        "target" => {
            closed(t, &["kind", "targetId"])?;
            HistoryTarget::Target {
                target_id: id(t.get("targetId"))?,
            }
        }
        "source" => {
            closed(t, &["kind", "selection", "nativeSessionId"])?;
            HistoryTarget::Source {
                selection: source_selection(t.get("selection"))?,
                native_session_id: bounded_non_empty_text(t.get("nativeSessionId"), true)?,
            }
        }
        _ => return Err(SessionProtocolError::InvalidCommand),
    };
    let m = object(p.get("mode"))?;
    let direction = || match text(m.get("direction"))? {
        "older" => Ok(Direction::Older),
        "newer" => Ok(Direction::Newer),
        _ => Err(SessionProtocolError::InvalidCommand),
    };
    let mode = match text(m.get("kind"))? {
        "first" => {
            closed(m, &["kind"])?;
            HistoryMode::First {}
        }
        "continue" => {
            closed(m, &["kind", "direction", "cursor"])?;
            HistoryMode::Continue {
                direction: direction()?,
                cursor: text(m.get("cursor"))?.to_owned(),
            }
        }
        "locate" => {
            closed(m, &["kind", "direction", "fromTurn"])?;
            let from_turn = decimal(m.get("fromTurn"))?;
            if from_turn.as_u64() == 0 {
                return Err(SessionProtocolError::InvalidDecimal);
            }
            HistoryMode::Locate {
                direction: direction()?,
                from_turn,
            }
        }
        _ => return Err(SessionProtocolError::InvalidCommand),
    };
    Ok(HistoryPageInput {
        target,
        mode,
        turns: p.get("turns").map(turns).transpose()?,
        child: p
            .get("child")
            .map(|v| bounded_non_empty_text(Some(v), true))
            .transpose()?,
    })
}
