//! `SYNC_PROTOCOL.md` §10.3 的跨 crate 一致性：适配器只产 ACP 派生字段，`turnId`/`version` 由 core 注入。
//!
//! 本文件用**真实适配器**（`agent-host` 的 `SessionEndpoint` 驱动 fake ACP 子进程）收集它实际发出的
//! `EndpointEvent`，并断言两件事：
//!
//! 1. §10.3 要求 `turnId`/`version` 的事件类型，适配器产出的 view **不含**这两个字段（因此 core 的注入
//!    是必需的，`crates/core` 的用例覆盖注入本身）；
//! 2. 同一 view 携带 §10.3 要求的其余最低字段——即 `适配器字段 ∪ {turnId}`（以及两个类型再加 `version`）
//!    满足 §10.3，字段名与 `docs/SYNC_PROTOCOL.md` §10.3 的表格逐项对应。
//!
//! 本文件不修改 `agent-host` 的行为实现，也不复制 core 的注入逻辑。

mod support;

use std::time::Duration;

use acp_core::model::EndpointEvent;
use acp_core::ports::SessionBackendFactory;
use agent_host::{AgentHost, HostConfig};
use serde_json::Value;
use support::{
    Collector, FAKE_AGENT, FakeCredentials, TestClock, TestIds, profile_with, timestamp,
};

/// §10.3 要求 `turnId`、且**由适配器产出**的事件类型 → 除 `turnId` 外的最低字段。
const TURN_SCOPED: &[(&str, &[&str])] = &[
    ("turn.completed", &["state"]),
    ("turn.cancelled", &["state"]),
    ("turn.failed", &["state", "error"]),
    ("user.message.delta", &["messageId", "deltaIndex", "text"]),
    ("agent.message.delta", &["messageId", "deltaIndex", "text"]),
    ("agent.thought.delta", &["messageId", "deltaIndex", "text"]),
    ("tool.call.started", &["toolCallId", "title", "state"]),
    ("tool.call.updated", &["toolCallId", "title", "state"]),
    ("tool.call.completed", &["toolCallId", "title", "state"]),
    (
        "permission.requested",
        &["interactionId", "title", "description", "options"],
    ),
    (
        "elicitation.requested",
        &["interactionId", "title", "schema", "initialValues"],
    ),
];

/// §10.3 要求 `version`、且由适配器产出的事件类型 → 除 `version` 外的最低字段。
const VERSION_SCOPED: &[(&str, &[&str])] = &[
    ("session.mode.changed", &["currentModeId"]),
    ("session.config.changed", &["configOptions"]),
];

/// §10.3 要求 `turnId`、但**只由 core 生成**的类型（适配器不得成为第二个生产者）。
const CORE_ONLY: &[&str] = &[
    "turn.queued",
    "turn.started",
    "turn.delta_compacted",
    "agent.message.completed",
];

/// §10.3 要求字段、但**本用例的三个场景不会产出**的类型（fake ACP 不发用户 chunk，也不驱动 cancel）。
/// 它们必须与本用例实际检查过的类型合起来恰好等于两张表（下面的集合相等断言），因此任何类型从
/// 「已检查」里滑进这里（或凭空消失）都会让用例变红。
const NOT_EXERCISED: &[&str] = &["user.message.delta", "turn.cancelled"];

fn host(scenario: &str) -> std::sync::Arc<AgentHost> {
    std::sync::Arc::new(AgentHost::new(
        std::sync::Arc::new(support::FakeConfig::new(vec![profile_with(
            "agent-1",
            FAKE_AGENT,
            &["--scenario", scenario],
        )])),
        std::sync::Arc::new(FakeCredentials::ok()),
        HostConfig::default(),
        std::sync::Arc::new(TestIds::new()),
        std::sync::Arc::new(TestClock::new()),
    ))
}

fn agent_ref() -> acp_core::model::AgentRef {
    acp_core::model::AgentRef::try_new(
        acp_core::model::AgentId::new("agent-1").expect("agent id"),
        "Agent agent-1",
    )
    .expect("agent ref")
}

