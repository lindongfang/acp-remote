use super::{AppError, IdentityError, PlatformError};
use std::error::Error;

const PRIVATE_CAUSE: &str = "redaction-development-private-cause";

fn assert_public_formats_are_safe(error: &dyn Error) {
    let mut current = Some(error);
    while let Some(error) = current {
        for text in [
            error.to_string(),
            format!("{error:?}"),
            format!("{error:#?}"),
        ] {
            assert!(
                !text.contains(PRIVATE_CAUSE),
                "private protection cause leaked"
            );
        }
        current = error.source();
    }
}

#[test]
fn protection_redaction_direct_formats_and_source_are_safe() {
    let error = PlatformError::Protection(PRIVATE_CAUSE.to_owned());
    assert_public_formats_are_safe(&error);
    assert!(
        error.to_string() == "secret protection failed",
        "protection display must be the safe category"
    );
    assert!(
        format!("{error:?}") == "Protection",
        "protection debug must be the safe category"
    );
    assert!(
        format!("{error:#?}") == "Protection",
        "alternate protection debug must be the safe category"
    );
    assert!(error.source().is_none());
    assert!(matches!(error, PlatformError::Protection(_)));
}

#[test]
fn protection_redaction_transparent_identity_and_app_are_safe() {
    let identity = IdentityError::from(PlatformError::Protection(PRIVATE_CAUSE.to_owned()));
    assert_public_formats_are_safe(&identity);
    assert!(
        identity.to_string() == "secret protection failed",
        "identity display must preserve safe protection category"
    );
    let error = AppError::from(identity);
    assert_public_formats_are_safe(&error);
    assert!(
        error.to_string() == "secret protection failed",
        "protection display must be the safe category"
    );
    assert!(matches!(
        error,
        AppError::Identity(IdentityError::Platform(PlatformError::Protection(_)))
    ));
    let error = AppError::from(PlatformError::Protection(PRIVATE_CAUSE.to_owned()));
    assert_public_formats_are_safe(&error);
    assert!(
        error.to_string() == "secret protection failed",
        "protection display must be the safe category"
    );
}

#[test]
fn protection_redaction_preserves_other_platform_categories() {
    let error = PlatformError::Unsupported;
    assert_eq!(format!("{error:?}"), "Unsupported");
    assert_eq!(
        error.to_string(),
        "this platform is not supported by the node access service"
    );
    let error = PlatformError::Windows("operation code 5".to_owned());
    assert_eq!(format!("{error:?}"), "Windows(\"operation code 5\")");
    assert_eq!(
        error.to_string(),
        "Windows platform operation failed: operation code 5"
    );
    let error = PlatformError::Random("source code 7".to_owned());
    assert_eq!(format!("{error:?}"), "Random(\"source code 7\")");
    assert_eq!(
        error.to_string(),
        "cryptographic random source failed: source code 7"
    );
}

#[cfg(windows)]
#[test]
fn protection_redaction_real_dpapi_failure_uses_same_boundary() {
    use crate::platform::SecretProtector;
    let error = crate::platform::DpapiProtector
        .unprotect(
            b"invalid DPAPI ciphertext",
            b"redaction-development-context",
        )
        .expect_err("malformed ciphertext must fail real DPAPI");
    assert!(matches!(error, PlatformError::Protection(_)));
    assert!(
        error.to_string() == "secret protection failed",
        "protection display must be the safe category"
    );
    assert!(
        format!("{error:?}") == "Protection",
        "protection debug must be the safe category"
    );
    assert!(error.source().is_none());
}
