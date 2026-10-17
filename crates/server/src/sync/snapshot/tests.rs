//! `server::sync::snapshot` 的行为用例（变更 `sync-server-connection` 的任务 4.4/4.5/4.6）。
//!
//! 驱动方式是**真实出站队列**：`snapshot::plan` 组装、`snapshot::send` 逐帧入队，用例只看对端真的收到的
//! 那几帧文本（`fixture::frames`）。因此"`chunkCount` 等于实发块数""摘要覆盖原始帧字节""明细资源不出现"
//! 都是对**将发出的字节**的断言，而不是对内部结构的断言。

use super::*;
use crate::sync::subscribe::NegotiatedFeatures;
use crate::sync::subscribe::fixture::{self, DEVICE, EPOCH, Frame, REMOTE_SESSION, SESSION, World};
use acp_core::model::Actor;

/// 明细资源名（六类 session-scoped 明细）：快照里**一个都不许出现**（§9.4）。
const DETAIL_RESOURCES: [&str; 6] = [
    "messages",
    "turns",
    "pending_interactions",
    "config_options",
    "capabilities",
    "history",
];

/// 组装并下发一次快照，返回对端收到的全部帧。
///
/// 设备 actor 只由标识构成（设备方向没有授权集合），因此没有 scopes 参数。
async fn snapshot(world: &World, features: &[&str]) -> Vec<Frame> {
    let (handle, mut receiver) = fixture::connection(DEVICE);
    let actor = Actor::Device {
        device: acp_core::model::DeviceId::new(DEVICE).expect("device id"),
    };
    let negotiated = NegotiatedFeatures::new(features.iter().map(|f| (*f).to_owned()).collect());
    let reachable = |_: &RemoteSessionRef| true;
    let inputs = SnapshotInputs {
        actor: &actor,
        features: &negotiated,
        reachable: &reachable,
    };
    let cursor = fixture::cursor(EPOCH, 7);
    let plan = plan(&world.core, inputs, &cursor)
        .await
        .expect("快照可组装");
    let sent = send(&handle, &plan).expect("快照可下发");
    let frames = fixture::frames(&mut receiver);
    assert_eq!(
        sent.chunk_count as usize,
        fixture::bodies(&frames, "sync.snapshot_chunk").len(),
        "chunkCount 必须等于实发块数"
    );
    frames
}

/// 预置一个"两类来源 + 目录资源"的世界。
fn world_with_catalog() -> World {
    let world = World::new();
    let directory_id = world.seed_directory(&fixture::workspace_path());
    world.seed_agent("codex", "Codex", true);
    world.store.push_session(fixture::core_session(
        SESSION,
        "本地会话",
        Some(directory_id.as_str()),
    ));
    world
        .deliveries
        .push(fixture::SyncDeliveries::record("远端会话"));
    world
}

/// 按资源取同名 chunk 的 `items`。
fn items<'a>(frames: &'a [Frame], resource: &str) -> Option<&'a serde_json::Value> {
    frames
        .iter()
        .find(|frame| {
            frame.kind == "sync.snapshot_chunk"
                && frame
                    .body
                    .get("resource")
                    .and_then(serde_json::Value::as_str)
                    == Some(resource)
        })
        .and_then(|frame| frame.body.get("items"))
}

#[tokio::test]
async fn a_full_snapshot_has_three_chunks_in_index_order() {
    let world = world_with_catalog();
    let frames = snapshot(&world, &[LOCAL_CATALOG_FEATURE, REMOTE_ORIGIN_FEATURE]).await;
    let begin = fixture::bodies(&frames, "sync.snapshot_begin");
    assert_eq!(begin.len(), 1, "先发 begin");
    assert_eq!(begin[0]["chunkCount"], 3, "协商目录 feature 时是三个资源");
    assert_eq!(begin[0]["schemaVersion"], 1);
    let chunks = fixture::bodies(&frames, "sync.snapshot_chunk");
    assert_eq!(chunks.len(), 3);
    for (index, chunk) in chunks.iter().enumerate() {
        assert_eq!(
            chunk["chunkIndex"],
            index.to_string(),
            "chunkIndex 从 0 起连续"
        );
    }
    let resources: Vec<&str> = chunks
        .iter()
        .map(|chunk| chunk["resource"].as_str().expect("resource"))
        .collect();
    assert_eq!(resources, ["sessions", "workspaces", "agents"], "顺序固定");
    let workspace_id = items(&frames, "workspaces").expect("workspaces")[0]
        ["directoryId"]
        .clone();
    assert!(workspace_id.is_string());
    assert_eq!(
        items(&frames, "agents").expect("agents")[0]["agentId"],
        "codex"
    );
    assert_eq!(
        items(&frames, "agents").expect("agents")[0]["default"],
        true
    );

    // 两类来源共存：自有条目在前、imported 在后，且 imported 携带来源标识与可达性（§9.6）。
    let sessions = items(&frames, "sessions").expect("sessions");
    assert_eq!(sessions.as_array().expect("数组").len(), 2);
    assert_eq!(sessions[0]["sessionId"], SESSION);
    assert_eq!(sessions[0]["origin"]["kind"], "local");
    assert_eq!(sessions[1]["sessionId"], REMOTE_SESSION);
    assert_eq!(sessions[1]["origin"]["kind"], "remote");
    assert_eq!(sessions[1]["origin"]["exportId"], fixture::EXPORT);
    assert_eq!(sessions[1]["origin"]["online"], true);
    assert_eq!(
        sessions[1]["workspace"],
        serde_json::Value::Null,
        "imported 不猜归属"
    );

    let end = fixture::bodies(&frames, "sync.snapshot_end");
    assert_eq!(end.len(), 1, "最后发 end");
    assert_eq!(end[0]["chunkCount"], 3, "begin/end 的 chunkCount 必须一致");
    assert_eq!(
        end[0]["cursor"]["globalSequence"], begin[0]["cursor"]["globalSequence"],
        "begin/end 携带同一快照游标"
    );
}

