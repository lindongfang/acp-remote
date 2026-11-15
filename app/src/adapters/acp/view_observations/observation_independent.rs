//! Nested independent tests for actual observation context validation.
macro_rules! tassert {($($arg:tt)*) => {{ $crate::adapters::acp::raw_preservation_independent::tick(); assert!($($arg)*); }}}
macro_rules! tassert_eq {($($arg:tt)*) => {{ $crate::adapters::acp::raw_preservation_independent::tick(); assert_eq!($($arg)*); }}}

use super::*;
use crate::adapters::acp::raw_preservation_independent as t;
#[tokio::test(flavor = "current_thread")]
async fn raw_preservation_independent_observation() {
    let vv = t::vectors("observation");
    tassert_eq!(vv.len(), 61);
    for v in vv {
        t::check_sync(&v, |f| {
            let bytes = t::input(&v);
            let raw = super::super::RawMessage::parse(bytes.clone()).unwrap();
            let s = f.strategy(t::version(&v), "all");
            let event = t::baseline_event(&s, t::id(&v).contains("-STATE-"));
            let r = context(&event, &raw);
            if t::expected(&v) == "ObservationError::InvalidObservation" {
                tassert!(matches!(r, Err(ObservationError::InvalidObservation)));
            } else {
                tassert_eq!(r.unwrap(), ("s".into(), "tool_call_update".into()));
            }
            tassert_eq!(raw.bytes(), bytes);
        })
        .await;
    }
}
