//! Unix endpoint 的真实用例（`[PV3]` 的 WP3a 部分；本机 Windows 不执行，Linux CI 覆盖）。
//!
//! 覆盖 R17/R19（socket 位置、目录 `0700`、socket `0600`、失败关闭）与 R20/R21（`SO_PEERCRED` 同用户被接受）。
//! 每个用例自建独占临时目录并在结束时删除（`LOCAL_ADMIN_PROTOCOL.md` §2.1 的 `XDG_RUNTIME_DIR` 通过
//! `LocalEndpointConfig::runtime_dir` 显式覆盖，因为 Rust 2024 下测试不能安全地设置进程环境变量）。
//!
//! 已知限制：跨用户拒绝（uid 不等）需要第二个 OS 账号，Linux CI 上不可得；`transport::local::platform::unix`
//! 的单元测试覆盖 `SO_PEERCRED` 的取值，拒绝分支由单一比较承担（见报告）。

#![cfg(unix)]

use std::os::unix::fs::{PermissionsExt as _, symlink};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use server::local_admin::{AdminResponse, UnroutedAdminHandler};
use server::transport::local::{
    CHANNEL_LOCAL_ADMIN_BYTE, EndpointError, InstanceId, LocalConnectionHandlers, LocalEndpoint,
    LocalEndpointConfig, LoggingAuditHook, UnsafePathKind, serve_connection,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

/// 独占临时目录（用例结束前必须删除）。
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "acp-remote-wp3-{label}-{}",
            InstanceId::generate().as_str()
        ));
        std::fs::create_dir_all(&path).expect("创建临时目录");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn config(&self, instance_id: InstanceId) -> LocalEndpointConfig {
        LocalEndpointConfig {
            data_dir: self.path.join("data"),
            instance_id,
            runtime_dir: Some(self.path.join("runtime")),
        }
    }

    fn endpoint_dir(&self) -> PathBuf {
        self.path.join("runtime").join("acp-remote")
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn mode_of(path: &Path) -> u32 {
    std::fs::metadata(path)
        .expect("metadata")
        .permissions()
        .mode()
        & 0o777
}

fn handlers() -> Arc<LocalConnectionHandlers> {
    Arc::new(LocalConnectionHandlers::new(Arc::new(UnroutedAdminHandler)))
}

fn frame(body: &[u8]) -> Vec<u8> {
    let length = u32::try_from(body.len() + 1).expect("用例 payload 很小");
    let mut out = Vec::with_capacity(4 + body.len() + 1);
    out.extend_from_slice(&length.to_be_bytes());
    out.push(CHANNEL_LOCAL_ADMIN_BYTE);
    out.extend_from_slice(body);
    out
}

async fn bind(config: LocalEndpointConfig) -> Result<LocalEndpoint, EndpointError> {
    LocalEndpoint::bind(config, Arc::new(LoggingAuditHook)).await
}

/// 必须被拒绝（失败关闭）的创建请求。
async fn refused(config: LocalEndpointConfig, label: &str) -> EndpointError {
    match bind(config).await {
        Ok(_) => panic!("{label}：必须拒绝启动（失败关闭）"),
        Err(error) => error,
    }
}

#[tokio::test]
async fn unix_endpoint_creates_socket_with_private_permissions() {
    let temp = TempDir::new("perms");
    let config = temp.config(InstanceId::new("0123456789abcdef").expect("形状合法"));
    let endpoint = bind(config.clone()).await.expect("创建 socket endpoint");
    let directory = temp.endpoint_dir();
    let socket = directory.join("0123456789abcdef.sock");

    assert_eq!(endpoint.describe(), socket.display().to_string());
    assert_eq!(mode_of(&directory), 0o700, "目录必须是 0700（§2.1）");
    assert_eq!(mode_of(&socket), 0o600, "socket 必须是 0600（§2.1）");
}

#[tokio::test]
async fn unix_endpoint_tightens_a_pre_existing_directory() {
    let temp = TempDir::new("tighten");
    let directory = temp.endpoint_dir();
    std::fs::create_dir_all(&directory).expect("预建目录");
    std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o777)).expect("放宽权限");
    assert_eq!(mode_of(&directory), 0o777);

    let endpoint = bind(temp.config(InstanceId::new("0123456789abcdef").expect("形状合法")))
        .await
        .expect("权限可以保证时必须成功并收紧");
    assert_eq!(mode_of(&directory), 0o700);
    assert!(endpoint.describe().ends_with(".sock"));
}

#[tokio::test]
async fn unix_endpoint_falls_back_to_data_dir_run_when_xdg_is_unset() {
    if std::env::var_os("XDG_RUNTIME_DIR").is_some() {
        println!(
            "[PV3] XDG_RUNTIME_DIR 已设置：规则由 local_endpoint_naming 的纯函数用例覆盖，本用例不创建目录"
        );
        return;
    }
    let temp = TempDir::new("fallback");
    let config = LocalEndpointConfig {
        data_dir: temp.path().join("data"),
        instance_id: InstanceId::new("0123456789abcdef").expect("形状合法"),
        runtime_dir: None,
    };
    let endpoint = bind(config).await.expect("回落路径必须可用");
    let directory = temp.path().join("data").join("run");
    assert_eq!(
        endpoint.describe(),
        directory
            .join("0123456789abcdef.sock")
            .display()
            .to_string()
    );
    assert_eq!(mode_of(&directory), 0o700);
}

