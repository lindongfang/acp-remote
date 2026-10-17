//! 设备/节点配对的安全确认仪式（`docs/LOCAL_ADMIN_PROTOCOL.md` §5.3/§5.4/§5.8、
//! `specs/cli-commands` 的「配对安全确认仪式」R64–R66）。
//!
//! 流程固定为三步：`*.pair.begin` → `*.pair.status`（CLI 轮询）→ `*.pair.confirm`。
//!
//! - `begin` 拿到的 `pairingUrl` **只**打印到终端（内存 → 终端），不写日志、不写文件、不进错误消息；
//! - 轮询由 CLI 负责（间隔 [`POLL_INTERVAL`]），有效期以 `status`/`begin` 返回的 `expiresAt` 为准
//!   （§1.1 的定宽文本，字典序即时间序，因此本地不需要日期库），到期以 `local.expired` 结束；
//! - 认领后**必须**先展示名称、指纹与过期时间（节点方向另含待授予的 `grants`），再允许确认；设备方向
//!   没有任何授权词汇，节点方向提交的是**展示过的** grants，而不是命令行上的 `--grant` 原文；
//! - 非交互（`stdin` 不是终端）时 `--fingerprint` 必须显式给出并与 `status` 的返回值**逐字**相等才调
//!   `confirm`；不相等即失败且不修改任何状态。没有「无参数自动确认」开关，也没有短验证码参数。

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

    /// `reject` 方法名（§5.3/§5.4）。
    fn reject(self) -> Method {
        match self {
            Self::Device => Method::DevicePairReject,
            Self::Node => Method::NodePairReject,
        }
    }

    /// `status` 结果里的指纹字段名（两个方向的名字不同，§5.3/§5.4）。
    fn fingerprint_key(self) -> &'static str {
        match self {
            Self::Device => "publicKeyFingerprint",
            Self::Node => "peerPublicKeyFingerprint",
        }
    }

    /// 对端展示名的字段名；`node.pair.status` 的 result 没有该字段（§5.4 的字段表），
    /// 因此节点方向返回 `None`，展示块与解析都不得要求它。
    fn peer_name_key(self) -> Option<&'static str> {
        match self {
            Self::Device => Some("displayName"),
            Self::Node => None,
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
    /// `node.pair.begin` 的 `mode`。
    pub(crate) mode: Option<String>,
    /// `node.pair.begin` 的 `pairingUrl`。
    pub(crate) pairing_url: Option<String>,
    /// 非交互确认用的指纹（逐字匹配）。
    pub(crate) fingerprint: Option<String>,
    /// `device.pair.begin` 的 `expiresInMs`（只能收窄 5 分钟窗口；`node.pair.begin` 没有该参数）。
    pub(crate) expires_in_ms: Option<u64>,
}

impl From<&super::DevicePair> for PairArgs {
    fn from(args: &super::DevicePair) -> Self {
        Self {
            mode: None,
            pairing_url: None,

            fingerprint: args.fingerprint.clone(),
            expires_in_ms: args.expires_in_ms,
        }
    }
}

impl From<&super::NodePair> for PairArgs {
    fn from(args: &super::NodePair) -> Self {
        Self {
            mode: args.mode.clone(),
            pairing_url: args.pairing_url.clone(),
            fingerprint: args.fingerprint.clone(),
            // `node.pair.begin` 没有 `expiresInMs` 参数：该方向不提供有效期收窄（与 §5.4 的字段表一致）。
            expires_in_ms: None,
        }
    }
}

/// 非交互确认用的值（来自用户在**另一台设备**屏幕上读到的指纹）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Supplied {
    /// 64 位小写 hex 指纹。
    pub(crate) fingerprint: String,
}

impl Supplied {
    /// 解析非交互输入。
    ///
    /// 缺 `--fingerprint` 且 `stdin` 不是终端时本 CLI 无法完成安全确认（§5.8：「非交互场景必须显式传入」；
    /// 不得有「无参数自动确认」）。
    pub(crate) fn parse(
        fingerprint: Option<&str>,
        interactive: bool,
    ) -> Result<Option<Self>, Failure> {
        match fingerprint {
            Some(fingerprint) => {
                if fingerprint.is_empty() {
                    return Err(Failure::local(
                        LocalErrorCode::InvalidRequest,
                        "`--fingerprint` 不得为空",
                    ));
                }
                Ok(Some(Self {
                    fingerprint: fingerprint.to_owned(),
                }))
            }
            None if interactive => Ok(None),
            None => Err(Failure::local(
                LocalErrorCode::InvalidRequest,
                "非交互场景必须显式传入 `--fingerprint`（不得无参数自动确认）",
            )),
        }
    }
}

