//! 设备/节点配对的安全确认仪式（`docs/LOCAL_ADMIN_PROTOCOL.md` §5.3/§5.4/§5.8、
//! `specs/cli-commands` 的「配对安全确认仪式」R64–R66）。
//!
//! 流程固定为三步：`*.pair.begin` → `*.pair.status`（CLI 轮询）→ `*.pair.confirm`。
//!
//! - `begin` 拿到的 `pairingUrl` **只**打印到终端（内存 → 终端），不写日志、不写文件、不进错误消息；
//! - 轮询由 CLI 负责（间隔 [`POLL_INTERVAL`]），有效期以 `status`/`begin` 返回的 `expiresAt` 为准
//!   （§1.1 的定宽文本，字典序即时间序，因此本地不需要日期库），到期以 `local.expired` 结束；
//! - 认领后**必须**先展示名称、指纹、SAS、请求集合与过期时间，再允许确认；提交的是**展示过的**请求集合，
//!   而不是命令行上的 `--request`/`--grant` 原文；
//! - 非交互（`stdin` 不是终端）时 `--sas` 与 `--fingerprint` 必须同时给出并与 `status` 的返回值**逐字**
//!   相等才调 `confirm`；不相等即失败且不修改任何状态。没有「无参数自动确认」开关。

use std::io::{IsTerminal as _, Write as _};
use std::time::{Duration, Instant};

use acp_core::ports::Clock as _;
use serde_json::Value;
use server::local_admin::{JsonObject, LocalErrorCode, Method};

use crate::cli::{Context, Failure, client_failure, print_result, strings, success};
use crate::client::{LocalAdminClient, call_once, outcome_of};

/// 轮询间隔（CLI 侧策略，Daemon 侧没有轮询）。
const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// 本地等待上限：方法侧的有效期固定 5 分钟（`expiresInMs` 只能收窄），本地再留一点余量。
const MAX_WAIT: Duration = Duration::from_secs(330);

/// 配对方向：设备（`device.pair.*`）或节点（`node.pair.*`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PairTarget {
    /// `device.pair.*`
    Device,
    /// `node.pair.*`
    Node,
}

/// `*.pair.status` 的状态归类（§5.3/§5.4 的闭合词表）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Poll {
    /// 还没到可确认的状态（设备：`pending`；节点：`pending`/`claimed`）。
    Waiting,
    /// 已认领，可以展示并确认。
    Claimable,
    /// 已被批准（本 CLI 无事可做）。
    Approved,
    /// 已被拒绝。
    Rejected,
    /// 已过期。
    Expired,
}

impl PairTarget {
    /// 终端展示用的名字。
    fn label(self) -> &'static str {
        match self {
            Self::Device => "设备",
            Self::Node => "节点",
        }
    }

    /// `begin` 方法名。
    fn begin(self) -> Method {
        match self {
            Self::Device => Method::DevicePairBegin,
            Self::Node => Method::NodePairBegin,
        }
    }

    /// `status` 方法名。
    fn status(self) -> Method {
        match self {
            Self::Device => Method::DevicePairStatus,
            Self::Node => Method::NodePairStatus,
        }
    }

    /// `confirm` 方法名。
    fn confirm(self) -> Method {
        match self {
            Self::Device => Method::DevicePairConfirm,
            Self::Node => Method::NodePairConfirm,
        }
    }

    /// `status` 结果里的指纹字段名（两个方向的名字不同，§5.3/§5.4）。
    fn fingerprint_key(self) -> &'static str {
        match self {
            Self::Device => "publicKeyFingerprint",
            Self::Node => "peerPublicKeyFingerprint",
        }
    }

    /// `status` 结果里的请求集合字段名。
    fn requested_key(self) -> &'static str {
        match self {
            Self::Device => "requestedScopes",
            Self::Node => "requestedGrants",
        }
    }

    /// `confirm` 参数里的集合字段名。
    fn granted_key(self) -> &'static str {
        match self {
            Self::Device => "scopes",
            Self::Node => "grants",
        }
    }

    /// 展示用的集合名（`scope`/`grant`）。
    fn requested_label(self) -> &'static str {
        match self {
            Self::Device => "scope",
            Self::Node => "grants",
        }
    }

    /// `confirm` 结果里的身份字段名。
    fn id_key(self) -> &'static str {
        match self {
            Self::Device => "deviceId",
            Self::Node => "nodeId",
        }
    }

    /// 状态词 → 轮询动作；未登记的单词回 `None`（`local.internal`，不静默当作等待）。
    fn poll(self, state: &str) -> Option<Poll> {
        match (self, state) {
            (_, "pending") => Some(Poll::Waiting),
            (Self::Device, "claimed") => Some(Poll::Claimable),
            // 节点方向在 claim 之后还要等 `pending_confirmation`（§5.4）。
            (Self::Node, "claimed") => Some(Poll::Waiting),
            (Self::Node, "pending_confirmation") => Some(Poll::Claimable),
            (_, "approved") => Some(Poll::Approved),
            (_, "rejected") => Some(Poll::Rejected),
            (_, "expired") => Some(Poll::Expired),
            _ => None,
        }
    }
}

