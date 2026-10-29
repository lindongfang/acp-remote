use std::{
    ffi::{OsStr, c_void},
    io,
    mem::size_of,
    os::windows::{
        ffi::{OsStrExt, OsStringExt},
        io::AsRawHandle,
    },
    path::Path,
    ptr::null_mut,
    slice,
};

use tokio::net::windows::named_pipe::ServerOptions;
use windows_sys::Win32::{
    Foundation::{CloseHandle, ERROR_SUCCESS, HANDLE, LocalFree},
    Security::Authorization::{
        ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
        GetNamedSecurityInfoW, SDDL_REVISION_1, SE_FILE_OBJECT, SetNamedSecurityInfoW,
    },
    Security::Cryptography::{
        CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
    },
    Security::{
        DACL_SECURITY_INFORMATION, GetSecurityDescriptorDacl, GetTokenInformation,
        OBJECT_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION,
        PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID, RevertToSelf,
        SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER, TokenUser,
    },
    System::{
        Pipes::ImpersonateNamedPipeClient,
        Threading::{GetCurrentProcess, GetCurrentThread, OpenProcessToken, OpenThreadToken},
    },
};
use zeroize::Zeroize;

use crate::error::PlatformError;

#[derive(Debug, Default)]
pub struct DpapiProtector;

impl super::SecretProtector for DpapiProtector {
    fn protect(&self, plaintext: &[u8], context: &[u8]) -> Result<Vec<u8>, PlatformError> {
        let input = dpapi_blob(plaintext)?;
        let entropy = dpapi_blob(context)?;
        let mut output = CRYPT_INTEGER_BLOB::default();
        // SAFETY: The input and entropy blobs remain valid for the call, and a successful
        // output allocation is copied and released by `take_dpapi_output`.
        let succeeded = unsafe {
            CryptProtectData(
                &raw const input,
                std::ptr::null(),
                &raw const entropy,
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &raw mut output,
            )
        };
        if succeeded == 0 {
            return Err(last_protection_error("CryptProtectData"));
        }
        take_dpapi_output(output)
    }

    fn unprotect(&self, ciphertext: &[u8], context: &[u8]) -> Result<Vec<u8>, PlatformError> {
        let input = dpapi_blob(ciphertext)?;
        let entropy = dpapi_blob(context)?;
        let mut output = CRYPT_INTEGER_BLOB::default();
        // SAFETY: The input and entropy blobs remain valid for the call, and a successful
        // output allocation is copied and released by `take_dpapi_output`.
        let succeeded = unsafe {
            CryptUnprotectData(
                &raw const input,
                null_mut(),
                &raw const entropy,
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &raw mut output,
            )
        };
        if succeeded == 0 {
            return Err(last_protection_error("CryptUnprotectData"));
        }
        take_dpapi_output(output)
    }
}

fn dpapi_blob(data: &[u8]) -> Result<CRYPT_INTEGER_BLOB, PlatformError> {
    let length = u32::try_from(data.len())
        .map_err(|_| PlatformError::Protection("DPAPI input is too large".to_owned()))?;
    Ok(CRYPT_INTEGER_BLOB {
        cbData: length,
        pbData: data.as_ptr().cast_mut(),
    })
}

fn take_dpapi_output(output: CRYPT_INTEGER_BLOB) -> Result<Vec<u8>, PlatformError> {
    let memory = if output.pbData.is_null() {
        None
    } else {
        Some(LocalMemory::secret(
            output.pbData.cast(),
            output.cbData as usize,
        ))
    };
    if output.cbData == 0 {
        return Ok(Vec::new());
    }
    if memory.is_none() {
        return Err(PlatformError::Protection(
            "DPAPI returned output without a buffer".to_owned(),
        ));
    }
    let length = usize::try_from(output.cbData)
        .map_err(|_| PlatformError::Protection("DPAPI output is too large".to_owned()))?;
    // SAFETY: A successful DPAPI call returns `cbData` readable bytes at `pbData`,
    // and the allocation is kept alive by `memory` until the copy completes.
    Ok(unsafe { slice::from_raw_parts(output.pbData, length) }.to_vec())
}

struct TokenHandle(HANDLE);

impl Drop for TokenHandle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: The handle is owned by this wrapper and closed exactly once.
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}

struct ImpersonationGuard {
    active: bool,
}