#[tokio::test]
async fn without_the_catalog_feature_only_sessions_are_sent() {
    let world = world_with_catalog();
    let frames = snapshot(&world, &[REMOTE_ORIGIN_FEATURE]).await;
    assert_eq!(
        fixture::bodies(&frames, "sync.snapshot_begin")[0]["chunkCount"],
        1
    );
    assert_eq!(fixture::bodies(&frames, "sync.snapshot_chunk").len(), 1);
    assert!(
        items(&frames, "workspaces").is_none(),
        "未协商时目录资源整个缺席"
    );
    assert!(items(&frames, "agents").is_none());
    for frame in &frames {
        assert!(
            !frame.text.contains("workspaces") && !frame.text.contains("agents"),
            "未协商目录 feature 时不得以空数组或任何字段占位：{}",
            frame.text
        );
    }
}

#[tokio::test]
async fn an_empty_session_set_still_sends_one_chunk() {
    // 未协商目录 feature 的设备：`chunkCount` 只能是 1——空会话集合也照发一个空数组
    // （`chunkCount: 0` 是形状错误）。
    let world = World::new();
    let frames = snapshot(&world, &[]).await;
    let chunks = fixture::bodies(&frames, "sync.snapshot_chunk");
    assert_eq!(
        chunks.len(),
        1,
        "空会话集合仍发一块（chunkCount: 0 是形状错误）"
    );
    assert_eq!(chunks[0]["resource"], "sessions");
    assert_eq!(chunks[0]["items"].as_array().expect("数组").len(), 0);
    assert_eq!(
        fixture::bodies(&frames, "sync.snapshot_begin")[0]["chunkCount"],
        1
    );

    // 协商了目录 feature 时是 3 块：目录资源**是空数组**（目录为空仍然下发），与会话是否为空无关。
    let catalog = snapshot(&world, &[LOCAL_CATALOG_FEATURE]).await;
    assert_eq!(fixture::bodies(&catalog, "sync.snapshot_chunk").len(), 3);
    assert_eq!(
        items(&catalog, "workspaces")
            .expect("workspaces")
            .as_array()
            .expect("数组")
            .len(),
        0
    );
}

#[tokio::test]
async fn detail_resources_never_reach_the_snapshot() {
    let world = world_with_catalog();
    let frames = snapshot(&world, &[LOCAL_CATALOG_FEATURE, REMOTE_ORIGIN_FEATURE]).await;
    for frame in &frames {
        if frame.kind != "sync.snapshot_chunk" {
            continue;
        }
        let resource = frame.body["resource"].as_str().expect("resource");
        assert!(
            ["sessions", "workspaces", "agents"].contains(&resource),
            "只允许三类清单资源，得到 {resource}"
        );
        for detail in DETAIL_RESOURCES {
            assert!(
                !frame.text.contains(detail),
                "明细资源 {detail} 不得进入快照：{}",
                frame.text
            );
        }
    }
}

#[tokio::test]
async fn the_digest_covers_the_exact_frame_bytes() {
    let world = world_with_catalog();
    let frames = snapshot(&world, &[LOCAL_CATALOG_FEATURE]).await;
    let chunk_texts: Vec<&str> = frames
        .iter()
        .filter(|frame| frame.kind == "sync.snapshot_chunk")
        .map(|frame| frame.text.as_str())
        .collect();
    let declared = fixture::bodies(&frames, "sync.snapshot_end")[0]["snapshotDigest"]
        .as_str()
        .expect("snapshotDigest")
        .to_owned();

    // 客户端侧复算：逐 chunk 对收到的**原始 UTF-8 文本**求 SHA-256，按 index 连接后再求一次。
    let recomputed = digest_of_texts(&chunk_texts);
    assert_eq!(recomputed, declared, "按收到的帧文本复算必须逐字节相同");

    // 反例一：任何字节差异（例如重新序列化引入的空白/键序变化）都会让摘要不一致。
    let mutated: Vec<String> = chunk_texts
        .iter()
        .map(|text| text.replacen("{\"snapshotId\"", "{ \"snapshotId\"", 1))
        .collect();
    let mutated: Vec<&str> = mutated.iter().map(String::as_str).collect();
    assert_ne!(
        digest_of_texts(&mutated),
        declared,
        "摘要必须覆盖将要发出的那几份字节，而不是等价的 JSON 语义"
    );

    // 反例二：同一份资源再发一次会得到新的 messageId/出站序号，摘要因此不同——这就是"重新序列化不能
    // 替代原帧字节"在服务端一侧的表现。
    let again = snapshot(&world, &[LOCAL_CATALOG_FEATURE]).await;
    let again_declared = fixture::bodies(&again, "sync.snapshot_end")[0]["snapshotDigest"]
        .as_str()
        .expect("snapshotDigest");
    assert_ne!(again_declared, declared, "不同帧文本 ⇒ 不同摘要");
}

