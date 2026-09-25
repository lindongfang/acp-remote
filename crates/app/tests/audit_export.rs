//! `audit.export` 的端到端验收：本地通道 → 真实 SQLite 写的文件 → `recordCount`/`sha256` 与文件一致。
//!
//! 为什么放在 `app`：`audit.export` 读的是 `LocalAdminDeps.audit`，而本工作包才把
//! `Arc<SqliteStore>` 作为 `AuditStore` 接进组合根（`compose.rs`）。`server` 的用例只覆盖到
//! fake `AuditStore`，因此「真实文件 + 真实摘要」这一段只能在这一层验证。
//!
//! 审计行的来源是真实用例：`provider.configure`（写 keystore + `provider.configured` 审计）与
//! `export.create`（`export.created` 审计）都经 core 的写集追加审计记录。

mod support;

use serde_json::{Value, json};
use server::local_admin::Method;
use sha2::Digest as _;
use support::{Daemon, params_of};

/// `provider.configure` 写入的凭据值：断言它**不**出现在导出文件里（`SECURITY_DESIGN.md` §14.2）。
const CREDENTIAL_VALUE: &str = "wp4a-secret-value-should-never-be-exported";

fn seed_profile() -> String {
    format!(
        r#"[[agents.profiles]]
agent_id = "codex"
display_name = "Codex (seed)"
command = "{command}"
args = []
env_allowlist = ["OPENAI_API_KEY"]
default = true
"#,
        command = env!("CARGO_BIN_EXE_acp-remote").replace('\\', "/"),
    )
}

/// 端到端：真实 SQLite 审计行 → 真实文件，`recordCount` 与 `sha256` 必须与文件字节一致。
#[test]
fn audit_export_writes_the_real_file_and_reports_matching_count_and_digest() {
    let mut daemon = Daemon::configure("audit-export", &seed_profile());
    daemon.start();

    // 产生两类审计行：provider.configured（含 keystore 写入）与 export.created。
    daemon.ok(
        Method::ProviderConfigure,
        params_of(json!({
            "providerId": "openai",
            "kind": "provider",
            "displayName": "OpenAI",
            "values": { "api_key": CREDENTIAL_VALUE },
        })),
    );
    let workspace_root = daemon.root().join("workspace");
    std::fs::create_dir_all(&workspace_root).expect("工作区目录");
    daemon.ok(
        Method::WorkspaceSelect,
        params_of(json!({
            "alias": "project",
            "displayName": "Project",
            "rootPath": workspace_root.display().to_string(),
        })),
    );
    daemon.ok(
        Method::ExportCreate,
        params_of(json!({
            "exportId": "export-1",
            "displayName": "Export One",
            "agentIds": ["codex"],
            "workspaceAliases": [{ "alias": "project", "displayName": "Project" }],
            "defaultWorkspaceAlias": "project",
            "templates": [{
                "templateId": "template-1",
                "displayName": "Template One",
                "workspaceAlias": "project",
                "params": [],
            }],
            "defaultTemplateId": "template-1",
            "scopes": ["grant.session.read"],
            "cachePolicy": "no-content-cache",
        })),
    );

    let output_path = daemon.root().join("audit-export.jsonl");
    let result = daemon.ok_value(
        Method::AuditExport,
        params_of(json!({
            "outputPath": output_path.display().to_string(),
            "format": "jsonl",
            "since": null,
            "until": null,
            "categories": [],
        })),
    );

    // 文件是真实 SQLite 写的记录导出（不是内存快照）。
    assert_eq!(
        result["outputPath"],
        json!(output_path.display().to_string())
    );
    let bytes = std::fs::read(&output_path).expect("导出文件必须存在");
    let digest = {
        let mut hasher = sha2::Sha256::new();
        hasher.update(&bytes);
        let mut text = String::with_capacity(64);
        for byte in hasher.finalize() {
            text.push_str(&format!("{byte:02x}"));
        }
        text
    };
    assert_eq!(
        result["sha256"],
        json!(digest),
        "sha256 必须是文件字节的摘要"
    );

    let text = String::from_utf8(bytes).expect("UTF-8");
    let lines: Vec<&str> = text.lines().filter(|line| !line.is_empty()).collect();
    assert_eq!(
        result["recordCount"].as_u64().expect("recordCount"),
        lines.len() as u64,
        "recordCount 必须等于文件行数"
    );
    assert!(lines.len() >= 2, "至少两条审计行：{text}");

    // 每行都是合法 JSON 对象，含文档化的字段，且正文/凭据一概不出现。
    let mut actions: Vec<String> = Vec::new();
    for line in &lines {
        let record: Value = serde_json::from_str(line).expect("每行都是合法 JSON");
        actions.push(
            record["action"]
                .as_str()
                .unwrap_or_else(|| panic!("缺少 action：{line}"))
                .to_owned(),
        );
        assert_eq!(record["actorKind"], json!("cli"));
        assert_eq!(record["actorId"], json!("cli"));
        assert_eq!(record["outcome"], json!("success"));
        assert!(record["targetId"].is_string(), "{line}");
    }
    assert!(
        actions.contains(&"provider.configured".to_owned()),
        "{actions:?}"
    );
    assert!(
        actions.contains(&"export.created".to_owned()),
        "{actions:?}"
    );
    assert!(
        !text.contains(CREDENTIAL_VALUE),
        "导出文件不得包含凭据值：{text}"
    );
    assert!(
        !text.contains("OPENAI_API_KEY"),
        "导出文件不得包含凭据字段名之外的秘密材料：{text}"
    );

    // §5.6：同一路径第二次导出必须是 `local.conflict`（不覆盖已有文件）。
    let code = daemon.error_code(
        Method::AuditExport,
        params_of(json!({
            "outputPath": output_path.display().to_string(),
            "format": "jsonl",
            "since": null,
            "until": null,
            "categories": [],
        })),
    );
    assert_eq!(code, "local.conflict");
    assert_eq!(
        std::fs::read(&output_path).expect("文件仍在"),
        text.as_bytes(),
        "冲突不得改写已有文件"
    );

    // 类别过滤走的是同一份持久记录（`categories` 只保留指定类别）。
    let filtered_path = daemon.root().join("audit-export-filtered.jsonl");
    let filtered = daemon.ok_value(
        Method::AuditExport,
        params_of(json!({
            "outputPath": filtered_path.display().to_string(),
            "format": "csv",
            "since": null,
            "until": null,
            "categories": ["provider.configured"],
        })),
    );
    assert_eq!(filtered["recordCount"], json!(1));
    let csv = std::fs::read_to_string(&filtered_path).expect("CSV 导出存在");
    assert!(csv.contains("provider.configured"), "{csv}");
    assert!(!csv.contains("export.created"), "{csv}");

    assert!(daemon.stop().success());
}
