use std::{
    io::{self, IsTerminal, Write},
    path::{Path, PathBuf},
    process::ExitCode,
    time::Duration,
};

use acp_remote::{
    config::NodeConfig,
    error::AppError,
    ipc::{self, LocalIpcAddress},
    node::NodeProcessOwner,
    pairing::PairingEvent,
    platform::DpapiProtector,
    trust::ClientStatus,
};
use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "acp-remote", version, about = "ACP Remote local node")]
struct Cli {
    /// Path to the node TOML configuration.
    #[arg(long, global = true, default_value = "acp-remote.toml")]
    config: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Manage the foreground node process.
    Daemon {
        #[command(subcommand)]
        command: DaemonCommand,
    },
    /// Manage trusted clients.
    Device {
        #[command(subcommand)]
        command: DeviceCommand,
    },
    /// Manage registered local workspaces.
    Workspace {
        #[command(subcommand)]
        command: WorkspaceCommand,
    },
    /// Manage registered native session targets.
    Target {
        #[command(subcommand)]
        command: TargetCommand,
    },
    /// Inspect safe node catalog metadata.
    Catalog {
        #[command(subcommand)]
        command: CatalogCommand,
    },
}

#[derive(Debug, Subcommand)]
enum WorkspaceCommand {
    /// Register an existing local workspace directory.
    Add {
        #[arg(long)]
        path: String,
    },
    /// List registered workspaces.
    List,
}

#[derive(Debug, Subcommand)]
enum TargetCommand {
    /// Register a native session target against a source and workspace.
    Add {
        #[arg(long = "source-id")]
        source_id: String,
        #[arg(long = "native-session-id")]
        native_session_id: String,
        #[arg(long = "workspace-id")]
        workspace_id: String,
    },
    /// List registered product targets.
    List,
}

#[derive(Debug, Subcommand)]
enum CatalogCommand {
    /// List registered workspaces, sources, harnesses, profiles, and targets.
    List,
}

#[derive(Debug, Subcommand)]
enum DaemonCommand {
    /// Start the node in the foreground.
    Start,
    /// Read node status through the owner IPC channel.
    Status,
    /// Stop the node through the owner IPC channel.
    Stop,
}

#[derive(Debug, Subcommand)]
enum DeviceCommand {
    /// Start a short-lived browser pairing request.
    Pair,
    /// List trusted clients.
    List,
    /// Revoke a trusted client.
    Revoke {
        #[arg(long = "device-id")]
        device_id: String,
    },
}

#[tokio::main]
async fn main() -> ExitCode {
    match run(Cli::parse()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            if !error.reported {
                eprintln!("error: {}", error.error);
            }
            ExitCode::FAILURE
        }
    }
}

struct CliFailure {
    error: AppError,
    reported: bool,
}
impl From<AppError> for CliFailure {
    fn from(error: AppError) -> Self {
        Self {
            error,
            reported: false,
        }
    }
}

async fn run(cli: Cli) -> Result<(), CliFailure> {
    match cli.command {
        Command::Daemon { command } => match command {
            DaemonCommand::Start => start_node(&cli.config).await,
            DaemonCommand::Status => {
                let address = control_address(&cli.config)?;
                let status = ipc::status(&address).await?;
                print_status(&status);
                Ok(())
            }
            DaemonCommand::Stop => {
                let address = control_address(&cli.config)?;
                let status = ipc::stop(&address).await?;
                print_status(&status);
                Ok(())
            }
        },
        Command::Device { command } => match command {
            DeviceCommand::Pair => pair_device(&cli.config).await.map_err(CliFailure::from),
            DeviceCommand::List => list_devices(&cli.config).await.map_err(CliFailure::from),
            DeviceCommand::Revoke { device_id } => revoke_device(&cli.config, &device_id)
                .await
                .map_err(CliFailure::from),
        },
        Command::Workspace { command } => match command {
            WorkspaceCommand::Add { path } => add_workspace(&cli.config, &path)
                .await
                .map_err(CliFailure::from),
            WorkspaceCommand::List => list_workspaces(&cli.config).await.map_err(CliFailure::from),
        },
        Command::Target { command } => match command {
            TargetCommand::Add {
                source_id,
                native_session_id,
                workspace_id,
            } => add_target(&cli.config, &source_id, &native_session_id, &workspace_id)
                .await
                .map_err(CliFailure::from),
            TargetCommand::List => list_targets(&cli.config).await.map_err(CliFailure::from),
        },
        Command::Catalog { command } => match command {
            CatalogCommand::List => list_catalog(&cli.config).await.map_err(CliFailure::from),
        },
    }
}