#[cfg(test)]
std::thread_local! {
    static PIPE_CHECK_FAILURE: std::cell::Cell<Option<&'static str>> = const {
        std::cell::Cell::new(None)
    };
}

#[cfg(test)]
fn fail_pipe_check_for_test(operation: &'static str) -> Result<(), PlatformError> {
    PIPE_CHECK_FAILURE.with(|point| {
        if point.get() == Some(operation) {
            point.set(None);
            Err(PlatformError::Windows(format!(
                "injected {operation} failure"
            )))
        } else {
            Ok(())
        }
    })
}

impl ImpersonationGuard {
    fn revert(&mut self) -> Result<(), PlatformError> {
        if !self.active {
            return Ok(());
        }
        #[cfg(test)]
        fail_pipe_check_for_test("RevertToSelf")?;
        // SAFETY: This balances one successful ImpersonateNamedPipeClient call.
        if unsafe { RevertToSelf() } == 0 {
            return Err(last_windows_error("RevertToSelf"));
        }
        self.active = false;
        Ok(())
    }
}

impl Drop for ImpersonationGuard {
    fn drop(&mut self) {
        if self.active {
            // SAFETY: Best-effort fallback if explicit reversion failed.
            unsafe {
                RevertToSelf();
            }
        }
    }
}

struct LocalMemory {
    pointer: PSECURITY_DESCRIPTOR,
    zeroize_len: usize,
}

impl LocalMemory {
    const fn new(pointer: PSECURITY_DESCRIPTOR) -> Self {
        Self {
            pointer,
            zeroize_len: 0,
        }
    }

    const fn secret(pointer: PSECURITY_DESCRIPTOR, zeroize_len: usize) -> Self {
        Self {
            pointer,
            zeroize_len,
        }
    }
}

impl Drop for LocalMemory {
    fn drop(&mut self) {
        if !self.pointer.is_null() {
            if self.zeroize_len != 0 {
                // SAFETY: Windows allocated this buffer with the recorded length. It remains live
                // until this wrapper releases it.
                unsafe {
                    std::slice::from_raw_parts_mut(self.pointer.cast::<u8>(), self.zeroize_len)
                        .zeroize();
                }
            }
            // SAFETY: This pointer was allocated by LocalAlloc-compatible Windows APIs.
            unsafe {
                LocalFree(self.pointer);
            }
        }
    }
}

/// Returns the SID string of the current Windows process user.
///
/// # Errors
///
/// Returns an error when the process token or its user SID cannot be read.
pub fn current_user_sid() -> Result<String, PlatformError> {
    let mut token = null_mut();
    // SAFETY: The pseudo process handle is valid for the process lifetime.
    let opened = unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &raw mut token) };
    if opened == 0 {
        return Err(last_windows_error("OpenProcessToken"));
    }
    let token = TokenHandle(token);
    token_user_sid(token.0)
}

/// Restricts a directory DACL to its current owner.
///
/// # Errors
///
/// Returns an error when the path is not owned by the current user or its DACL cannot be changed.
pub fn protect_private_directory(path: &Path) -> Result<(), PlatformError> {
    let expected_sid = current_user_sid()?;
    verify_path_owner(path, &expected_sid)?;
    let sddl = format!("D:P(A;OICI;FA;;;{expected_sid})");
    set_path_dacl(path, &sddl)
}

/// Restricts a file DACL to its current owner.
///
/// # Errors
///
/// Returns an error when the path is not owned by the current user or its DACL cannot be changed.
pub fn protect_private_file(path: &Path) -> Result<(), PlatformError> {
    let expected_sid = current_user_sid()?;
    verify_path_owner(path, &expected_sid)?;
    let sddl = format!("D:P(A;;FA;;;{expected_sid})");
    set_path_dacl(path, &sddl)
}

/// Verifies that the named-pipe client token belongs to the expected owner SID.
///
/// # Errors
///
/// Returns an error when client impersonation, token inspection, SID comparison, or reversion
/// fails.
pub fn verify_named_pipe_owner(
    pipe: &impl AsRawHandle,
    expected_sid: &str,
) -> Result<(), PlatformError> {
    let pipe_handle: HANDLE = pipe.as_raw_handle().cast();
    if pipe_handle.is_null() {
        return Err(PlatformError::Windows(
            "named pipe handle is missing".to_owned(),
        ));
    }
    #[cfg(test)]
    fail_pipe_check_for_test("ImpersonateNamedPipeClient")?;
    // SAFETY: The caller passes a connected named pipe handle.
    if unsafe { ImpersonateNamedPipeClient(pipe_handle) } == 0 {
        return Err(last_windows_error("ImpersonateNamedPipeClient"));
    }
    let mut guard = ImpersonationGuard { active: true };
    let verification = verify_impersonated_owner(expected_sid);
    let reverted = guard.revert();
    reverted?;
    verification
}

