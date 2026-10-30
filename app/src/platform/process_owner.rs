//! Windows process ownership. A Job contains descendants before any child code runs.
//!
//! The instance owner first requests protocol closure and cancels its stdio tasks.
//! Task joins share the total deadline: if IO stalls, call `shutdown` to kill the
//! Job and unblock pipes before joining. Never require a successful join before kill.
//! Taking IO never transfers the Job; root exit alone never proves tree exit.
use std::{
    ffi::{OsStr, OsString},
    mem::{size_of, zeroed},
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::PathBuf,
    ptr::{null, null_mut},
    time::Duration,
};
use tokio::{net::windows::named_pipe::NamedPipeServer, time::Instant};
use windows_sys::Win32::{
    Foundation::{HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT},
    Globalization::{CSTR_EQUAL, CompareStringOrdinal},
    Security::SECURITY_ATTRIBUTES,
    Storage::FileSystem::{CreateFileW, FILE_GENERIC_READ, FILE_GENERIC_WRITE, OPEN_EXISTING},
    System::{
        JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JobObjectBasicAccountingInformation, JobObjectExtendedLimitInformation,
            QueryInformationJobObject, SetInformationJobObject, TerminateJobObject,
        },
        Threading::{
            CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT, CreateProcessW,
            DeleteProcThreadAttributeList, EXTENDED_STARTUPINFO_PRESENT, GetExitCodeProcess,
            InitializeProcThreadAttributeList, PROC_THREAD_ATTRIBUTE_HANDLE_LIST,
            PROCESS_INFORMATION, ResumeThread, STARTF_USESTDHANDLES, STARTUPINFOEXW,
            TerminateProcess, UpdateProcThreadAttribute, WaitForSingleObject,
        },
    },
};

/// Launch inputs may contain secrets; intentionally no Debug implementation.
pub struct ProcessSpec {
    pub executable: PathBuf,
    pub args: Vec<OsString>,
    pub cwd: PathBuf,
    /// Overrides inherited environment; names are Windows case-insensitive.
    pub env: Vec<(OsString, OsString)>,
}

/// Overlapped parent handles. Drop/abort IO tasks to cancel pending operations.
/// Child handles are synchronous standard handles, inherited via an explicit allowlist.
pub struct ProcessIo {
    pub stdin: NamedPipeServer,
    pub stdout: NamedPipeServer,
    pub stderr: NamedPipeServer,
}

