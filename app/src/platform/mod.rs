#[cfg(windows)]
mod console;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use console::{ConsoleText, poll_console_character};

#[cfg(windows)]
pub use windows::{
    DpapiProtector, create_private_pipe, current_user_sid, protect_private_directory,
    protect_private_file, verify_named_pipe_owner,
};

use crate::error::PlatformError;

pub trait SecretProtector: Send + Sync {
    /// Protects plaintext under the operating system's current-user secret boundary.
    ///
    /// # Errors
    ///
    /// Returns an error when the platform cannot protect the supplied data.
    fn protect(&self, plaintext: &[u8], context: &[u8]) -> Result<Vec<u8>, PlatformError>;

    /// Recovers plaintext protected under the operating system's current-user secret boundary.
    ///
    /// # Errors
    ///
    /// Returns an error when the ciphertext is invalid, the context does not match, or the
    /// current user cannot recover it.
    fn unprotect(&self, ciphertext: &[u8], context: &[u8]) -> Result<Vec<u8>, PlatformError>;
}

#[cfg(not(windows))]
pub struct DpapiProtector;

#[cfg(not(windows))]
impl SecretProtector for DpapiProtector {
    fn protect(&self, _plaintext: &[u8], _context: &[u8]) -> Result<Vec<u8>, PlatformError> {
        Err(PlatformError::Unsupported)
    }

    fn unprotect(&self, _ciphertext: &[u8], _context: &[u8]) -> Result<Vec<u8>, PlatformError> {
        Err(PlatformError::Unsupported)
    }
}

#[cfg(not(windows))]
pub fn current_user_sid() -> Result<String, PlatformError> {
    Err(PlatformError::Unsupported)
}

#[cfg(not(windows))]
pub fn protect_private_directory(_path: &std::path::Path) -> Result<(), PlatformError> {
    Err(PlatformError::Unsupported)
}

#[cfg(not(windows))]
pub fn protect_private_file(_path: &std::path::Path) -> Result<(), PlatformError> {
    Err(PlatformError::Unsupported)
}

#[cfg(windows)]
pub mod process_owner;
