use super::ATTACHMENT_FEATURE;

#[test]
fn formal_attachment_service_keeps_the_owned_transfer_feature() {
    assert_eq!(ATTACHMENT_FEATURE, "core.attachment-transfer.v1");
}
