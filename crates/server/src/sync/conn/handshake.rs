//! 握手阶段的判定与装配（`docs/SYNC_PROTOCOL.md` §5/§8、`design.md` D1/D2）。
//!
//! 本模块是纯函数层：由 `conn::session` 调用，输入是解码后的 `auth.client_hello` 与 core 的
//! [`SyncDeviceHandshakeView`]（当次持久事实快照），输出是 `PeerTrust`、连接绑定、协商出的 feature 与
//! 错误分类。**没有任何 IO、不读系统时间**（时间与签名都经 `identity_auth::Authority`）。
//!
//! 三个不变量：
//!
//! - **验签公钥只来自持久化快照**：`PeerTrust.public_key` 取自 `TrustStore::peer_key`（经用例面读取），
//!   从不取自握手消息自报的公钥（`IDENTITY_AND_AUTH_CONTRACT.md` §5.1）；
//! - **不得用错误区分设备是否存在**：未知设备的 `PeerTrust` 只是 `public_key = None` +
//!   `CredentialStatus::Unknown`，`auth.server_challenge` 照常签发；对端可见的失败推迟到 proof 阶段
//!   并统一为 `auth.proof_invalid`（§8.5）；
//! - **feature 词表只有一个来源**：`compatibility/features/v1/features.json` 的 `sync` 子集中
//!   `delivery = mvp` 的条目（§5.2 的 v1 baseline），逐条常量在此登记并有契约测试比对。

use acp_core::model::{
    DeviceId, DeviceRecord, DeviceState, PairingRecord, PairingState, PeerIdentity,
};
use acp_core::use_cases::SyncDeviceHandshakeView;
use identity_auth::{CanonicalOrigin, ConnectionBinding, CredentialStatus, FeatureList, PeerTrust};
use sync_protocol::auth::ClientHello;
use sync_protocol::common::{FeatureId, FeatureList as WireFeatureList};

/// 本切片实现的 Sync feature（`features.json` 的 `sync` 子集里 `delivery = mvp` 中已完整接线的二十一
/// 条，§5.2）。
///
/// 必需集合 = [`REQUIRED_FEATURES`] ∪ 对端 `requiredFeatures`。v1 baseline 里只有
/// `core.session-activity.v1` 是 `required: true`：三态摘要的取值语义必须两端一致，缺任何一边都会让
/// 同一份 `sessionSummary` 被按不同词表解码。`core.session-files.v1` 在真实路由、连接期内容版本释放与
/// 组合根装配完成后才登记在这里：宣告即承诺四条 `session.files.*`/`session.git.*` 查询的真实实现，
/// 不能先宣告后接线。
///
/// 协商是**交集**（见 [`negotiate_features`]），因此这里漏登记一条 feature 会让它**静默失效**：客户端
/// 声明了却被交集丢掉，服务端随后的门控（快照的 `path` 键、`workspace.browse`、`file.changed` 的
/// 差异行）一律按「未协商」处置。新增 feature 时必须同时把它加进本表。
pub const SUPPORTED_FEATURES: [&str; 21] = [
    // §10.2：会话主状态三态合同（`idle`/`running`/`pending`）。两端都必须选中，否则摘要词表可能不一致。
    "core.session-activity.v1",
    "resource.stateless-origin.v1",
    "core.snapshot.v1",
    "core.command-status.v1",
    "core.event-ack.v1",
    "acp.raw-payload.v1",
    "resource.remote-origin.v1",
    "core.local-catalog.v1",
    "core.session-create.v1",
    "core.session-history.v1",
    "core.event-origin.v1",
    "core.workspace-browse.v1",
    "core.workspace-mkdir.v1",
    "core.file-diff.v1",
    "core.session-files.v1",
    "core.session-git-stats.v1",
    "core.session-git-compare.v1",
    // §10.4：`channel`/`partial`、`audio_ref`/`file_ref` 与 `image_ref.attachmentId`（事件视图与
    // `session.read` 的消息页都有投递侧门控）。
    "core.typed-media.v1",
    // §11.7：`session.read` 的 `pending_interactions` 返回 `effectiveInteraction[]`（应答版本校验由 core
    // 的条件更新承担）。
    "core.pending-interactions.v1",
    // §3.4：独立附件连接的帧族与门控（`sync::attachments`）。组合根注册 `/sync/v1/attachments`
    // 后才把本表（或它的子集）传给连接；未注册端点时用 [`feature_list_without`] 去掉它。
    "core.attachment-transfer.v1",
    // §11.5：`session.title.set`（Owner 元数据 mutation；Sync 与 Node Link 共享同一 ID）。宣告即承诺
    // 真实接线——命令面在 `sync::command::CommandRoute::on_session_title` 里按 feature 门控，未协商
    // 时回 `protocol.feature_required`，因此只有真实路由存在时才登记。
    "core.session-title.v1",
];
// `core.remote-history.v1` **不在此表**：imported 会话的历史回源需要 Access 侧的正文路径
// （`node-link-client`）。宣告它会让客户端对一个当前必然失败的读取路径保留入口——能力协商不得
// 虚报（`AGENTS.md` §4）。WP5 接线后把它加进本表并补一条「imported 会话 history 走 Owner 回源」的
// 用例。

