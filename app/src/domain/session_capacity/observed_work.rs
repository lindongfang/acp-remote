//! Missing optional snapshots never erase work already observed on this connection.
use super::Coverage;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObservedWork {
    foreground: Coverage,
    background: Coverage,
    foreground_reported: bool,
}

impl Default for ObservedWork {
    fn default() -> Self {
        Self {
            foreground: Coverage::Unknown,
            background: Coverage::Unknown,
            foreground_reported: false,
        }
    }
}

impl ObservedWork {
    /// ACP new has an idle initial foreground; a later native report takes precedence.
    pub const fn created(&mut self) {
        if !self.foreground_reported {
            self.foreground = Coverage::Idle;
        }
    }

    /// Only current control snapshots belong here. Historical replay is activity, not state.
    pub fn observe(&mut self, foreground: Coverage, background: Option<Coverage>) {
        self.foreground = foreground;
        self.foreground_reported = true;
        if let Some(background) = background
            && background != Coverage::Unknown
        {
            self.background = background;
        }
    }

    #[must_use]
    pub const fn foreground(self) -> Coverage {
        self.foreground
    }

    #[must_use]
    pub const fn background(self) -> Coverage {
        self.background
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_background_keeps_known_work_until_explicit_empty_snapshot() {
        let mut work = ObservedWork::default();
        work.created();
        assert_eq!(work.background(), Coverage::Unknown);
        work.observe(Coverage::Idle, Some(Coverage::Active));
        for background in [None, Some(Coverage::Unknown)] {
            work.observe(Coverage::Idle, background);
            assert_eq!(work.background(), Coverage::Active);
        }
        work.observe(Coverage::Idle, Some(Coverage::Idle));
        assert_eq!(work.background(), Coverage::Idle);
    }

    #[test]
    fn created_does_not_overwrite_a_current_native_report() {
        for foreground in [Coverage::Active, Coverage::Unknown, Coverage::Idle] {
            let mut work = ObservedWork::default();
            work.observe(foreground, None);
            work.created();
            assert_eq!(work.foreground(), foreground);
        }
        let mut work = ObservedWork::default();
        assert_eq!(work.foreground(), Coverage::Unknown);
        work.created();
        assert_eq!(work.foreground(), Coverage::Idle);
    }

    #[test]
    fn unknown_foreground_does_not_clear_background() {
        let mut work = ObservedWork::default();
        work.observe(Coverage::Active, Some(Coverage::Active));
        work.observe(Coverage::Unknown, None);
        assert_eq!(work.foreground(), Coverage::Unknown);
        assert_eq!(work.background(), Coverage::Active);
    }
}
