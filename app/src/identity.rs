use std::{fs::OpenOptions, io::Write, path::Path};

use p256::ecdsa::{Signature, SigningKey, signature::Signer};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::{
    error::{IdentityError, StorageError},
    jws::{self, JwsError, JwsType},
    platform::SecretProtector,
    random,
    storage::{NodeMeta, SCHEMA_VERSION, Storage},
};

const KEY_REFERENCE: &str = "identity.key";
const KEY_CONTEXT: &[u8] = b"acp-remote/node-identity/v1";
const KEY_MAGIC: &[u8; 6] = b"ACPRK1";

pub struct NodeIdentity {
    signing_key: SigningKey,
    public_key: Vec<u8>,
    node_id: String,
}

impl std::fmt::Debug for NodeIdentity {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NodeIdentity")
            .field("node_id", &self.node_id)
            .field("public_key_len", &self.public_key.len())
            .finish_non_exhaustive()
    }
}

impl NodeIdentity {
    #[must_use]
    pub fn node_id(&self) -> &str {
        &self.node_id
    }

    #[must_use]
    pub fn public_key(&self) -> &[u8] {
        &self.public_key
    }

    #[must_use]
    pub fn sign(&self, message: &[u8]) -> [u8; 64] {
        let signature: Signature = self.signing_key.sign(message);
        signature.to_bytes().into()
    }

    /// Signs one strict protocol JWS with the node identity.
    ///
    /// # Errors
    ///
    /// Returns an error when the payload is not a bounded JSON object or signing fails.
    pub fn sign_jws(&self, typ: JwsType, payload: &[u8]) -> Result<String, JwsError> {
        jws::sign(typ, payload, &self.signing_key)
    }

    fn from_signing_key(signing_key: SigningKey) -> Result<Self, IdentityError> {
        let public_key = signing_key
            .verifying_key()
            .to_sec1_point(false)
            .as_bytes()
            .to_vec();
        if public_key.len() != 65 || public_key.first() != Some(&4) {
            return Err(IdentityError::CorruptKey);
        }
        let node_id = hex::encode(Sha256::digest(&public_key));
        Ok(Self {
            signing_key,
            public_key,
            node_id,
        })
    }

    #[cfg(test)]
    pub(crate) fn for_test(seed: [u8; 32]) -> Self {
        Self::from_signing_key(SigningKey::from_slice(&seed).expect("test signing key"))
            .expect("test identity")
    }
}

/// Loads the node identity or creates one only when no identity history exists.
///
/// # Errors
///
/// Returns an error when identity history is inconsistent, the protected key cannot be read or
/// verified, or metadata cannot be recovered or committed.
pub async fn initialize(
    data_dir: &Path,
    storage: &Storage,
    protector: &dyn SecretProtector,
) -> Result<NodeIdentity, IdentityError> {
    let key_path = data_dir.join(KEY_REFERENCE);
    let meta = storage.load_node_meta().await?;
    let key_exists = validate_private_key_entry(&key_path)?;

    if !key_exists && meta.is_some() {
        return Err(IdentityError::KeyMissing);
    }
    if !key_exists && meta.is_none() && storage.has_identity_history().await? {
        return Err(StorageError::MissingNodeMeta.into());
    }

    let identity = if key_exists {
        #[cfg(windows)]
        crate::platform::protect_private_file(&key_path).map_err(IdentityError::Platform)?;
        load_identity(&key_path, protector)?
    } else {
        let identity = generate_identity()?;
        persist_private_key(&key_path, &identity, protector)?;
        #[cfg(test)]
        crate::test_support::crash_at("identity-after-key-commit");
        identity
    };

    if let Some(meta) = meta {
        verify_meta(&identity, &meta)?;
    } else {
        let next_meta = meta_for(&identity);
        storage.write_node_meta(&next_meta).await?;
    }

    Ok(identity)
}

fn validate_private_key_entry(path: &Path) -> Result<bool, IdentityError> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(IdentityError::CorruptKey);
            }
            Ok(true)
        }
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(source) => Err(IdentityError::Io {
            context: "inspecting the protected node identity",
            source,
        }),
    }
}

