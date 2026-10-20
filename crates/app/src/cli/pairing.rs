//! 设备/节点配对的安全确认仪式（`docs/LOCAL_ADMIN_PROTOCOL.md` §5.3/§5.4/§5.8、
//! `specs/cli-commands` 的「配对安全确认仪式」R64–R66）。
//!
//! 流程固定为三步：`*.pair.begin` → `*.pair.status`（CLI 轮询）→ `*.pair.confirm`。
//!
//! - `begin` 拿到的 `pairingUrl` 与（设备方向的）`pairingCode` **只**打印到终端（内存 → 终端），不写日志、
//!   不写文件、不进错误消息；`pairingUrl` 供扫码，`pairingCode` 供网页 `/pair` 输入；
//! - 轮询由 CLI 负责（间隔 [`POLL_INTERVAL`]），有效期以 `status`/`begin` 返回的 `expiresAt` 为准
//!   （§1.1 的定宽文本，字典序即时间序，因此本地不需要日期库），到期以 `local.expired` 结束；
//! - 认领后**必须**先展示名称、指纹与过期时间（节点方向另含待授予的 `grants`），再允许确认；设备方向
//!   没有任何授权词汇，节点方向提交的是**展示过的** grants，而不是命令行上的 `--grant` 原文；
//! - 非交互（`stdin` 不是终端）时 `--fingerprint` 必须显式给出并与 `status` 的返回值**逐字**相等才调
//!   `confirm`；不相等即失败且不修改任何状态。没有「无参数自动确认」开关，也没有短验证码参数。

use std::io::{IsTerminal as _, Write as _};
use std::time::{Duration, Instant};

use acp_core::ports::Clock as _;
use qrcode::{Color, EcLevel, QrCode};
use serde_json::Value;
use server::local_admin::{JsonObject, LocalErrorCode, Method};

use crate::cli::{Context, Failure, client_failure, print_result, success};
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
    // `node pair --mode access`：本机是**被 Owner 确认**的一侧，流程是「claim → 等待 Owner 本机确认」，
    // 没有本地 confirm（ONLY Owner 的确认才创建信任，见 `LOCAL_ADMIN_PROTOCOL.md` §5.4）。
    if target == PairTarget::Node && args.mode.as_deref() == Some("access") {
        return run_access(context, args, supplied);
    }
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
        answer,
    ))?;
    print_result(&result);
    let id = text_field(&result, target.id_key())?;
    println!("已配对：{}={id}", target.id_key());
    Ok(())
}

/// `node pair --mode access` 的完整仪式。
///
/// 与 owner 方向的三点差别（`LOCAL_ADMIN_PROTOCOL.md` §5.4）：
///
/// - 没有本地 `confirm`：claim 之后由 Owner 本机确认，本 CLI 只轮询到 `approved`；
/// - `--fingerprint` 对照的是 **Owner** 的指纹（二维码页面显示的那一枚），在轮询之前校验，不一致即
///   调 `reject` 终结本地 claim 并失败；
/// - 不打印配对 URL（它含一次性的 pairing secret）：用户已经把它交给命令，终端不再重复回显。
fn run_access(
    context: &mut Context,
    args: &PairArgs,
    supplied: Option<Supplied>,
) -> Result<(), Failure> {
    let endpoint = context.endpoint()?;
    let runtime = context.runtime()?;
    let claim = runtime.block_on(access_begin(&endpoint, args))?;
    let shown = Claim {
        display_name: None,
        fingerprint: claim.fingerprint.clone(),
        expires_at: claim.expires_at.clone(),
    };
    let verified = match &supplied {
        Some(supplied) => {
            verify_supplied(&shown, supplied)?;
            true
        }
        None => ask_owner_fingerprint(&claim)?,
    };
    if !verified {
        let _ = runtime.block_on(access_reject(&endpoint, &claim.pairing_id));
        return Err(Failure::local(
            LocalErrorCode::Conflict,
            "用户未确认 Owner 指纹：已调用 `node.pair.reject` 终结本地 claim（未建立信任）",
        ));
    }
    let approved = runtime.block_on(access_poll(&endpoint, &claim))?;
    println!("已配对：nodeId={}", approved.node_id);
    Ok(())
}