/// `device pair`/`node pair` 的参数（kebab-case flags → 方法 `params` 的中间形态）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct PairArgs {
    /// `device.pair.begin` 的 `requestedScopes`。
    pub(crate) scopes: Vec<String>,
    /// `device.pair.begin` 的 `requestedPacks`。
    pub(crate) packs: Vec<String>,
    /// `node.pair.begin` 的 `mode`。
    pub(crate) mode: Option<String>,
    /// `node.pair.begin` 的 `pairingUrl`。
    pub(crate) pairing_url: Option<String>,
    /// `node.pair.begin` 的 `displayName`。
    pub(crate) display_name: Option<String>,
    /// `node.pair.begin` 的 `requestedGrants`。
    pub(crate) grants: Vec<String>,
    /// 非交互确认用的 SAS（逐字匹配）。
    pub(crate) sas: Option<String>,
    /// 非交互确认用的指纹（逐字匹配）。
    pub(crate) fingerprint: Option<String>,
    /// `expiresInMs`（只能收窄 5 分钟窗口）。
    pub(crate) expires_in_ms: Option<u64>,
}

impl From<&super::DevicePair> for PairArgs {
    fn from(args: &super::DevicePair) -> Self {
        Self {
            scopes: args.request.clone(),
            packs: args.pack.clone(),
            mode: None,
            pairing_url: None,
            display_name: None,
            grants: Vec::new(),
            sas: args.sas.clone(),
            fingerprint: args.fingerprint.clone(),
            expires_in_ms: args.expires_in_ms,
        }
    }
}

impl From<&super::NodePair> for PairArgs {
    fn from(args: &super::NodePair) -> Self {
        Self {
            scopes: Vec::new(),
            packs: Vec::new(),
            mode: args.mode.clone(),
            pairing_url: args.pairing_url.clone(),
            display_name: args.display_name.clone(),
            grants: args.grant.clone(),
            sas: args.sas.clone(),
            fingerprint: args.fingerprint.clone(),
            expires_in_ms: args.expires_in_ms,
        }
    }
}

/// 非交互确认用的两个值（都来自用户在**另一台设备**上读到的显示内容）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Supplied {
    /// 6 位 SAS。
    pub(crate) sas: String,
    /// 64 位小写 hex 指纹。
    pub(crate) fingerprint: String,
}

impl Supplied {
    /// 解析非交互输入。
    ///
    /// 两个值必须**同时**给出：只给一个必然是脚本错误，缺两个且 `stdin` 不是终端则本 CLI 无法完成
    /// 安全仪式（§5.8：「非交互场景必须显式传入」；不得有「无参数自动确认」）。
    pub(crate) fn parse(
        sas: Option<&str>,
        fingerprint: Option<&str>,
        interactive: bool,
    ) -> Result<Option<Self>, Failure> {
        match (sas, fingerprint) {
            (Some(sas), Some(fingerprint)) => {
                if sas.is_empty() || fingerprint.is_empty() {
                    return Err(Failure::local(
                        LocalErrorCode::InvalidRequest,
                        "`--sas` 与 `--fingerprint` 都不得为空",
                    ));
                }
                Ok(Some(Self {
                    sas: sas.to_owned(),
                    fingerprint: fingerprint.to_owned(),
                }))
            }
            (None, None) if interactive => Ok(None),
            (None, None) => Err(Failure::local(
                LocalErrorCode::InvalidRequest,
                "非交互场景必须显式传入 `--sas` 与 `--fingerprint`（不得无参数自动确认）",
            )),
            _ => Err(Failure::local(
                LocalErrorCode::InvalidRequest,
                "`--sas` 与 `--fingerprint` 必须同时给出",
            )),
        }
    }
}