/// registry 里 `delivery = mvp` 但 Sync 面**尚未接线**、因此不宣告的 feature。
///
/// 显式差异而不是静默漏项：`supported_features_match_the_registry_mvp_subset` 用「mvp 减去本表」与
/// [`SUPPORTED_FEATURES`] 逐条比对，任何既没宣告、也没登记为 deferred 的 mvp feature 都会让用例失败。
pub const DEFERRED_FEATURES: [&str; 1] = ["core.remote-history.v1"];

/// 每次协商都必须选中的 feature（`features.json` 里 `required: true` 的 Sync 条目，§5.2）。
///
/// 目前只有三态会话主状态合同：缺它时客户端与 Owner 对 `sessionSummary.state` 的取值集合可能不一致，
/// 因此不能仅靠客户端自愿声明。
pub const REQUIRED_FEATURES: [&str; 1] = ["core.session-activity.v1"];

/// 本表全部 feature 的排序去重文本。
pub fn feature_list() -> Vec<String> {
    let mut features: Vec<String> = SUPPORTED_FEATURES
        .iter()
        .map(|entry| (*entry).to_owned())
        .collect();
    features.sort();
    features.dedup();
    features
}

/// 从 [`SUPPORTED_FEATURES`] 里去掉一条（组合根未注册对应端点时使用）。
pub fn feature_list_without(feature: &str) -> Vec<String> {
    let mut features: Vec<String> = SUPPORTED_FEATURES
        .iter()
        .filter(|entry| **entry != feature)
        .map(|entry| (*entry).to_owned())
        .collect();
    features.sort();
    features.dedup();
    features
}

/// v1 唯一的协议版本（§5.1 的版本交集与 `auth.server_challenge.selectedProtocolVersion`）。
pub const PROTOCOL_VERSION: u64 = 1;

/// 协商失败：版本无交集，或客户端必需 feature 未被选中。
///
/// `missing` 已排序去重（§12.2：`details.features` 的取值域要求）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NegotiationFault {
    /// 版本交集为空 → `protocol.version_unsupported`（§5.1 的 4406 路径）。
    VersionUnsupported,
    /// 必需 feature 未被选中 → `protocol.feature_required` + `details.features`。
    FeaturesMissing(Vec<String>),
}

/// 版本交集：`[minProtocolVersion, maxProtocolVersion] ∩ [1, 1]`。
pub fn negotiated_version(hello: &ClientHello) -> Result<u64, NegotiationFault> {
    let min = hello.min_protocol_version.get();
    let max = hello.max_protocol_version.get();
    if min <= PROTOCOL_VERSION && PROTOCOL_VERSION <= max {
        Ok(PROTOCOL_VERSION)
    } else {
        Err(NegotiationFault::VersionUnsupported)
    }
}

