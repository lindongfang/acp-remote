//! Nested closed profile admission checks; these twelve do not claim `Intent::decode` coverage.
macro_rules! tassert {($($arg:tt)*) => {{ $crate::adapters::acp::raw_preservation_independent::tick(); assert!($($arg)*); }}}
macro_rules! tassert_eq {($($arg:tt)*) => {{ $crate::adapters::acp::raw_preservation_independent::tick(); assert_eq!($($arg)*); }}}

use super::*;
use crate::adapters::acp::raw_preservation_independent as t;
#[test]
fn raw_preservation_independent_profile() {
    let vv = t::vectors("profile");
    tassert_eq!(vv.len(), 12);
    for v in vv {
        t::mark(&v, "start");
        tassert_eq!(v["operation"], "profile_parse");
        tassert_eq!(t::expected(&v), "CatalogError::InvalidConfiguration");
        tassert!(matches!(
            parse_profile(t::input(&v)),
            Err(CatalogError::InvalidConfiguration)
        ));
        t::mark(&v, "pass");
    }
}
