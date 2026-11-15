//! Nested closed management parser checks; no service/auth mutation.
macro_rules! tassert {($($arg:tt)*) => {{ $crate::adapters::acp::raw_preservation_independent::tick(); assert!($($arg)*); }}}
macro_rules! tassert_eq {($($arg:tt)*) => {{ $crate::adapters::acp::raw_preservation_independent::tick(); assert_eq!($($arg)*); }}}

use super::*;
use crate::adapters::acp::raw_preservation_independent as t;
#[test]
fn raw_preservation_independent_management() {
    let vv = t::vectors("management");
    tassert_eq!(vv.len(), 15);
    for v in vv {
        t::mark(&v, "start");
        let b = t::input(&v);
        match v["operation"].as_str().unwrap() {
            "strict_parse_object" => {
                let r = crate::strict_json::parse_object(
                    &b,
                    crate::strict_json::MAX_PROTOCOL_JSON_DEPTH,
                );
                let e = r.unwrap_err();
                match t::expected(&v) {
                    "StrictJsonError::DuplicateField" => tassert!(matches!(
                        e,
                        crate::strict_json::StrictJsonError::DuplicateField
                    )),
                    "StrictJsonError::TooDeep" => {
                        tassert!(matches!(e, crate::strict_json::StrictJsonError::TooDeep));
                    }
                    _ => panic!("unknown strict oracle"),
                }
            }
            "management_parse_frame" => {
                let e = parse_frame(std::str::from_utf8(&b).unwrap())
                    .expect_err("closed frame rejected");
                tassert_eq!(e.code, 4400);
                tassert_eq!(e.reason, "invalid frame");
            }
            _ => panic!("unknown management operation"),
        }
        t::mark(&v, "pass");
    }
}