/// feature 协商：`selected = 客户端 supportedFeatures ∩ 服务端支持集合`（排序去重），
/// 然后核对必需 feature 是否全部被选中（§5.2）。
///
/// 必需集合 = 服务端 [`REQUIRED_FEATURES`] ∪ 对端 `requiredFeatures`：`core.session-activity.v1` 是
/// 协议要求每次协商都必须选中的条目，客户端未声明它时服务端仍要求它。未被选中的必需 feature 按
/// UTF-8 字节升序、去重后返回。返回值是协商结果的 feature ID 文本（排序去重）。
///
/// 本函数用 [`SUPPORTED_FEATURES`] 全集；组合根在某个端点未注册时改用
/// [`negotiate_features_with`] 传入收窄后的集合。
pub fn negotiate_features(hello: &ClientHello) -> Result<Vec<String>, NegotiationFault> {
    negotiate_features_with(hello, &feature_list())
}

/// [`negotiate_features`] 的动态版本：`supported` 是本端本次真正支持（且端点已注册）的集合。
pub fn negotiate_features_with(
    hello: &ClientHello,
    supported: &[String],
) -> Result<Vec<String>, NegotiationFault> {
    let declared: Vec<&str> = hello
        .supported_features
        .as_slice()
        .iter()
        .map(FeatureId::as_str)
        .collect();
    let mut selected: Vec<String> = supported
        .iter()
        .filter(|feature| declared.contains(&feature.as_str()))
        .cloned()
        .collect();
    // `SUPPORTED_FEATURES` 的登记顺序不是字典序（按 §5.2 的表格顺序），这里统一排序，
    // 使 `auth.server_challenge.selectedFeatures` 与逐字节升序的 transcript 输入一致。
    selected.sort();
    selected.dedup();

    let mut required: Vec<String> = hello
        .required_features
        .as_slice()
        .iter()
        .map(|feature| feature.as_str().to_owned())
        .collect();
    required.extend(
        REQUIRED_FEATURES
            .iter()
            .map(|feature| (*feature).to_owned()),
    );
    required.sort();
    required.dedup();

    let missing: Vec<String> = required
        .into_iter()
        .filter(|feature| !selected.contains(feature))
        .collect();
    if !missing.is_empty() {
        return Err(NegotiationFault::FeaturesMissing(missing));
    }
    Ok(selected)
}

/// `auth.server_challenge.selectedFeatures` 的 wire 形状（`selected` 已排序去重）。
///
/// 返回 `None` 只可能来自接线错误（ID 不是合法 feature ID 或超出 64 项）：握手按内部不可用失败关闭。
pub fn wire_features(selected: &[String]) -> Option<WireFeatureList> {
    let ids = selected
        .iter()
        .map(|feature| FeatureId::parse(feature).ok())
        .collect::<Option<Vec<_>>>()?;
    WireFeatureList::new(ids).ok()
}

/// `identity-auth` 的 negotiated features（它自己的 `FeatureList`：排序后以 NUL 连接进 transcript）。
pub fn transcript_features(selected: &[String]) -> FeatureList {
    FeatureList::new(selected.iter().cloned())
}

