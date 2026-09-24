//! 测试共用辅助：定位仓库根、读取 fixture 与矩阵。
//!
//! fixture 与矩阵是**仓库级合同资产**，Rust 侧只读不复制：新增用例必须登记进
//! `fixtures/acp/v1/manifest.json` 与 `compatibility/acp/v1/matrix.json`（`AGENTS.md` §10）。
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use serde_json::Value;

/// 仓库根下的路径。集成测试的 cwd 不保证是仓库根，因此一律用 `CARGO_MANIFEST_DIR` 定位。
pub fn repo_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join(relative)
}

/// 读取原始字节（保真类断言必须比较字节，不能先 parse 再序列化）。
pub fn read_bytes(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|error| panic!("无法读取 {}：{error}", path.display()))
}

/// 读取 JSON。
pub fn read_json(relative: &str) -> Value {
    let path = repo_path(relative);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("无法读取 {}：{error}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|error| panic!("无法解析 {}：{error}", path.display()))
}

/// ACP 兼容性矩阵。
pub fn matrix() -> Value {
    read_json("compatibility/acp/v1/matrix.json")
}

/// `fixtures/acp/v1/manifest.json` 的一条用例。
#[derive(Debug, Clone)]
pub struct ManifestCase {
    pub fixture: String,
    pub valid: bool,
    pub schema_pointer: Option<String>,
    pub expected_keyword: Option<String>,
}

/// manifests 的用例列表。
pub fn manifest_cases() -> Vec<ManifestCase> {
    read_json("fixtures/acp/v1/manifest.json")["cases"]
        .as_array()
        .expect("manifest.cases")
        .iter()
        .map(|case| ManifestCase {
            fixture: case["fixture"].as_str().expect("fixture").to_owned(),
            valid: case["valid"].as_bool().expect("valid"),
            schema_pointer: case["schemaPointer"].as_str().map(str::to_owned),
            expected_keyword: case["expectedKeyword"].as_str().map(str::to_owned),
        })
        .collect()
}

/// fixture 的字节。
pub fn fixture_bytes(name: &str) -> Vec<u8> {
    read_bytes(&repo_path(&format!("fixtures/acp/v1/{name}")))
}