/// `node.pair.begin`（access）的结果（§5.4 的 `result` 中本 CLI 需要的字段）。
struct AccessClaim {
    pairing_id: String,
    owner_node_id: String,
    fingerprint: String,
    expires_at: String,
}

/// 发起 claim 并展示 Owner 身份。
async fn access_begin(endpoint: &str, args: &PairArgs) -> Result<AccessClaim, Failure> {
    let mut client = LocalAdminClient::connect(endpoint)
        .await
        .map_err(client_failure)?;
    let begin = call(
        &mut client,
        Method::NodePairBegin,
        begin_params(PairTarget::Node, args),
    )
    .await?;
    let claim = AccessClaim {
        pairing_id: text_field(&begin, "pairingId")?,
        owner_node_id: text_field(&begin, "peerNodeId")?,
        fingerprint: text_field(&begin, "peerPublicKeyFingerprint")?,
        expires_at: text_field(&begin, "expiresAt")?,
    };
    println!("claim 已提交，等待 Owner 本机确认…");
    println!("  Owner           {}", claim.owner_node_id);
    println!("  指纹            {}", claim.fingerprint);
    println!("  过期            {}", claim.expires_at);
    Ok(claim)
}

/// 汇总结果：Owner 已批准。
struct AccessApproved {
    node_id: String,
}