fn verify_impersonated_owner(expected_sid: &str) -> Result<(), PlatformError> {
    #[cfg(test)]
    fail_pipe_check_for_test("OpenThreadToken")?;
    let mut token = null_mut();
    // SAFETY: The current thread is impersonating the pipe client at this point.
    let opened = unsafe { OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, 1, &raw mut token) };
    if opened == 0 {
        return Err(last_windows_error("OpenThreadToken"));
    }
    let token = TokenHandle(token);
    let actual_sid = token_user_sid(token.0)?;
    if !actual_sid.eq_ignore_ascii_case(expected_sid) {
        return Err(PlatformError::Windows(
            "named pipe client does not match the node owner".to_owned(),
        ));
    }
    Ok(())
}

/// Creates a local-only named pipe with an owner-only DACL.
///
/// # Errors
///
/// Returns an error when the owner does not match the current user, the security descriptor
/// cannot be built, or Windows cannot create the pipe instance.
pub fn create_private_pipe(
    pipe_name: &OsStr,
    owner_sid: &str,
    first_instance: bool,
) -> io::Result<tokio::net::windows::named_pipe::NamedPipeServer> {
    let current_sid = current_user_sid().map_err(|error| io::Error::other(error.to_string()))?;
    if !current_sid.eq_ignore_ascii_case(owner_sid) {
        return Err(invalid_input(
            "named pipe owner SID must match the current Windows user",
        ));
    }
    let sddl = format!("D:P(A;;GA;;;{owner_sid})");
    let descriptor =
        security_descriptor(&sddl).map_err(|error| io::Error::other(error.to_string()))?;
    let mut attributes = SECURITY_ATTRIBUTES {
        nLength: u32::try_from(size_of::<SECURITY_ATTRIBUTES>())
            .map_err(|_| invalid_input("security attributes are too large"))?,
        lpSecurityDescriptor: descriptor.pointer,
        bInheritHandle: 0,
    };
    let mut options = ServerOptions::new();
    options
        .first_pipe_instance(first_instance)
        .reject_remote_clients(true)
        // Handler concurrency is limited by IPC's semaphore. Windows can retain a
        // disconnected instance while its client handle remains open.
        .max_instances(254);
    // SAFETY: The descriptor and attributes remain alive for the duration of this call.
    unsafe {
        options
            .create_with_security_attributes_raw(pipe_name, (&raw mut attributes).cast::<c_void>())
    }
}

fn token_user_sid(token: HANDLE) -> Result<String, PlatformError> {
    #[cfg(test)]
    fail_pipe_check_for_test("GetTokenInformation(size)")?;
    let mut required = 0_u32;
    // SAFETY: A null buffer with zero length is the documented size query.
    unsafe {
        GetTokenInformation(token, TokenUser, null_mut(), 0, &raw mut required);
    }
    if required == 0 {
        return Err(last_windows_error("GetTokenInformation(size)"));
    }

    #[cfg(test)]
    fail_pipe_check_for_test("GetTokenInformation(TokenUser)")?;
    let words = (required as usize).div_ceil(size_of::<usize>());
    let mut buffer = vec![0_usize; words];
    // SAFETY: The buffer is pointer-aligned and at least `required` bytes long.
    let result = unsafe {
        GetTokenInformation(
            token,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            required,
            &raw mut required,
        )
    };
    if result == 0 {
        return Err(last_windows_error("GetTokenInformation(TokenUser)"));
    }
    // SAFETY: Windows returned a TOKEN_USER at the start of the aligned buffer.
    let token_user = unsafe { &*(buffer.as_ptr().cast::<TOKEN_USER>()) };
    sid_to_string(token_user.User.Sid)
}

