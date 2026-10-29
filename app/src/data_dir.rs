use std::{
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
};

use fs4::{FileExt, TryLockError};

use crate::{
    error::AppError,
    platform::{protect_private_directory, protect_private_file},
};

const PRIVATE_FILE_NAMES: [&str; 6] = [
    ".node.lock",
    "identity.key",
    "node.sqlite3",
    "node.sqlite3-journal",
    "node.sqlite3-shm",
    "node.sqlite3-wal",
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

    pub(crate) fn secure_managed_files(&self) -> Result<(), AppError> {
        for name in PRIVATE_FILE_NAMES {
            let path = self.path.join(name);
            match std::fs::symlink_metadata(&path) {
                Ok(metadata) => {
                    if metadata.file_type().is_symlink() || !metadata.is_file() {
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
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    fn test_directory() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "acp-remote-data-dir-{}-{nonce}",
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
    fn secures_existing_managed_files() {
        let path = test_directory();
        std::fs::create_dir_all(&path).expect("test directory");
        for name in ["identity.key", "node.sqlite3", "node.sqlite3-wal"] {
            std::fs::write(path.join(name), []).expect("existing managed file");
        }

        let directory = DataDirectory::acquire(&path).expect("secure data directory");
        for name in ["identity.key", "node.sqlite3", "node.sqlite3-wal"] {
            assert!(directory.path().join(name).is_file());
        }
        drop(directory);
        std::fs::remove_dir_all(path).expect("cleanup");
    }
}
