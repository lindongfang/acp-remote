use super::*;

fn pair(name: &str, value: &str) -> (OsString, OsString) {
    (name.into(), value.into())
}

#[test]
fn inherited_hidden_environment_preserves_shape_and_ordinal_identity() {
    let block = environment_from(
        [
            pair("=::", "colon=value"),
            pair("=ACP_OWNER_UNKNOWN", "old"),
            pair("=acp_owner_unknown", "new=value"),
            pair("=exitcode", "00000025"),
            pair("=EXITCODEASCII", "%"),
            pair("=c:", "C:\\test"),
            pair("Path", "old-path"),
        ],
        &[pair("PATH", "new-path")],
    )
    .expect("OS-inherited hidden entries must serialize");
    let expected: Vec<u16> = "=::=colon=value\0=acp_owner_unknown=new=value\0=c:=C:\\test\0=exitcode=00000025\0=EXITCODEASCII=%\0PATH=new-path\0\0"
        .encode_utf16()
        .collect();
    assert_eq!(block, expected);
}

#[test]
fn explicit_hidden_and_malformed_environment_names_remain_invalid() {
    for name in [
        "=::",
        "=ACP_OWNER_UNKNOWN",
        "=exitcode",
        "=c:",
        "=",
        "A=B",
        "",
        "A\0B",
    ] {
        let result = environment_from([], &[pair(name, "SECRET-test-value")]);
        assert!(matches!(result, Err(ProcessOwnerError::InvalidSpec(_))));
        assert!(!result.unwrap_err().to_string().contains("SECRET"));
    }
    for name in ["", "=", "==hidden", "=hidden=extra", "A=B", "A\0B"] {
        assert!(matches!(
            environment_from([pair(name, "value")], &[]),
            Err(ProcessOwnerError::InvalidSpec(_))
        ));
    }
    let long = "A".repeat(32767);
    assert!(environment_from([pair(&long, "value")], &[]).is_err());
    let max_name = "A".repeat(32766);
    assert!(environment_from([pair(&max_name, "value")], &[]).is_ok());
    assert!(environment_from([pair("valid", "value\0suffix")], &[]).is_err());
    assert!(environment_from([pair("=hidden", "")], &[]).is_ok());
}

#[test]
fn inherited_hidden_environment_keeps_utf16_and_empty_block_termination() {
    use std::os::windows::ffi::OsStringExt;
    let name = OsString::from_wide(&[61, 0xd800, 88]);
    let value = OsString::from_wide(&[0xdc00, 61, 88]);
    assert_eq!(
        environment_from([(name, value)], &[]).unwrap(),
        [61, 0xd800, 88, 61, 0xdc00, 61, 88, 0, 0]
    );
    assert_eq!(environment_from([], &[]).unwrap(), [0, 0]);
}