fn sid_to_string(sid: PSID) -> Result<String, PlatformError> {
    let mut raw = null_mut();
    // SAFETY: `sid` comes from a valid TOKEN_USER or security descriptor owner.
    if unsafe { ConvertSidToStringSidW(sid, &raw mut raw) } == 0 {
        return Err(last_windows_error("ConvertSidToStringSidW"));
    }
    let memory = LocalMemory::new(raw.cast());
    // SAFETY: ConvertSidToStringSidW returns a null-terminated UTF-16 string.
    let length = unsafe {
        let mut length = 0;
        while *raw.add(length) != 0 {
            length += 1;
        }
        length
    };
    // SAFETY: The range was measured above within the returned allocation.
    let units = unsafe { std::slice::from_raw_parts(raw, length) };
    let value = std::ffi::OsString::from_wide(units)
        .into_string()
        .map_err(|_| PlatformError::Windows("owner SID is not valid Unicode".to_owned()))?;
    drop(memory);
    Ok(value)
}

fn verify_path_owner(path: &Path, expected_sid: &str) -> Result<(), PlatformError> {
    let wide = wide_null(path.as_os_str());
    let mut owner: PSID = null_mut();
    let mut descriptor: PSECURITY_DESCRIPTOR = null_mut();
    let security_information: OBJECT_SECURITY_INFORMATION = OWNER_SECURITY_INFORMATION;
    // SAFETY: Pointers are valid out parameters; the returned descriptor is freed on success.
    let status = unsafe {
        GetNamedSecurityInfoW(
            wide.as_ptr(),
            SE_FILE_OBJECT,
            security_information,
            &raw mut owner,
            null_mut(),
            null_mut(),
            null_mut(),
            &raw mut descriptor,
        )
    };
    if status != ERROR_SUCCESS {
        return Err(windows_status("GetNamedSecurityInfoW", status));
    }
    let memory = LocalMemory::new(descriptor);
    let actual_sid = sid_to_string(owner)?;
    drop(memory);
    if !actual_sid.eq_ignore_ascii_case(expected_sid) {
        return Err(PlatformError::Windows(
            "data path is not owned by the current Windows user".to_owned(),
        ));
    }
    Ok(())
}

fn set_path_dacl(path: &Path, sddl: &str) -> Result<(), PlatformError> {
    let descriptor = security_descriptor(sddl)?;
    let mut dacl = null_mut();
    let mut present = 0;
    let mut defaulted = 0;
    // SAFETY: The descriptor was just created from a valid SDDL string.
    if unsafe {
        GetSecurityDescriptorDacl(
            descriptor.pointer,
            &raw mut present,
            &raw mut dacl,
            &raw mut defaulted,
        )
    } == 0
    {
        return Err(last_windows_error("GetSecurityDescriptorDacl"));
    }
    if present == 0 || dacl.is_null() {
        return Err(PlatformError::Windows(
            "generated security descriptor has no DACL".to_owned(),
        ));
    }

    let wide = wide_null(path.as_os_str());
    let security_information = DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION;
    // SAFETY: The DACL and path remain valid for the duration of the call.
    let status = unsafe {
        SetNamedSecurityInfoW(
            wide.as_ptr(),
            SE_FILE_OBJECT,
            security_information,
            null_mut(),
            null_mut(),
            dacl,
            null_mut(),
        )
    };
    if status != ERROR_SUCCESS {
        return Err(windows_status("SetNamedSecurityInfoW", status));
    }
    Ok(())
}

fn security_descriptor(sddl: &str) -> Result<LocalMemory, PlatformError> {
    let wide = wide_null(OsStr::new(sddl));
    let mut descriptor: PSECURITY_DESCRIPTOR = null_mut();
    let mut size = 0;
    // SAFETY: Inputs are valid and the returned descriptor is owned by LocalMemory.
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            wide.as_ptr(),
            SDDL_REVISION_1,
            &raw mut descriptor,
            &raw mut size,
        )
    } == 0
    {
        return Err(last_windows_error(
            "ConvertStringSecurityDescriptorToSecurityDescriptorW",
        ));
    }
    Ok(LocalMemory::new(descriptor))
}

fn last_windows_error(operation: &str) -> PlatformError {
    let error = io::Error::last_os_error();
    PlatformError::Windows(format!("{operation} failed: {error}"))
}

fn last_protection_error(operation: &str) -> PlatformError {
    let error = io::Error::last_os_error();
    PlatformError::Protection(format!("{operation} failed: {error}"))
}

