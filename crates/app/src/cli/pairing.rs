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

    /// 本方向是否有授权维度：设备配对不授予任何授权（`pairing-model` 删除设备权限层），
    /// 只有节点方向有待授予的 `grant.*`。
    fn grants_dimension(self) -> bool {
        matches!(self, Self::Node)
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
    /// `node.pair.begin` 的 `displayName`。
    pub(crate) display_name: Option<String>,
    /// `node.pair.begin` 的 `requestedGrants`。
    pub(crate) grants: Vec<String>,
    /// `node.pair.confirm` 的 `exportIds`（可重复的 `--export-id`；零次 = 空清单）。
    pub(crate) export_ids: Vec<String>,
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
            display_name: None,
            grants: Vec::new(),
            export_ids: Vec::new(),
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
            display_name: args.display_name.clone(),
            grants: args.grant.clone(),
            export_ids: args.export_id.clone(),
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
    /// 待授予的 `grant.*`（节点方向才有；设备方向恒为空——设备配对不授予任何授权）。
    pub(crate) grants: Vec<String>,
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
            // 设备方向没有任何授权集合可展示（`device.pair.status` 不回显集合）。
            grants: if target.grants_dimension() {
                text_array_field(result, "requestedGrants")?
            } else {
                Vec::new()
            },
            expires_at: text_field(result, "expiresAt")?,
        })
    }

    /// 确认前必须展示的块：名称（该方向的状态响应有名称字段时）、指纹与过期时间；节点方向另含
    /// 待授予的 `grant.*`。
    fn display(&self, target: PairTarget) -> String {
        let grants = if target.grants_dimension() {
            let grants = if self.grants.is_empty() {
                "（空）".to_owned()
            } else {
                self.grants.join(", ")
            };
            format!("\n  待授予 grants   {grants}")
        } else {
            String::new()
        };
        let name = match target.peer_name_key().zip(self.display_name.as_deref()) {
            Some((_, name)) => format!("\n  名称            {name}"),
            None => String::new(),
        };
        format!(
            "{label}已认领：{name}\n  指纹            {fingerprint}{grants}\n  过期            {expires}",
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

/// `*.pair.confirm` 的 `params`：设备方向**不提交任何授权集合**（§5.3 只有 `pairingId`）。
///
/// 节点方向提交**展示过的** grants（§5.4；不是命令行 `--grant` 原文），并另带必需的 `exportIds`：它是
/// **用户命令行给出的**清单（`--export-id`，可重复），因此不经 `status` 回显，也不受「提交展示过的集合」
/// 约束——清单是本次授权的一部分，不是对端的请求值。
pub(crate) fn confirm_params(
    target: PairTarget,
    pairing_id: &str,
    claim: &Claim,
    export_ids: &[String],
) -> JsonObject {
    let mut params = JsonObject::new();
    params.insert("pairingId".to_owned(), Value::from(pairing_id));
    if target.grants_dimension() {
        params.insert("grants".to_owned(), strings(&claim.grants));
        params.insert("exportIds".to_owned(), strings(export_ids));
    }
    params
}

/// 空清单的醒目提醒（§5.4/`design.md` D6）：零个 `--export-id` 是合法输入（空清单 = 该节点看不到
/// 任何 Export），但必须让人当场看见这个后果。提醒**不阻断**：确认仍照常执行（不引入新开关）。
pub(crate) fn empty_export_ids_notice(
    target: PairTarget,
    export_ids: &[String],
) -> Option<&'static str> {
    (target == PairTarget::Node && export_ids.is_empty()).then_some(
        "警告：本次确认没有点名任何 Export（--export-id 一次都没给）：该节点将看不到任何 Export。",
    )
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
/// 零个 `--export-id` 时在「展示认领信息」与「确认」之间打印 [`empty_export_ids_notice`]：提醒就在
/// 用户落定前一刻出现，且**不阻断**（确认照常执行）。提醒走 stdout，与配对提示同一通道——stderr 的
/// 契约是「失败时恰好一行 JSON」（见 `crates/app/tests/support`），不得混入人类可读文本。
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
    if let Some(notice) = empty_export_ids_notice(target, &args.export_ids) {
        println!("{notice}");
    }
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
        &args.export_ids,
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
    export_ids: &[String],
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
        .call(
            target.confirm(),
            confirm_params(target, pairing_id, claim, export_ids),
        )
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
    // 设备方向没有授权维度，因此确认的是「是否与本机建立信任」本身，而不是某个集合。
    let prompt = if target.grants_dimension() {
        format!("确认授予以上 grants（{} 项）？[y/N]: ", claim.grants.len())
    } else {
        format!(
            "确认与本机记录该{}（不授予任何授权）？[y/N]: ",
            target.label()
        )
    };
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{fail, failure_line};

    fn claim() -> Claim {
        Claim {
            display_name: Some("Zhang's Phone".to_owned()),
            fingerprint: "ab12cd34".to_owned(),
            grants: vec![
                "grant.session.list".to_owned(),
                "grant.session.read".to_owned(),
            ],
            expires_at: "2026-09-23T14:31:00.000Z".to_owned(),
        }
    }

    /// 配对方法调用的 spy：记录方法与 `params`，用于断言「哪条路径调了哪些方法」。
    #[derive(Default)]
    struct CallSpy {
        calls: Vec<(Method, JsonObject)>,
        /// 注入的方法侧失败（`Some` 时每条调用都以它失败）。
        failure: Option<(String, String)>,
    }

    impl CallSpy {
        fn methods(&self) -> Vec<Method> {
            self.calls.iter().map(|(method, _)| *method).collect()
        }

        fn params(&self, index: usize) -> Value {
            Value::Object(self.calls[index].1.clone())
        }
    }

    #[async_trait::async_trait]
    impl PairCalls for CallSpy {
        async fn call(
            &mut self,
            method: Method,
            params: JsonObject,
        ) -> Result<JsonObject, Failure> {
            self.calls.push((method, params));
            match &self.failure {
                Some((code, message)) => Err(Failure::reported(code.clone(), message.clone())),
                None => Ok(JsonObject::new()),
            }
        }
    }

    /// 交互式拒绝：调用对应的 `*.pair.reject`（`reason: null`）且**不**调用 `confirm`；退出码非零。
    ///
    /// 交互路径要求 stdin 是 TTY，进程级用例无法覆盖，因此断言落在可注入的 [`PairCalls`] 接缝上。
    #[tokio::test]
    async fn an_interactive_rejection_calls_reject_and_never_confirm() {
        for (target, expected) in [
            (PairTarget::Device, Method::DevicePairReject),
            (PairTarget::Node, Method::NodePairReject),
        ] {
            let mut spy = CallSpy::default();
            let error = settle_claim(&mut spy, target, "P", &claim(), &[], Answer::Rejected)
                .await
                .expect_err("用户拒绝必须以失败退出（不得改判成功）");
            assert_eq!(
                spy.methods(),
                vec![expected],
                "拒绝路径必须终结配对会话且不调 confirm"
            );
            assert_eq!(
                spy.params(0),
                serde_json::json!({ "pairingId": "P", "reason": null })
            );
            assert_eq!(error.code(), LocalErrorCode::Conflict.as_str());
            assert!(
                error.message().contains(expected.as_str()),
                "消息必须说明调了哪个 reject：{}",
                error.message()
            );
            // 退出码与 stderr 结构化行由统一出口决定：`cli::fail` 与 `cli::failure_line` 是同一构造。
            assert_ne!(fail(&error), std::process::ExitCode::SUCCESS);
            let line: Value = serde_json::from_str(&failure_line(error.code(), error.message()))
                .expect("stderr 行必须是合法 JSON");
            assert_eq!(line["code"], Value::from(error.code()));
        }
    }

    /// `reject` 失败：按方法返回的码报错退出（非零），不吞错、不改判成功、不转而调 `confirm`。
    #[tokio::test]
    async fn a_failed_reject_exits_with_the_reported_code() {
        let mut spy = CallSpy {
            failure: Some((
                LocalErrorCode::InvalidParams.as_str().to_owned(),
                "pairing is not claimable".to_owned(),
            )),
            ..CallSpy::default()
        };
        let error = settle_claim(
            &mut spy,
            PairTarget::Device,
            "P",
            &claim(),
            &[],
            Answer::Rejected,
        )
        .await
        .expect_err("reject 失败即失败退出");
        assert_eq!(spy.methods(), vec![Method::DevicePairReject]);
        assert_eq!(
            error.code(),
            LocalErrorCode::InvalidParams.as_str(),
            "必须原样带出方法返回的码"
        );
        assert_ne!(fail(&error), std::process::ExitCode::SUCCESS);
        let line: Value = serde_json::from_str(&failure_line(error.code(), error.message()))
            .expect("stderr 行必须是合法 JSON");
        assert_eq!(line["code"], Value::from("local.invalid_params"));
    }

    /// 确认路径：只调 `confirm`，不调 `reject`。设备方向**不提交任何授权集合**（只有 `pairingId`），
    /// 节点方向提交展示过的 grants 与本次点名的 `exportIds`。
    #[tokio::test]
    async fn a_confirmation_calls_only_confirm() {
        let mut device = CallSpy::default();
        settle_claim(
            &mut device,
            PairTarget::Device,
            "P",
            &claim(),
            &[],
            Answer::Confirmed,
        )
        .await
        .expect("确认必须调 confirm");
        assert_eq!(device.methods(), vec![Method::DevicePairConfirm]);
        assert_eq!(
            device.params(0),
            serde_json::json!({ "pairingId": "P" }),
            "设备方向不得提交任何授权集合"
        );

        let mut node = CallSpy::default();
        settle_claim(
            &mut node,
            PairTarget::Node,
            "P",
            &claim(),
            &[],
            Answer::Confirmed,
        )
        .await
        .expect("确认必须调 confirm");
        assert_eq!(node.methods(), vec![Method::NodePairConfirm]);
        assert_eq!(
            node.params(0),
            serde_json::json!({
                "pairingId": "P",
                "grants": ["grant.session.list", "grant.session.read"],
                "exportIds": [],
            })
        );
    }

    /// 回答词：只有 `y`/`Y`（去首尾空白后）算确认，其余一切（含空行与 EOF）都是拒绝（因此会走 reject 路径）。
    #[test]
    fn only_an_explicit_yes_is_a_confirmation() {
        assert_eq!(answer_for("y\n"), Answer::Confirmed);
        assert_eq!(answer_for("Y\n"), Answer::Confirmed);
        // 首尾空白按既有行为容忍（用户按回车前的空格不该被当成拒绝）。
        assert_eq!(answer_for(" y \n"), Answer::Confirmed);
        for rejected in ["n\n", "no\n", "\n", "", "yes\n"] {
            assert_eq!(answer_for(rejected), Answer::Rejected, "{rejected:?}");
        }
    }

    /// 逐字匹配：相等才放行；前缀、大小写、空白、长度差异一律失败（R66 的判定）。
    #[test]
    fn supplied_values_must_match_the_status_values_verbatim() {
        let claim = claim();
        let exact = Supplied {
            fingerprint: "ab12cd34".to_owned(),
        };
        verify_supplied(&claim, &exact).expect("逐字一致必须放行");

        for fingerprint in [
            "ab12cd3",   // 只匹配前缀
            "ab12cd340", // 更长
            "AB12CD34",  // 大小写不同
            " ab12cd34", // 带前导空白
            "ab12cd34 ", // 带尾随空白
            "",          // 空值
        ] {
            let supplied = Supplied {
                fingerprint: fingerprint.to_owned(),
            };
            let error =
                verify_supplied(&claim, &supplied).expect_err(&format!("{fingerprint} 必须失败"));
            assert_eq!(error.code(), "local.invalid_params");
            assert!(
                !error.message().contains("ab12cd34"),
                "错误消息不得回显指纹：{}",
                error.message()
            );
        }
    }

    /// 不匹配时**不改状态**：`run` 的顺序是「先逐字校验，再落定」，校验失败即返回，任何配对方法都不被调用
    /// （未调 confirm、也未调 reject），退出码非零。
    #[tokio::test]
    async fn a_mismatched_fingerprint_settles_nothing() {
        let mut spy = CallSpy::default();
        let claim = claim();
        let supplied = Supplied {
            fingerprint: "ab12cd3".to_owned(),
        };
        let outcome = match verify_supplied(&claim, &supplied) {
            Ok(()) => settle_claim(
                &mut spy,
                PairTarget::Device,
                "P",
                &claim,
                &[],
                Answer::Confirmed,
            )
            .await
            .map(|_| ()),
            Err(error) => Err(error),
        };
        let error = outcome.expect_err("前缀匹配必须失败");
        assert_eq!(error.code(), "local.invalid_params");
        assert!(
            spy.methods().is_empty(),
            "指纹不匹配时不得调用任何配对方法：{:?}",
            spy.methods()
        );
        assert_ne!(fail(&error), std::process::ExitCode::SUCCESS);
    }

    /// 非交互必须显式给出指纹；交互式可以只靠终端提示（`None`）。
    #[test]
    fn non_interactive_confirmation_requires_the_fingerprint() {
        assert!(
            Supplied::parse(Some("ab12cd34"), false)
                .expect("给了指纹")
                .is_some()
        );
        assert!(Supplied::parse(None, true).expect("交互式").is_none());
        for (fingerprint, interactive) in [
            (None, false),     // 非交互 + 没给
            (Some(""), true),  // 空指纹
            (Some(""), false), // 非交互 + 空指纹
        ] {
            let error = Supplied::parse(fingerprint, interactive).expect_err("必须明确失败");
            assert_eq!(error.code(), "local.invalid_request");
        }
    }

    /// `begin`/`confirm` 的 `params` 与 §5.3/§5.4 的字段同名同形（kebab-case flags → camelCase）。
    #[test]
    fn pairing_params_match_the_wire_fields() {
        let device = PairArgs {
            expires_in_ms: Some(60_000),
            ..PairArgs::default()
        };
        assert_eq!(
            Value::Object(begin_params(PairTarget::Device, &device)),
            serde_json::json!({ "expiresInMs": 60_000 })
        );
        assert_eq!(
            Value::Object(begin_params(PairTarget::Device, &PairArgs::default())),
            serde_json::json!({ "expiresInMs": null })
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
        // 设备方向**没有** `exportIds` 面（§5.3 的形状），也不提交任何授权集合。
        assert_eq!(
            Value::Object(confirm_params(
                PairTarget::Device,
                "P",
                &claim,
                &["export-1".to_owned()],
            )),
            serde_json::json!({ "pairingId": "P" })
        );
        // 节点方向：零个 `--export-id` → 空清单；多个 → 按命令行顺序原样提交（Daemon 归一化）。
        assert_eq!(
            Value::Object(confirm_params(PairTarget::Node, "P", &claim, &[])),
            serde_json::json!({
                "pairingId": "P",
                "grants": ["grant.session.list", "grant.session.read"],
                "exportIds": [],
            })
        );
        assert_eq!(
            Value::Object(confirm_params(
                PairTarget::Node,
                "P",
                &claim,
                &["export-b".to_owned(), "export-a".to_owned()],
            )),
            serde_json::json!({
                "pairingId": "P",
                "grants": ["grant.session.list", "grant.session.read"],
                "exportIds": ["export-b", "export-a"],
            })
        );
    }

    /// §5.4/`design.md` D6：零个 `--export-id` 时给出醒目提醒但**不改判**（确认仍照常执行），
    /// 且提醒只属于节点方向（设备方向没有这一面）。
    #[tokio::test]
    async fn an_empty_export_id_list_warns_without_blocking() {
        let notice = empty_export_ids_notice(PairTarget::Node, &[])
            .expect("空清单必须给出提醒（该节点将看不到任何 Export）");
        assert!(notice.contains("该节点将看不到任何 Export"), "{notice}");
        assert!(notice.contains("警告"), "{notice}");
        assert!(
            empty_export_ids_notice(PairTarget::Node, &["export-1".to_owned()]).is_none(),
            "给了清单就不提醒"
        );
        assert!(
            empty_export_ids_notice(PairTarget::Device, &[]).is_none(),
            "设备方向没有 exportIds 面，不提醒"
        );

        // 「不阻断」的机器可判据：提醒存在的同时，确认路径照常调 `confirm`，且 `exportIds` 是空数组
        // （不是省略字段——省略会被 Daemon 以 `local.invalid_params` 拒绝）。
        let mut spy = CallSpy::default();
        settle_claim(
            &mut spy,
            PairTarget::Node,
            "P",
            &claim(),
            &[],
            Answer::Confirmed,
        )
        .await
        .expect("提醒不得阻断确认");
        assert_eq!(spy.methods(), vec![Method::NodePairConfirm]);
        assert_eq!(spy.params(0)["exportIds"], serde_json::json!([]));
    }

    /// `node pair` 的命令行映射：`--export-id` 可重复、零次得到空清单（`node list` 的展示由 Daemon 的
    /// `result` 原样输出，见 `view::node` 的 `exportIds`）。
    #[test]
    fn the_node_pair_flags_map_to_the_confirm_params() {
        let empty = PairArgs::from(&super::super::NodePair {
            mode: Some("owner".to_owned()),
            pairing_url: None,
            display_name: None,
            grant: Vec::new(),
            export_id: Vec::new(),
            fingerprint: None,
        });
        assert!(empty.export_ids.is_empty(), "零次 = 空清单");
        let two = PairArgs::from(&super::super::NodePair {
            mode: Some("owner".to_owned()),
            pairing_url: None,
            display_name: None,
            grant: Vec::new(),
            export_id: vec!["export-1".to_owned(), "export-2".to_owned()],
            fingerprint: None,
        });
        assert_eq!(two.export_ids, vec!["export-1", "export-2"]);
    }

    /// 设备方向的展示块必须含名称、指纹与过期时间，且**不含**短验证码、也不含任何授权集合行
    /// （`device.pair.status` 已不返回这些字段）。
    #[test]
    fn the_claim_display_shows_every_required_field() {
        let device = claim().display(PairTarget::Device);
        for expected in ["Zhang's Phone", "ab12cd34", "2026-09-23T14:31:00.000Z"] {
            assert!(device.contains(expected), "展示块缺 `{expected}`：{device}");
        }
        for forbidden in ["SAS", "481502", "grant.", "scope"] {
            assert!(
                !device.contains(forbidden),
                "设备方向不得展示 `{forbidden}`：{device}"
            );
        }

        // 节点方向另展示待授予的 grants（§5.4 / `SECURITY_DESIGN.md` §9.4）。
        let node = claim().display(PairTarget::Node);
        for expected in [
            "ab12cd34",
            "grant.session.list, grant.session.read",
            "2026-09-23T14:31:00.000Z",
        ] {
            assert!(node.contains(expected), "节点展示块缺 `{expected}`：{node}");
        }
        // §5.4 的 `node.pair.status` 没有对端展示名字段：节点方向不展示名称行（即使 Claim 里有）。
        assert!(
            !node.contains("Zhang's Phone"),
            "节点方向不展示名称：{node}"
        );
        assert!(!node.contains("SAS"), "节点方向也不展示短验证码：{node}");
    }

    /// 认领字段的读取面按方向不同：设备方向的状态响应没有 `requestedGrants`（读到空集合仍成功），
    /// 节点方向缺该字段即 `local.internal`（不静默用空集合确认）。
    #[test]
    fn the_claim_fields_are_read_per_direction() {
        let mut device = JsonObject::new();
        device.insert("displayName".to_owned(), Value::from("Phone"));
        device.insert("publicKeyFingerprint".to_owned(), Value::from("ab12cd34"));
        device.insert(
            "expiresAt".to_owned(),
            Value::from("2026-09-23T14:31:00.000Z"),
        );
        let claim = Claim::from_status(PairTarget::Device, &device).expect("设备方向四字段即可");
        assert!(claim.grants.is_empty(), "设备方向不得有授权集合");

        // §5.4 的 `node.pair.status` **没有** `displayName`：节点方向的认领与展示都不得要求它。
        let mut node = JsonObject::new();
        node.insert(
            "peerPublicKeyFingerprint".to_owned(),
            Value::from("ab12cd34"),
        );
        node.insert(
            "expiresAt".to_owned(),
            Value::from("2026-09-23T14:31:00.000Z"),
        );
        node.insert(
            "requestedGrants".to_owned(),
            Value::from(Vec::<String>::new()),
        );
        let claim = Claim::from_status(PairTarget::Node, &node).expect("节点方向不需要名称字段");
        assert!(
            claim.display_name.is_none(),
            "节点方向的状态响应没有名称字段"
        );

        let mut missing = node.clone();
        missing.remove("requestedGrants");
        let error = Claim::from_status(PairTarget::Node, &missing).expect_err("缺 requestedGrants");
        assert_eq!(error.code(), "local.internal");
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
