//! ACPR-CJ1 的跨实现判据：用仓库 fixture 里「同时带 `payload` 与 `payloadDigest`」的条目，判定
//! [`acpr_wire::cj1::canonicalize`] 与 JS 参考实现（`scripts/check-contract-assets.mjs` 的 `acprCj1`）
//! 逐字节一致——`payloadDigest` 就是那条 JS 实现算出来的值。
//!
//! 发现规则与 `checkPayloadDigest` 相同：走遍 `fixtures/sync/v1` 与 `fixtures/node-link/v1` 下的所有
//! `.json`，只看顶层 `body` 上的 `payload` / `payloadDigest`。`payload` 取**文件里的原始文本**（而不是
//! 重新序列化的结果），这样普通 JSON 的空白与键序也一并进入被测路径。

use std::path::{Path, PathBuf};

use acpr_wire::cj1::{canonicalize, canonicalize_bytes};
use serde::Deserialize;
use serde_json::value::RawValue;
use sha2::{Digest, Sha256};

/// 仓库根相对路径 → 绝对路径（不依赖进程 cwd）。
fn repo_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

/// 递归收集 `.json` 文件（与 JS 的 `walkJson` 同一规则）。
fn walk_json(directory: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_json(&path, found);
        } else if path
            .extension()
            .is_some_and(|extension| extension == "json")
        {
            found.push(path);
        }
    }
}

#[derive(Deserialize)]
struct Fixture {
    #[serde(default)]
    body: Option<Body>,
}

#[derive(Deserialize)]
struct Body {
    #[serde(default)]
    payload: Option<Box<RawValue>>,
    #[serde(rename = "payloadDigest", default)]
    payload_digest: Option<String>,
}

#[test]
fn fixture_payloads_canonicalize_to_their_payload_digest() {
    // 覆盖清单：节点链树里目前正好这六条同时带 `payload` 与 `payloadDigest`（sync 树没有）。
    // fixture 增删时这里必须同步——否则覆盖会悄悄变少（与 `check-schema-fixtures.mjs` 的覆盖门禁同旨）。
    let expected = [
        "fixtures/node-link/v1/invalid/resource-event-missing-origin.json",
        "fixtures/node-link/v1/invalid/resource-event-missing-session.json",
        "fixtures/node-link/v1/valid/resource-event-acp-only.json",
        "fixtures/node-link/v1/valid/resource-event-view-agent-message-delta.json",
        "fixtures/node-link/v1/valid/resource-event-view-session-created.json",
        "fixtures/node-link/v1/valid/resource-event.json",
    ];
    let mut covered = Vec::new();
    for root in ["fixtures/sync/v1", "fixtures/node-link/v1"] {
        let base = repo_path(root);
        let mut paths = Vec::new();
        walk_json(&base, &mut paths);
        paths.sort();
        for path in paths {
            let text = std::fs::read_to_string(&path).expect("fixture 必须是 UTF-8");
            let fixture: Fixture = serde_json::from_str(&text).expect("fixture 必须是 JSON");
            let Some(body) = fixture.body else { continue };
            let (Some(payload), Some(expected_digest)) = (body.payload, body.payload_digest) else {
                continue;
            };
            let name = format!(
                "{root}/{}",
                path.strip_prefix(&base)
                    .expect("walk_json 只会产出 base 下的路径")
                    .to_string_lossy()
                    .replace('\\', "/")
            );
            covered.push(name.clone());

            let canonical = canonicalize(payload.get())
                .unwrap_or_else(|error| panic!("{name}: 规范 {name} 的 payload 失败：{error}"));
            let digest: Vec<u8> = Sha256::digest(canonical.as_bytes()).to_vec();
            assert_eq!(
                acpr_transcript::encode_base64url(&digest),
                expected_digest,
                "{name}: payloadDigest 必须等于 base64url(SHA-256(ACPR-CJ1(payload)))"
            );
            // 规范化本身幂等，字节入口与文本入口等价（`payload_json` 以 BLOB 落地）。
            assert_eq!(
                canonicalize(&canonical)
                    .unwrap_or_else(|error| panic!("{name}: 再规范化失败：{error}")),
                canonical,
                "{name}: 已是规范形式的文本必须逐字节不变"
            );
            assert_eq!(
                canonicalize_bytes(canonical.as_bytes())
                    .unwrap_or_else(|error| panic!("{name}: 字节入口失败：{error}")),
                canonical,
                "{name}: 字节入口与文本入口必须一致"
            );
        }
    }
    assert_eq!(covered, expected, "fixture 覆盖集变化时必须同步这份清单");
}
