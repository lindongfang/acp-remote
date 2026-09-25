//! endpoint 命名规则的跨平台用例（`docs/LOCAL_ADMIN_PROTOCOL.md` §2.1 的 R17）。
//!
//! 命名算法是纯函数，因此两个平台的规则都能在任何平台上被断言；真实创建/权限/凭据校验分别在
//! `local_endpoint_windows.rs`（[PV5]）与 `local_endpoint_unix.rs`（[PV3] 的 Linux CI 部分）。

use std::path::{Path, PathBuf};

use server::transport::local::{
    InstanceId, LocalEndpointConfig, named_pipe_path, unix_endpoint_directory, unix_socket_path,
    user_identity_hash,
};
use sha2::{Digest as _, Sha256};

/// `<userSidHashHex>` = 输入字符串 UTF-8 字节的 SHA-256 小写 hex 前 16 字符（不附加换行或前缀）。
#[test]
fn user_identity_hash_is_the_first_16_hex_characters_of_sha256() {
    for input in [
        "S-1-5-21-2039568721-3854868400-2889826567-1002",
        "1000",
        "0",
    ] {
        let digest = Sha256::digest(input.as_bytes());
        let expected: String = digest
            .iter()
            .take(8)
            .map(|byte| format!("{byte:02x}"))
            .collect();
        assert_eq!(expected.len(), 16);
        assert_eq!(user_identity_hash(input), expected, "输入 {input}");
    }
    // 不得附加换行或前缀：两种输入都必须是「逐字节原样」进入 SHA-256。
    assert_ne!(
        user_identity_hash("S-1-5-21-1"),
        user_identity_hash("S-1-5-21-1\n")
    );
    assert_ne!(user_identity_hash("1000"), user_identity_hash("1000\n"));
    assert_ne!(user_identity_hash("1000"), user_identity_hash("uid=1000"));
}

#[test]
fn named_pipe_path_follows_the_documented_rule() {
    let instance = InstanceId::new("0123456789abcdef").expect("形状合法");
    let sid = "S-1-5-21-2039568721-3854868400-2889826567-1002";
    let path = named_pipe_path(sid, &instance);
    assert_eq!(
        path,
        format!(
            r"\\.\pipe\acp-remote-{}-0123456789abcdef",
            user_identity_hash(sid)
        )
    );
    assert!(path.starts_with(r"\\.\pipe\acp-remote-"));
    assert!(path.ends_with("-0123456789abcdef"));
    // 实例标识不同则 endpoint 不同（同一运行期内由单实例锁保证唯一）。
    let other = named_pipe_path(sid, &InstanceId::new("fedcba9876543210").expect("形状合法"));
    assert_ne!(path, other);
}

#[test]
fn unix_endpoint_uses_xdg_runtime_dir_then_data_dir_run() {
    let config = LocalEndpointConfig {
        data_dir: PathBuf::from("/var/lib/acp-remote"),
        instance_id: InstanceId::new("0123456789abcdef").expect("形状合法"),
        runtime_dir: None,
    };

    let with_xdg = unix_endpoint_directory(&config, Some(Path::new("/run/user/1000")));
    assert_eq!(with_xdg, PathBuf::from("/run/user/1000/acp-remote"));
    assert_eq!(
        unix_socket_path(&with_xdg, &config.instance_id),
        PathBuf::from("/run/user/1000/acp-remote/0123456789abcdef.sock")
    );

    // `XDG_RUNTIME_DIR` 未设置 → 回落 `<daemon.data_dir>/run`。
    let without_xdg = unix_endpoint_directory(&config, None);
    assert_eq!(without_xdg, PathBuf::from("/var/lib/acp-remote/run"));
    assert_eq!(
        unix_socket_path(&without_xdg, &config.instance_id),
        PathBuf::from("/var/lib/acp-remote/run/0123456789abcdef.sock")
    );

    // 显式覆盖优先于环境变量（测试可判定性，见 `LocalEndpointConfig::runtime_dir`）。
    let overridden = LocalEndpointConfig {
        runtime_dir: Some(PathBuf::from("/tmp/acp-remote-test")),
        ..config
    };
    assert_eq!(
        unix_endpoint_directory(&overridden, Some(Path::new("/run/user/1000"))),
        PathBuf::from("/tmp/acp-remote-test/acp-remote")
    );
}

#[test]
fn instance_id_shape_is_validated() {
    assert!(InstanceId::new("0123456789abcdef").is_ok());
    assert!(InstanceId::new("0123456789ABCDEF").is_err());
    assert!(InstanceId::new("0123456789abcde").is_err());
    assert!(InstanceId::new("0123456789abcdef0").is_err());
    assert!(InstanceId::new("0123456789abcde-").is_err());
    assert_eq!(InstanceId::generate().as_str().len(), 16);
}