fn generate_identity() -> Result<NodeIdentity, IdentityError> {
    let private = Zeroizing::new(random::bytes::<32>()?);
    let signing_key =
        SigningKey::from_slice(private.as_slice()).map_err(|_| IdentityError::CorruptKey)?;
    NodeIdentity::from_signing_key(signing_key)
}

fn load_identity(
    path: &Path,
    protector: &dyn SecretProtector,
) -> Result<NodeIdentity, IdentityError> {
    let ciphertext = std::fs::read(path).map_err(|source| IdentityError::Io {
        context: "reading the protected node identity",
        source,
    })?;
    let plaintext = protector
        .unprotect(&ciphertext, KEY_CONTEXT)
        .map_err(IdentityError::Platform)?;
    let plaintext = Zeroizing::new(plaintext);
    if plaintext.len() != KEY_MAGIC.len() + 32 || &plaintext[..KEY_MAGIC.len()] != KEY_MAGIC {
        return Err(IdentityError::UnsupportedKeyFormat);
    }
    let mut private = Zeroizing::new([0_u8; 32]);
    private.copy_from_slice(&plaintext[KEY_MAGIC.len()..]);
    let signing_key =
        SigningKey::from_slice(private.as_slice()).map_err(|_| IdentityError::CorruptKey)?;
    NodeIdentity::from_signing_key(signing_key)
}

fn persist_private_key(
    path: &Path,
    identity: &NodeIdentity,
    protector: &dyn SecretProtector,
) -> Result<(), IdentityError> {
    let mut plaintext = Zeroizing::new(Vec::with_capacity(KEY_MAGIC.len() + 32));
    plaintext.extend_from_slice(KEY_MAGIC);
    plaintext.extend_from_slice(&identity.signing_key.to_bytes());
    let ciphertext = protector
        .protect(plaintext.as_slice(), KEY_CONTEXT)
        .map_err(IdentityError::Platform)?;

    let nonce = random::hex(8)?;
    let temporary_path = path.with_extension(format!("tmp-{nonce}"));
    let mut temporary = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary_path)
        .map_err(|source| IdentityError::Io {
            context: "creating the temporary node identity",
            source,
        })?;
    if let Err(error) = crate::platform::protect_private_file(&temporary_path) {
        drop(temporary);
        let _ = std::fs::remove_file(&temporary_path);
        return Err(IdentityError::Platform(error));
    }
    #[cfg(not(test))]
    let written = temporary
        .write_all(&ciphertext)
        .and_then(|()| temporary.sync_all());
    #[cfg(test)]
    let written = (|| {
        let prefix_length = ciphertext.len() / 2;
        temporary.write_all(&ciphertext[..prefix_length])?;
        crate::test_support::crash_identity_write_at(
            "identity-partial-key-write",
            prefix_length,
            ciphertext.len(),
        );
        temporary.write_all(&ciphertext[prefix_length..])?;
        crate::test_support::crash_identity_write_at(
            "identity-before-key-flush",
            ciphertext.len(),
            ciphertext.len(),
        );
        temporary.sync_all()
    })();
    if let Err(source) = written {
        drop(temporary);
        let _ = std::fs::remove_file(&temporary_path);
        return Err(IdentityError::Io {
            context: "writing the protected node identity",
            source,
        });
    }
    #[cfg(test)]
    crate::test_support::crash_at("identity-before-key-rename");
    drop(temporary);

    if path.exists() {
        let _ = std::fs::remove_file(&temporary_path);
        return Err(IdentityError::IdentityMismatch);
    }
    if let Err(source) = std::fs::rename(&temporary_path, path) {
        let _ = std::fs::remove_file(&temporary_path);
        if source.kind() == std::io::ErrorKind::AlreadyExists {
            return Err(IdentityError::IdentityMismatch);
        }
        return Err(IdentityError::Io {
            context: "committing the protected node identity",
            source,
        });
    }
    Ok(())
}

fn meta_for(identity: &NodeIdentity) -> NodeMeta {
    NodeMeta {
        schema_version: SCHEMA_VERSION,
        node_id: identity.node_id.clone(),
        public_key: identity.public_key.clone(),
        key_file_ref: KEY_REFERENCE.to_owned(),
    }
}