async fn start_node(config_path: &Path) -> Result<(), CliFailure> {
    let config = NodeConfig::load(config_path)?;
    let origin = config.canonical_origin.clone();
    let owner = NodeProcessOwner::begin(
        &tokio::runtime::Handle::current(),
        config,
        std::sync::Arc::new(DpapiProtector),
    );
    run_owned_node(owner, &origin, &mut io::stdout()).await
}

fn write_node_ready(
    output: &mut impl Write,
    origin: &str,
    node: &acp_remote::node::NodeRuntime,
) -> Result<(), AppError> {
    (|| -> io::Result<()> {
        writeln!(output, "ACP Remote node ready")?;
        writeln!(output, "origin={origin}")?;
        writeln!(output, "nodeId={}", node.node_id())?;
        writeln!(output, "listen={}", node.listen_addr())?;
        output.flush()
    })()
    .map_err(|source| AppError::Io {
        context: "writing node readiness",
        source,
    })
}

async fn run_owned_node(
    owner: NodeProcessOwner,
    origin: &str,
    output: &mut impl Write,
) -> Result<(), CliFailure> {
    let result = match owner.ready().await {
        Ok(node) => {
            async {
                write_node_ready(output, origin, &node)?;
                node.run().await
            }
            .await
        }
        Err(error) => Err(error),
    };
    if let Err(error) = &result {
        eprintln!("error: {error}");
    }
    // Keep this same #[tokio::main] alive after publishing the budget result.
    let joined = owner.join_actual().await;
    if let Err(error) = result {
        Err(CliFailure {
            error,
            reported: true,
        })
    } else {
        joined?;
        println!("ACP Remote node stopped");
        Ok(())
    }
}

fn control_address(config_path: &Path) -> Result<LocalIpcAddress, AppError> {
    let config = NodeConfig::load(config_path)?;
    LocalIpcAddress::for_data_directory(&config.data_dir)
}

fn print_status(status: &acp_remote::ipc::NodeStatus) {
    println!(
        "state={} nodeId={} origin={} pid={} version={}",
        status.state, status.node_id, status.origin, status.pid, status.version
    );
}

async fn list_devices(config_path: &Path) -> Result<(), AppError> {
    let address = control_address(config_path)?;
    let mut cursor = None;
    loop {
        let page = ipc::device_list(&address, cursor.as_deref()).await?;
        for client in page.items {
            let revoked_at = client
                .revoked_at
                .map_or_else(|| "-".to_owned(), |value| value.to_string());
            println!(
                "clientId={} status={:?} name={} fingerprint={} createdAt={} revokedAt={}",
                client.client_id,
                client.status,
                client.display_name,
                client.public_key_fingerprint,
                client.created_at,
                revoked_at
            );
        }
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    Ok(())
}

async fn pair_device(config_path: &Path) -> Result<(), AppError> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(AppError::Ipc(
            "device pair requires an interactive terminal".to_owned(),
        ));
    }
    let address = control_address(config_path)?;
    let (mut session, start) = ipc::begin_pairing(&address).await?;
    println!("nodeFingerprint={}", start.node_id);
    println!("origin={}", start.origin);
    println!("pairingCode={} expiresAt={}", start.code, start.expires_at);
    io::stdout().flush().map_err(|source| AppError::Io {
        context: "writing the pairing code",
        source,
    })?;

    let Some(event) = wait_for_pair_event(&mut session).await? else {
        println!("Pairing cancelled before approval.");
        return Ok(());
    };
    match event.event.as_str() {
        "pending_confirmation" => {}
        "expired" => {
            return Err(AppError::Ipc(
                "pairing expired before a client was approved".to_owned(),
            ));
        }
        "cancelled" => {
            return Err(AppError::Ipc("pairing was cancelled".to_owned()));
        }
        other => {
            return Err(AppError::Ipc(format!(
                "pairing returned an unexpected event: {other}"
            )));
        }
    }
    println!(
        "clientName={} clientFingerprint={} resultExpiresAt={}",
        event.display_name.as_deref().unwrap_or("-"),
        event.fingerprint.as_deref().unwrap_or("-"),
        event.expires_at
    );
    let Some(approved) = prompt_for_approval().await? else {
        println!("Pairing cancelled before approval.");
        return Ok(());
    };
    if approved {
        session.approve().await?;
    } else {
        session.reject().await?;
    }
    let Some(result) = wait_for_pair_event(&mut session).await? else {
        drop(session);
        return report_interrupted_decision(config_path, &address, event.fingerprint.as_deref())
            .await;
    };
    match result.event.as_str() {
        "approved" => {
            let client_id = result.client_id.as_deref().ok_or_else(|| {
                AppError::Ipc("approved pairing omitted the client identifier".to_owned())
            })?;
            print_approved_client(config_path, client_id);
            Ok(())
        }
        "rejected" => {
            println!("Pairing rejected; no client trust was created.");
            Ok(())
        }
        "expired" => Err(AppError::Ipc(
            "pairing expired before the decision was committed".to_owned(),
        )),
        other => Err(AppError::Ipc(format!(
            "pairing returned an unexpected final event: {other}"
        ))),
    }
}