#[tokio::test]
async fn unix_endpoint_refuses_to_replace_a_live_socket() {
    let temp = TempDir::new("live");
    let config = temp.config(InstanceId::new("0123456789abcdef").expect("形状合法"));
    let _live = bind(config.clone()).await.expect("第一个实例");

    let second = bind(config).await;
    assert!(
        matches!(second, Err(EndpointError::AlreadyInUse)),
        "有存活 listener 时必须拒绝启动（§7），实际 {:?}",
        second.err().map(|error| error.to_string())
    );
}

#[tokio::test]
async fn unix_endpoint_replaces_a_stale_socket_and_accepts_same_user_clients() {
    let temp = TempDir::new("stale");
    let config = temp.config(InstanceId::new("0123456789abcdef").expect("形状合法"));
    let first = bind(config.clone()).await.expect("第一个实例");
    let socket = PathBuf::from(first.describe());
    drop(first);
    assert!(socket.exists(), "socket 文件在实例退出后仍然存在（残留）");

    let mut second = bind(config).await.expect("残留 socket 应可删除重建");
    let mut client = UnixStream::connect(&socket)
        .await
        .expect("同用户客户端连接");
    let stream = second
        .accept()
        .await
        .expect("同用户连接必须通过 SO_PEERCRED 校验");

    let server = tokio::spawn(serve_connection(stream, handlers()));
    let body = br#"{"v":1,"id":"5b1f0c2e-8a4d-4b6f-9c31-0d2a7e5f4b10","method":"daemon.status","params":{}}"#;
    client.write_all(&frame(body)).await.expect("写入请求");
    let mut header = [0u8; 4];
    client.read_exact(&mut header).await.expect("帧头");
    let mut payload = vec![0u8; usize::try_from(u32::from_be_bytes(header)).expect("32 位长度")];
    client.read_exact(&mut payload).await.expect("响应帧");
    assert_eq!(payload[0], CHANNEL_LOCAL_ADMIN_BYTE);
    let response = AdminResponse::decode(&payload[1..]).expect("响应可解码");
    assert_eq!(
        response.id().as_str(),
        "5b1f0c2e-8a4d-4b6f-9c31-0d2a7e5f4b10"
    );

    drop(client);
    let _ = server.await;
}

#[tokio::test]
async fn unix_endpoint_rejects_unsafe_paths() {
    let temp = TempDir::new("unsafe");
    let instance = InstanceId::new("0123456789abcdef").expect("形状合法");
    let config = temp.config(instance);
    let directory = temp.endpoint_dir();
    let socket = directory.join("0123456789abcdef.sock");

    // 普通文件占位。
    std::fs::create_dir_all(&directory).expect("预建目录");
    std::fs::write(&socket, b"not a socket").expect("写普通文件");
    let error = refused(config.clone(), "普通文件占位").await;
    assert!(matches!(
        error,
        EndpointError::UnsafePath {
            kind: UnsafePathKind::NotASocket
        }
    ));

    // socket 位置被符号链接占用：绝不被跟随。
    std::fs::remove_file(&socket).expect("清理普通文件");
    let target = temp.path().join("elsewhere.sock");
    std::fs::write(&target, b"x").expect("写目标文件");
    symlink(&target, &socket).expect("建立符号链接");
    let error = refused(config.clone(), "socket 位置被符号链接占用").await;
    assert!(matches!(
        error,
        EndpointError::UnsafePath {
            kind: UnsafePathKind::Symlink
        }
    ));

    // 目录位置被符号链接占用。
    std::fs::remove_file(&socket).expect("清理符号链接");
    std::fs::remove_dir_all(&directory).expect("清理目录");
    let real_directory = temp.path().join("real-runtime-dir");
    std::fs::create_dir_all(&real_directory).expect("真实目录");
    symlink(&real_directory, &directory).expect("目录符号链接");
    let error = refused(config, "目录位置被符号链接占用").await;
    assert!(matches!(
        error,
        EndpointError::UnsafePath {
            kind: UnsafePathKind::Symlink
        }
    ));

    // 目录位置被普通文件占用。
    std::fs::remove_file(&directory).expect("清理目录符号链接");
    std::fs::write(&directory, b"not a directory").expect("写普通文件");
    let error = refused(
        temp.config(InstanceId::new("0123456789abcdef").expect("形状合法")),
        "目录位置被普通文件占用",
    )
    .await;
    assert!(matches!(
        error,
        EndpointError::UnsafePath {
            kind: UnsafePathKind::NotADirectory
        }
    ));
}