#[derive(Debug, thiserror::Error)]
pub enum ProcessOwnerError {
    #[error("process IO was already taken")]
    AlreadyTaken,
    #[error("process spawn deadline elapsed")]
    SpawnTimeout,
    #[error("invalid process specification: {0}")]
    InvalidSpec(&'static str),
    #[error("process owner operation {operation} failed (Windows {code})")]
    Windows { operation: &'static str, code: i32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownDisposition {
    Graceful,
    Forced,
    Incomplete,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShutdownReport {
    pub disposition: ShutdownDisposition,
    /// Last observed Job count, including descendants after root exit.
    pub active_processes: u32,
    pub root_exit_code: Option<u32>,
}

pub struct OwnedProcess {
    job: OwnedHandle,
    process: OwnedHandle,
    pid: u32,
    io: Option<ProcessIo>,
    forced: bool,
}

impl OwnedProcess {
    /// Starts suspended, assigns the non-inheritable kill-on-close Job, then resumes.
    /// No shell is involved. Requires a Tokio IO runtime.
    ///
    /// # Errors
    /// Fails closed on invalid input, pipe setup, process creation or Job assignment.
    pub async fn spawn(spec: ProcessSpec, deadline: Instant) -> Result<Self, ProcessOwnerError> {
        if Instant::now() >= deadline {
            return Err(ProcessOwnerError::SpawnTimeout);
        }
        tokio::time::timeout_at(deadline, Self::spawn_inner(spec, deadline))
            .await
            .map_err(|_| ProcessOwnerError::SpawnTimeout)?
    }
    async fn spawn_inner(spec: ProcessSpec, deadline: Instant) -> Result<Self, ProcessOwnerError> {
        if !spec.executable.is_absolute() || !spec.cwd.is_absolute() {
            return Err(ProcessOwnerError::InvalidSpec(
                "absolute executable and cwd required",
            ));
        }
        let executable = wide_null(spec.executable.as_os_str())?;
        let cwd = wide_null(spec.cwd.as_os_str())?;
        let mut command = command_line(spec.executable.as_os_str(), &spec.args)?;
        let environment = environment(&spec.env)?;
        let (stdin, child_in) = pipe_pair().await?;
        let (stdout, child_out) = pipe_pair().await?;
        let (stderr, child_err) = pipe_pair().await?;
        // SAFETY: Null security attributes prohibit inheritance; null name creates a private Job.
        let job = own(
            unsafe { CreateJobObjectW(null(), null()) },
            "CreateJobObjectW",
        )?;
        // SAFETY: Zero is the Win32 default for all unconfigured Job limits.
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        // SAFETY: Properly sized live limits; Job handle is uniquely owned.
        if unsafe {
            SetInformationJobObject(
                raw(&job),
                JobObjectExtendedLimitInformation,
                (&raw const limits).cast(),
                win_size::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>()?,
            )
        } == 0
        {
            return Err(last_error("SetInformationJobObject"));
        }
        let mut handles = [raw(&child_in), raw(&child_out), raw(&child_err)];
        let mut attributes = Attributes::new(&mut handles)?;
        // SAFETY: Zeroed STARTUPINFOEX has all optional pointers null.
        let mut startup: STARTUPINFOEXW = unsafe { zeroed() };
        startup.StartupInfo.cb = win_size::<STARTUPINFOEXW>()?;
        startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
        startup.StartupInfo.hStdInput = handles[0];
        startup.StartupInfo.hStdOutput = handles[1];
        startup.StartupInfo.hStdError = handles[2];
        startup.lpAttributeList = attributes.pointer();
        // SAFETY: PROCESS_INFORMATION is an output record.
        let mut info: PROCESS_INFORMATION = unsafe { zeroed() };
        // SAFETY: All strings, allowlist and structures remain valid. Only listed stdio
        // handles can be inherited; the Job and parent overlapped handles cannot escape.
        if Instant::now() >= deadline {
            return Err(ProcessOwnerError::SpawnTimeout);
        }
        let created = unsafe {
            CreateProcessW(
                executable.as_ptr(),
                command.as_mut_ptr(),
                null(),
                null(),
                1,
                CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT | EXTENDED_STARTUPINFO_PRESENT,
                environment.as_ptr().cast(),
                cwd.as_ptr(),
                &raw const startup.StartupInfo,
                &raw mut info,
            )
        };
        if created == 0 {
            return Err(last_error("CreateProcessW"));
        }
        let process = own(info.hProcess, "process handle")?;
        let mut suspended = SuspendedProcess {
            process: Some(process),
        };
        let thread = own(info.hThread, "thread handle")?;
        let process_ref = suspended
            .process
            .as_ref()
            .ok_or(ProcessOwnerError::InvalidSpec("missing process handle"))?;
        // SAFETY: Child is suspended and cannot create descendants before this succeeds.
        if unsafe { AssignProcessToJobObject(raw(&job), raw(process_ref)) } == 0 {
            return Err(last_error("AssignProcessToJobObject"));
        }
        if Instant::now() >= deadline {
            return Err(ProcessOwnerError::SpawnTimeout);
        }
        // SAFETY: This is the suspended child's primary thread, still uniquely owned.
        if unsafe { ResumeThread(raw(&thread)) } == u32::MAX {
            return Err(last_error("ResumeThread"));
        }
        let process = suspended
            .process
            .take()
            .ok_or(ProcessOwnerError::InvalidSpec("missing process handle"))?;
        Ok(Self {
            job,
            process,
            pid: info.dwProcessId,
            io: Some(ProcessIo {
                stdin,
                stdout,
                stderr,
            }),
            forced: false,
        })
    }

    #[must_use]
    pub const fn id(&self) -> u32 {
        self.pid
    }
    /// Takes standard IO exactly once.
    /// # Errors
    /// Returns an already-taken error after the first take or shutdown.
    pub fn take_io(&mut self) -> Result<ProcessIo, ProcessOwnerError> {
        self.io.take().ok_or(ProcessOwnerError::AlreadyTaken)
    }

    /// Waits only for the root. Cancellation leaves process and Job owned.
    /// # Errors
    /// Returns a sanitized Windows error if querying the root fails.
    pub async fn wait_exit(&mut self) -> Result<u32, ProcessOwnerError> {
        loop {
            if let Some(code) = self.root_exit_code()? {
                return Ok(code);
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    /// Waits for the complete Job, reserving up to 100ms of the total budget for
    /// forced termination and exit observation. Never resets the caller deadline.
    /// An incomplete result retains Job ownership; later calls retry observation/termination.
    /// This cannot close IO already taken by the instance owner; Job termination unblocks
    /// those pipes even if a protocol writer/reader is stalled.
    /// # Errors
    /// Returns sanitized Windows errors; ownership remains intact on error/cancellation.
    pub async fn shutdown(
        &mut self,
        deadline: Instant,
    ) -> Result<ShutdownReport, ProcessOwnerError> {
        // Close any untaken IO now, including stdin, to allow ordinary EOF shutdown.
        self.io.take();
        let now = Instant::now();
        let grace_deadline = deadline
            .checked_sub(Duration::from_millis(100))
            .unwrap_or(now)
            .max(now);
        loop {
            let active = self.active_processes()?;
            if active == 0 {
                return self.report(active);
            }
            if Instant::now() >= grace_deadline && !self.forced {
                self.forced = true;
                // SAFETY: Only this owned Job and its non-breakaway descendants are affected.
                if unsafe { TerminateJobObject(raw(&self.job), 1) } == 0 {
                    self.forced = false;
                    return Err(last_error("TerminateJobObject"));
                }
            }
            if Instant::now() >= deadline {
                return self.report(self.active_processes()?);
            }
            let next = if self.forced {
                deadline
            } else {
                grace_deadline
            };
            tokio::time::sleep_until(next.min(Instant::now() + Duration::from_millis(10))).await;
        }
    }

    fn report(&self, active: u32) -> Result<ShutdownReport, ProcessOwnerError> {
        Ok(ShutdownReport {
            disposition: if active != 0 {
                ShutdownDisposition::Incomplete
            } else if self.forced {
                ShutdownDisposition::Forced
            } else {
                ShutdownDisposition::Graceful
            },
            active_processes: active,
            root_exit_code: self.root_exit_code()?,
        })
    }
    fn active_processes(&self) -> Result<u32, ProcessOwnerError> {
        // SAFETY: A zeroed accounting record is a valid output buffer.
        let mut accounting: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = unsafe { zeroed() };
        // SAFETY: Buffer matches the requested information class and stays live.
        if unsafe {
            QueryInformationJobObject(
                raw(&self.job),
                JobObjectBasicAccountingInformation,
                (&raw mut accounting).cast(),
                win_size::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>()?,
                null_mut(),
            )
        } == 0
        {
            return Err(last_error("QueryInformationJobObject"));
        }
        Ok(accounting.ActiveProcesses)
    }
    fn root_exit_code(&self) -> Result<Option<u32>, ProcessOwnerError> {
        // SAFETY: A zero timeout probes the owned process without blocking any runtime thread.
        match unsafe { WaitForSingleObject(raw(&self.process), 0) } {
            WAIT_TIMEOUT => Ok(None),
            WAIT_OBJECT_0 => {
                let mut code = 0;
                // SAFETY: Owned process handle and valid writable exit-code buffer.
                if unsafe { GetExitCodeProcess(raw(&self.process), &raw mut code) } == 0 {
                    return Err(last_error("GetExitCodeProcess"));
                }
                Ok(Some(code))
            }
            _ => Err(last_error("WaitForSingleObject")),
        }
    }
}
impl Drop for OwnedProcess {
    fn drop(&mut self) {
        // SAFETY: Best effort tree termination before final kill-on-close; no task/thread is spawned.
        unsafe {
            TerminateJobObject(raw(&self.job), 1);
        }
    }
}
struct SuspendedProcess {
    process: Option<OwnedHandle>,
}
impl Drop for SuspendedProcess {
    fn drop(&mut self) {
        if let Some(process) = &self.process {
            // SAFETY: Fail-closed rollback of our own still-suspended or just-resumed child.
            unsafe {
                TerminateProcess(raw(process), 1);
            }
        }
    }
}
struct Attributes {
    storage: Vec<usize>,
    initialized: bool,
}
impl Attributes {
    fn new(handles: &mut [HANDLE; 3]) -> Result<Self, ProcessOwnerError> {
        let mut bytes = 0;
        // SAFETY: Documented first call measures the allocation; no list exists yet.
        unsafe {
            InitializeProcThreadAttributeList(null_mut(), 1, 0, &raw mut bytes);
        }
        if bytes == 0 {
            return Err(last_error("InitializeProcThreadAttributeList(size)"));
        }
        let mut result = Self {
            storage: vec![0; bytes.div_ceil(size_of::<usize>())],
            initialized: false,
        };
        // SAFETY: Pointer-aligned allocation has at least the requested number of bytes.
        if unsafe { InitializeProcThreadAttributeList(result.pointer(), 1, 0, &raw mut bytes) } == 0
        {
            // No initialized list to delete on error.

            return Err(last_error("InitializeProcThreadAttributeList"));
        }
        result.initialized = true;
        // SAFETY: Three inheritable handles remain live until CreateProcess completes.
        if unsafe {
            UpdateProcThreadAttribute(
                result.pointer(),
                0,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                handles.as_mut_ptr().cast(),
                size_of::<[HANDLE; 3]>(),
                null_mut(),
                null(),
            )
        } == 0
        {
            return Err(last_error("UpdateProcThreadAttribute"));
        }
        Ok(result)
    }
    const fn pointer(
        &mut self,
    ) -> windows_sys::Win32::System::Threading::LPPROC_THREAD_ATTRIBUTE_LIST {
        self.storage.as_mut_ptr().cast()
    }
}
impl Drop for Attributes {
    fn drop(&mut self) {
        // SAFETY: This allocation contains exactly one initialized attribute list.
        if self.initialized {
            unsafe {
                DeleteProcThreadAttributeList(self.pointer());
            }
        }
    }
}
async fn pipe_pair() -> Result<(NamedPipeServer, OwnedHandle), ProcessOwnerError> {
    let mut nonce = [0_u8; 24];
    getrandom::fill(&mut nonce).map_err(|_| ProcessOwnerError::Windows {
        operation: "pipe random",
        code: 0,
    })?;
    let name = OsString::from(format!(r"\\.\pipe\acp-process-{}", hex::encode(nonce)));
    let sid = super::current_user_sid().map_err(|_| ProcessOwnerError::Windows {
        operation: "pipe owner",
        code: 0,
    })?;
    let server = super::create_private_pipe(&name, &sid, true)
        .map_err(|error| io_error("create stdio pipe", &error))?;
    let path = wide_null(&name)?;
    let client = {
        let attributes = SECURITY_ATTRIBUTES {
            nLength: win_size::<SECURITY_ATTRIBUTES>()?,
            lpSecurityDescriptor: null_mut(),
            bInheritHandle: 1,
        };
        // SAFETY: Open our freshly created random, owner-only local pipe. Child end is
        // deliberately synchronous for standard-library/CLI compatibility; parent is overlapped.
        own(
            unsafe {
                CreateFileW(
                    path.as_ptr(),
                    FILE_GENERIC_READ | FILE_GENERIC_WRITE,
                    0,
                    &raw const attributes,
                    OPEN_EXISTING,
                    0,
                    null_mut(),
                )
            },
            "open stdio pipe",
        )?
    };
    server
        .connect()
        .await
        .map_err(|error| io_error("connect stdio pipe", &error))?;
    Ok((server, client))
}
fn own(handle: HANDLE, operation: &'static str) -> Result<OwnedHandle, ProcessOwnerError> {
    if handle.is_null() || handle == windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE {
        return Err(last_error(operation));
    }
    // SAFETY: Only successful newly owned Windows handles enter this helper.
    Ok(unsafe { OwnedHandle::from_raw_handle(handle) })
}
fn raw(handle: &OwnedHandle) -> HANDLE {
    handle.as_raw_handle()
}
fn last_error(operation: &'static str) -> ProcessOwnerError {
    io_error(operation, &std::io::Error::last_os_error())
}
fn io_error(operation: &'static str, error: &std::io::Error) -> ProcessOwnerError {
    ProcessOwnerError::Windows {
        operation,
        code: error.raw_os_error().unwrap_or(0),
    }
}
fn wide_null(value: &OsStr) -> Result<Vec<u16>, ProcessOwnerError> {
    let mut wide: Vec<u16> = value.encode_wide().collect();
    if wide.contains(&0) {
        return Err(ProcessOwnerError::InvalidSpec("embedded NUL"));
    }
    wide.push(0);
    Ok(wide)
}
fn command_line(executable: &OsStr, args: &[OsString]) -> Result<Vec<u16>, ProcessOwnerError> {
    let mut result = Vec::new();
    for argument in std::iter::once(executable).chain(args.iter().map(OsString::as_os_str)) {
        if !result.is_empty() {
            result.push(u16::from(b' '));
        }
        let wide = wide_null(argument)?;
        result.push(u16::from(b'"'));
        let mut slashes = 0;
        for &unit in &wide[..wide.len() - 1] {
            if unit == u16::from(b'\\') {
                slashes += 1;
                continue;
            }
            if unit == u16::from(b'"') {
                result.extend(std::iter::repeat_n(u16::from(b'\\'), slashes * 2 + 1));
            } else {
                result.extend(std::iter::repeat_n(u16::from(b'\\'), slashes));
            }
            slashes = 0;
            result.push(unit);
        }
        result.extend(std::iter::repeat_n(u16::from(b'\\'), slashes * 2));
        result.push(u16::from(b'"'));
    }
    // CreateProcessW includes the terminating NUL in the 32767-unit limit.
    if result.len() >= 32767 {
        return Err(ProcessOwnerError::InvalidSpec("command line too long"));
    }
    result.push(0);
    Ok(result)
}
fn environment(overrides: &[(OsString, OsString)]) -> Result<Vec<u16>, ProcessOwnerError> {
    // Windows uses ordinal case-insensitive comparison, not Unicode uppercasing
    // with expansions or locale-dependent collation. Keep original UTF-16 names.
    let mut variables: Vec<(Vec<u16>, Vec<u16>)> = Vec::new();
    let inherited = std::env::vars_os().map(|(name, value)| (name, value, true));
    let explicit = overrides
        .iter()
        .cloned()
        .map(|(name, value)| (name, value, false));
    for (name, value, inherited) in inherited.chain(explicit) {
        let name = wide_null(&name)?;
        if name.len() <= 1
            || name.len() > 32767
            || (name[..name.len() - 1].contains(&u16::from(b'='))
                && !(inherited && inherited_drive_name(&name)))
        {
            return Err(ProcessOwnerError::InvalidSpec("invalid environment name"));
        }
        let value = wide_null(&value)?;
        let mut matching = None;
        for (index, (existing, _)) in variables.iter().enumerate() {
            if compare_names(&name, existing)? == CSTR_EQUAL {
                matching = Some(index);
                break;
            }
        }
        if let Some(index) = matching {
            variables[index] = (name, value);
        } else {
            variables.push((name, value));
        }
    }
    // Sort with the same checked Windows comparison; insertion sort avoids a fallible
    // comparator being silently replaced by a different equivalence relation.
    for index in 1..variables.len() {
        let mut cursor = index;
        while cursor > 0
            && compare_names(&variables[cursor - 1].0, &variables[cursor].0)? > CSTR_EQUAL
        {
            variables.swap(cursor - 1, cursor);
            cursor -= 1;
        }
    }
    let mut result = Vec::new();
    for (name, value) in variables {
        result.extend_from_slice(&name[..name.len() - 1]);
        result.push(u16::from(b'='));
        result.extend(value);
    }
    if result.is_empty() {
        result.push(0);
    }
    result.push(0);
    Ok(result)
}
// Windows cmd stores each drive's current directory under =<ASCII drive>:.
// Only entries inherited from the OS block may use this reserved name form.
const fn inherited_drive_name(name: &[u16]) -> bool {
    matches!(name, [61, 65..=90 | 97..=122, 58, 0])
}
fn compare_names(left: &[u16], right: &[u16]) -> Result<i32, ProcessOwnerError> {
    // SAFETY: Both slices contain live NUL-terminated UTF-16; -1 selects NUL termination.
    let result = unsafe { CompareStringOrdinal(left.as_ptr(), -1, right.as_ptr(), -1, 1) };
    if result == 0 {
        return Err(last_error("CompareStringOrdinal"));
    }
    Ok(result)
}
fn win_size<T>() -> Result<u32, ProcessOwnerError> {
    u32::try_from(size_of::<T>())
        .map_err(|_| ProcessOwnerError::InvalidSpec("Windows structure too large"))
}
