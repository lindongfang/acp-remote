use super::*;
#[test]
fn context_matches_independent_plain_byte_vector() {
    // Synthetic public bytes exercise encoding only, never a Node identity or DPAPI grant.
    let request = CatalogRequestId::decode("BBBBBBBBBBBBBBBBBBBBBA").expect("canonical request");
    let key: Vec<u8> = (0..65).collect();
    let context = protection_context("AAAAAAAAAAAAAAAAAAAAAA", &key, request).expect("context");
    assert_eq!(context.len(), 178);
    assert_eq!(
        hex::encode(Sha256::digest(&context)),
        "672f0d8272a4b8708f6681c7cedb6e40fab2953ec00aab758b69d5a0472763da"
    );
    let changed = protection_context("AAAAAAAAAAAAAAAAAAAAAB", &key, request).expect("changed");
    assert_ne!(context, changed);
    let changed = protection_context(
        "AAAAAAAAAAAAAAAAAAAAAA",
        &key,
        CatalogRequestId::new(EntityId::from_bytes([1; 16])),
    )
    .expect("changed");
    assert_ne!(context, changed);
}

struct TempWorkspaceDir(std::path::PathBuf);
impl TempWorkspaceDir {
    fn new(label: &str) -> Self {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "acp-nodecfg-{label}-{}-{nonce}",
            std::process::id()
        ));
        std::fs::create_dir(&path).expect("temp dir");
        Self(path)
    }
}
impl Drop for TempWorkspaceDir {
    fn drop(&mut self) {
        if self.0.exists() {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

#[tokio::test]
async fn cl028_directory_and_launch_path_disk_verification() {
    let root = TempWorkspaceDir::new("cl028-root");
    let nested = root.0.join("nested");
    std::fs::create_dir(&nested).expect("nested dir");
    let sibling = TempWorkspaceDir::new("cl028-sibling");
    let file_path = root.0.join("not-a-dir.txt");
    std::fs::write(&file_path, b"file").expect("file");

    let canonical_root = source_sessions::verify_directory_on_disk(root.0.to_str().expect("utf8"))
        .await
        .expect("valid root directory");
    let canonical_nested =
        source_sessions::verify_directory_on_disk(nested.to_str().expect("utf8"))
            .await
            .expect("valid nested directory");

    assert!(source_sessions::launch_path_within_workspace(&canonical_root, &canonical_root).await);
    assert!(
        source_sessions::launch_path_within_workspace(&canonical_nested, &canonical_root).await
    );
    // Relative / NUL / empty / missing / file / sibling directory outside workspace are all refused.
    for invalid in [
        "",
        "relative/dir",
        "C:\\missing\0sub",
        file_path.to_str().expect("utf8"),
    ] {
        assert_eq!(
            source_sessions::verify_directory_on_disk(invalid).await,
            Err(CatalogError::InvalidConfiguration)
        );
        assert!(!source_sessions::launch_path_within_workspace(invalid, &canonical_root).await);
    }
    let sibling_str = sibling.0.to_str().expect("utf8");
    assert!(!source_sessions::launch_path_within_workspace(sibling_str, &canonical_root).await);

    // Deleting a verified directory on disk causes both checks to refuse immediately.
    std::fs::remove_dir(&nested).expect("remove nested");
    assert_eq!(
        source_sessions::verify_directory_on_disk(&canonical_nested).await,
        Err(CatalogError::InvalidConfiguration)
    );
    assert!(
        !source_sessions::launch_path_within_workspace(&canonical_nested, &canonical_root).await
    );

    // Replacing a workspace path with a regular file causes both checks to refuse.
    std::fs::write(&nested, b"replaced-by-file").expect("file in place of dir");
    assert_eq!(
        source_sessions::verify_directory_on_disk(&canonical_nested).await,
        Err(CatalogError::InvalidConfiguration)
    );
    assert!(
        !source_sessions::launch_path_within_workspace(&canonical_nested, &canonical_root).await
    );
}
