use std::{
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
};

use fs4::{FileExt, TryLockError};

use crate::{
    error::AppError,
    platform::{protect_private_directory, protect_private_file},
};

const PRIVATE_FILE_NAMES: [&str; 10] = [
    ".node.lock",
    "identity.key",
    "node.sqlite3",
    "node.sqlite3-journal",
    "node.sqlite3-shm",
    "node.sqlite3-wal",
    "transcript-cache.sqlite3",
    "transcript-cache.sqlite3-journal",
    "transcript-cache.sqlite3-shm",
    "transcript-cache.sqlite3-wal",
];

const TRANSCRIPT_CACHE_FILE_NAMES: [&str; 4] = [
    "transcript-cache.sqlite3",
    "transcript-cache.sqlite3-journal",
    "transcript-cache.sqlite3-shm",
    "transcript-cache.sqlite3-wal",
];

pub struct DataDirectory {
    path: PathBuf,
    lock_file: File,
}

impl DataDirectory {
    /// Acquires the single-node lock for a data directory and restricts access to its owner.
    ///
    /// # Errors
    ///
    /// Returns an error when the directory cannot be created, secured, locked, or is already
    /// owned by another node process.
    pub fn acquire(path: &Path) -> Result<Self, AppError> {
        std::fs::create_dir_all(path).map_err(|source| AppError::Io {
            context: "creating the data directory",
            source,
        })?;
        let canonical = std::fs::canonicalize(path).map_err(|source| AppError::Io {
            context: "canonicalizing the data directory",
            source,
        })?;
        protect_private_directory(&canonical)?;

        let lock_path = canonical.join(".node.lock");
        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|source| AppError::Io {
                context: "opening the data directory lock",
                source,
            })?;
        protect_private_file(&lock_path)?;
        match FileExt::try_lock(&lock_file) {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => {
                return Err(AppError::DataDirectoryBusy);
            }
            Err(TryLockError::Error(source)) => {
                return Err(AppError::Io {
                    context: "locking the data directory",
                    source,
                });
            }
        }
        let directory = Self {
            path: canonical,
            lock_file,
        };
        directory.secure_managed_files()?;
        Ok(directory)
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[must_use]
    pub fn database_path(&self) -> PathBuf {
        self.path.join("node.sqlite3")
    }

    #[must_use]
    pub fn transcript_cache_path(&self) -> PathBuf {
        self.path.join("transcript-cache.sqlite3")
    }

    #[must_use]
    pub fn identity_key_path(&self) -> PathBuf {
        self.path.join("identity.key")
    }

    /// Releases the advisory lock held by this data-directory owner.
    ///
    /// # Errors
    ///
    /// Returns an error when the operating system cannot release the lock.
    pub fn unlock(&self) -> Result<(), AppError> {
        FileExt::unlock(&self.lock_file).map_err(|source| AppError::Io {
            context: "unlocking the data directory",
            source,
        })
    }

    /// Re-verifies and applies owner-only ACLs to `transcript-cache.sqlite3` and its WAL/SHM
    /// companion files inside `data_dir`.
    ///
    /// # Errors
    ///
    /// Returns `AppError::UnsafeDataDirectoryEntry` if any entry is a symlink, reparse point, or
    /// non-file, or `AppError::Platform` / `AppError::Io` when ACL enforcement fails.
    pub fn secure_transcript_cache_files(&self) -> Result<(), AppError> {
        secure_transcript_cache_at_base(&self.path)
    }

    pub(crate) fn secure_managed_files(&self) -> Result<(), AppError> {
        secure_named_files(&self.path, &PRIVATE_FILE_NAMES)
    }
}

pub(crate) fn secure_transcript_cache_at_base(base: &Path) -> Result<(), AppError> {
    secure_named_files(base, &TRANSCRIPT_CACHE_FILE_NAMES)
}

fn is_unsafe_entry(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return true;
        }
    }
    metadata.file_type().is_symlink() || !metadata.is_file()
}