fn print_approved_client(config_path: &Path, client_id: &str) {
    println!("clientId={client_id}");
    println!(
        "The client is now trusted. Revoke it with: acp-remote --config \"{}\" device revoke --device-id {client_id}",
        config_path.display()
    );
}

async fn report_interrupted_decision(
    config_path: &Path,
    address: &LocalIpcAddress,
    fingerprint: Option<&str>,
) -> Result<(), AppError> {
    // Closing the pairing stream cancels only work that has not committed.
    // Always retain recovery instructions when the final event was not observed.
    println!("Pairing decision interrupted; approval may already have committed.");
    println!(
        "Check acp-remote --config \"{}\" device list. If approved, use device revoke --device-id <ID>; cancellation does not revoke trust.",
        config_path.display()
    );
    io::stdout().flush().map_err(|source| AppError::Io {
        context: "writing the interrupted pairing result",
        source,
    })?;
    match tokio::time::timeout(
        Duration::from_secs(5),
        find_trusted_fingerprint(address, fingerprint),
    )
    .await
    {
        Ok(Ok(Some(client_id))) => print_approved_client(config_path, &client_id),
        Ok(Ok(None)) => println!(
            "Approval was not confirmed. Recheck device list before starting another pairing."
        ),
        Ok(Err(_)) | Err(_) => {
            println!("Approval could not be checked. Use device list when the node is available.");
        }
    }
    Ok(())
}

async fn find_trusted_fingerprint(
    address: &LocalIpcAddress,
    fingerprint: Option<&str>,
) -> Result<Option<String>, AppError> {
    let Some(fingerprint) = fingerprint else {
        return Ok(None);
    };
    let mut cursor = None;
    loop {
        let page = ipc::device_list(address, cursor.as_deref()).await?;
        if let Some(client) = page.items.into_iter().find(|client| {
            client.status == ClientStatus::Trusted && client.public_key_fingerprint == fingerprint
        }) {
            return Ok(Some(client.client_id));
        }
        cursor = page.next_cursor;
        if cursor.is_none() {
            return Ok(None);
        }
    }
}

async fn wait_for_pair_event(
    session: &mut ipc::PairingSession,
) -> Result<Option<PairingEvent>, AppError> {
    tokio::select! {
        signal = tokio::signal::ctrl_c() => {
            signal.map_err(|source| AppError::Io {
                context: "waiting for Ctrl-C during pairing",
                source,
            })?;
            Ok(None)
        }
        event = session.next_event() => event.map(Some),
    }
}

async fn prompt_for_approval() -> Result<Option<bool>, AppError> {
    print!("Approve this client? [y/N] ");
    io::stdout().flush().map_err(|source| AppError::Io {
        context: "writing the pairing prompt",
        source,
    })?;
    tokio::select! {
        signal = tokio::signal::ctrl_c() => {
            signal.map_err(|source| AppError::Io {
                context: "waiting for Ctrl-C during pairing approval",
                source,
            })?;
            Ok(None)
        }
        read = read_approval_line() => {
            let answer = read?;
            let normalized = answer.trim().to_ascii_lowercase();
            Ok(Some(matches!(normalized.as_str(), "y" | "yes")))
        }
    }
}

