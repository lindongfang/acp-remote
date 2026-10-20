//! `session/prompt` 的输入映射：Sync `PromptContentBlock` → ACP content block。
//!
//! 三条约束（`openspec/changes/conversation-area-complete/design.md` D4、`ACP_COMPATIBILITY_MATRIX.md`
//! §5 第 3 条）：
//!
//! 1. **只按真实能力派发**：图片要 Agent 宣告 `promptCapabilities.image`、音频要 `audio`、通用文件要
//!    `embeddedContext`。未宣告时**整条拒绝**，不把附件降级成文本、不删掉附件只发文字、不擅自改成
//!    Agent 未宣告可读的路径。
//! 2. **预算先预估、派发前再精确校验**：附件字节按声明长度先算 base64 后的帧内体积，明显超出
//!    `MAX_MESSAGE_BYTES` 的请求在**读取任何字节之前**就被拒绝；编码完成后再对最终 `params` 做一次
//!    精确校验。ACP 单条消息上限是 1 MiB（`SECURITY_DESIGN.md` §12.2），超限的 prompt 一个字节都不发。
//! 3. **工作目录只传路径**：`workspace_path_ref` 只表示当前会话工作目录内的相对路径。本层做形状校验
//!    （无绝对路径、无盘符/ADS 冒号、无 `.`/`..` 分量、无控制字符）与规范化后的前缀关系/链接逃逸校验，
//!    然后**只把这个相对路径作为文本块**交给 Agent（ACP 没有「路径引用」内容类型；工作目录已由
//!    `session/new` 的 `cwd` 确定）。不读取文件、不固定版本、不生成附件快照、不追踪内容变化。
//!    外部链接不会被本层获取：这里不发起任何网络请求。
//!
//! 附件字节来自 [`AttachmentStore`]（core 在受理阶段已校验清单绑定）。本层再次校验长度与 SHA-256，
//! 不一致即整条拒绝——不把「另一个文件」冒充成用户选择的内容。

use std::path::Path;

use acp_core::model::{AttachmentId, Digest, PromptContentBlock, PromptRequest};
use acp_core::ports::AttachmentStore;
use acp_protocol::capability::AgentCapabilities;
use acp_protocol::content::{
    AudioContent, ContentBlock, EmbeddedResource, EmbeddedResourceContents, ImageContent,
    TextContent,
};
use acp_protocol::limits::MAX_MESSAGE_BYTES;
use base64::Engine as _;
use serde_json::{Map, Value};
use sha2::{Digest as _, Sha256};

use crate::error::HostError;

/// 帧内留给信封、方法名、`id` 与 `params` 外层键的字节数（远大于实际占用，宁严勿松）。
const FRAME_RESERVE: usize = 160;

/// 单个附件块在 JSON 里除 base64 正文之外的固定开销估计（键名、MIME、URI、引号）。
const ATTACHMENT_BLOCK_OVERHEAD: usize = 256;

/// 把 core 的 prompt 内容映射成 ACP `prompt` 数组。
///
/// 返回的是 ACP content block 的 JSON 视图（`session/prompt` 的 `prompt` 字段）；
/// **任何一块失败都返回错误**，调用方不会得到「部分 prompt」。
pub(crate) async fn encode_prompt(
    request: &PromptRequest,
    capabilities: &AgentCapabilities,
    attachments: Option<&dyn AttachmentStore>,
    workspace: &Path,
) -> Result<Vec<Value>, HostError> {
    request.validate().map_err(|_| HostError::InvalidPrompt)?;

    let mut blocks: Vec<InputBlock> = Vec::with_capacity(request.content.len());
    for block in &request.content {
        blocks.push(parse_block(block)?);
    }

    // 能力门控在**读取任何字节之前**完成：未宣告时连附件都不必读。
    for block in &blocks {
        required_capability(block, capabilities)?;
    }

    // 预算预估：文本可以精确编码；附件按声明长度算 base64 体积，避免为一个 20 MiB 的文件先读进内存。
    estimate(&blocks)?;

    let mut encoded: Vec<Value> = Vec::with_capacity(blocks.len());
    let mut used = 0usize;
    for block in blocks {
        let value = match block {
            InputBlock::Text(text) => text_block(text)?,
            InputBlock::WorkspacePath(path) => {
                check_workspace_path(workspace, &path)?;
                // 只传路径：不做文件读取、不固定版本、不把路径当附件快照。
                text_block(path)?
            }
            InputBlock::Attachment(attachment) => {
                let store = attachments.ok_or_else(|| HostError::PromptRejected {
                    detail: "附件存储未接线".to_owned(),
                })?;
                attachment_block(store, attachment).await?
            }
        };
        used = used.saturating_add(json_len(&value));
        if used.saturating_add(FRAME_RESERVE) > MAX_MESSAGE_BYTES {
            return Err(HostError::PromptRejected {
                detail: "prompt 超出 ACP 单条消息预算".to_owned(),
            });
        }
        encoded.push(value);
    }

    // 派发前的精确校验：最终数组加上信封预留必须留在 1 MiB 内。
    let exact = json_len(&Value::Array(encoded.clone()));
    if exact == usize::MAX || exact.saturating_add(FRAME_RESERVE) > MAX_MESSAGE_BYTES {
        return Err(HostError::PromptRejected {
            detail: "prompt 超出 ACP 单条消息预算".to_owned(),
        });
    }
    Ok(encoded)
}