/// 认领后的展示内容（全部取自 `*.pair.status`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Claim {
    /// 对端展示名。
    pub(crate) display_name: String,
    /// 对端公钥指纹。
    pub(crate) fingerprint: String,
    /// 6 位 SAS（必须与对端屏幕上显示的逐字一致）。
    pub(crate) sas: String,
    /// 请求的 scope/grant 集合（确认时提交这一份，而不是命令行原文）。
    pub(crate) requested: Vec<String>,
    /// 过期时间（§1.1 定宽文本）。
    pub(crate) expires_at: String,
}

impl Claim {
    /// 由 `*.pair.status` 的 result 构造；认领前（字段为 `null`）即失败——调用方只在 `Claimable` 状态下调用。
    fn from_status(target: PairTarget, result: &JsonObject) -> Result<Self, Failure> {
        Ok(Self {
            display_name: text_field(result, "displayName")?,
            fingerprint: text_field(result, target.fingerprint_key())?,
            sas: text_field(result, "sas")?,
            requested: text_array_field(result, target.requested_key())?,
            expires_at: text_field(result, "expiresAt")?,
        })
    }

    /// 确认前必须展示的块（名称、指纹、SAS、请求集合、过期时间）。
    fn display(&self, target: PairTarget) -> String {
        let requested = if self.requested.is_empty() {
            "（空）".to_owned()
        } else {
            self.requested.join(", ")
        };
        format!(
            "{label}已认领：\n  名称            {name}\n  指纹            {fingerprint}\n  SAS             {sas}    # 必须与{label}屏幕上显示的 6 位数字一致\n  请求 {requested_label:<11}{requested}\n  过期            {expires}",
            label = target.label(),
            name = self.display_name,
            fingerprint = self.fingerprint,
            sas = self.sas,
            requested_label = target.requested_label(),
            requested = requested,
            expires = self.expires_at,
        )
    }
}

/// `*.pair.begin` / `*.pair.confirm` 的 `params`（kebab-case flags → camelCase 字段）。
pub(crate) fn begin_params(target: PairTarget, args: &PairArgs) -> JsonObject {
    let mut params = JsonObject::new();
    match target {
        PairTarget::Device => {
            params.insert("requestedPacks".to_owned(), strings(&args.packs));
            params.insert("requestedScopes".to_owned(), strings(&args.scopes));
            params.insert(
                "expiresInMs".to_owned(),
                match args.expires_in_ms {
                    Some(ms) => Value::from(ms),
                    None => Value::Null,
                },
            );
        }
        PairTarget::Node => {
            params.insert(
                "mode".to_owned(),
                match args.mode.as_deref() {
                    Some(mode) => Value::from(mode),
                    None => Value::Null,
                },
            );
            params.insert(
                "pairingUrl".to_owned(),
                match args.pairing_url.as_deref() {
                    Some(url) => Value::from(url),
                    None => Value::Null,
                },
            );
            params.insert(
                "displayName".to_owned(),
                match args.display_name.as_deref() {
                    Some(name) => Value::from(name),
                    None => Value::Null,
                },
            );
            params.insert("requestedGrants".to_owned(), strings(&args.grants));
        }
    }
    params
}

/// `*.pair.confirm` 的 `params`：提交**展示过的**请求集合。
pub(crate) fn confirm_params(target: PairTarget, pairing_id: &str, claim: &Claim) -> JsonObject {
    let mut params = JsonObject::new();
    params.insert("pairingId".to_owned(), Value::from(pairing_id));
    params.insert(target.granted_key().to_owned(), strings(&claim.requested));
    params
}

/// 非交互的逐字校验（§5.8）。
///
/// **逐字**相等：不 trim、不折叠大小写、不允许前缀匹配。SAS/指纹是防中间人的最后一道人工核对，任何宽容
/// 比较都会削弱它；失败时错误消息也不回显这两个值。
pub(crate) fn verify_supplied(claim: &Claim, supplied: &Supplied) -> Result<(), Failure> {
    if supplied.sas != claim.sas || supplied.fingerprint != claim.fingerprint {
        return Err(Failure::local(
            LocalErrorCode::InvalidParams,
            "`--sas`/`--fingerprint` 与 Daemon 返回值不一致：未调用 confirm，未修改任何状态",
        ));
    }
    Ok(())
}