fn prompt(text: &str) -> acp_core::model::PromptRequest {
    acp_core::model::PromptRequest::new(vec![
        acp_core::model::PromptContentBlock::from_json_text(&format!(
            "{{\"type\":\"text\",\"text\":\"{text}\"}}"
        ))
        .expect("block"),
    ])
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn adapter_views_leave_turn_and_version_identity_to_core() {
    let mut collected: Vec<EndpointEvent> = Vec::new();
    for (index, (scenario, wait_for)) in [
        ("chunked-updates", "turn.completed"),
        ("permission-request", "permission.requested"),
        ("elicitation", "elicitation.requested"),
        ("crash-on-prompt", "turn.failed"),
    ]
    .into_iter()
    .enumerate()
    {
        let session =
            acp_core::model::SessionId::new(&format!("00000000-0000-4000-8000-{:012}", index + 1))
                .expect("session id");
        let collector = Collector::new();
        let host = host(scenario);
        let endpoint = host
            .create(
                &session,
                acp_core::model::CreateSessionRequest::new(
                    agent_ref(),
                    Some(support::workspace()),
                    None,
                    acp_core::model::ResourceOrigin::Local,
                ),
                collector.sink(),
            )
            .await
            .expect("create session");
        endpoint
            .prompt(prompt("你好"), timestamp())
            .await
            .expect("prompt");
        assert!(
            collector
                .wait_for_type(wait_for, Duration::from_secs(10))
                .await,
            "{scenario} 必须在超时前产生 {wait_for}：{:?}",
            collector.event_types()
        );
        collected.extend(collector.snapshot());
        drop(endpoint);
        host.shutdown_all().await;
    }

    let types: Vec<String> = collected
        .iter()
        .map(|event| event.event_type.as_str().to_owned())
        .collect();
    let mut checked: Vec<&str> = Vec::new();
    for (event_type, required) in TURN_SCOPED.iter().chain(VERSION_SCOPED.iter()) {
        let events: Vec<&EndpointEvent> = collected
            .iter()
            .filter(|event| event.event_type.as_str() == *event_type)
            .collect();
        if events.is_empty() {
            continue;
        }
        for event in events {
            let view: Value =
                serde_json::from_str(event.payload.view.as_str()).expect("view 是 JSON 对象");
            assert!(
                event.turn.is_none(),
                "{event_type}: EndpointEvent.turn 必须由 core 定稿"
            );
            assert!(
                view.get("turnId").is_none(),
                "{event_type}: 适配器不得自带 turnId（core 注入）"
            );
            assert!(
                view.get("version").is_none(),
                "{event_type}: 适配器不得自带 version（core 注入）"
            );
            for field in *required {
                assert!(
                    view.get(*field).is_some(),
                    "{event_type}: 缺少 §10.3 最低字段 {field}：{}",
                    event.payload.view.as_str()
                );
            }
        }
        checked.push(event_type);
    }

    // 让断言可证伪：本用例检查过的类型必须与「两张表 − 显式豁免」集合相等。
    let mut expected: Vec<String> = TURN_SCOPED
        .iter()
        .chain(VERSION_SCOPED.iter())
        .map(|(event_type, _)| (*event_type).to_owned())
        .filter(|event_type| !NOT_EXERCISED.contains(&event_type.as_str()))
        .collect();
    expected.sort();
    let mut checked_sorted: Vec<String> = checked.iter().map(|kind| (*kind).to_owned()).collect();
    checked_sorted.sort();
    assert_eq!(
        checked_sorted, expected,
        "每个 §10.3 类型要么被真实检查到、要么列入 NOT_EXERCISED；产物类型清单：{types:?}"
    );
    // 只由 core 生成的类型不得出现在适配器输出里（避免双生产者导致字段来源不清）。
    for core_only in CORE_ONLY {
        assert!(
            !types.iter().any(|kind| kind == core_only),
            "适配器不得自己生成 {core_only}：{types:?}"
        );
    }
}