/// 认领后的展示内容（全部取自 `*.pair.status`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Claim {
    /// 对端展示名；节点方向没有该字段（§5.4），因此为 `None`。
    pub(crate) display_name: Option<String>,
    /// 对端公钥指纹。
    pub(crate) fingerprint: String,
    /// 过期时间（§1.1 定宽文本）。
    pub(crate) expires_at: String,
}

impl Claim {
    /// 由 `*.pair.status` 的 result 构造；认领前（字段为 `null`）即失败——调用方只在 `Claimable` 状态下调用。
    fn from_status(target: PairTarget, result: &JsonObject) -> Result<Self, Failure> {
        Ok(Self {
            display_name: match target.peer_name_key() {
                Some(key) => Some(text_field(result, key)?),
                None => None,
            },
            fingerprint: text_field(result, target.fingerprint_key())?,
            expires_at: text_field(result, "expiresAt")?,
        })
    }

    /// 确认前必须展示的块：名称（该方向的状态响应有名称字段时）、指纹与过期时间；节点方向另含
    /// 待授予的 `grant.*`。
    fn display(&self, target: PairTarget) -> String {
        let name = match target.peer_name_key().zip(self.display_name.as_deref()) {
            Some((_, name)) => format!("\n  名称            {name}"),
            None => String::new(),
        };
        format!(
            "{label}已认领：{name}\n  指纹            {fingerprint}\n  过期            {expires}",
            label = target.label(),
            fingerprint = self.fingerprint,
            expires = self.expires_at,
        )
    }
}

/// `*.pair.begin` / `*.pair.confirm` 的 `params`（kebab-case flags → camelCase 字段）。
pub(crate) fn begin_params(target: PairTarget, args: &PairArgs) -> JsonObject {
    let mut params = JsonObject::new();
    match target {
        // §5.3：设备方向**只**接受 `expiresInMs`——`requestedPacks`/`requestedScopes` 之类的键
        // 不再是本方法的输入，Daemon 侧出现即 `local.invalid_params`（`params` 是 closed object）。
        PairTarget::Device => {
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
        }
    }
    params
}

/// `*.pair.confirm` 的 `params`：设备方向**不提交任何授权集合**（§5.3 只有 `pairingId`）。
///
/// 两个方向都只提交 `pairingId`：配对不携带授权集合。
pub(crate) fn confirm_params(pairing_id: &str) -> JsonObject {
    let mut params = JsonObject::new();
    params.insert("pairingId".to_owned(), Value::from(pairing_id));
    params
}

/// `*.pair.reject` 的 `params`（§5.3/§5.4 两个方向同形）：`reason = null` = 用户主动拒绝，不附理由。
///
/// 消息里**不**回显指纹与名称，也不写日志（配对 secret 的展示面只有终端）。
pub(crate) fn reject_params(pairing_id: &str) -> JsonObject {
    let mut params = JsonObject::new();
    params.insert("pairingId".to_owned(), Value::from(pairing_id));
    params.insert("reason".to_owned(), Value::Null);
    params
}

/// 非交互的逐字校验（§5.8）。
///
/// **逐字**相等：不 trim、不折叠大小写、不允许前缀匹配。指纹是防中间人的最后一道人工核对，任何宽容比较
/// 都会削弱它；失败时错误消息也不回显该值。
pub(crate) fn verify_supplied(claim: &Claim, supplied: &Supplied) -> Result<(), Failure> {
    if supplied.fingerprint != claim.fingerprint {
        return Err(Failure::local(
            LocalErrorCode::InvalidParams,
            "`--fingerprint` 与 Daemon 返回值不一致：未调用 confirm，未修改任何状态",
        ));
    }
    Ok(())
}

/// 配对仪式（§5.8）。成功后打印对端 id。
///
pub(crate) fn run(
    context: &mut Context,
    target: PairTarget,
    args: &PairArgs,
) -> Result<(), Failure> {
    let interactive = std::io::stdin().is_terminal();
    let supplied = Supplied::parse(args.fingerprint.as_deref(), interactive)?;
    let endpoint = context.endpoint()?;
    let runtime = context.runtime()?;
    let claimed = runtime.block_on(await_claim(&endpoint, target, args))?;
    // 认领后**先**展示，再允许确认（§5.8 的第 2 步）。
    println!("{}", claimed.claim.display(target));
    let answer = match &supplied {
        // 非交互：`--fingerprint` 与 `status` 返回值逐字一致才继续；不一致即失败（不调 confirm、不调 reject、不改状态）。
        Some(supplied) => {
            verify_supplied(&claimed.claim, supplied)?;
            Answer::Confirmed
        }
        None => ask_confirmation(target, &claimed.claim)?,
    };
    let result = runtime.block_on(settle_claim(
        &mut OneshotCalls(endpoint.as_str()),
        target,
        &claimed.pairing_id,
        &claimed.claim,
        answer,
    ))?;
    print_result(&result);
    let id = text_field(&result, target.id_key())?;
    println!("已配对：{}={id}", target.id_key());
    Ok(())
}