/// 配对仪式（§5.8）。成功后打印对端 id。
pub(crate) fn run(
    context: &mut Context,
    target: PairTarget,
    args: &PairArgs,
) -> Result<(), Failure> {
    let interactive = std::io::stdin().is_terminal();
    let supplied = Supplied::parse(
        args.sas.as_deref(),
        args.fingerprint.as_deref(),
        interactive,
    )?;
    let endpoint = context.endpoint()?;
    let runtime = context.runtime()?;
    let claimed = runtime.block_on(await_claim(&endpoint, target, args))?;
    // 认领后**先**展示，再允许确认（§5.8 的第 2 步）。
    println!("{}", claimed.claim.display(target));
    match &supplied {
        Some(supplied) => verify_supplied(&claimed.claim, supplied)?,
        None => ask_confirmation(target, &claimed.claim)?,
    }
    let response = runtime
        .block_on(call_once(
            &endpoint,
            target.confirm(),
            confirm_params(target, &claimed.pairing_id, &claimed.claim),
        ))
        .map_err(client_failure)?;
    let result = success(&outcome_of(&response))?;
    print_result(&result);
    let id = text_field(&result, target.id_key())?;
    println!("已配对：{}={id}", target.id_key());
    Ok(())
}

/// 认领结果：`pairingId`（`confirm` 的必需参数）+ 展示/确认用的领域值。
struct Claimed {
    pairing_id: String,
    claim: Claim,
}