/// 当次持久事实 → `PeerTrust`（`IDENTITY_AND_AUTH_CONTRACT.md` §5.1 的握手输入）。
///
/// 凭据状态按设备信任行的三态映射：`active` → `Active`；`revoked` → `Revoked`；`pending` 与「无信任行」
/// 都按 `Unknown` 处理（配对批准写下的行是 `active`，`pending` 行尚未建立信任）。设备侧**没有授权
/// 集合**（`pairing-model` 删除设备权限层），因此这里既不取、也不填任何 scope 字段：`node_kind`/`grants`
/// 同样对设备不适用。
pub fn peer_trust(view: &SyncDeviceHandshakeView, device: &DeviceId) -> PeerTrust {
    let credential = match view.device.as_ref().map(DeviceRecord::state) {
        Some(DeviceState::Active) => CredentialStatus::Active,
        Some(DeviceState::Revoked) => CredentialStatus::Revoked,
        Some(DeviceState::Pending) | None => CredentialStatus::Unknown,
    };
    PeerTrust {
        peer: PeerIdentity::Device(device.clone()),
        public_key: view.public_key.clone(),
        credential,
        // 设备侧的绑定是「登记方在该配对里宣告的 canonical origin」（设备信任行不带该列，与 Node Link
        // 同形）：`Authority::check_binding` 在这里逐字比较 `origin.as_str()`。
        host_binding: view
            .pairing
            .as_ref()
            .map(|pairing| pairing.host_binding().to_owned()),
        node_kind: None,
    }
}

/// 连接绑定：设备侧的 `canonical_origin` 必须是 `Some`、`endpoint` 必须是 `None`（`check_binding` 的
/// `SyncDevice` 分支），`host` 取该 origin 的 authority。
///
/// **`host` 的形态是这里唯一正确的选择**：`Authority::check_binding` 要求
/// `binding.host` 与 `origin.authority()` **大小写不敏感相等**，而 `authority()` **含显式端口**
/// （`https://pc.example.test:8443` → `pc.example.test:8443`）。接入层的 `HostPolicy` 归一化忽略端口，
/// 因此**不得**用请求的 `Host` 头充当该字段。从同一个解析结果派生两处，也使「自洽校验」不可能失败。
pub fn connection_binding(origin: &CanonicalOrigin) -> ConnectionBinding {
    ConnectionBinding {
        canonical_origin: Some(origin.clone()),
        host: origin.authority().to_owned(),
        endpoint: None,
    }
}