fn secure_named_files(base: &Path, names: &[&str]) -> Result<(), AppError> {
    for name in names {
        let path = base.join(name);
        match std::fs::symlink_metadata(&path) {
            Ok(metadata) => {
                if is_unsafe_entry(&metadata) {
                    return Err(AppError::UnsafeDataDirectoryEntry);
                }
                protect_private_file(&path)?;
            }
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => {
                return Err(AppError::Io {
                    context: "inspecting an existing managed data file",
                    source,
                });
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;

    static DIR_SEQ: AtomicU64 = AtomicU64::new(1);

    fn test_directory() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let seq = DIR_SEQ.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "acp-remote-data-dir-{}-{nonce}-{seq}",
            std::process::id()
        ))
    }

    #[test]
    fn excludes_a_second_node_using_the_same_directory() {
        let path = test_directory();
        let first = DataDirectory::acquire(&path).expect("first data directory owner");
        let second = DataDirectory::acquire(&path);
        assert!(matches!(second, Err(AppError::DataDirectoryBusy)));
        drop(first);
        let third = DataDirectory::acquire(&path).expect("lock released after owner exits");
        drop(third);
        std::fs::remove_dir_all(path).expect("cleanup");
    }

    #[test]
    fn secures_existing_managed_files_including_transcript_cache() {
        let path = test_directory();
        std::fs::create_dir_all(&path).expect("test directory");
        for name in [
            "identity.key",
            "node.sqlite3",
            "node.sqlite3-wal",
            "transcript-cache.sqlite3",
            "transcript-cache.sqlite3-wal",
            "transcript-cache.sqlite3-shm",
        ] {
            std::fs::write(path.join(name), []).expect("existing managed file");
        }

        let directory = DataDirectory::acquire(&path).expect("secure data directory");
        assert_eq!(
            directory.transcript_cache_path(),
            directory.path().join("transcript-cache.sqlite3")
        );
        assert_ne!(directory.transcript_cache_path(), directory.database_path());
        directory
            .secure_transcript_cache_files()
            .expect("re-secure transcript cache files");
        for name in [
            "identity.key",
            "node.sqlite3",
            "node.sqlite3-wal",
            "transcript-cache.sqlite3",
            "transcript-cache.sqlite3-wal",
            "transcript-cache.sqlite3-shm",
        ] {
            assert!(directory.path().join(name).is_file());
        }
        drop(directory);
        std::fs::remove_dir_all(path).expect("cleanup");
    }

    #[test]
    fn rejects_directory_or_reparse_entry_at_transcript_cache_path() {
        let path = test_directory();
        std::fs::create_dir_all(path.join("transcript-cache.sqlite3"))
            .expect("impostor directory at transcript-cache.sqlite3");
        let result = DataDirectory::acquire(&path);
        assert!(matches!(result, Err(AppError::UnsafeDataDirectoryEntry)));
        std::fs::remove_dir_all(&path).expect("cleanup impostor directory");

        #[cfg(windows)]
        {
            let target_dir = test_directory();
            std::fs::create_dir_all(&target_dir).expect("junction target");
            std::fs::create_dir_all(&path).expect("data dir");
            let junction_path = path.join("transcript-cache.sqlite3");
            let status = std::process::Command::new("cmd")
                .args([
                    "/C",
                    "mklink",
                    "/J",
                    &junction_path.to_string_lossy(),
                    &target_dir.to_string_lossy(),
                ])
                .output()
                .expect("mklink /J");
            assert!(status.status.success(), "mklink /J should succeed");
            let rejected = DataDirectory::acquire(&path);
            assert!(matches!(rejected, Err(AppError::UnsafeDataDirectoryEntry)));
            std::fs::remove_dir(&junction_path).expect("remove junction");
            std::fs::remove_dir_all(&path).expect("cleanup data dir");
            std::fs::remove_dir_all(&target_dir).expect("cleanup target dir");
        }
    }
}