fn windows_status(operation: &str, status: u32) -> PlatformError {
    let error = io::Error::from_raw_os_error(status.cast_signed());
    PlatformError::Windows(format!("{operation} failed: {error}"))
}

fn invalid_input(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

fn wide_null(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use std::{
        ffi::c_void,
        fs::File,
        os::windows::io::AsRawHandle,
        path::PathBuf,
        ptr::null_mut,
        time::{SystemTime, UNIX_EPOCH},
    };

    use tokio::net::windows::named_pipe::ClientOptions;
    use windows_sys::Win32::{
        Foundation::{ERROR_NO_TOKEN, ERROR_SUCCESS, HANDLE},
        Security::Authorization::{
            GetNamedSecurityInfoW, GetSecurityInfo, SE_FILE_OBJECT, SE_KERNEL_OBJECT,
        },
        Security::{
            ACCESS_ALLOWED_ACE, ACL, ACL_SIZE_INFORMATION, AclSizeInformation,
            DACL_SECURITY_INFORMATION, GetAce, GetAclInformation, GetSecurityDescriptorDacl,
            PSECURITY_DESCRIPTOR, PSID,
        },
    };

    use super::*;
    use crate::platform::SecretProtector;

    struct TestDirectory {
        path: PathBuf,
    }

    impl TestDirectory {
        fn new() -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "acp-remote-platform-{}-{nonce}",
                std::process::id()
            ));
            std::fs::create_dir(&path).expect("test directory");
            Self { path }
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    fn descriptor_dacl_sids(descriptor: PSECURITY_DESCRIPTOR) -> Vec<String> {
        let memory = LocalMemory::new(descriptor);
        let mut dacl: *mut ACL = null_mut();
        let mut present = 0;
        let mut defaulted = 0;
        // SAFETY: The caller supplied a descriptor from a successful security API call.
        assert_ne!(
            unsafe {
                GetSecurityDescriptorDacl(
                    memory.pointer,
                    &raw mut present,
                    &raw mut dacl,
                    &raw mut defaulted,
                )
            },
            0,
            "GetSecurityDescriptorDacl"
        );
        assert_ne!(present, 0, "DACL is absent");
        assert!(!dacl.is_null(), "DACL pointer is null");

        let mut information = ACL_SIZE_INFORMATION::default();
        // SAFETY: `dacl` points into the live descriptor.
        assert_ne!(
            unsafe {
                GetAclInformation(
                    dacl,
                    (&raw mut information).cast(),
                    u32::try_from(size_of::<ACL_SIZE_INFORMATION>()).expect("ACL size fits"),
                    AclSizeInformation,
                )
            },
            0,
            "GetAclInformation"
        );

        let mut sids = Vec::with_capacity(information.AceCount as usize);
        for index in 0..information.AceCount {
            let mut ace: *mut c_void = null_mut();
            // SAFETY: The index is bounded by AceCount and `dacl` remains live.
            assert_ne!(unsafe { GetAce(dacl, index, &raw mut ace) }, 0, "GetAce");
            // SAFETY: Generated ACLs only contain ACCESS_ALLOWED_ACE entries.
            let allowed = ace.cast::<ACCESS_ALLOWED_ACE>();
            // SAFETY: `GetAce` returned a valid ACE for the live DACL.
            let sid: PSID = unsafe { std::ptr::addr_of_mut!((*allowed).SidStart) }.cast();
            sids.push(sid_to_string(sid).expect("ACE SID"));
        }
        sids
    }

    fn path_dacl_sids(path: &Path) -> Vec<String> {
        let wide = wide_null(path.as_os_str());
        let mut owner: PSID = null_mut();
        let mut descriptor: PSECURITY_DESCRIPTOR = null_mut();
        // SAFETY: All outputs are valid and the returned descriptor is freed by LocalMemory.
        let status = unsafe {
            GetNamedSecurityInfoW(
                wide.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                &raw mut owner,
                null_mut(),
                null_mut(),
                null_mut(),
                &raw mut descriptor,
            )
        };
        assert_eq!(status, ERROR_SUCCESS, "GetNamedSecurityInfoW");
        descriptor_dacl_sids(descriptor)
    }

    fn handle_dacl_sids(handle: HANDLE) -> Vec<String> {
        let mut descriptor: PSECURITY_DESCRIPTOR = null_mut();
        // SAFETY: The handle refers to a live named pipe kernel object.
        let status = unsafe {
            GetSecurityInfo(
                handle,
                SE_KERNEL_OBJECT,
                DACL_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                null_mut(),
                null_mut(),
                &raw mut descriptor,
            )
        };
        assert_eq!(status, ERROR_SUCCESS, "GetSecurityInfo");
        descriptor_dacl_sids(descriptor)
    }

    fn assert_thread_is_not_impersonating() {
        let mut token = null_mut();
        // SAFETY: This queries the current thread and closes any unexpected token below.
        let opened = unsafe { OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, 1, &raw mut token) };
        if opened != 0 {
            // SAFETY: The successful query returned an owned token handle.
            unsafe {
                CloseHandle(token);
            }
            panic!("thread still has an impersonation token");
        }
        assert_eq!(
            io::Error::last_os_error().raw_os_error(),
            Some(ERROR_NO_TOKEN.cast_signed())
        );
    }

    #[test]
    fn dpapi_round_trip_uses_current_user_scope() {
        let protector = DpapiProtector;
        let encrypted = protector
            .protect(b"private node material", b"test-context")
            .expect("protect");
        let decrypted = protector
            .unprotect(&encrypted, b"test-context")
            .expect("unprotect");
        assert_eq!(decrypted, b"private node material");
        assert!(protector.unprotect(&encrypted, b"wrong-context").is_err());
    }

    #[test]
    fn private_paths_allow_only_the_current_owner() {
        let directory = TestDirectory::new();
        let sid = current_user_sid().expect("current SID");
        protect_private_directory(&directory.path).expect("protect directory");
        verify_path_owner(&directory.path, &sid).expect("owner remains current user");
        assert_eq!(path_dacl_sids(&directory.path), vec![sid.clone()]);

        let file = directory.path.join("identity.key");
        File::create(&file).expect("create file");
        protect_private_file(&file).expect("protect file");
        assert_eq!(path_dacl_sids(&file), vec![sid]);
    }

    #[tokio::test]
    async fn named_pipe_check_failures_reject_and_restore_the_current_thread() {
        let sid = current_user_sid().expect("current SID");
        let pipe_name = format!(
            r"\\.\pipe\acp-remote-failure-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        );
        let server = create_private_pipe(OsStr::new(&pipe_name), &sid, true).expect("owned pipe");
        let client = ClientOptions::new().open(&pipe_name).expect("connect");
        server.connect().await.expect("accept");
        for operation in [
            "ImpersonateNamedPipeClient",
            "OpenThreadToken",
            "GetTokenInformation(size)",
            "GetTokenInformation(TokenUser)",
            "RevertToSelf",
        ] {
            // No await between installation and verification: the fault is thread-local.
            PIPE_CHECK_FAILURE.with(|point| point.set(Some(operation)));
            let error = verify_named_pipe_owner(&server, &sid).expect_err(operation);
            assert!(error.to_string().contains(operation), "{error}");
            PIPE_CHECK_FAILURE.with(|point| assert_eq!(point.get(), None, "fault hit"));
            assert_thread_is_not_impersonating();
            verify_named_pipe_owner(&server, &sid).expect("subsequent valid verification");
            assert_thread_is_not_impersonating();
        }
        drop(client);
        drop(server);
    }

    #[tokio::test]
    async fn named_pipe_enforces_owner_dacl_sid_and_first_instance() {
        let sid = current_user_sid().expect("current SID");
        let pipe_name = format!(
            r"\\.\pipe\acp-remote-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        );
        let server =
            create_private_pipe(OsStr::new(&pipe_name), &sid, true).expect("first pipe instance");
        assert_eq!(
            handle_dacl_sids(server.as_raw_handle().cast()),
            vec![sid.clone()]
        );
        assert!(create_private_pipe(OsStr::new(&pipe_name), &sid, true).is_err());

        let name_for_client = pipe_name.clone();
        let client = tokio::spawn(async move {
            ClientOptions::new()
                .open(name_for_client)
                .expect("connect named pipe")
        });
        server.connect().await.expect("accept named pipe");
        verify_named_pipe_owner(&server, &sid).expect("same owner");
        assert_thread_is_not_impersonating();
        assert!(
            verify_named_pipe_owner(&server, "S-1-5-21-0-0-0-1000").is_err(),
            "a different token SID must be rejected"
        );
        assert_thread_is_not_impersonating();
        drop(client.await.expect("client task"));
    }
}