/// 轮询 `node.pair.status` 直到终态；`pending_confirmation` 之外的词表一律按协议错误失败。
async fn access_poll(endpoint: &str, claim: &AccessClaim) -> Result<AccessApproved, Failure> {
    let mut client = LocalAdminClient::connect(endpoint)
        .await
        .map_err(client_failure)?;
    let started = Instant::now();
    loop {
        if now_text().as_str() >= claim.expires_at.as_str() {
            return Err(Failure::local(
                LocalErrorCode::Expired,
                "Owner 配对已过期（未在有效期内完成本机确认）",
            ));
        }
        if started.elapsed() >= MAX_WAIT {
            return Err(Failure::local(
                LocalErrorCode::Unavailable,
                "等待 Owner 确认超时",
            ));
        }
        let mut params = JsonObject::new();
        params.insert(
            "pairingId".to_owned(),
            Value::from(claim.pairing_id.as_str()),
        );
        let status = call(&mut client, Method::NodePairStatus, params).await?;
        let state = text_field(&status, "state")?;
        match state.as_str() {
            "approved" => {
                return Ok(AccessApproved {
                    node_id: text_field(&status, "peerNodeId")?,
                });
            }
            "pending_confirmation" => {}
            "rejected" => {
                return Err(Failure::local(
                    LocalErrorCode::Expired,
                    "Owner 拒绝了本次配对（未建立信任）",
                ));
            }
            "expired" | "consumed" => {
                return Err(Failure::local(
                    LocalErrorCode::Expired,
                    "该配对已过期或 secret 已被使用（重新生成二维码后再试）",
                ));
            }
            _ => {
                return Err(Failure::local(
                    LocalErrorCode::Internal,
                    format!("node.pair.status 返回了未登记的状态 `{state}`"),
                ));
            }
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

/// 终结本地 claim（协议没有 Owner 侧的拒绝端点；这条调用只清本机内存态）。
async fn access_reject(endpoint: &str, pairing_id: &str) -> Result<(), Failure> {
    let response = call_once(endpoint, Method::NodePairReject, reject_params(pairing_id))
        .await
        .map_err(client_failure)?;
    success(&outcome_of(&response))?;
    Ok(())
}

/// 交互式核对 Owner 指纹：只有明确的 `y`/`Y` 才继续（与 owner 方向的确认仪式同一口径）。
fn ask_owner_fingerprint(claim: &AccessClaim) -> Result<bool, Failure> {
    print!(
        "确认 Owner 指纹与本机二维码页面显示的一致（{}）？[y/N]: ",
        claim.fingerprint
    );
    if std::io::stdout().flush().is_err() {
        return Err(Failure::local(
            LocalErrorCode::Internal,
            "无法把确认提示写到 stdout",
        ));
    }
    let mut line = String::new();
    let read = std::io::BufRead::read_line(&mut std::io::stdin().lock(), &mut line);
    if read.is_err() {
        // 读失败（不是 EOF）：用户意图未知，因此既不确认也不替用户拒绝。
        return Err(Failure::local(LocalErrorCode::Internal, "无法读取确认输入"));
    }
    Ok(matches!(line.trim(), "y" | "Y"))
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
    // `pairingUrl` 只经终端展示；设备方向 `#q2=` fragment 是扫码取回凭据（配对秘密不在地址里），
    // 节点方向 `#data=` 仍含完整资料。两者都不落日志/文件/错误消息。
    let pairing_url = text_field(&begin, "pairingUrl")?;
    if target == PairTarget::Device {
        // 网页端用裸 `/pair` + 8 位配对码；完整 fragment 只由二维码承载，供原生 App 扫码，
        // 不作为文本输出（stderr/stdout/文件都不出现完整 URL）。
        println!("网页端：{}", bare_pairing_url(&pairing_url));
        let pairing_code = text_field(&begin, "pairingCode")?;
        println!("配对码：{}", readable_pairing_code(&pairing_code)?);
        println!();
        println!("二维码：");
        let qr = terminal_qr(&pairing_url)?;
        println!("{qr}");
        println!();
        println!("（网页端打开 /pair 后输入配对码；原生 App 扫码后按提示确认）");
    } else {
        println!("{pairing_url}");
    }
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

/// 网页端入口：`pairingUrl` 去掉 fragment（`#q2=…`）后的裸地址。
fn bare_pairing_url(pairing_url: &str) -> &str {
    pairing_url
        .split_once('#')
        .map(|(base, _)| base)
        .unwrap_or(pairing_url)
}

/// 把完整 `pairingUrl` 渲染成终端二维码；二维码是移动端入口，矩阵不把 URL 作为文本输出。
fn terminal_qr(pairing_url: &str) -> Result<String, Failure> {
    // L 级纠错在完整 URL 上选到更小的版本；终端二维码只用于近距离扫码，不需要海报级冗余。
    let code =
        QrCode::with_error_correction_level(pairing_url.as_bytes(), EcLevel::L).map_err(|_| {
            Failure::local(
                LocalErrorCode::Internal,
                "无法生成终端二维码；请重试 device pair",
            )
        })?;
    Ok(block_qr(&code))
}

/// 每个字符绘制上下两个模块，固定黑字白底，四周保留至少四模块白边。
fn block_qr(code: &QrCode) -> String {
    let width = code.width();
    let canvas = (width + 8).div_ceil(2) * 2;
    let colors = code.to_colors();
    let dark = |x: usize, y: usize| {
        x >= 4
            && y >= 4
            && x < width + 4
            && y < width + 4
            && colors[(y - 4) * width + x - 4] == Color::Dark
    };
    let mut out = String::new();
    for y in (0..canvas).step_by(2) {
        if y > 0 {
            out.push('\n');
        }
        out.push_str("\x1b[0;30;107m");
        for x in 0..canvas {
            out.push(match (dark(x, y), dark(x, y + 1)) {
                (true, true) => '█',
                (true, false) => '▀',
                (false, true) => '▄',
                (false, false) => ' ',
            });
        }
        out.push_str("\x1b[0m");
    }
    out
}

/// 把服务端返回的规范配对码（8 位大写、无分隔符）展示为 `XXXX-XXXX`。
///
/// 复用 `identity-auth` 的归一化解析器而不是在这里切片：非法形状按协议违规失败（消息不回显码），
/// 规范形态由同一个实现决定，CLI 不另造一套编解码。
fn readable_pairing_code(code: &str) -> Result<String, Failure> {
    let code = identity_auth::PairingCode::parse(code).map_err(|_| {
        Failure::local(
            LocalErrorCode::Internal,
            "配对方法的响应包含非法配对码（未输出该值）",
        )
    })?;
    let text = code.as_str();
    Ok(format!("{}-{}", &text[..4], &text[4..]))
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 规范码与用户输入形态都展示为 `XXXX-XXXX`；解析失败时不回显原值。
    #[test]
    fn pairing_code_is_printed_in_readable_groups() {
        assert_eq!(
            readable_pairing_code("ABCD1234").expect("规范形态"),
            "ABCD-1234"
        );
        // 输入归一化（小写、易混字符、分隔符）由 `PairingCode::parse` 统一完成。
        assert_eq!(
            readable_pairing_code("abcd-1234").expect("大小写与分隔符"),
            "ABCD-1234"
        );
        assert_eq!(
            readable_pairing_code("OIL1-2345").expect("Crockford 归一化"),
            "0111-2345"
        );

        let failure = readable_pairing_code("not-a-code").expect_err("非法形状必须失败");
        assert_eq!(failure.code(), "local.internal");
        assert!(
            !failure.message().contains("not-a-code"),
            "错误消息不得回显配对码：{}",
            failure.message()
        );
    }

    /// 固定测试前缀下的新短地址：134 字节，L 级矩阵含四模块白边不超过 58 列 × 29 行
    /// （`specs/cli-commands`「配对三步流程与终端输出」、`SYNC_PROTOCOL.md` §7.1 的长度预算）。
    #[test]
    fn short_qr_fits_the_documented_full_size_budget() {
        use base64::Engine as _;
        let mut payload = Vec::with_capacity(65);
        payload.push(2_u8);
        payload.extend_from_slice(&[7_u8; 32]);
        payload.extend_from_slice(&[9_u8; 32]);
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload);
        assert_eq!(payload.len(), 87, "65 字节载荷编码为 87 个 ASCII 字符");
        let url = format!("https://faraday.tail74842e.ts.net:8765/pair#q2={payload}");
        assert_eq!(url.len(), 134, "固定前缀 + q2 + 87 字符的短地址长度");
        assert!(url.len() <= 150, "测试地址承诺的总长度上界");
        assert!(!url.contains("#data="), "不得回退到旧设备完整资料格式");

        let rendered = terminal_qr(&url).expect("终端二维码");
        let lines: Vec<Vec<char>> = rendered
            .lines()
            .map(|line| {
                line.strip_prefix("\x1b[0;30;107m")
                    .expect("黑字白底")
                    .strip_suffix("\x1b[0m")
                    .expect("每行复位")
                    .chars()
                    .collect()
            })
            .collect();
        let columns = lines.first().expect("至少一行").len();
        assert!(
            lines.len() <= 29,
            "矩阵含白边不得超过 29 行，实际 {}",
            lines.len()
        );
        assert!(columns <= 58, "矩阵含白边不得超过 58 列，实际 {columns}");
        assert!(
            lines.iter().all(|line| line.len() == columns),
            "每一行的列数必须一致"
        );
    }

    #[test]
    fn terminal_qr_preserves_modules_and_white_quiet_zone() {
        use base64::Engine as _;
        let mut payload = Vec::with_capacity(65);
        payload.push(2_u8);
        payload.extend_from_slice(&[3_u8; 32]);
        payload.extend_from_slice(&[5_u8; 32]);
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload);
        let url = format!("https://faraday.tail74842e.ts.net:8765/pair#q2={payload}");
        let code =
            QrCode::with_error_correction_level(url.as_bytes(), EcLevel::L).expect("参考矩阵");
        let rendered = terminal_qr(&url).expect("终端二维码");
        let lines: Vec<Vec<char>> = rendered
            .lines()
            .map(|line| {
                line.strip_prefix("\x1b[0;30;107m")
                    .expect("黑字白底")
                    .strip_suffix("\x1b[0m")
                    .expect("每行复位")
                    .chars()
                    .collect()
            })
            .collect();
        let canvas = lines[0].len();
        assert_eq!(canvas, lines.len() * 2);
        assert!(lines.iter().all(|line| line.len() == canvas));
        for y in 0..canvas {
            for x in 0..canvas {
                let actual = match lines[y / 2][x] {
                    '█' => true,
                    '▀' => y % 2 == 0,
                    '▄' => y % 2 == 1,
                    ' ' => false,
                    other => panic!("非半块字符：{other}"),
                };
                let expected = x >= 4
                    && y >= 4
                    && x < code.width() + 4
                    && y < code.width() + 4
                    && code[(x - 4, y - 4)] == Color::Dark;
                assert_eq!(actual, expected, "模块 ({x}, {y}) 必须保真，含四周白边");
            }
        }
    }
}
