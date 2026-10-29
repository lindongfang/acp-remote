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
    node::NodeRuntime,
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
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> Result<(), AppError> {
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
            DeviceCommand::Pair => pair_device(&cli.config).await,
            DeviceCommand::List => list_devices(&cli.config).await,
            DeviceCommand::Revoke { device_id } => revoke_device(&cli.config, &device_id).await,
        },
    }
}

async fn start_node(config_path: &Path) -> Result<(), AppError> {
    let config = NodeConfig::load(config_path)?;
    let origin = config.canonical_origin.clone();
    let node = NodeRuntime::start(config, &DpapiProtector).await?;
    println!("ACP Remote node ready");
    println!("origin={origin}");
    println!("nodeId={}", node.node_id());
    println!("listen={}", node.listen_addr());
    std::io::stdout().flush().map_err(|source| AppError::Io {
        context: "writing node readiness",
        source,
    })?;
    node.run().await?;
    println!("ACP Remote node stopped");
    Ok(())
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
