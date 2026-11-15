//! Real protocol Core and IO only; not a Node Saved-decision or `OwnedProcess` acceptance test.
#![allow(
    clippy::too_many_lines,
    reason = "Preserves existing module structure and intentional scaffolding"
)]
use super::*;
use crate::{
    application::native_dispatch::NativeApprovalPort, domain::approval_owner::choice_fingerprint,
};
use sha2::{Digest, Sha256};

#[tokio::test]
async fn approval_protocol_empty_original_id_inspection_and_real_zero_keep_single_core_slot() {
    for version in [Version::V1, Version::V2] {
        let (mut client, mut peer) = ready(version, version).await;
        let (foreign, _foreign_peer) = ready(version, version).await;
        let mut frame = permission(version, &json!("approval-empty"));
        frame["params"]["options"][0]["optionId"] = json!("");
        frame["params"]["options"][0]["kind"] = json!("future_original_kind");
        if version == Version::V2 {
            frame["params"]["subject"] = json!({"type":"future_subject","opaque":[1,null]});
        }
        feed(&mut peer, frame).await;
        let Incoming::Permission(original) = client.next(deadline()).await.expect("actual request")
        else {
            panic!("actual permission expected")
        };
        let adapter = native_dispatch::AcpNativeAdapter::default();
        let choices = adapter
            .permission_choices(&original)
            .expect("original choices");
        assert_eq!(adapter.permission_choice_id(&choices[0]), "");
        assert_eq!(
            adapter.permission_choice_kind(&choices[0]),
            "future_original_kind"
        );
        let actual_digest: [u8; 32] = Sha256::digest(original.raw().bytes()).into();
        assert_eq!(adapter.permission_request_digest(&original), actual_digest);
        assert_ne!(
            choice_fingerprint(actual_digest, 0, "").expect("empty ID data"),
            choice_fingerprint(actual_digest, 1, "").expect("different ordinal")
        );
        for _ in 0..2 {
            assert_eq!(
                client
                    .connection
                    .inspect_permission_request(&original)
                    .expect("readonly"),
                PermissionPendingSnapshot::Waiting
            );
        }
        assert_eq!(
            foreign
                .connection
                .inspect_permission_request(&original)
                .expect_err("foreign source"),
            AcpError::InvalidRequestScope
        );
        let reservation = client
            .connection
            .reserve_permission_response(
                &original,
                PermissionSelection::Selected(choices[0].id().clone()),
            )
            .expect("real unique empty ID reservation");
        assert_eq!(
            client
                .connection
                .inspect_permission_request(&original)
                .expect("reserved"),
            PermissionPendingSnapshot::Reserved
        );
        let probe = reservation.cancel();
        assert!(matches!(
            probe.progress(),
            WriteProgress::Finished(WriteReport::Zero {
                reason: ZeroReason::Cancelled
            })
        ));
        assert_eq!(
            client
                .connection
                .inspect_permission_request(&original)
                .expect("same real Waiting"),
            PermissionPendingSnapshot::Waiting
        );
        client
            .permission_response(&original, Some(choices[0].id()), deadline())
            .await
            .expect("actual selected write");
        let reply = sent(&mut peer).await;
        assert_eq!(reply["result"]["outcome"]["outcome"], json!("selected"));
        assert_eq!(reply["result"]["outcome"]["optionId"], json!(""));
        assert_eq!(
            client
                .connection
                .inspect_permission_request(&original)
                .expect("spent"),
            PermissionPendingSnapshot::SpentOrInvalid
        );
        assert!(
            client
                .connection
                .reserve_permission_response(&original, PermissionSelection::Cancelled)
                .is_err()
        );
        client
            .shutdown(deadline())
            .await
            .expect("original IO joined");
        foreign
            .shutdown(deadline())
            .await
            .expect("foreign original IO joined");
    }
}

#[tokio::test]
async fn approval_protocol_duplicate_empty_ids_are_not_unique_but_safety_cancel_still_uses_original()
 {
    for version in [Version::V1, Version::V2] {
        let (mut client, mut peer) = ready(version, version).await;
        let mut frame = permission(version, &json!("duplicate-empty"));
        frame["params"]["options"][0]["optionId"] = json!("");
        frame["params"]["options"][1]["optionId"] = json!("");
        feed(&mut peer, frame).await;
        let Incoming::Permission(original) =
            client.next(deadline()).await.expect("actual original")
        else {
            panic!("permission")
        };
        assert_eq!(
            client
                .connection
                .reserve_permission_response(
                    &original,
                    PermissionSelection::Selected(original.options()[0].id().clone())
                )
                .expect_err("duplicate original IDs"),
            AcpError::InvalidOption
        );
        assert_eq!(
            client
                .connection
                .inspect_permission_request(&original)
                .expect("still original pending"),
            PermissionPendingSnapshot::Waiting
        );
        client
            .permission_response(&original, None, deadline())
            .await
            .expect("explicit safety cancelled");
        assert_eq!(
            sent(&mut peer).await["result"]["outcome"]["outcome"],
            json!("cancelled")
        );
        client
            .shutdown(deadline())
            .await
            .expect("same actual IO joined");
    }
}
