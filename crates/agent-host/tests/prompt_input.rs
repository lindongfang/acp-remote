//! `session/prompt` 的 ACP 输入映射：媒体、资源、路径引用与预算/能力门控。
//!
//! 用真实 fake ACP 子进程记录它实际收到的 `session/prompt` 参数；附件字节由测试里的 fake
//! `AttachmentStore` 提供（真实实现属于组合根）。断言重点是「发出去的形状」与「拒绝时一个字节都不发」。

mod support;

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use acp_core::model::{
    AgentId, AgentRef, AttachmentGeneration, AttachmentId, CreateSessionRequest, Digest,
    PromptContentBlock, PromptRequest, ResourceOrigin, SessionId, Timestamp,
};
use acp_core::ports::{
    AttachmentRef, AttachmentStore, PruneReport, SessionBackendFactory, SessionEndpoint,
};
use agent_host::{AgentHost, HostConfig};
use base64::Engine as _;
use serde_json::{Value, json};
use support::{
    Collector, FAKE_AGENT, TempFile, TestClock, TestIds, digest_of, params_of, profile_with,
    wait_for_dumped_method,
};

const AGENT: &str = "agent-1";
const SESSION: &str = "11111111-1111-4111-8111-111111111111";
const IMAGE: &str = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
const AUDIO: &str = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
const FILE: &str = "cccccccc-cccc-4ccc-8ccc-cccccccccccc";
const DUMP_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Default)]
struct FakeAttachments {
    bytes: Mutex<HashMap<String, Vec<u8>>>,
    reads: AtomicUsize,
}

impl FakeAttachments {
    fn with(self: &Arc<Self>, id: &str, bytes: &[u8]) {
        self.bytes
            .lock()
            .expect("锁")
            .insert(id.to_owned(), bytes.to_vec());
    }