/// 配对方法的调用接缝。
///
/// 生产实现是 [`OneshotCalls`]（每条方法一条一次性连接）；把「调用哪个方法、带什么 `params`」抽成
/// 可注入的 trait，是为了让「拒绝路径调了 `reject` 且没调 `confirm`」成为**断言**而不是只能靠人工看
/// 终端（交互路径要求 stdin 是 TTY，进程级用例无法覆盖）。
#[async_trait::async_trait]
trait PairCalls {
    /// 调用一个配对方法并返回 `result`（失败的码原样带出）。
    async fn call(&mut self, method: Method, params: JsonObject) -> Result<JsonObject, Failure>;
}

/// `PairCalls` 的生产实现：一条一次性连接（与 `confirm` 原有的 `call_once` 行为完全一致）。
struct OneshotCalls<'a>(&'a str);

#[async_trait::async_trait]
impl PairCalls for OneshotCalls<'_> {
    async fn call(&mut self, method: Method, params: JsonObject) -> Result<JsonObject, Failure> {
        let response = call_once(self.0, method, params)
            .await
            .map_err(client_failure)?;
        success(&outcome_of(&response))
    }
}

/// 交互式确认的两种回答。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Answer {
    /// 明确的 `y`/`Y`。
    Confirmed,
    /// 其余一切（`n`、空行、EOF）。
    Rejected,
}

/// 认领之后的落定：确认 → 调 `confirm`；交互式拒绝 → **先终结配对会话**（`*.pair.reject`）再失败退出。
///
/// 拒绝时必须调 `reject`：否则已认领的配对会悬置到 5 分钟到期，而对端还在等着一个永远不会来的决定。
/// `reject` 自身失败时按方法返回的码报错退出（不改判成功、不吞错）；无论哪条路径都**不**在拒绝时调
/// `confirm`。
async fn settle_claim<M: PairCalls + Send>(
    calls: &mut M,
    target: PairTarget,
    pairing_id: &str,
    claim: &Claim,
    answer: Answer,
) -> Result<JsonObject, Failure> {
    if answer == Answer::Rejected {
        calls
            .call(target.reject(), reject_params(pairing_id))
            .await?;
        return Err(Failure::local(
            LocalErrorCode::Conflict,
            format!(
                "用户未确认{}配对：已调用 `{}` 终结该配对会话（未调用 confirm，未建立信任）",
                target.label(),
                target.reject().as_str()
            ),
        ));
    }
    calls
        .call(target.confirm(), confirm_params(pairing_id))
        .await
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

/// 交互式确认：只有明确的 `y`/`Y` 才算确认；其余（含 EOF）都算拒绝。
///
/// 拒绝是**用户意图**（不是「什么都没发生」）：调用方必须据 [`Answer::Rejected`] 调 `*.pair.reject` 终结
/// 已认领的配对会话，而不是把它留给 5 分钟到期。
fn ask_confirmation(target: PairTarget, claim: &Claim) -> Result<Answer, Failure> {
    let _ = claim;
    let prompt = format!(
        "确认与本机记录该{}（配对即完全信任）？[y/N]: ",
        target.label()
    );
    print!("{prompt}");
    if std::io::stdout().flush().is_err() {
        return Err(Failure::local(
            LocalErrorCode::Internal,
            "无法把确认提示写到 stdout",
        ));
    }
    let mut line = String::new();
    let read = std::io::BufRead::read_line(&mut std::io::stdin().lock(), &mut line);
    if read.is_err() {
        // 读失败（不是 EOF）：用户意图未知，因此既不确认也**不**替用户拒绝。
        return Err(Failure::local(
            LocalErrorCode::Internal,
            "无法从 stdin 读取确认回答",
        ));
    }
    Ok(answer_for(&line))
}

/// 回答词 → [`Answer`]：只有 `y`/`Y`（去首尾空白后）算确认，其余一切（`n`、空行、EOF）都是拒绝。
fn answer_for(line: &str) -> Answer {
    match line.trim() {
        "y" | "Y" => Answer::Confirmed,
        _ => Answer::Rejected,
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