/// `begin` + 轮询到可确认状态（或明确失败）。
async fn await_claim(
    endpoint: &str,
    target: PairTarget,
    args: &PairArgs,
) -> Result<Claimed, Failure> {
    let mut client = LocalAdminClient::connect(endpoint)
        .await
        .map_err(client_failure)?;
    let begin = call(&mut client, target.begin(), begin_params(target, args)).await?;
    let pairing_id = text_field(&begin, "pairingId")?;
    // `pairingUrl` 只经终端展示（含配对 secret 的 fragment），不落日志/文件/错误消息。
    let pairing_url = text_field(&begin, "pairingUrl")?;
    println!("{pairing_url}");
    let expires_at = text_field(&begin, "expiresAt")?;
    println!(
        "正在等待{label}认领…（有效期至 {expires_at}，Ctrl-C 取消）",
        label = target.label()
    );

    let started = Instant::now();
    loop {
        // 本地也判一次到期：Daemon 在维护任务之外不会主动推进状态（`expiresAt` 是定宽文本，字典序即时间序）。
        if now_text().as_str() >= expires_at.as_str() {
            return Err(expired(target));
        }
        if started.elapsed() >= MAX_WAIT {
            return Err(Failure::local(
                LocalErrorCode::Unavailable,
                format!("等待{}配对状态超时", target.label()),
            ));
        }
        let mut params = JsonObject::new();
        params.insert("pairingId".to_owned(), Value::from(pairing_id.as_str()));
        let status = call(&mut client, target.status(), params).await?;
        let state = text_field(&status, "state")?;
        match target.poll(&state) {
            Some(Poll::Waiting) => {}
            Some(Poll::Claimable) => {
                return Ok(Claimed {
                    pairing_id,
                    claim: Claim::from_status(target, &status)?,
                });
            }
            Some(Poll::Approved) => {
                return Err(Failure::local(
                    LocalErrorCode::Conflict,
                    format!("该{}配对已被批准，无需再次确认", target.label()),
                ));
            }
            Some(Poll::Rejected) => return Err(expired(target)),
            Some(Poll::Expired) => return Err(expired(target)),
            None => {
                return Err(Failure::local(
                    LocalErrorCode::Internal,
                    format!("{} 返回了未登记的状态 `{state}`", target.status().as_str()),
                ));
            }
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

/// 交互式确认：只有明确的 `y`/`Y` 才算确认；其余（含 EOF）都不修改任何状态。
///
/// 本切片**不**在拒绝时调用 `*.pair.reject`：CLI 不替用户做未请求的状态变更，未确认的配对自然到期
/// （有效期 5 分钟，且未确认前不可能成为信任记录）。
fn ask_confirmation(target: PairTarget, claim: &Claim) -> Result<(), Failure> {
    println!(
        "确认授予以上 {label}（{} 项）？[y/N]: ",
        claim.requested.len(),
        label = target.requested_label()
    );
    if std::io::stdout().flush().is_err() {
        return Err(Failure::local(
            LocalErrorCode::Internal,
            "无法把确认提示写到 stdout",
        ));
    }
    let mut line = String::new();
    let read = std::io::BufRead::read_line(&mut std::io::stdin().lock(), &mut line);
    let answer = line.trim();
    if read.is_ok() && (answer == "y" || answer == "Y") {
        Ok(())
    } else {
        Err(Failure::local(
            LocalErrorCode::Conflict,
            "用户未确认配对：未调用 confirm，未修改任何状态",
        ))
    }
}

/// 一条请求（连接由调用方持有，轮询期间不重连）。
async fn call(
    client: &mut LocalAdminClient,
    method: Method,
    params: JsonObject,
) -> Result<JsonObject, Failure> {
    let response = client.call(method, params).await.map_err(client_failure)?;
    success(&outcome_of(&response))
}

/// 当前时间（§1.1 定宽文本，可与 `expiresAt` 直接做字典序比较）。
fn now_text() -> String {
    crate::clock::SystemClock::new().now().as_str().to_owned()
}

/// 到期/已被拒绝的失败：§6 规定两者都是 `local.expired`。
fn expired(target: PairTarget) -> Failure {
    Failure::local(
        LocalErrorCode::Expired,
        format!("{label}配对已过期或已被拒绝", label = target.label()),
    )
}

/// 取必需字符串字段；缺失或类型不符即 `local.internal`（协议违规，而不是用户输入问题）。
fn text_field(result: &JsonObject, key: &str) -> Result<String, Failure> {
    result
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| {
            Failure::local(
                LocalErrorCode::Internal,
                format!("配对方法的响应缺少字符串字段 `{key}`"),
            )
        })
}

/// 取字符串数组字段。
fn text_array_field(result: &JsonObject, key: &str) -> Result<Vec<String>, Failure> {
    let items = result.get(key).and_then(Value::as_array).ok_or_else(|| {
        Failure::local(
            LocalErrorCode::Internal,
            format!("配对方法的响应缺少数组字段 `{key}`"),
        )
    })?;
    items
        .iter()
        .map(|item| {
            item.as_str().map(str::to_owned).ok_or_else(|| {
                Failure::local(
                    LocalErrorCode::Internal,
                    format!("`{key}` 的每一项都必须是字符串"),
                )
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claim() -> Claim {
        Claim {
            display_name: "Zhang's Phone".to_owned(),
            fingerprint: "ab12cd34".to_owned(),
            sas: "481502".to_owned(),
            requested: vec!["session.list".to_owned(), "session.read".to_owned()],
            expires_at: "2026-09-23T14:31:00.000Z".to_owned(),
        }
    }

    /// 逐字匹配：相等才放行；前缀、大小写、空白、长度差异一律失败（R66 的判定）。
    #[test]
    fn supplied_values_must_match_the_status_values_verbatim() {
        let claim = claim();
        let exact = Supplied {
            sas: "481502".to_owned(),
            fingerprint: "ab12cd34".to_owned(),
        };
        verify_supplied(&claim, &exact).expect("逐字一致必须放行");

        for (sas, fingerprint) in [
            ("4815", "ab12cd34"),    // SAS 只匹配前缀
            ("4815020", "ab12cd34"), // SAS 更长
            ("481502", "ab12cd3"),   // 指纹只匹配前缀
            ("481502", "AB12CD34"),  // 指纹大小写不同
            ("481502", " ab12cd34"), // 指纹带前导空白
            ("", "ab12cd34"),        // 空值
        ] {
            let supplied = Supplied {
                sas: sas.to_owned(),
                fingerprint: fingerprint.to_owned(),
            };
            let error = verify_supplied(&claim, &supplied)
                .expect_err(&format!("{sas}/{fingerprint} 必须失败"));
            assert_eq!(error.code(), "local.invalid_params");
            assert!(
                !error.message().contains("481502") && !error.message().contains("ab12cd34"),
                "错误消息不得回显 SAS/指纹：{}",
                error.message()
            );
        }
    }

    /// 非交互必须同时给出两个值；交互式可以只给终端提示（`None`）。
    #[test]
    fn non_interactive_confirmation_requires_both_values() {
        assert!(
            Supplied::parse(Some("481502"), Some("ab12cd34"), false)
                .expect("两个都给")
                .is_some()
        );
        assert!(Supplied::parse(None, None, true).expect("交互式").is_none());
        for (sas, fingerprint, interactive) in [
            (None, None, false),                // 非交互 + 两个都缺
            (Some("481502"), None, true),       // 只给 SAS
            (None, Some("ab12cd34"), true),     // 只给指纹
            (Some("481502"), None, false),      // 非交互 + 只给 SAS
            (Some(""), Some("ab12cd34"), true), // 空 SAS
            (Some("481502"), Some(""), true),   // 空指纹
        ] {
            let error = Supplied::parse(sas, fingerprint, interactive).expect_err("必须明确失败");
            assert_eq!(error.code(), "local.invalid_request");
        }
    }

    /// `begin`/`confirm` 的 `params` 与 §5.3/§5.4 的字段同名同形（kebab-case flags → camelCase）。
    #[test]
    fn pairing_params_match_the_wire_fields() {
        let device = PairArgs {
            scopes: vec!["session.read".to_owned()],
            packs: vec!["pack.read".to_owned()],
            expires_in_ms: Some(60_000),
            ..PairArgs::default()
        };
        assert_eq!(
            Value::Object(begin_params(PairTarget::Device, &device)),
            serde_json::json!({
                "requestedPacks": ["pack.read"],
                "requestedScopes": ["session.read"],
                "expiresInMs": 60_000,
            })
        );
        assert_eq!(
            Value::Object(begin_params(PairTarget::Device, &PairArgs::default())),
            serde_json::json!({ "requestedPacks": [], "requestedScopes": [], "expiresInMs": null })
        );

        let node = PairArgs {
            mode: Some("owner".to_owned()),
            display_name: Some("My Node".to_owned()),
            grants: vec!["grant.session.read".to_owned()],
            ..PairArgs::default()
        };
        assert_eq!(
            Value::Object(begin_params(PairTarget::Node, &node)),
            serde_json::json!({
                "mode": "owner",
                "pairingUrl": null,
                "displayName": "My Node",
                "requestedGrants": ["grant.session.read"],
            })
        );

        let claim = claim();
        assert_eq!(
            Value::Object(confirm_params(PairTarget::Device, "P", &claim)),
            serde_json::json!({ "pairingId": "P", "scopes": ["session.list", "session.read"] })
        );
        assert_eq!(
            Value::Object(confirm_params(PairTarget::Node, "P", &claim)),
            serde_json::json!({ "pairingId": "P", "grants": ["session.list", "session.read"] })
        );
    }

    /// 展示块必须含名称、指纹、SAS、请求集合与过期时间（确认前的人工核对材料）。
    #[test]
    fn the_claim_display_shows_every_required_field() {
        let text = claim().display(PairTarget::Device);
        for expected in [
            "Zhang's Phone",
            "ab12cd34",
            "481502",
            "session.list, session.read",
            "2026-09-23T14:31:00.000Z",
        ] {
            assert!(text.contains(expected), "展示块缺 `{expected}`：{text}");
        }
    }

    /// 状态词归类：设备与节点的差异、终态与未知词。
    #[test]
    fn pairing_states_are_classified_per_direction() {
        assert_eq!(PairTarget::Device.poll("pending"), Some(Poll::Waiting));
        assert_eq!(PairTarget::Device.poll("claimed"), Some(Poll::Claimable));
        assert_eq!(PairTarget::Node.poll("claimed"), Some(Poll::Waiting));
        assert_eq!(
            PairTarget::Node.poll("pending_confirmation"),
            Some(Poll::Claimable)
        );
        assert_eq!(PairTarget::Device.poll("approved"), Some(Poll::Approved));
        assert_eq!(PairTarget::Device.poll("rejected"), Some(Poll::Rejected));
        assert_eq!(PairTarget::Device.poll("expired"), Some(Poll::Expired));
        assert_eq!(PairTarget::Node.poll("consumed"), None);
        assert_eq!(
            PairTarget::Device.poll("pending_confirmation"),
            None,
            "设备方向没有该状态（§5.3 的词表）"
        );
    }

    /// `status` 的字段缺失即 `local.internal`（不静默用空值确认）。
    #[test]
    fn a_status_without_the_claim_fields_is_an_internal_error() {
        let mut status = JsonObject::new();
        status.insert("state".to_owned(), Value::from("claimed"));
        let error = Claim::from_status(PairTarget::Device, &status).expect_err("缺字段必须失败");
        assert_eq!(error.code(), "local.internal");
    }
}