/// 编码前的体积预估（不读取任何附件字节）。
fn estimate(blocks: &[InputBlock]) -> Result<(), HostError> {
    let mut total = FRAME_RESERVE;
    for block in blocks {
        total = match block {
            InputBlock::Text(text) | InputBlock::WorkspacePath(text) => total
                .saturating_add(json_len(&Value::String(text.clone())))
                .saturating_add(32),
            InputBlock::Attachment(attachment) => {
                let base64 = attachment.byte_length.div_ceil(3).saturating_mul(4);
                total
                    .saturating_add(usize::try_from(base64).unwrap_or(usize::MAX))
                    .saturating_add(ATTACHMENT_BLOCK_OVERHEAD)
            }
        };
        if total > MAX_MESSAGE_BYTES {
            return Err(HostError::PromptRejected {
                detail: "prompt 超出 ACP 单条消息预算".to_owned(),
            });
        }
    }
    Ok(())
}

/// 一条 Sync prompt 内容块（`schemas/sync/v1/common.schema.json#/$defs/promptContentBlock`）。
#[derive(Debug)]
enum InputBlock {
    Text(String),
    Attachment(AttachmentRef),
    WorkspacePath(String),
}

/// `attachment_ref` 分支的取值。`sha256` 保持线上文本（校验时与重新计算的结果逐字比较）。
#[derive(Debug)]
struct AttachmentRef {
    id: AttachmentId,
    media_type: String,
    byte_length: u64,
    sha256: Digest,
}

fn parse_block(block: &PromptContentBlock) -> Result<InputBlock, HostError> {
    let value: Value =
        serde_json::from_str(block.as_str()).map_err(|_| HostError::InvalidPrompt)?;
    let wire_type = value
        .get("type")
        .and_then(Value::as_str)
        .ok_or(HostError::InvalidPrompt)?;
    match wire_type {
        "text" => {
            let text = value
                .get("text")
                .and_then(Value::as_str)
                .ok_or(HostError::InvalidPrompt)?;
            if text.is_empty() {
                // 空文本块不是「什么也没说」：它会占掉一个内容项却没有任何内容。
                return Err(HostError::InvalidPrompt);
            }
            Ok(InputBlock::Text(text.to_owned()))
        }
        "attachment_ref" => {
            let id_text = value
                .get("attachmentId")
                .and_then(Value::as_str)
                .ok_or(HostError::InvalidPrompt)?;
            let id = AttachmentId::new(id_text).map_err(|_| HostError::InvalidPrompt)?;
            let media_type = value
                .get("mediaType")
                .and_then(Value::as_str)
                .ok_or(HostError::InvalidPrompt)?;
            if media_type.is_empty() || media_type.len() > 255 {
                return Err(HostError::InvalidPrompt);
            }
            let byte_length = decimal_u64(
                value
                    .get("byteLength")
                    .and_then(Value::as_str)
                    .ok_or(HostError::InvalidPrompt)?,
            )?;
            let sha256 = value
                .get("sha256")
                .and_then(Value::as_str)
                .ok_or(HostError::InvalidPrompt)?;
            let sha256 = Digest::new(sha256).map_err(|_| HostError::InvalidPrompt)?;
            Ok(InputBlock::Attachment(AttachmentRef {
                id,
                media_type: media_type.to_owned(),
                byte_length,
                sha256,
            }))
        }
        "workspace_path_ref" => {
            let path = value
                .get("path")
                .and_then(Value::as_str)
                .ok_or(HostError::InvalidPrompt)?;
            Ok(InputBlock::WorkspacePath(path.to_owned()))
        }
        other => Err(HostError::PromptRejected {
            detail: format!("未登记的 prompt 内容类型 {other}"),
        }),
    }
}

