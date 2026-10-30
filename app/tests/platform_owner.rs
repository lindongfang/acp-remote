#![cfg(windows)]
use acp_remote::platform::process_owner::{
    OwnedProcess, ProcessOwnerError, ProcessSpec, ShutdownDisposition,
};
use std::{
    ffi::OsString,
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    path::PathBuf,
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    time::{Instant, timeout},
};
use windows_sys::Win32::{
    Foundation::{HANDLE, WAIT_OBJECT_0},
    System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject},
};

fn powershell() -> PathBuf {
    PathBuf::from(std::env::var_os("SystemRoot").unwrap())
        .join("System32/WindowsPowerShell/v1.0/powershell.exe")
}
fn spec(script: &str) -> ProcessSpec {
    ProcessSpec {
        executable: powershell(),
        args: vec![
            "-NoProfile".into(),
            "-NonInteractive".into(),
            "-Command".into(),
            script.into(),
        ],
        cwd: std::env::temp_dir(),
        env: vec![],
    }
}
async fn launch(script: &str) -> OwnedProcess {
    OwnedProcess::spawn(spec(script), Instant::now() + Duration::from_secs(10))
        .await
        .unwrap()
}
fn process_handle(pid: u32) -> OwnedHandle {
    // SAFETY: Opens exactly the PID just emitted by our dedicated owned test process.
    let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
    assert!(!handle.is_null(), "test child process must be observable");
    // SAFETY: OpenProcess transferred this owned handle.
    unsafe { OwnedHandle::from_raw_handle(handle) }
}
async fn assert_exited(handle: &OwnedHandle) {
    timeout(Duration::from_secs(5), async {
        // SAFETY: Owned synchronize handle, zero timeout never blocks the runtime.
        while unsafe { WaitForSingleObject(handle.as_raw_handle() as HANDLE, 0) } != WAIT_OBJECT_0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("owned descendant must exit");
}
async fn finish(owner: &mut OwnedProcess) {
    let report = owner
        .shutdown(Instant::now() + Duration::from_millis(150))
        .await
        .unwrap();
    if report.disposition == ShutdownDisposition::Incomplete {
        let report = owner
            .shutdown(Instant::now() + Duration::from_secs(5))
            .await
            .unwrap();
        assert_eq!(report.active_processes, 0);
    }
}
fn grandchild_script(parent_exits: bool) -> String {
    format!(
        "$p=Start-Process -FilePath '{}' -ArgumentList '-NoProfile','-NonInteractive','-Command','Start-Sleep -Seconds 120' -PassThru -NoNewWindow; [Console]::Out.WriteLine($p.Id); [Console]::Out.Flush(); {}",
        powershell().display(),
        if parent_exits {
            "exit 7"
        } else {
            "Start-Sleep -Seconds 120"
        }
    )
}

#[tokio::test]
async fn parent_exit_does_not_mean_tree_exit_and_shutdown_is_repeatable() {
    let mut owner = launch(&grandchild_script(true)).await;
    let io = owner.take_io().unwrap();
    assert!(matches!(
        owner.take_io(),
        Err(ProcessOwnerError::AlreadyTaken)
    ));
    let mut lines = BufReader::new(io.stdout).lines();
    let pid: u32 = timeout(Duration::from_secs(10), lines.next_line())
        .await
        .unwrap()
        .unwrap()
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    let descendant = process_handle(pid);
    assert_eq!(
        timeout(Duration::from_secs(10), owner.wait_exit())
            .await
            .unwrap()
            .unwrap(),
        7
    );
    // Root already exited but a system PowerShell descendant is still alive.
    assert_ne!(
        unsafe { WaitForSingleObject(descendant.as_raw_handle(), 0) },
        WAIT_OBJECT_0
    );
    finish(&mut owner).await;
    assert_exited(&descendant).await;
    let report = owner
        .shutdown(Instant::now() + Duration::from_secs(1))
        .await
        .unwrap();
    assert_eq!(report.disposition, ShutdownDisposition::Forced);
    assert_eq!(report.active_processes, 0);
    assert_eq!(report.root_exit_code, Some(7));
}

#[tokio::test]
async fn stalled_stdout_stderr_and_stdin_cannot_block_tree_shutdown() {
    let mut owner = launch("[Console]::Out.WriteLine($PID); [Console]::Out.Flush(); $s='x'*65536; while($true) { [Console]::Out.Write($s); [Console]::Error.Write($s) }").await;
    let io = owner.take_io().unwrap();
    let mut out = BufReader::new(io.stdout);
    let mut line = String::new();
    timeout(Duration::from_secs(10), out.read_line(&mut line))
        .await
        .unwrap()
        .unwrap();
    let handle = process_handle(line.trim().parse().unwrap());
    // Child does not read stdin; output is intentionally held without a drain task.
    let writer = tokio::spawn(async move {
        let mut stdin = io.stdin;
        let block = vec![b'z'; 65536];
        loop {
            if stdin.write_all(&block).await.is_err() {
                return;
            }
        }
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    finish(&mut owner).await;
    assert_exited(&handle).await;
    timeout(Duration::from_secs(5), writer)
        .await
        .unwrap()
        .unwrap();
    drop((out, io.stderr));
}

#[tokio::test]
async fn drop_kills_tree_with_taken_io_and_no_graceful_wait() {
    let mut owner = launch(&grandchild_script(false)).await;
    let io = owner.take_io().unwrap();
    let root = process_handle(owner.id());
    let mut reader = BufReader::new(io.stdout).lines();
    let pid: u32 = timeout(Duration::from_secs(10), reader.next_line())
        .await
        .unwrap()
        .unwrap()
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    let child = process_handle(pid);
    drop(owner);
    assert_exited(&root).await;
    assert_exited(&child).await;
    drop((reader, io.stdin, io.stderr));
}

#[tokio::test]
async fn stdin_eof_allows_graceful_exit_and_pending_read_is_cancellable() {
    let mut owner = launch("while ($null -ne [Console]::ReadLine()) {}; exit 9").await;
    let io = owner.take_io().unwrap();
    let mut stdout = io.stdout;
    let mut byte = [0];
    assert!(
        timeout(Duration::from_millis(50), stdout.read(&mut byte))
            .await
            .is_err()
    );
    drop((io.stdin, stdout, io.stderr));
    assert_eq!(
        timeout(Duration::from_secs(10), owner.wait_exit())
            .await
            .unwrap()
            .unwrap(),
        9
    );
    let report = owner
        .shutdown(Instant::now() + Duration::from_secs(1))
        .await
        .unwrap();
    assert_eq!(report.disposition, ShutdownDisposition::Graceful);
}

#[tokio::test]
async fn invalid_spec_and_expired_spawn_do_not_expose_secrets() {
    let mut input = spec("secret command");
    input.args.push(OsString::from("secret\0argument"));
    let Err(error) = OwnedProcess::spawn(input, Instant::now() + Duration::from_secs(1)).await
    else {
        panic!("invalid input accepted")
    };
    assert!(!format!("{error:?}").contains("secret"));
    assert!(matches!(error, ProcessOwnerError::InvalidSpec(_)));
    let mut input = spec("exit 0");
    input.env.push(("A=B".into(), "secret".into()));
    assert!(matches!(
        OwnedProcess::spawn(input, Instant::now() + Duration::from_secs(1)).await,
        Err(ProcessOwnerError::InvalidSpec(_))
    ));
    let input = spec("Start-Sleep -Seconds 120");
    // A past deadline must be rejected before starting any child, even when setup is ready.
    assert!(matches!(
        OwnedProcess::spawn(input, Instant::now() - Duration::from_secs(1)).await,
        Err(ProcessOwnerError::SpawnTimeout)
    ));
}

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let mut bytes = [0; 16];
        getrandom::fill(&mut bytes).unwrap();
        let path = std::env::temp_dir().join(format!("acp-owner-test-{}", hex::encode(bytes)));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn fixture_spec(mode: &str, path: &std::path::Path) -> ProcessSpec {
    ProcessSpec {
        executable: std::env::current_exe().unwrap(),
        args: vec![
            "--exact".into(),
            "fixture_process".into(),
            "--nocapture".into(),
        ],
        cwd: std::env::temp_dir(),
        env: vec![
            ("ACP_OWNER_FIXTURE".into(), mode.into()),
            ("ACP_OWNER_FIXTURE_PATH".into(), path.as_os_str().to_owned()),
        ],
    }
}

#[tokio::test]
async fn abnormal_owner_exit_closes_noninherited_job_inside_nested_job() {
    let scratch = Scratch::new();
    let path = scratch.0.join("child.pid");
    let mut outer = OwnedProcess::spawn(
        fixture_spec("crash", &path),
        Instant::now() + Duration::from_secs(10),
    )
    .await
    .unwrap();
    // The fixture itself is in outer's Job; its OwnedProcess must create a nested Job.
    timeout(Duration::from_secs(15), async {
        while !path.exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let pid: u32 = std::fs::read_to_string(&path).unwrap().parse().unwrap();
    let descendant = process_handle(pid);
    // Release fixture only after opening a stable process handle to avoid PID reuse.
    std::fs::write(scratch.0.join("exit"), b"go").unwrap();
    assert_eq!(
        timeout(Duration::from_secs(10), outer.wait_exit())
            .await
            .unwrap()
            .unwrap(),
        23
    );
    // outer's Job is still open: only fixture's noninherited inner Job closure can kill child.
    assert_exited(&descendant).await;
    let report = outer
        .shutdown(Instant::now() + Duration::from_secs(1))
        .await
        .unwrap();
    assert_eq!(report.disposition, ShutdownDisposition::Graceful);
}

#[tokio::test]
async fn argument_quoting_and_case_insensitive_environment_overrides_roundtrip() {
    let scratch = Scratch::new();
    let path = scratch.0.join("args.json");
    let mut input = fixture_spec("argv", &path);
    let arguments: Vec<OsString> = [
        "a b",
        "",
        "quote\"inside",
        "trailing slash \\",
        "slash\\\"quote",
        "中文路径",
    ]
    .into_iter()
    .map(Into::into)
    .collect();
    input.args.extend(arguments.clone());
    input.env.extend([
        ("ACP_OWNER_CASE".into(), "old".into()),
        ("acp_owner_case".into(), "secret-new".into()),
    ]);
    let mut owner = OwnedProcess::spawn(input, Instant::now() + Duration::from_secs(10))
        .await
        .unwrap();
    assert_eq!(
        timeout(Duration::from_secs(10), owner.wait_exit())
            .await
            .unwrap()
            .unwrap(),
        0
    );
    let value: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(value["env"], "secret-new");
    let actual: Vec<String> = serde_json::from_value(value["args"].clone()).unwrap();
    assert_eq!(
        &actual[3..],
        arguments
            .iter()
            .map(|v| v.to_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    );
    finish(&mut owner).await;
}

// Dedicated child fixture. With no selector it is an ordinary no-op test; the
// crash path deliberately bypasses Rust destructors to exercise OS Job ownership.
#[tokio::test]
async fn fixture_process() {
    let Ok(mode) = std::env::var("ACP_OWNER_FIXTURE") else {
        return;
    };
    let path = PathBuf::from(std::env::var_os("ACP_OWNER_FIXTURE_PATH").unwrap());
    if mode == "hidden-preserved" {
        let key = std::env::var_os("ACP_OWNER_EXPECTED_HIDDEN_KEY").unwrap();
        let expected = std::env::var_os("ACP_OWNER_EXPECTED_HIDDEN_VALUE").unwrap();
        assert!(
            std::env::var_os(key).is_some_and(|actual| actual == expected),
            "inherited hidden drive value was not preserved"
        );
        return;
    }
    if mode == "hidden-drive" || mode == "hidden-exit-code" {
        hidden_drive_fixture(&path, mode == "hidden-exit-code").await;
        return;
    }
    if mode == "escape" {
        use std::os::windows::process::CommandExt;
        use windows_sys::Win32::{
            Foundation::GetHandleInformation, System::Threading::CREATE_BREAKAWAY_FROM_JOB,
        };
        let inherited: usize = std::env::var("ACP_OWNER_SENTINEL")
            .unwrap()
            .parse()
            .unwrap();
        let mut flags = 0;
        // SAFETY: A numeric sentinel is probed without taking ownership/closing it.
        assert_eq!(
            unsafe { GetHandleInformation(inherited as HANDLE, &raw mut flags) },
            0,
            "unlisted inheritable handle escaped"
        );
        let attempted = std::process::Command::new(powershell())
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Start-Sleep -Seconds 120",
            ])
            .creation_flags(CREATE_BREAKAWAY_FROM_JOB)
            .spawn();
        if let Ok(mut escaped) = attempted {
            let _ = escaped.kill();
            let _ = escaped.wait();
            panic!("breakaway succeeded");
        }
        return;
    }
    if mode == "argv" {
        let value = serde_json::json!({"args":std::env::args().skip(1).collect::<Vec<_>>(),"env":std::env::var("ACP_OWNER_CASE").unwrap()});
        std::fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
        return;
    }
    assert_eq!(mode, "crash");
    let mut child =
        launch("[Console]::Out.WriteLine($PID); [Console]::Out.Flush(); Start-Sleep -Seconds 120")
            .await;
    let io = child.take_io().unwrap();
    let mut lines = BufReader::new(io.stdout).lines();
    let pid = lines.next_line().await.unwrap().unwrap();
    std::fs::write(&path, pid.trim()).unwrap();
    timeout(Duration::from_secs(15), async {
        while !path.with_file_name("exit").exists() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    std::process::exit(23);
}

#[tokio::test]
async fn unlisted_inheritable_handle_and_breakaway_are_rejected() {
    use windows_sys::Win32::{Security::SECURITY_ATTRIBUTES, System::Threading::CreateEventW};
    let scratch = Scratch::new();
    let attributes = SECURITY_ATTRIBUTES {
        nLength: u32::try_from(std::mem::size_of::<SECURITY_ATTRIBUTES>()).unwrap(),
        lpSecurityDescriptor: std::ptr::null_mut(),
        bInheritHandle: 1,
    };
    // SAFETY: A dedicated inheritable unnamed event tests the explicit child allowlist.
    let event = unsafe { CreateEventW(&raw const attributes, 1, 0, std::ptr::null()) };
    assert!(!event.is_null());
    // SAFETY: CreateEvent transferred this event handle.
    let event = unsafe { OwnedHandle::from_raw_handle(event) };
    let mut input = fixture_spec("escape", &scratch.0.join("unused"));
    input.env.push((
        "ACP_OWNER_SENTINEL".into(),
        format!("{}", event.as_raw_handle() as usize).into(),
    ));
    let mut owner = OwnedProcess::spawn(input, Instant::now() + Duration::from_secs(10))
        .await
        .unwrap();
    let io = owner.take_io().unwrap();
    let stderr = tokio::spawn(async move {
        let mut buffer = Vec::new();
        io.stderr
            .take(16384)
            .read_to_end(&mut buffer)
            .await
            .unwrap();
        buffer
    });
    let exit = timeout(Duration::from_secs(10), owner.wait_exit())
        .await
        .unwrap()
        .unwrap();
    finish(&mut owner).await;
    let detail = timeout(Duration::from_secs(5), stderr)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(exit, 0, "{}", String::from_utf8_lossy(&detail));
    drop((event, io.stdin, io.stdout));
}

#[tokio::test]
async fn cmd_hidden_drive_environment_is_inherited_without_rejecting_spawn() {
    cmd_environment_fixture(false).await;
}

#[tokio::test]
async fn cmd_exit_code_environment_is_inherited_without_rejecting_spawn() {
    cmd_environment_fixture(true).await;
}

async fn cmd_environment_fixture(with_exit_code: bool) {
    let scratch = Scratch::new();
    let command_interpreter = PathBuf::from(std::env::var_os("SystemRoot").unwrap())
        .join("System32")
        .join("cmd.exe");
    let executable = std::env::current_exe().unwrap();
    let batch = scratch.0.join("hidden-drive.cmd");
    let preceding_command = if with_exit_code {
        "cmd /d /c exit 37\r\n"
    } else {
        ""
    };
    std::fs::write(&batch, format!("@echo off\r\ncd /d \"%~dp0\"\r\n{preceding_command}\"{}\" --exact fixture_process --nocapture\r\nexit /b %errorlevel%\r\n", executable.display())).unwrap();
    let mut command = tokio::process::Command::new(command_interpreter);
    command.args(["/d", "/c"]);
    command.current_dir(&scratch.0).arg("hidden-drive.cmd");
    command
        .env(
            "ACP_OWNER_FIXTURE",
            if with_exit_code {
                "hidden-exit-code"
            } else {
                "hidden-drive"
            },
        )
        .env("ACP_OWNER_FIXTURE_PATH", scratch.0.join("unused"))
        .kill_on_drop(true);
    let output = timeout(Duration::from_secs(20), command.output())
        .await
        .unwrap()
        .unwrap();
    assert!(
        output.status.success(),
        "hidden environment spawn regression: {} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
#[tokio::test]
async fn explicit_hidden_drive_overrides_remain_invalid() {
    for name in [
        "=C:",
        "=c:",
        "=C",
        "=1:",
        "=ExitCode",
        "=exitcode",
        "=ExitCodeAscii",
        "=exitcodeascii",
        "=ExitCode=extra",
        "A=B",
        "",
    ] {
        let mut input = spec("exit 0");
        input
            .env
            .push((name.into(), "SECRET-override-value".into()));
        let Err(error) = OwnedProcess::spawn(input, Instant::now() + Duration::from_secs(1)).await
        else {
            panic!("invalid override was accepted")
        };
        assert!(matches!(
            error,
            ProcessOwnerError::InvalidSpec("invalid environment name")
        ));
        assert!(!error.to_string().contains("SECRET"));
    }
}

async fn hidden_drive_fixture(path: &std::path::Path, with_exit_code: bool) {
    let (name, value) = std::env::vars_os()
        .find(|(name, _)| {
            let units: Vec<u16> =
                std::os::windows::ffi::OsStrExt::encode_wide(name.as_os_str()).collect();
            units.len() == 3 && units[0] == u16::from(b'=') && units[2] == u16::from(b':')
        })
        .expect("cmd must establish a real hidden drive environment entry");
    let mut inherited_entries = vec![(name, value)];
    if with_exit_code {
        let exit_code = std::env::var_os("=ExitCode")
            .expect("cmd external command must establish a real hidden exit code entry");
        assert_eq!(exit_code, "00000025");
        inherited_entries.push(("=ExitCode".into(), exit_code));
        let exit_character = std::env::var_os("=ExitCodeAscii")
            .expect("cmd character exit code must establish a real hidden ASCII entry");
        assert_eq!(exit_character, "%");
        inherited_entries.push(("=ExitCodeAscii".into(), exit_character));
    }
    let mut child = launch("exit 19").await;
    assert_eq!(
        timeout(Duration::from_secs(10), child.wait_exit())
            .await
            .unwrap()
            .unwrap(),
        19
    );
    let report = child
        .shutdown(Instant::now() + Duration::from_secs(1))
        .await
        .unwrap();
    assert_eq!(report.disposition, ShutdownDisposition::Graceful);
    for (name, value) in inherited_entries {
        let mut input = fixture_spec("hidden-preserved", path);
        input.env.extend([
            ("ACP_OWNER_EXPECTED_HIDDEN_KEY".into(), name),
            ("ACP_OWNER_EXPECTED_HIDDEN_VALUE".into(), value),
        ]);
        let mut nested = OwnedProcess::spawn(input, Instant::now() + Duration::from_secs(10))
            .await
            .unwrap();
        assert_eq!(
            timeout(Duration::from_secs(10), nested.wait_exit())
                .await
                .unwrap()
                .unwrap(),
            0,
            "hidden environment preservation fixture failed"
        );
        finish(&mut nested).await;
    }
}
