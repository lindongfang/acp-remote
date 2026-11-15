//! Nested independent tests for the actual private shared projector.
#![allow(
    clippy::match_same_arms,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
macro_rules! tassert {($($arg:tt)*) => {{ $crate::adapters::acp::raw_preservation_independent::tick(); assert!($($arg)*); }}}
macro_rules! tassert_eq {($($arg:tt)*) => {{ $crate::adapters::acp::raw_preservation_independent::tick(); assert_eq!($($arg)*); }}}

use super::*;
use crate::adapters::acp::raw_preservation_independent as t;
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
    panic!("fixed projection field absent")
}
fn check_set(
    raw: &RawMessage,
    field: FieldChange,
    k: FieldKnowledge,
    code: Option<u8>,
    name: &str,
    expected_append: bool,
) {
    let FieldChange::Set {
        knowledge,
        span,
        code: got,
        append,
    } = field
    else {
        panic!("expected Set")
    };
    tassert_eq!(knowledge, k);
    tassert_eq!(got, code);
    tassert_eq!(append, expected_append);
    let (a, b) = span.expect("present source field has exact raw span");
    let d = super::super::raw_json::RawDocument::parse(raw.bytes().to_vec()).unwrap();
    let expected = t::field(t::field(t::field(d.root(), "params"), "update"), name);
    tassert_eq!(&raw.bytes()[a..b], expected.fragment().as_bytes());
}
#[tokio::test(flavor = "current_thread")]
async fn raw_preservation_independent_projection() {
    let vv = t::vectors("projection");
    tassert_eq!(vv.len(), 132);
    for v in vv {
        t::check_sync(&v,|_|{let bytes=t::input(&v);let raw=RawMessage::parse(bytes.clone()).unwrap();let e=t::expected(&v);let r=envelope(&raw,t::version(&v));
    if e.starts_with("ProjectionFailure"){match r{Err((_,err))=>tassert_eq!(err,ViewError::ProjectionFailure),Ok(p)=>{tassert_eq!(p.failure,Some(ViewError::ProjectionFailure));tassert_eq!(p.projection,ProjectionState::ProjectionUnavailable);tassert!(p.fields.iter().all(|x|matches!(x,FieldChange::Keep)),"no chosen partial field published")}}}
    else {let p=r.unwrap_or_else(|_|panic!("unexpected outer projection failure"));tassert!(p.failure.is_none());tassert_eq!(p.session.as_deref(),Some("s"));tassert!(matches!(p.identity,Some(ContentIdentity::Tool(ref id))if &**id=="t"));
        if e.starts_with("Available with"){tassert_eq!(p.projection,ProjectionState::Available);check_set(&raw,p.fields[0],FieldKnowledge::Deferred,None,"content",false);check_set(&raw,p.fields[7],FieldKnowledge::Deferred,None,"rawInput",false);}
        else if e.starts_with("content Deferred;"){check_set(&raw,p.fields[0],FieldKnowledge::Deferred,None,"content",true);tassert!(matches!(p.fields[1],FieldChange::Keep));}
        else if e.starts_with("Ok with _meta"){check_set(&raw,p.fields[1],FieldKnowledge::Deferred,None,"_meta",false);}
        else {let i=slot(&v);let name=["content","_meta","name","title","kind","status","locations","rawInput","rawOutput"][i];match e{
            "Keep"=>tassert!(matches!(p.fields[i],FieldChange::Keep)),
            "KnownAbsent/code0"=>check_set(&raw,p.fields[i],FieldKnowledge::KnownAbsent,None,name,false),
            "KnownAbsent"=>check_set(&raw,p.fields[i],FieldKnowledge::KnownAbsent,None,name,false),
            "KnownEmpty"=>check_set(&raw,p.fields[i],FieldKnowledge::KnownEmpty,None,name,false),
            "Uninterpreted"=>check_set(&raw,p.fields[i],FieldKnowledge::Uninterpreted,None,name,false),
            "Deferred raw false"=>check_set(&raw,p.fields[i],FieldKnowledge::Deferred,None,name,false),
            "FieldChange::Set(knowledge=Known,code=Some(0),span=exact null,append=false)"=>check_set(&raw,p.fields[i],FieldKnowledge::Known,Some(0),name,false),
            _=>panic!("unhandled exact projection oracle")}}
    }tassert_eq!(raw.bytes(),bytes);}).await;
    }
}

#[test]
fn raw_preservation_oracle_append_self_check() {
    let vv = t::vectors("projection");
    let lookup = |wanted: &str| vv.iter().find(|v| t::id(v) == wanted).unwrap();
    for (wanted, want_append, label) in [
        (
            "RAW-DERIVED-projection_envelope-CHUNK-SCOPED-META",
            true,
            "chunk",
        ),
        ("RAW-PROJECTION-OPAQUE", false, "replace"),
    ] {
        let v = lookup(wanted);
        let bytes = t::input(v);
        let raw = RawMessage::parse(bytes.clone()).unwrap();
        let p = envelope(&raw, t::version(v)).unwrap();
        tassert!(p.failure.is_none());
        check_set(
            &raw,
            p.fields[0],
            FieldKnowledge::Deferred,
            None,
            "content",
            want_append,
        );
        if want_append {
            tassert!(matches!(p.fields[1], FieldChange::Keep));
        }
        println!("APPEND_SELF_CHECK positive {label}");
        let FieldChange::Set {
            knowledge,
            span,
            code,
            ..
        } = p.fields[0]
        else {
            panic!("expected Set")
        };
        let injected = FieldChange::Set {
            knowledge,
            span,
            code,
            append: !want_append,
        };
        tassert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                check_set(
                    &raw,
                    injected,
                    FieldKnowledge::Deferred,
                    None,
                    "content",
                    want_append,
                );
            }))
            .is_err()
        );
        println!("APPEND_SELF_CHECK rejected {label}");
        tassert_eq!(raw.bytes(), bytes);
    }
    for wanted in [
        "RAW-DERIVED-projection_envelope-CHUNK-CONTENT-TYPE-DUP",
        "RAW-DERIVED-projection_envelope-CHUNK-META-PARENT-DUP",
    ] {
        let v = lookup(wanted);
        let bytes = t::input(v);
        let raw = RawMessage::parse(bytes.clone()).unwrap();
        match envelope(&raw, t::version(v)) {
            Err((_, error)) => tassert_eq!(error, ViewError::ProjectionFailure),
            Ok(p) => {
                tassert_eq!(p.failure, Some(ViewError::ProjectionFailure));
                tassert_eq!(p.projection, ProjectionState::ProjectionUnavailable);
                tassert!(
                    p.fields
                        .iter()
                        .all(|field| matches!(field, FieldChange::Keep))
                );
            }
        }
        tassert_eq!(raw.bytes(), bytes);
        println!("APPEND_SELF_CHECK guard {wanted}");
    }
    println!("APPEND_SELF_CHECK complete positive=2 rejected=2 guard=2");
}