/// 规范十进制字符串（`decimalString`）：只有 `0` 或非零数字串，无符号、无前导零。
fn decimal_u64(text: &str) -> Result<u64, HostError> {
    let canonical = !text.is_empty()
        && text.bytes().all(|byte| byte.is_ascii_digit())
        && (text == "0" || !text.starts_with('0'));
    if !canonical {
        return Err(HostError::InvalidPrompt);
    }
    text.parse::<u64>().map_err(|_| HostError::InvalidPrompt)
}

/// 该输入块要求 Agent 宣告的能力（`None` = 无 gate）。
fn required_capability(
    block: &InputBlock,
    capabilities: &AgentCapabilities,
) -> Result<(), HostError> {
    let (capability, declared) = match block {
        InputBlock::Text(_) | InputBlock::WorkspacePath(_) => return Ok(()),
        InputBlock::Attachment(attachment) => {
            let media_type = attachment.media_type.as_str();
            if media_type.starts_with("image/") {
                (
                    "agentCapabilities.promptCapabilities.image",
                    capabilities.supports_prompt_image(),
                )
            } else if media_type.starts_with("audio/") {
                (
                    "agentCapabilities.promptCapabilities.audio",
                    capabilities.supports_prompt_audio(),
                )
            } else {
                (
                    "agentCapabilities.promptCapabilities.embeddedContext",
                    capabilities.supports_embedded_context(),
                )
            }
        }
    };
    if declared {
        Ok(())
    } else {
        Err(HostError::CapabilityNotDeclared {
            capability: capability.to_owned(),
        })
    }
}

fn text_block(text: String) -> Result<Value, HostError> {
    serde_json::to_value(ContentBlock::Text(TextContent {
        text,
        annotations: None,
        meta: None,
    }))
    .map_err(|_| HostError::InvalidPrompt)
}

/// 读取并校验一个附件，映射成 ACP 原生 content block。
async fn attachment_block(
    store: &dyn AttachmentStore,
    attachment: AttachmentRef,
) -> Result<Value, HostError> {
    let bytes = store
        .get(&attachment.id)
        .await
        .map_err(|_| HostError::PromptRejected {
            detail: "附件读取失败".to_owned(),
        })?
        .ok_or_else(|| HostError::PromptRejected {
            detail: "附件不存在".to_owned(),
        })?;
    if bytes.len() as u64 != attachment.byte_length {
        // 长度不符：声明与内容不是同一个东西，整条拒绝。
        return Err(HostError::PromptRejected {
            detail: "附件长度与声明不符".to_owned(),
        });
    }
    let actual = digest_text(&bytes);
    if actual != attachment.sha256.as_str() {
        return Err(HostError::PromptRejected {
            detail: "附件摘要与声明不符".to_owned(),
        });
    }
    let data = base64::engine::general_purpose::STANDARD.encode(&bytes);
    let media_type = attachment.media_type.clone();
    if media_type.starts_with("image/") {
        return serde_json::to_value(ContentBlock::Image(ImageContent {
            data,
            mime_type: media_type,
            uri: None,
            annotations: None,
            meta: None,
        }))
        .map_err(|_| HostError::InvalidPrompt);
    }
    if media_type.starts_with("audio/") {
        return serde_json::to_value(ContentBlock::Audio(AudioContent {
            data,
            mime_type: media_type,
            annotations: None,
            meta: None,
        }))
        .map_err(|_| HostError::InvalidPrompt);
    }
    serde_json::to_value(ContentBlock::Resource(EmbeddedResource {
        resource: EmbeddedResourceContents {
            // 内嵌资源要用一个 URI 标识内容；这是本机附件标识，不是可下载地址或路径。
            uri: format!("urn:acp-remote:attachment:{}", attachment.id),
            mime_type: Some(media_type),
            text: None,
            blob: Some(data),
            extra: Map::new(),
        },
        annotations: None,
        meta: None,
    }))
    .map_err(|_| HostError::InvalidPrompt)
}

fn json_len(value: &Value) -> usize {
    serde_json::to_vec(value).map_or(usize::MAX, |bytes| bytes.len())
}

fn digest_text(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hasher.finalize())
}

// -------------------------------------------------------------------------------------------
// 工作目录路径引用
// -------------------------------------------------------------------------------------------