#[tokio::test]
async fn imported_sessions_follow_the_remote_origin_gate() {
    let world = world_with_catalog();
    let gated = snapshot(&world, &[REMOTE_ORIGIN_FEATURE]).await;
    assert_eq!(
        items(&gated, "sessions")
            .expect("sessions")
            .as_array()
            .expect("数组")
            .len(),
        2
    );

    let ungated = snapshot(&world, &[]).await;
    let sessions = items(&ungated, "sessions").expect("sessions");
    assert_eq!(
        sessions.as_array().expect("数组").len(),
        1,
        "未协商来源 feature 时外来会话被过滤"
    );
    assert_eq!(sessions[0]["origin"]["kind"], "local");
    for frame in &ungated {
        assert!(
            !frame.text.contains(REMOTE_SESSION) && !frame.text.contains("remote"),
            "不得出现任何来源为其它节点的会话或占位条目：{}",
            frame.text
        );
    }
}

#[tokio::test]
async fn the_workspace_key_has_three_states() {
    let world = world_with_catalog();
    world.store.push_session(fixture::core_session(
        "7d73cd10-a465-43cd-b3f1-704e2d49e9aa",
        "未分组",
        None,
    ));

    let grouped = snapshot(&world, &[LOCAL_CATALOG_FEATURE]).await;
    let sessions = items(&grouped, "sessions").expect("sessions");
    let directory_id = items(&grouped, "workspaces").expect("workspaces")[0]["directoryId"].clone();
    assert_eq!(
        sessions[0]["workspace"]["directoryId"], directory_id,
        "归属目录时键在场并带引用"
    );
    assert_eq!(
        sessions[1]["workspace"],
        serde_json::Value::Null,
        "未分组是 null"
    );
    assert!(
        sessions[1].get("workspace").is_some(),
        "协商了目录 feature 时键必须在场"
    );

    let ungated = snapshot(&world, &[]).await;
    let sessions = items(&ungated, "sessions").expect("sessions");
    assert!(
        sessions[0].get("workspace").is_none(),
        "未协商目录 feature 时键必须缺席（发 null 会让旧客户端拒绝整条摘要）"
    );
}

/// 目录资源始终同时携带目录标识与规范化路径（`core.workspace-path.v1` 已撤销）。
#[tokio::test]
async fn the_workspace_catalog_always_carries_ids_and_paths() {
    let world = World::new();
    let path = fixture::workspace_path();
    let directory_id = world.seed_directory(&path);

    let frames = snapshot(&world, &[LOCAL_CATALOG_FEATURE]).await;
    let entry = &items(&frames, "workspaces").expect("workspaces")[0];
    assert_eq!(entry["directoryId"], directory_id.as_str());
    assert_eq!(
        entry["path"],
        serde_json::Value::String(path.clone()),
        "路径必须与标识成对下发：{entry}"
    );
}

/// 设备方向不再有授权过滤：一台**没有任何授权集合**的已认证设备照常读到全部自有会话
/// （`pairing-model` 删除设备权限层；旧实现会以 `authorization.scope_denied` 拒绝）。
#[tokio::test]
async fn a_device_without_any_scope_still_reads_every_session() {
    let world = world_with_catalog();
    let actor = Actor::Device {
        device: acp_core::model::DeviceId::new(DEVICE).expect("device id"),
    };
    let negotiated = NegotiatedFeatures::new(Vec::new());
    let reachable = |_: &RemoteSessionRef| true;
    let inputs = SnapshotInputs {
        actor: &actor,
        features: &negotiated,
        reachable: &reachable,
    };
    let plan = plan(&world.core, inputs, &fixture::cursor(EPOCH, 7))
        .await
        .expect("已认证设备必须能读取会话清单");
    let (handle, mut receiver) = fixture::connection(DEVICE);
    send(&handle, &plan).expect("快照可下发");
    let frames = fixture::frames(&mut receiver);
    assert!(
        !items(&frames, "sessions")
            .expect("sessions 资源")
            .as_array()
            .expect("items 是数组")
            .is_empty(),
        "无授权集合的设备仍必须看到自己的会话"
    );
}

/// 客户端侧摘要复算（与本模块 [`digest`] 相互独立：这里只按收到的文本算）。
fn digest_of_texts(texts: &[&str]) -> String {
    use base64::Engine as _;
    let mut hasher = Sha256::new();
    for text in texts {
        hasher.update(Sha256::digest(text.as_bytes()));
    }
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hasher.finalize())
}
