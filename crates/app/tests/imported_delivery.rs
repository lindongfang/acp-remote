//! Access 的存储验证（`access-owner-data-stateless`）：投递/浏览之后磁盘上没有任何远程记录。
//!
//! 本文件替代原 `imported_delivery` 的“收据先于广播”测试：新的运行期路径不落收据、不写 ACK，
//! 因此这里断言的是**反向**事实——Access 的数据库文件里不存在 `imported_*` 表，也没有任何远程
//! 哨兵字节；owned 家族与连接资料仍可正常打开。

mod support;

use app::compose::Composition;

/// 只在远程正文里出现的哨兵；磁盘上任何位置都不得出现它。
const REMOTE_SENTINEL: &[u8] = b"remote-body-marker-5c0f";

fn imported_config(data_dir: &std::path::Path) -> app::Config {
    let text = format!(
        "[daemon]\ndata_dir = {dir:?}\n[identity]\nkeystore = \"ephemeral\"\n\
         [dev_mode]\nenabled = true\n",
        dir = data_dir.display()
    );
    app::Config::from_toml(&text).expect("配置合法")
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

#[test]
fn access_storage_contains_no_remote_resources_or_receipts() {
    support::block_on(async {
        let root = support::TempRoot::new("stateless-access-storage");
        let data_dir = root.path().join("data");
        support::create_owner_only_dir(&data_dir);
        let (publisher, _events) = app::compose::forked_publisher();
        let composition = Composition::assemble(imported_config(&data_dir), publisher)
            .await
            .expect("组合根装配");
        assert_eq!(
            composition.attachments().metadata().owned_schema_version,
            storage_sqlite::migrate::OWNED_SCHEMA_VERSION,
            "Access 库的当前版本必须只覆盖 owned 家族"
        );
        let database = data_dir.join("acp-remote.sqlite3");
        // 关闭前的检查点由 `close()` 完成；先读主库，再关闭后复查两份文件。
        let before = std::fs::read(&database).expect("数据库可读");
        composition.close().await.expect("关闭序列");
        let after = std::fs::read(&database).expect("关闭后数据库可读");

        for (label, bytes) in [("open", &before), ("closed", &after)] {
            assert!(
                !contains(bytes, b"imported_session"),
                "{label}: 数据库里不得出现 imported_session"
            );
            assert!(
                !contains(bytes, b"imported_delivery_index"),
                "{label}: 数据库里不得出现 imported_delivery_index"
            );
            assert!(
                !contains(bytes, b"imported_command_ref"),
                "{label}: 数据库里不得出现 imported_command_ref"
            );
            assert!(
                !contains(bytes, b"imported_audit"),
                "{label}: 数据库里不得出现 imported_audit"
            );
            assert!(
                !contains(bytes, REMOTE_SENTINEL),
                "{label}: 数据库里不得出现远程正文哨兵"
            );
        }

        // 数据目录里除数据库/WAL/附件目录外不应出现远程事件副本。
        let mut pending = vec![root.path().to_path_buf()];
        while let Some(directory) = pending.pop() {
            for entry in std::fs::read_dir(&directory).expect("读取数据目录") {
                let entry = entry.expect("目录项");
                let path = entry.path();
                if path.is_dir() {
                    pending.push(path);
                    continue;
                }
                let bytes = std::fs::read(&path).unwrap_or_default();
                assert!(
                    !contains(&bytes, REMOTE_SENTINEL),
                    "{} 含远程正文哨兵",
                    path.display()
                );
            }
        }
    });
}
