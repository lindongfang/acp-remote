//! Evidence for turn boundaries and page coordinates within one reading generation.
pub mod cursor;

#[derive(Clone, Eq, PartialEq)]
pub enum FrameClass {
    User(Option<String>),
    Assistant,
    Notification,
    Unclassifiable,
}

#[derive(Clone, Default)]
pub struct TurnIndexer {
    count: u64,
    last_id: Option<String>,
    responded: bool,
    ambiguous: bool,
}

impl TurnIndexer {
    /// Returns the new one-based turn number only on a proven boundary.
    pub fn observe(&mut self, frame: FrameClass) -> Option<u64> {
        if self.ambiguous {
            return None;
        }
        match frame {
            FrameClass::Notification => return None,
            FrameClass::Unclassifiable => self.ambiguous = true,
            FrameClass::Assistant => self.responded = true,
            FrameClass::User(id) => {
                let id = id.filter(|id| !id.is_empty());
                if self.count > 0 && id.is_some() && self.last_id == id {
                    return None;
                }
                if self.count == 0 || self.responded || (self.last_id.is_some() && id.is_some()) {
                    self.last_id = id;
                    self.responded = false;
                    self.count += 1;
                    return Some(self.count);
                }
                self.ambiguous = true;
            }
        }
        None
    }

    #[must_use]
    pub const fn provable(&self) -> bool {
        self.count != 0 && !self.ambiguous
    }

    #[must_use]
    pub const fn turn_count(&self) -> u64 {
        if self.provable() { self.count } else { 0 }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum PageError {
    #[error("invalid page parameter")]
    InvalidRequest,
    #[error("invalid history cursor")]
    InvalidCursor,
    #[error("stale history cursor")]
    StaleCursor,
    #[error("turn not found")]
    NotFound,
    #[error("history cannot be paginated")]
    UnsupportedCapability,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Older,
    Newer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PageSize(u8);

impl Default for PageSize {
    fn default() -> Self {
        Self(10)
    }
}

impl PageSize {
    /// # Errors
    /// Rejects noncanonical decimal strings or values outside 1..=50.
    pub fn parse(value: Option<&str>) -> Result<Self, PageError> {
        let Some(value) = value else {
            return Ok(Self::default());
        };
        let number: u8 = value.parse().map_err(|_| PageError::InvalidRequest)?;
        if !(1..=50).contains(&number) || number.to_string() != value {
            return Err(PageError::InvalidRequest);
        }
        Ok(Self(number))
    }

    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl serde::Serialize for PageSize {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&self.0)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TurnRange {
    pub first: u64,
    pub last: u64,
}

impl TurnRange {
    /// # Errors
    /// A generation with no proven turns cannot be paginated.
    pub fn first(total: u64, size: PageSize, is_child: bool) -> Result<Self, PageError> {
        if is_child {
            Self::locate(total, size, Direction::Newer, 1)
        } else {
            Self::locate(total, size, Direction::Older, total)
        }
    }

    /// # Errors
    /// Rejects an empty history or a coordinate outside this generation.
    pub fn locate(
        total: u64,
        size: PageSize,
        direction: Direction,
        anchor: u64,
    ) -> Result<Self, PageError> {
        if total == 0 {
            return Err(PageError::UnsupportedCapability);
        }
        if !(1..=total).contains(&anchor) {
            return Err(PageError::NotFound);
        }
        let width = u64::from(size.0) - 1;
        Ok(match direction {
            Direction::Older => Self {
                first: anchor.saturating_sub(width).max(1),
                last: anchor,
            },
            Direction::Newer => Self {
                first: anchor,
                last: anchor.saturating_add(width).min(total),
            },
        })
    }
    #[must_use]
    pub const fn includes_prelude(self, generation_has_prelude: bool) -> bool {
        self.first == 1 && generation_has_prelude
    }

    #[must_use]
    pub const fn older_anchor(self) -> Option<u64> {
        if self.first > 1 {
            Some(self.first - 1)
        } else {
            None
        }
    }
    #[must_use]
    pub const fn newer_anchor(self, total: u64) -> Option<u64> {
        if self.last < total {
            Some(self.last + 1)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests;