    fn reads(&self) -> usize {
        self.reads.load(Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl AttachmentStore for FakeAttachments {
    async fn put(
        &self,
        _bytes: &[u8],
        _media_type: &str,
        _at: Timestamp,
    ) -> Result<AttachmentRef, acp_core::model::PortError> {
        unimplemented!("本用例只读取附件")
    }

    async fn get(&self, id: &AttachmentId) -> Result<Option<Vec<u8>>, acp_core::model::PortError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        Ok(self.bytes.lock().expect("锁").get(id.as_str()).cloned())
    }

    async fn link(
        &self,
        _session: &SessionId,
        _attachment: &AttachmentId,
        _generation: AttachmentGeneration,
        _at: Timestamp,
    ) -> Result<(), acp_core::model::PortError> {
        unimplemented!("本用例只读取附件")
    }

    async fn prune_lru(
        &self,
        _budget_bytes: u64,
        _at: Timestamp,
    ) -> Result<PruneReport, acp_core::model::PortError> {
        unimplemented!("本用例只读取附件")
    }

    async fn sweep_orphans(
        &self,
        _at: Timestamp,
        _limit: u32,
    ) -> Result<u32, acp_core::model::PortError> {
        unimplemented!("本用例只读取附件")
    }
}

fn host(args: &[&str], attachments: Option<Arc<FakeAttachments>>) -> Arc<AgentHost> {
    let host = AgentHost::new(
        Arc::new(vec![profile_with(AGENT, FAKE_AGENT, args)]),
        HostConfig::default(),
        Arc::new(TestIds::new()),
        Arc::new(TestClock::new()),
    );
    let host = match attachments {
        Some(attachments) => host.with_attachments(attachments),
        None => host,
    };
    Arc::new(host)
}

fn agent_ref() -> AgentRef {
    AgentRef::try_new(AgentId::new(AGENT).expect("id"), "Agent").expect("ref")
}

fn session() -> SessionId {
    SessionId::new(SESSION).expect("session id")
}

async fn create(host: &Arc<AgentHost>, collector: &Collector) -> Box<dyn SessionEndpoint> {
    host.create(
        &session(),
        CreateSessionRequest::new(
            agent_ref(),
            Some(support::workspace()),
            None,
            ResourceOrigin::Local,
        ),
        collector.sink(),
    )
    .await
    .expect("create")
}

fn block(value: Value) -> PromptContentBlock {
    PromptContentBlock::from_json_text(&value.to_string()).expect("block")
}

fn attachment_block(
    id: &str,
    media_type: &str,
    bytes: &[u8],
    digest: Option<&Digest>,
) -> PromptContentBlock {
    block(json!({
        "type": "attachment_ref",
        "attachmentId": id,
        "mediaType": media_type,
        "byteLength": bytes.len().to_string(),
        "sha256": digest.unwrap_or(&digest_of("")).as_str(),
    }))
}

fn prompt(blocks: Vec<PromptContentBlock>) -> PromptRequest {
    PromptRequest::new(blocks)
}

fn prompt_params(rows: &[Value]) -> Option<Value> {
    rows.iter()
        .find(|row| row["method"] == "session/prompt")
        .map(|row| row["params"].clone())
}

/// 每个用例**独占**一个 dump 文件：并行运行时不能共用 `acpr-prompt-<pid>.jsonl`
/// （三个用例会互相看到对方的 `session/prompt`，断言与轮询都会串台）。
fn dumped(tag: &str) -> TempFile {
    TempFile::new(&format!("acpr-prompt-{}-{tag}.jsonl", std::process::id()))
}

const ALL_CAPABILITIES: &str =
    r#"{"promptCapabilities":{"image":true,"audio":true,"embeddedContext":true}}"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn media_resources_and_workspace_paths_map_to_native_acp_blocks() {
    let dump = dumped("media");
    let mut args = vec![
        "--scenario",
        "normal",
        "--capabilities",
        ALL_CAPABILITIES,
        "--dump-request-params",
    ];
    let dump_text = dump.to_string_lossy().into_owned();
    args.push(&dump_text);
    let attachments = Arc::new(FakeAttachments::default());
    let image_bytes = b"png-image-bytes";
    let audio_bytes = b"RIFF....WAVEfmt";
    let file_bytes = b"%PDF-1.7 file";
    attachments.with(IMAGE, image_bytes);
    attachments.with(AUDIO, audio_bytes);
    attachments.with(FILE, file_bytes);
    let host = host(&args, Some(Arc::clone(&attachments)));
    let collector = Collector::new();
    let endpoint = create(&host, &collector).await;

    let image_digest = digest_of("png-image-bytes");
    let image = attachment_block(IMAGE, "image/png", image_bytes, Some(&image_digest));
    let digest = digest_of("RIFF....WAVEfmt");
    let audio = attachment_block(AUDIO, "audio/wav", audio_bytes, Some(&digest));
    let digest = digest_of("%PDF-1.7 file");
    let file = attachment_block(FILE, "application/pdf", file_bytes, Some(&digest));
    endpoint
        .prompt(
            prompt(vec![
                block(json!({ "type": "text", "text": "看一下这些" })),
                image,
                audio,
                file,
                block(json!({ "type": "workspace_path_ref", "path": "src/main.rs" })),
            ]),
            support::timestamp(),
        )
        .await
        .expect("prompt 可派发");
    assert!(
        collector
            .wait_for_type("turn.completed", Duration::from_secs(20))
            .await,
        "{:?}",
        collector.event_types()
    );

    let rows = wait_for_dumped_method(&dump, "session/prompt", DUMP_TIMEOUT).await;
    let params = params_of(&rows, "session/prompt");
    let blocks = params["prompt"].as_array().expect("prompt 数组");
    assert_eq!(blocks.len(), 5, "五个输入块必须整条映射：{params}");
    assert_eq!(blocks[0], json!({ "type": "text", "text": "看一下这些" }));

    assert_eq!(blocks[1]["type"], "image");
    assert_eq!(blocks[1]["mimeType"], "image/png");
    assert_eq!(
        blocks[1]["data"],
        json!(base64::engine::general_purpose::STANDARD.encode(image_bytes))
    );

    assert_eq!(blocks[2]["type"], "audio");
    assert_eq!(blocks[2]["mimeType"], "audio/wav");
    assert_eq!(
        blocks[2]["data"],
        json!(base64::engine::general_purpose::STANDARD.encode(audio_bytes))
    );

    assert_eq!(blocks[3]["type"], "resource");
    assert_eq!(blocks[3]["resource"]["mimeType"], "application/pdf");
    assert_eq!(
        blocks[3]["resource"]["blob"],
        json!(base64::engine::general_purpose::STANDARD.encode(file_bytes))
    );
    assert!(
        blocks[3]["resource"]["uri"]
            .as_str()
            .expect("uri")
            .starts_with("urn:acp-remote:attachment:"),
        "内嵌资源只带本机附件标识，不是可下载地址：{}",
        blocks[3]
    );

    // 工作目录引用只传相对路径文本：不读文件、不携带内容。
    assert_eq!(blocks[4], json!({ "type": "text", "text": "src/main.rs" }));

    // 适配器内部标识（attachment_ref/workspace_path_ref）不得泄漏给 Agent。
    let text = params.to_string();
    assert!(!text.contains("attachment_ref"), "{text}");
    assert!(!text.contains("workspace_path_ref"), "{text}");
    assert_eq!(attachments.reads(), 3, "每个附件只读取一次");

    endpoint.close().await.expect("close");
    host.shutdown_all().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unsupported_media_rejects_the_whole_prompt_before_sending() {
    let dump = dumped("unsupported");
    let mut args = vec!["--scenario", "normal", "--dump-request-params"];
    let dump_text = dump.to_string_lossy().into_owned();
    args.push(&dump_text);
    let attachments = Arc::new(FakeAttachments::default());
    attachments.with(IMAGE, b"png");
    let host = host(&args, Some(Arc::clone(&attachments)));
    let collector = Collector::new();
    let endpoint = create(&host, &collector).await;

    let error = endpoint
        .prompt(
            prompt(vec![
                block(json!({ "type": "text", "text": "带图" })),
                attachment_block(IMAGE, "image/png", b"png", None),
            ]),
            support::timestamp(),
        )
        .await
        .expect_err("Agent 未宣告 image 时必须整条拒绝");
    assert!(matches!(
        error,
        acp_core::model::PortError::Unavailable(
            acp_core::model::UnavailableKind::BackendUnsupported
        )
    ));
    assert_eq!(attachments.reads(), 0, "未宣告的能力不该读取附件字节");
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(
        prompt_params(&support::dumped_requests(&dump)).is_none(),
        "被拒绝的 prompt 一个字节都不能发给 Agent"
    );

    endpoint.close().await.expect("close");
    host.shutdown_all().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn oversized_and_escaping_inputs_are_rejected_before_dispatch() {
    let dump = dumped("oversized");
    let mut args = vec![
        "--scenario",
        "normal",
        "--capabilities",
        ALL_CAPABILITIES,
        "--dump-request-params",
    ];
    let dump_text = dump.to_string_lossy().into_owned();
    args.push(&dump_text);
    let attachments = Arc::new(FakeAttachments::default());
    // 20 MiB 的声明长度：base64 后约 27 MiB，远超 1 MiB 的 ACP 帧预算。
    attachments.with(IMAGE, b"tiny");
    let host = host(&args, Some(Arc::clone(&attachments)));
    let collector = Collector::new();
    let endpoint = create(&host, &collector).await;

    let oversized = block(json!({
        "type": "attachment_ref",
        "attachmentId": IMAGE,
        "mediaType": "image/png",
        "byteLength": (20 * 1024 * 1024).to_string(),
        "sha256": digest_of("tiny").as_str(),
    }));
    assert!(
        endpoint
            .prompt(prompt(vec![oversized]), support::timestamp())
            .await
            .is_err(),
        "超出 ACP 帧预算必须整条拒绝"
    );
    assert_eq!(attachments.reads(), 0, "预算预估必须在读取字节之前失败");

    // 路径逃逸：`..` 分量在形状阶段就被拒绝（不触碰文件系统）。
    assert!(
        endpoint
            .prompt(
                prompt(vec![block(json!({
                    "type": "workspace_path_ref",
                    "path": "../escape.txt",
                }))]),
                support::timestamp(),
            )
            .await
            .is_err(),
        "工作目录引用不得逃逸"
    );

    // 摘要不符：即使能读，也不能把另一个文件冒充成用户选择的内容。
    let wrong = block(json!({
        "type": "attachment_ref",
        "attachmentId": IMAGE,
        "mediaType": "image/png",
        "byteLength": "4",
        "sha256": digest_of("other").as_str(),
    }));
    assert!(
        endpoint
            .prompt(prompt(vec![wrong]), support::timestamp())
            .await
            .is_err(),
        "摘要不符必须整条拒绝"
    );

    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(
        prompt_params(&support::dumped_requests(&dump)).is_none(),
        "三次拒绝都不得向 Agent 发出 session/prompt"
    );

    endpoint.close().await.expect("close");
    host.shutdown_all().await;
}
