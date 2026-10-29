use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("configuration is invalid: {0}")]
    Config(String),
    #[error("I/O failed while {context}: {source}")]
    Io {
        context: &'static str,
        #[source]
        source: std::io::Error,
    },
    #[error(transparent)]
    Platform(#[from] PlatformError),
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error(transparent)]
    Identity(#[from] IdentityError),
    #[error("data directory is already in use")]
    DataDirectoryBusy,
    #[error("data directory contains an unsafe entry for a managed private file")]
    UnsafeDataDirectoryEntry,
    #[error("local IPC failed: {0}")]
    Ipc(String),
    #[error("TLS initialization failed: {0}")]
    Tls(String),
    #[error(transparent)]
    Pairing(#[from] crate::pairing::PairingError),
    #[error("node startup failed: {0}")]
    Startup(String),
    #[error("node shutdown failed: {0}")]
    Shutdown(String),
    #[error("command is not implemented in this build")]
    NotImplemented,
}

#[derive(Debug, Error)]
pub enum PlatformError {
    #[error("this platform is not supported by the node access service")]
    Unsupported,
    #[error("Windows platform operation failed: {0}")]
    Windows(String),
    #[error("secret protection failed: {0}")]
    Protection(String),
    #[error("cryptographic random source failed: {0}")]
    Random(String),
}

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("database operation failed: {0}")]
    Database(#[from] sqlx::Error),
    #[error("database schema version is unsupported: {0}")]
    UnknownSchema(i64),
    #[error("database contains an unrecognized pre-versioned schema")]
    UnrecognizedSchema,
    #[error("database identity metadata is missing")]
    MissingNodeMeta,
    #[error("database contains more than one node identity")]
    MultipleNodeIdentities,
}

#[derive(Debug, Error)]
pub enum IdentityError {
    #[error("node identity key is missing while metadata exists")]
    KeyMissing,
    #[error("node identity key and metadata do not describe the same identity")]
    IdentityMismatch,
    #[error("node identity key file is invalid")]
    CorruptKey,
    #[error("node identity key file has an unsupported format")]
    UnsupportedKeyFormat,
    #[error(transparent)]
    Platform(#[from] PlatformError),
    #[error(transparent)]
    Storage(#[from] StorageError),
    #[error("I/O failed while {context}: {source}")]
    Io {
        context: &'static str,
        #[source]
        source: std::io::Error,
    },
}