/// 该设备待消费的配对（`approved` 尚未 `consumed`）：`Authority::complete_auth` 的配对目标。
pub fn pending_pairing(view: &SyncDeviceHandshakeView) -> Option<PairingRecord> {
    view.pairing
        .as_ref()
        .filter(|pairing| pairing.state() == PairingState::Approved)
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hello(supported: &[&str], required: &[&str]) -> ClientHello {
        ClientHello {
            min_protocol_version: sync_protocol::common::UIntAtLeast::<1>::new(1).expect("v1"),
            max_protocol_version: sync_protocol::common::UIntAtLeast::<1>::new(1).expect("v1"),
            host_id: sync_protocol::common::Uuid::parse("10111213-1415-4617-9819-1a1b1c1d1e1f")
                .expect("uuid"),
            device_id: sync_protocol::common::Uuid::parse("20212223-2425-4627-a829-2a2b2c2d2e2f")
                .expect("uuid"),
            client_kind: sync_protocol::auth::ClientKind::Pwa,
            client_nonce: sync_protocol::common::Base64Url::<32>::parse(
                "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8",
            )
            .expect("nonce"),
            supported_features: WireFeatureList::new(
                supported
                    .iter()
                    .map(|feature| FeatureId::parse(feature).expect("feature"))
                    .collect(),
            )
            .expect("feature list"),
            required_features: WireFeatureList::new(
                required
                    .iter()
                    .map(|feature| FeatureId::parse(feature).expect("feature"))
                    .collect(),
            )
            .expect("feature list"),
        }
    }

    /// 未注册附件端点时，组合根用 [`feature_list_without`] 收窄本端集合：对端声明了该 feature 也不会被
    /// 选中；对端把它列为必需时协商必须失败（能力协商不得虚报，`AGENTS.md` §4）。
    #[test]
    fn an_unregistered_attachment_endpoint_is_never_announced() {
        let attachment = crate::sync::attachments::ATTACHMENT_FEATURE;
        let narrowed = feature_list_without(attachment);
        assert!(
            !narrowed.iter().any(|feature| feature == attachment),
            "未注册的端点必须从本端集合里去掉"
        );

        let mut supported = REQUIRED_FEATURES.to_vec();
        supported.push(attachment);
        supported.push("core.snapshot.v1");
        let declared = hello(&supported, &[]);
        let selected =
            negotiate_features_with(&declared, &narrowed).expect("服务端必需 feature 已满足");
        assert!(
            !selected.iter().any(|feature| feature == attachment),
            "未注册的端点不得出现在 selectedFeatures 里"
        );
        assert!(selected.iter().any(|feature| feature == "core.snapshot.v1"));

        let required = hello(&supported, &[attachment]);
        assert!(
            negotiate_features_with(&required, &narrowed).is_err(),
            "对端必需但本端未注册时必须失败关闭"
        );
    }

    /// `core.session-activity.v1` 是服务端必需 feature：客户端只声明别的 feature 时也必须拒绝，
    /// 且未满足集合按 UTF-8 字节升序去重。
    #[test]
    fn a_client_without_the_three_state_contract_is_rejected() {
        let fault = negotiate_features(&hello(&["core.snapshot.v1"], &["core.snapshot.v1"]))
            .expect_err("core.session-activity.v1 是服务端必需 feature");
        assert_eq!(
            fault,
            NegotiationFault::FeaturesMissing(vec!["core.session-activity.v1".to_owned()])
        );
    }

    /// 仓库根下的路径（集成测试的 cwd 不保证是仓库根）。
    fn repo_path(relative: &str) -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join(relative)
    }

    /// 宣告集合必须与 registry 的 `sync` 子集逐条相等（减去显式 deferred 的那一条）。
    ///
    /// 词表的唯一机器来源是 `compatibility/features/v1/features.json`：这里同时挡住「宣告了 registry
    /// 之外的 ID」「漏掉一条 mvp」「把 post_mvp 当成 mvp」与「required 集合漂移」四种偏差——能力协商
    /// 不得虚报，也不得静默漏项（`AGENTS.md` §4）。
    #[test]
    fn supported_features_match_the_registry_mvp_subset() {
        let registry =
            std::fs::read_to_string(repo_path("compatibility/features/v1/features.json"))
                .expect("features.json 必须存在");
        let registry: serde_json::Value = serde_json::from_str(&registry).expect("features.json");
        let entries = registry["protocols"]["sync"]["features"]
            .as_array()
            .expect("sync.features");
        let mvp: Vec<String> = entries
            .iter()
            .filter(|entry| entry["delivery"].as_str() == Some("mvp"))
            .map(|entry| entry["id"].as_str().expect("feature id").to_owned())
            .collect();
        let mut expected: Vec<String> = mvp
            .iter()
            .filter(|id| !DEFERRED_FEATURES.contains(&id.as_str()))
            .cloned()
            .collect();
        let mut declared: Vec<String> = SUPPORTED_FEATURES
            .iter()
            .map(|id| (*id).to_owned())
            .collect();
        excluded(&mut expected);
        excluded(&mut declared);
        assert_eq!(declared, expected);
        // deferred 必须真的是 registry 的 mvp，且不得同时出现在宣告集合里。
        for id in DEFERRED_FEATURES {
            assert!(
                mvp.iter().any(|mvp| mvp == id),
                "{id} is not an mvp feature"
            );
            assert!(
                !SUPPORTED_FEATURES.contains(&id),
                "{id} cannot be both declared and deferred"
            );
        }
        // registry 里 `required: true` 的条目必须正好是本端必需集合（顺序无关）。
        let mut registry_required: Vec<&str> = entries
            .iter()
            .filter(|entry| entry["required"].as_bool() == Some(true))
            .map(|entry| entry["id"].as_str().expect("id"))
            .collect();
        registry_required.sort();
        let mut expected_required = REQUIRED_FEATURES.to_vec();
        expected_required.sort();
        assert_eq!(registry_required, expected_required);
    }

    /// 排序去重，使比较只看集合语义（登记顺序是 §5.2 的表格顺序，不是字典序）。
    fn excluded(features: &mut Vec<String>) {
        features.sort();
        features.dedup();
    }
}