fn verify_meta(identity: &NodeIdentity, meta: &NodeMeta) -> Result<(), IdentityError> {
    if meta.schema_version != SCHEMA_VERSION
        || meta.node_id != identity.node_id
        || meta.public_key != identity.public_key
        || meta.key_file_ref != KEY_REFERENCE
    {
        return Err(IdentityError::IdentityMismatch);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        ffi::OsString,
        path::PathBuf,
        process::Command,
        sync::Mutex,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;
    use crate::error::PlatformError;

    const DPAPI_CHILD_DIR: &str = "ACP_REMOTE_DPAPI_CHILD_DIR";
    const DPAPI_CHILD_NODE_ID: &str = "ACP_REMOTE_DPAPI_CHILD_NODE_ID";
    const CRASH_CHILD_DIR: &str = "ACP_REMOTE_IDENTITY_CRASH_DIR";
    const CRASH_CHILD_POINT: &str = "ACP_REMOTE_IDENTITY_CRASH_POINT";

    #[derive(Default)]
    struct FakeProtector {
        values: Mutex<Vec<Vec<u8>>>,
    }

    impl SecretProtector for FakeProtector {
        fn protect(&self, plaintext: &[u8], context: &[u8]) -> Result<Vec<u8>, PlatformError> {
            let mut value = b"FAKE".to_vec();
            value.extend_from_slice(context);
            value.extend_from_slice(plaintext);
            self.values
                .lock()
                .expect("fake protector lock")
                .push(value.clone());
            Ok(value)
        }

        fn unprotect(&self, ciphertext: &[u8], context: &[u8]) -> Result<Vec<u8>, PlatformError> {
            let expected = [b"FAKE".as_slice(), context].concat();
            if !ciphertext.starts_with(&expected) {
                return Err(PlatformError::Protection("bad fake context".to_owned()));
            }
            Ok(ciphertext[expected.len()..].to_vec())
        }
    }

    struct TestArea {
        directory: PathBuf,
        storage: Storage,
    }

    impl TestArea {
        async fn new() -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos();
            let directory = std::env::temp_dir().join(format!(
                "acp-remote-identity-{}-{nonce}",
                std::process::id()
            ));
            std::fs::create_dir_all(&directory).expect("test directory");
            let storage = Storage::open(&directory.join("node.sqlite3"))
                .await
                .expect("storage");
            Self { directory, storage }
        }

        async fn reopen_storage(&mut self) {
            self.storage.close().await;
            self.storage = Storage::open(&self.directory.join("node.sqlite3"))
                .await
                .expect("reopen storage");
        }

        async fn run_crash_child(&self, point: &str) {
            self.run_crash_child_with_output(point).await;
        }

        async fn run_crash_child_with_output(&self, point: &str) -> std::process::Output {
            self.storage.close().await;
            let executable = std::env::current_exe().expect("test executable path");
            let output = Command::new(executable)
                .arg("identity::tests::identity_crash_child")
                .arg("--exact")
                .arg("--nocapture")
                .env(CRASH_CHILD_DIR, OsString::from(&self.directory))
                .env(CRASH_CHILD_POINT, point)
                .output()
                .expect("run identity crash child");
            assert!(!output.status.success(), "crash child must terminate");
            assert!(
                String::from_utf8_lossy(&output.stdout).contains(&format!("CRASH_POINT={point}")),
                "crash child did not reach {point}: {}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            output
        }
    }

    impl Drop for TestArea {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.directory);
        }
    }

    #[tokio::test]
    async fn fresh_identity_persists_across_reopen() {
        let mut area = TestArea::new().await;
        let protector = FakeProtector::default();
        let first = initialize(&area.directory, &area.storage, &protector)
            .await
            .expect("first identity");
        let first_id = first.node_id().to_owned();
        area.reopen_storage().await;
        let second = initialize(&area.directory, &area.storage, &protector)
            .await
            .expect("recovered identity");
        assert_eq!(second.node_id(), first_id);
    }

    #[tokio::test]
    async fn key_without_meta_recovers_same_identity() {
        let area = TestArea::new().await;
        let protector = FakeProtector::default();
        let first = initialize(&area.directory, &area.storage, &protector)
            .await
            .expect("first identity");
        sqlx::query("DELETE FROM node_meta")
            .execute(area.storage.pool_for_test())
            .await
            .expect("remove meta");
        let second = initialize(&area.directory, &area.storage, &protector)
            .await
            .expect("recovered identity");
        assert_eq!(second.node_id(), first.node_id());
    }

    #[tokio::test]
    async fn meta_without_key_fails_closed() {
        let area = TestArea::new().await;
        let protector = FakeProtector::default();
        initialize(&area.directory, &area.storage, &protector)
            .await
            .expect("identity");
        std::fs::remove_file(area.directory.join(KEY_REFERENCE)).expect("remove key");
        let error = initialize(&area.directory, &area.storage, &protector)
            .await
            .expect_err("must fail");
        assert!(matches!(error, IdentityError::KeyMissing));
    }

    #[tokio::test]
    async fn key_and_meta_mismatch_fails_closed() {
        let area = TestArea::new().await;
        let protector = FakeProtector::default();
        initialize(&area.directory, &area.storage, &protector)
            .await
            .expect("identity");
        let replacement = generate_identity().expect("replacement key");
        std::fs::remove_file(area.directory.join(KEY_REFERENCE)).expect("remove key");
        persist_private_key(
            &area.directory.join(KEY_REFERENCE),
            &replacement,
            &protector,
        )
        .expect("write replacement");
        let error = initialize(&area.directory, &area.storage, &protector)
            .await
            .expect_err("must fail");
        assert!(matches!(error, IdentityError::IdentityMismatch));
    }

    #[tokio::test]
    async fn unknown_schema_blocks_identity_initialization() {
        let area = TestArea::new().await;
        sqlx::query("PRAGMA user_version = 99")
            .execute(area.storage.pool_for_test())
            .await
            .expect("change schema");
        area.storage.close().await;
        let error = Storage::open(&area.directory.join("node.sqlite3"))
            .await
            .err()
            .expect("unknown schema must fail");
        assert!(matches!(error, StorageError::UnknownSchema(99)));
        assert!(!area.directory.join(KEY_REFERENCE).exists());
    }

    #[tokio::test]
    async fn historical_state_without_key_or_meta_fails_closed() {
        let area = TestArea::new().await;
        sqlx::query(
            "INSERT INTO clients(
                client_id, public_key, display_name, status, auth_generation, created_at, revoked_at
             ) VALUES (?, ?, ?, 'trusted', 1, 0, NULL)",
        )
        .bind("client-id")
        .bind(vec![4_u8; 65])
        .bind("Web")
        .execute(area.storage.pool_for_test())
        .await
        .expect("insert historical client");

        let protector = FakeProtector::default();
        let error = initialize(&area.directory, &area.storage, &protector)
            .await
            .expect_err("historical state must not create an identity");
        assert!(matches!(
            error,
            IdentityError::Storage(StorageError::MissingNodeMeta)
        ));
        assert!(!area.directory.join(KEY_REFERENCE).exists());
    }

    #[tokio::test]
    async fn meta_write_failure_preserves_the_key_for_recovery() {
        let area = TestArea::new().await;
        sqlx::query(
            "CREATE TRIGGER fail_node_meta
             BEFORE INSERT ON node_meta
             BEGIN SELECT RAISE(ABORT, 'injected metadata failure'); END",
        )
        .execute(area.storage.pool_for_test())
        .await
        .expect("install failing metadata trigger");

        let protector = FakeProtector::default();
        let error = initialize(&area.directory, &area.storage, &protector)
            .await
            .expect_err("metadata write must fail");
        assert!(matches!(
            error,
            IdentityError::Storage(StorageError::Database(_))
        ));
        let persisted_key =
            std::fs::read(area.directory.join(KEY_REFERENCE)).expect("persisted key");
        assert!(
            area.storage
                .load_node_meta()
                .await
                .expect("meta read")
                .is_none()
        );

        sqlx::query("DROP TRIGGER fail_node_meta")
            .execute(area.storage.pool_for_test())
            .await
            .expect("remove failing trigger");
        let recovered = initialize(&area.directory, &area.storage, &protector)
            .await
            .expect("recover metadata from key");
        assert_eq!(
            std::fs::read(area.directory.join(KEY_REFERENCE)).expect("key after recovery"),
            persisted_key
        );
        assert!(!recovered.node_id().is_empty());
        assert!(area.storage.load_node_meta().await.expect("meta").is_some());
    }

    #[tokio::test]
    async fn malformed_private_key_fails_closed() {
        let area = TestArea::new().await;
        let protector = FakeProtector::default();
        initialize(&area.directory, &area.storage, &protector)
            .await
            .expect("identity");
        let malformed = protector
            .protect(b"not-a-node-key", KEY_CONTEXT)
            .expect("fake ciphertext");
        std::fs::write(area.directory.join(KEY_REFERENCE), malformed).expect("replace key");

        let error = initialize(&area.directory, &area.storage, &protector)
            .await
            .expect_err("malformed key must fail");
        assert!(matches!(error, IdentityError::UnsupportedKeyFormat));
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn dpapi_identity_is_recovered_by_a_new_process() {
        if std::env::var_os(DPAPI_CHILD_DIR).is_some() {
            return;
        }

        let area = TestArea::new().await;
        let protector = crate::platform::DpapiProtector;
        let first = initialize(&area.directory, &area.storage, &protector)
            .await
            .expect("identity protected with DPAPI");
        let expected_node_id = first.node_id().to_owned();
        area.storage.close().await;

        let executable = std::env::current_exe().expect("test executable path");
        let output = Command::new(executable)
            .arg("identity::tests::dpapi_identity_child")
            .arg("--exact")
            .arg("--nocapture")
            .env(DPAPI_CHILD_DIR, OsString::from(&area.directory))
            .env(DPAPI_CHILD_NODE_ID, OsString::from(&expected_node_id))
            .output()
            .expect("run DPAPI identity child");
        assert!(
            output.status.success(),
            "DPAPI identity child failed: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains(&expected_node_id),
            "child did not report the recovered node identity"
        );
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn dpapi_identity_child() {
        let Some(directory) = std::env::var_os(DPAPI_CHILD_DIR) else {
            return;
        };
        let expected_node_id =
            std::env::var(DPAPI_CHILD_NODE_ID).expect("child node identity is configured");
        let directory = PathBuf::from(directory);
        let storage = Storage::open(&directory.join("node.sqlite3"))
            .await
            .expect("child storage");
        let identity = initialize(&directory, &storage, &crate::platform::DpapiProtector)
            .await
            .expect("child identity recovery");
        assert_eq!(identity.node_id(), expected_node_id);
        println!("NODE_ID={}", identity.node_id());
        storage.close().await;
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn process_crashes_recover_identity_without_replacement_material() {
        if std::env::var_os(CRASH_CHILD_DIR).is_some() {
            return;
        }
        let protector = crate::platform::DpapiProtector;

        let mut before_rename = TestArea::new().await;
        before_rename
            .run_crash_child("identity-before-key-rename")
            .await;
        before_rename.reopen_storage().await;
        assert!(!before_rename.directory.join(KEY_REFERENCE).exists());
        assert!(
            before_rename
                .storage
                .load_node_meta()
                .await
                .expect("meta after pre-rename crash")
                .is_none()
        );
        assert!(
            std::fs::read_dir(&before_rename.directory)
                .expect("read crash directory")
                .filter_map(Result::ok)
                .any(|entry| entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("identity.tmp-")),
            "pre-rename crash should leave an uncommitted temporary key"
        );
        initialize(&before_rename.directory, &before_rename.storage, &protector)
            .await
            .expect("fresh identity after pre-rename crash");

        for point in ["identity-after-key-commit", "identity-before-meta-commit"] {
            let mut area = TestArea::new().await;
            area.run_crash_child(point).await;
            area.reopen_storage().await;
            let key_before = std::fs::read(area.directory.join(KEY_REFERENCE))
                .expect("key committed before crash");
            assert!(
                area.storage
                    .load_node_meta()
                    .await
                    .expect("meta after key-only crash")
                    .is_none()
            );
            initialize(&area.directory, &area.storage, &protector)
                .await
                .expect("recover identity from committed key");
            assert_eq!(
                std::fs::read(area.directory.join(KEY_REFERENCE))
                    .expect("key after metadata recovery"),
                key_before
            );
        }

        let mut after_meta = TestArea::new().await;
        after_meta
            .run_crash_child("identity-after-meta-commit")
            .await;
        after_meta.reopen_storage().await;
        let key_before = std::fs::read(after_meta.directory.join(KEY_REFERENCE))
            .expect("key before metadata commit");
        assert!(
            after_meta
                .storage
                .load_node_meta()
                .await
                .expect("meta after commit")
                .is_some()
        );
        initialize(&after_meta.directory, &after_meta.storage, &protector)
            .await
            .expect("recover identity after metadata commit");
        assert_eq!(
            std::fs::read(after_meta.directory.join(KEY_REFERENCE))
                .expect("key after committed restart"),
            key_before
        );
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn partial_write_and_pre_flush_crashes_preserve_uncommitted_identity_boundaries() {
        let protector = crate::platform::DpapiProtector;
        for point in ["identity-partial-key-write", "identity-before-key-flush"] {
            let mut area = TestArea::new().await;
            let marker = area.directory.join("keep-user-marker.txt");
            std::fs::write(&marker, b"preserve fixture marker").expect("marker");
            let output = area.run_crash_child_with_output(point).await;
            let stdout = String::from_utf8_lossy(&output.stdout);
            let lengths = stdout
                .lines()
                .find_map(|line| line.strip_prefix("IDENTITY_WRITE_BYTES="))
                .expect("explicit write stage metadata");
            let (written, total) = lengths.split_once('/').expect("length metadata");
            let written: u64 = written.parse().expect("written length");
            let total: u64 = total.parse().expect("total length");
            let temporary: Vec<_> = std::fs::read_dir(&area.directory)
                .expect("directory")
                .map(|entry| entry.expect("entry"))
                .filter(|entry| {
                    entry
                        .file_name()
                        .to_string_lossy()
                        .starts_with("identity.tmp-")
                })
                .collect();
            assert_eq!(temporary.len(), 1, "one owned temporary ciphertext");
            assert!(written > 0);
            assert_eq!(
                temporary[0].metadata().expect("temporary metadata").len(),
                written
            );
            if point == "identity-partial-key-write" {
                assert!(written < total);
            } else {
                assert_eq!(written, total);
            }
            assert!(!area.directory.join(KEY_REFERENCE).exists());
            area.reopen_storage().await;
            assert!(
                area.storage
                    .load_node_meta()
                    .await
                    .expect("metadata query")
                    .is_none()
            );
            let first = initialize(&area.directory, &area.storage, &protector)
                .await
                .expect("legal first initialization resumes");
            let node_id = first.node_id().to_owned();
            let public_key = first.public_key().to_vec();
            drop(first);
            area.reopen_storage().await;
            let second = initialize(&area.directory, &area.storage, &protector)
                .await
                .expect("committed identity restores");
            assert_eq!(second.node_id(), node_id);
            assert_eq!(second.public_key(), public_key);
            assert_eq!(
                std::fs::read(&marker).expect("original marker"),
                b"preserve fixture marker"
            );
            assert!(
                temporary[0].path().exists(),
                "no directory wipe conceals the interrupted write"
            );
            drop(second);
            area.storage.close().await;
            let owned_directory = area.directory.clone();
            drop(area);
            assert!(
                !owned_directory.exists(),
                "owned crash fixture was not cleaned"
            );
        }
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn identity_crash_child() {
        let Some(directory) = std::env::var_os(CRASH_CHILD_DIR) else {
            return;
        };
        let directory = PathBuf::from(directory);
        let storage = Storage::open(&directory.join("node.sqlite3"))
            .await
            .expect("crash child storage");
        let _identity = initialize(&directory, &storage, &crate::platform::DpapiProtector)
            .await
            .expect("crash child identity setup");
        panic!("configured identity crash point was not reached");
    }
}