async fn read_approval_line() -> Result<String, AppError> {
    let mut answer = String::new();
    let mut overflow = false;
    loop {
        if let Some(text) = acp_remote::platform::poll_console_character()? {
            for _ in 0..text.repeat_count {
                match text.character {
                    '\r' | '\n' => {
                        println!();
                        return Ok(if overflow { String::new() } else { answer });
                    }
                    '\u{8}' if !answer.is_empty() => {
                        answer.pop();
                        print!("\u{8} \u{8}");
                    }
                    value if !value.is_control() => {
                        if answer.len() < 80 && !overflow {
                            answer.push(value);
                            print!("{value}");
                        } else {
                            overflow = true;
                        }
                    }
                    _ => {}
                }
            }
            io::stdout().flush().map_err(|source| AppError::Io {
                context: "echoing the pairing decision",
                source,
            })?;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

async fn revoke_device(config_path: &Path, client_id: &str) -> Result<(), AppError> {
    let address = control_address(config_path)?;
    let result = ipc::device_revoke(&address, client_id).await?;
    println!("clientId={} status={:?}", result.client_id, result.status);
    Ok(())
}

async fn add_workspace(config_path: &Path, path: &str) -> Result<(), AppError> {
    let address = control_address(config_path)?;
    let workspace = ipc::workspace_register(&address, path).await?;
    println!(
        "workspaceId={} canonicalPath={}",
        workspace.workspace_id, workspace.canonical_path
    );
    Ok(())
}

async fn list_workspaces(config_path: &Path) -> Result<(), AppError> {
    let address = control_address(config_path)?;
    let inventory = ipc::catalog_inventory(&address).await?;
    for workspace in inventory.workspaces {
        println!(
            "workspaceId={} canonicalPath={}",
            workspace.workspace_id, workspace.canonical_path
        );
    }
    Ok(())
}

async fn add_target(
    config_path: &Path,
    source_id: &str,
    native_session_id: &str,
    workspace_id: &str,
) -> Result<(), AppError> {
    let address = control_address(config_path)?;
    let target = ipc::target_register(&address, source_id, native_session_id, workspace_id).await?;
    println!(
        "targetId={} sourceId={} nativeSessionId={} workspaceId={}",
        target.target_id,
        target.source_id,
        target.native_session_id,
        target.workspace_id.as_deref().unwrap_or("-")
    );
    Ok(())
}

async fn list_targets(config_path: &Path) -> Result<(), AppError> {
    let address = control_address(config_path)?;
    let inventory = ipc::catalog_inventory(&address).await?;
    for target in inventory.targets {
        println!(
            "targetId={} sourceId={} nativeSessionId={} workspaceId={} boundProfileId={}",
            target.target_id,
            target.source_id,
            target.native_session_id,
            target.workspace_id.as_deref().unwrap_or("-"),
            target
                .binding
                .as_ref()
                .map_or("-", |b| b.profile_id.as_str())
        );
    }
    Ok(())
}

async fn list_catalog(config_path: &Path) -> Result<(), AppError> {
    let address = control_address(config_path)?;
    let inventory = ipc::catalog_inventory(&address).await?;
    for workspace in inventory.workspaces {
        println!(
            "workspace workspaceId={} canonicalPath={}",
            workspace.workspace_id, workspace.canonical_path
        );
    }
    for source in inventory.sources {
        println!("source sourceId={} name={}", source.source_id, source.name);
    }
    for harness in inventory.harnesses {
        println!(
            "harness harnessId={} name={} sourceId={}",
            harness.harness_id, harness.name, harness.source_id
        );
    }
    for profile in inventory.profiles {
        println!(
            "profile profileId={} revision={} harnessId={} sourceId={}",
            profile.profile_id, profile.revision, profile.harness_id, profile.source_id
        );
    }
    for target in inventory.targets {
        println!(
            "target targetId={} sourceId={} nativeSessionId={} workspaceId={} boundProfileId={}",
            target.target_id,
            target.source_id,
            target.native_session_id,
            target.workspace_id.as_deref().unwrap_or("-"),
            target
                .binding
                .as_ref()
                .map_or("-", |b| b.profile_id.as_str())
        );
    }
    Ok(())
}

#[cfg(test)]
mod lifecycle_development_tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tokio::time::timeout;
    struct Area {
        directory: PathBuf,
        config: NodeConfig,
    }
    impl Area {
        fn new() -> Self {
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let directory =
                std::env::temp_dir().join(format!("acp-main-life-{}-{nonce}", std::process::id()));
            std::fs::create_dir(&directory).unwrap();
            let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
            let tls_cert = directory.join("cert.pem");
            let tls_key = directory.join("key.pem");
            std::fs::write(&tls_cert, cert.cert.pem()).unwrap();
            std::fs::write(&tls_key, cert.signing_key.serialize_pem()).unwrap();
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let config = NodeConfig {
                data_dir: directory.join("data"),
                listen: listener.local_addr().unwrap(),
                canonical_origin: "https://node.example.test".into(),
                tls_cert,
                tls_key,
            };
            Self { directory, config }
        }
    }
    impl Drop for Area {
        fn drop(&mut self) {
            let result = std::fs::remove_dir_all(&self.directory);
            eprintln!(
                "MAIN_LIFECYCLE_FIRST_REMOVE result={:?}",
                result
                    .as_ref()
                    .err()
                    .map(|error| (error.kind(), error.raw_os_error()))
            );
        }
    }
    struct Output {
        bytes: Arc<Mutex<Vec<u8>>>,
        flushed: Arc<tokio::sync::Notify>,
        fail_flush: bool,
    }
    impl Write for Output {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.bytes.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            self.flushed.notify_one();
            if self.fail_flush {
                Err(io::Error::other("development flush failure"))
            } else {
                Ok(())
            }
        }
    }
    #[tokio::test]
    async fn main_normal_stop_joins_original_runtime_and_releases_directory() {
        let area = Area::new();
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let flushed = Arc::new(tokio::sync::Notify::new());
        let mut output = Output {
            bytes: bytes.clone(),
            flushed: flushed.clone(),
            fail_flush: false,
        };
        let config = area.config.clone();
        let running = tokio::spawn(async move {
            let owner = NodeProcessOwner::begin(
                &tokio::runtime::Handle::current(),
                config,
                Arc::new(DpapiProtector),
            );
            run_owned_node(owner, "https://node.example.test", &mut output).await
        });
        timeout(Duration::from_secs(5), flushed.notified())
            .await
            .unwrap();
        let address = LocalIpcAddress::for_data_directory(&area.config.data_dir).unwrap();
        timeout(Duration::from_secs(18), ipc::stop(&address))
            .await
            .expect("owner stop remains within the product deadline")
            .expect("normal owner stop succeeds");
        timeout(Duration::from_secs(18), running)
            .await
            .expect("main path joins after the owner stop")
            .expect("main task")
            .unwrap_or_else(|failure| panic!("normal main path succeeds: {}", failure.error));
        assert!(acp_remote::data_dir::DataDirectory::acquire(&area.config.data_dir).is_ok());
    }

    #[tokio::test]
    async fn main_flush_failure_keeps_projection_until_release_then_joins_cleanup() {
        let area = Area::new();
        let owner = NodeProcessOwner::begin(
            &tokio::runtime::Handle::current(),
            area.config.clone(),
            Arc::new(DpapiProtector),
        );
        let projection = owner.ready().await.unwrap();
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let flushed = Arc::new(tokio::sync::Notify::new());
        let mut output = Output {
            bytes: bytes.clone(),
            flushed: flushed.clone(),
            fail_flush: true,
        };
        let running = tokio::spawn(async move {
            run_owned_node(owner, "https://node.example.test", &mut output).await
        });
        timeout(Duration::from_secs(5), flushed.notified())
            .await
            .unwrap();
        tokio::task::yield_now().await;
        assert!(
            !running.is_finished(),
            "the live projection keeps the original runtime owner alive after readiness fails"
        );
        assert!(matches!(
            acp_remote::data_dir::DataDirectory::acquire(&area.config.data_dir),
            Err(AppError::DataDirectoryBusy)
        ));
        let output = String::from_utf8(bytes.lock().unwrap().clone()).unwrap();
        assert_eq!(output.matches("ACP Remote node ready").count(), 1);
        assert_eq!(output.lines().count(), 4);
        assert!(!output.contains("PRIVATE KEY") && !output.contains("development flush failure"));

        drop(projection);
        let failure = timeout(Duration::from_secs(18), running)
            .await
            .expect("releasing the last projection triggers actual closure")
            .expect("main task")
            .expect_err("readiness failure remains the main result");
        assert!(failure.error.to_string().contains("writing node readiness"));
        assert!(acp_remote::data_dir::DataDirectory::acquire(&area.config.data_dir).is_ok());
    }
}