/// 校验并规范化一个 `workspace_path_ref`。
///
/// 规则（`SYNC_PROTOCOL.md` §11.5 `/workspace_path_ref`，`SECURITY_DESIGN.md` §12.3）：
///
/// 1. 形状：非空、≤ 4096 字符、非绝对（`/` 或 `\` 开头）、无盘符/ADS 冒号、无控制字符、
///    不含 `.`/`..` 分量、没有空分量（`a//b`）；
/// 2. **按平台分隔符拆分量**：`..\..\secret` 在只按 `/` 判定的 schema 里能蒙混过去，因此这里两种
///    分隔符都拆——不做这一步，Windows 上的前缀校验就是摆设；
/// 3. 规范化前缀：取最深的已存在祖先做 `canonicalize`，它必须仍以会话工作目录的规范形式开头。
///    这同时挡住「中间层是逃逸 symlink/junction」的情形；文件不存在是允许的（引用只表示路径，
///    不固定版本），但**至少会话根必须存在**。
fn check_workspace_path(root: &Path, relative: &str) -> Result<(), HostError> {
    if relative.is_empty() || relative.len() > 4096 {
        return Err(HostError::PromptRejected {
            detail: "工作目录引用长度不合法".to_owned(),
        });
    }
    if relative.starts_with('/') || relative.starts_with('\\') || relative.contains(':') {
        return Err(HostError::PromptRejected {
            detail: "工作目录引用不得是绝对路径/盘符或 ADS".to_owned(),
        });
    }
    if relative.chars().any(char::is_control) {
        return Err(HostError::PromptRejected {
            detail: "工作目录引用含控制字符".to_owned(),
        });
    }
    for component in relative.split(['/', '\\']) {
        if component.is_empty() || component == "." || component == ".." {
            return Err(HostError::PromptRejected {
                detail: "工作目录引用不得含空分量或父目录引用".to_owned(),
            });
        }
    }

    let root = root.canonicalize().map_err(|_| HostError::PromptRejected {
        detail: "会话工作目录不可用".to_owned(),
    })?;
    let candidate = root.join(relative);
    let mut probe = candidate.as_path();
    loop {
        match probe.canonicalize() {
            Ok(canonical) => {
                if canonical.starts_with(&root) {
                    return Ok(());
                }
                return Err(HostError::PromptRejected {
                    detail: "工作目录引用逃逸出会话根".to_owned(),
                });
            }
            // 文件（或某一级父目录）尚不存在是合法的：引用只表示路径。退回上一级继续判定。
            Err(_) => match probe.parent() {
                Some(parent) if parent.starts_with(&root) => probe = parent,
                _ => {
                    return Err(HostError::PromptRejected {
                        detail: "工作目录引用逃逸出会话根".to_owned(),
                    });
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimal_strings_are_canonical_only() {
        assert_eq!(decimal_u64("0").ok(), Some(0));
        assert_eq!(decimal_u64("18446744073709551615").ok(), Some(u64::MAX));
        for bad in ["", "00", "01", "-1", "+1", " 1", "1 ", "1.0", "1e3"] {
            assert!(decimal_u64(bad).is_err(), "{bad:?} 必须被拒绝");
        }
        assert!(decimal_u64("18446744073709551616").is_err(), "溢出必须拒绝");
    }

    #[test]
    fn path_shape_rejects_traversal_and_absolute_forms() {
        let root = std::env::temp_dir();
        for bad in [
            "",
            "/etc/passwd",
            "\\Windows\\System32",
            "C:secret",
            "a/../../b",
            "a\\..\\b",
            "a//b",
            "a/./b",
            "a\u{0}b",
        ] {
            assert!(
                check_workspace_path(&root, bad).is_err(),
                "{bad:?} 必须被拒绝"
            );
        }
        // 一个不存在的根内相对路径是合法的（引用只表示路径）。
        assert!(check_workspace_path(&root, "definitely-not-created/sub/file.txt").is_ok());
    }

    #[test]
    fn estimate_rejects_an_attachment_before_reading_it() {
        let attachment = InputBlock::Attachment(AttachmentRef {
            id: AttachmentId::new("11111111-1111-4111-8111-111111111111").expect("id"),
            media_type: "image/png".to_owned(),
            // 20 MiB 的附件 base64 后约 27 MiB，远大于 1 MiB 预算。
            byte_length: 20 * 1024 * 1024,
            sha256: Digest::new("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA").expect("digest"),
        });
        assert!(matches!(
            estimate(&[attachment]),
            Err(HostError::PromptRejected { .. })
        ));
    }
}
