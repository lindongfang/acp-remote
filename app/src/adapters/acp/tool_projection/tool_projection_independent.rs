//! Nested independent tests; Incoming is ordinary data, never a forged Received.
#![allow(
    clippy::many_single_char_names,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
macro_rules! tassert {($($arg:tt)*) => {{ $crate::adapters::acp::raw_preservation_independent::tick(); assert!($($arg)*); }}}
macro_rules! tassert_eq {($($arg:tt)*) => {{ $crate::adapters::acp::raw_preservation_independent::tick(); assert_eq!($($arg)*); }}}

use super::*;
use crate::adapters::acp::raw_preservation_independent as t;
fn set(a: Assignment, k: Knowledge) {
    tassert!(matches!(a, Assignment::Set{knowledge,..} if knowledge==k));
}
fn set_code(a: Assignment, k: Knowledge, wanted: u8) {
    tassert!(matches!(a, Assignment::Set{knowledge,code} if knowledge==k && code==wanted));
}
fn slot(v: &serde_json::Value) -> usize {
    for (k, i) in [
        ("name", 2),
        ("title", 3),
        ("kind", 4),
        ("status", 5),
        ("content", 0),
        ("locations", 6),
        ("rawInput", 7),
        ("rawOutput", 8),
        ("_meta", 1),
    ] {
        if t::id(v).contains(&format!("-{k}-")) {
            return i;
        }
    }
    panic!("fixed tool field absent")
}
#[tokio::test(flavor = "current_thread")]
async fn raw_preservation_independent_tool_projection() {
    let vv = t::vectors("tool");
    tassert_eq!(vv.len(), 130);
    for v in vv {
        t::check_sync(&v,|f|{let bytes=t::input(&v);let raw=super::super::RawMessage::parse(bytes.clone()).unwrap();let s=f.strategy(t::version(&v),"all");let event=t::baseline_event(&s,false);let incoming=Incoming::Update{event,origin:UpdateOrigin::Live,raw:raw.clone()};let r=tool_patch(t::version(&v),&incoming,v["recorded"].as_bool().unwrap_or(true));let e=t::expected(&v);
    if e.contains("MalformedUpdate"){tassert!(matches!(r,Err(ProjectionError::MalformedUpdate)))}else if e=="UnsupportedAnchorKind"{tassert!(matches!(r,Err(ProjectionError::UnsupportedAnchorKind)))}else if e=="Ok(None)"{tassert!(matches!(r,Ok(None)))}else{let p=r.unwrap().unwrap();tassert_eq!(p.id,"t");if e=="Ok(Some(ToolPatch)); name KnownAbsent"{set(p.assignments[2],Knowledge::KnownAbsent);set(p.assignments[3],Knowledge::Known)}else if e.starts_with("content Deferred;"){set(p.assignments[0],Knowledge::Deferred);tassert!(matches!(p.assignments[1],Assignment::Keep))}else if e.starts_with("Ok with _meta"){set(p.assignments[1],Knowledge::Deferred)}else{let a=p.assignments[slot(&v)];match e{"Keep"=>tassert!(matches!(a,Assignment::Keep)),"KnownAbsent/code0"=>{tassert!(matches!(slot(&v),2|3),"legacy text-null descriptor alias only");set(a,Knowledge::KnownAbsent)},"KnownAbsent"=>set(a,Knowledge::KnownAbsent),"KnownEmpty"=>set(a,Knowledge::KnownEmpty),"Uninterpreted"=>set(a,Knowledge::Uninterpreted),"Deferred raw false"=>set(a,Knowledge::Deferred),"Ok(Some(ToolPatch)); Assignment::Set(knowledge=Known,code=0); no span claim"=>set_code(a,Knowledge::Known,0),_=>panic!("unhandled exact legacy tool oracle")}}}tassert_eq!(raw.bytes(),bytes);}).await;
    }
}
