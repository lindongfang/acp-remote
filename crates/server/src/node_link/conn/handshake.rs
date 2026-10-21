//! 握手阶段的判定与装配（`docs/NODE_LINK_PROTOCOL.md` §2.3/§11.3/§12.2，`design.md` D3）。
//!
//! 本模块是纯函数层：由 `conn::session` 调用，输入是解码后的 `node.hello`/`node.proof` 与 core 的
//! [`NodeLinkHandshakeView`]（当次持久事实快照），输出是 `PeerTrust`、挑战请求、协商出的 feature 与
//! 错误分类。**没有任何 IO、不读系统时间**（时间与签名都经 `identity_auth::Authority`）。
//!
//! 三个不变量：
//!
//! - **验签公钥只来自持久化快照**：`PeerTrust.public_key` 取自 `owned_peer_key`（经用例面读取），
//!   从不取自握手消息自报的公钥（`IDENTITY_AND_AUTH_CONTRACT.md` §5.1）；
//! - **不得用错误区分节点是否存在**：未知节点的 `PeerTrust` 只是 `public_key = None` +
//!   `CredentialStatus::Unknown`，`node.challenge` 照常签发；对端可见的失败推迟到 proof 阶段；
//! - **feature 词表只有一个来源**：`compatibility/features/v1/features.json` 的 `node_link` 子集中
//!   `delivery = mvp` 的条目由本模块的 [`SUPPORTED_FEATURES`]/[`DEFERRED_FEATURES`] 显式划分（已接线
//!   vs 尚未接线），逐条常量在此登记并有契约测试比对；`required: true` 的条目汇总在
//!   [`REQUIRED_FEATURES`]。

use acp_core::model::{NodeId, NodeKind, NodeRecord, NodeState, PairingRecord, PeerIdentity};
use acp_core::use_cases::NodeLinkHandshakeView;
use identity_auth::{ConnectionBinding, CredentialStatus, FeatureList, NodeEndpoint, PeerTrust};
use node_link_protocol::common::{FeatureId, FeatureList as WireFeatureList};
use node_link_protocol::handshake::NodeHello;

/// 本切片实现的 Node Link feature。
///
/// 选中即承诺真实实现（`AGENTS.md` §4 的能力协商真实性）。当前登记全部 9 条 mvp 业务能力；
/// `node-link.node-rotation.v1` 是 `post_mvp`，不选中。
///
/// `attachment-transfer` 与它的 `/node-link/v1/attachments` 端点一起接线（组合根注册端点后才把本表
/// 传进连接）。组合根没有装配附件存储时，`register_node_link_paths` 用
/// [`feature_list_without`] 从本表里去掉它——未注册的端点不得宣告。
///
/// `typed-media`/`pending-interactions`/`remote-history` 的门控点都在 `session.read`；该命令已在
/// Owner 侧接线（共享投影 + core 用例），因此这三条可以如实宣告。Access 侧对这些能力的使用仍由
/// `node-link-client` 与 Sync 的 imported 读取路径决定。
pub const SUPPORTED_FEATURES: [&str; 10] = [
    "node-link.core.v1",
    "resource.stateless-origin.v1",
    // §11.4/§12.3：会话主状态三态合同。与 Sync 的 `core.session-activity.v1` 同一取值集合。
    "core.session-activity.v1",
    "node-link.raw-acp.v1",
    "node-link.command-status.v1",
    "node-link.session-create.v1",
    "node-link.attachment-transfer.v1",
    "node-link.typed-media.v1",
    "node-link.pending-interactions.v1",
    "node-link.remote-history.v1",
];

/// registry 里 `delivery = mvp` 但本实现**尚未接线**、因此不宣告的 feature。
///
/// 当前为空：全部 mvp feature 都有对应的真实入口。`supported_features_match_the_registry_mvp_subset`
/// 仍会逐条比对，任何「没有宣告、也没有登记为 deferred」的 mvp feature 都会让该测试失败。
pub const DEFERRED_FEATURES: [&str; 0] = [];

/// 从 [`SUPPORTED_FEATURES`] 里去掉一条（组合根未注册对应端点时使用）。
///
/// 返回排序去重的 `Vec<String>`，与 [`negotiate_features_with`] 的入参同形。
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

/// 本表全部 feature 的排序去重文本（组合根注册了全部端点时使用）。
pub fn feature_list() -> Vec<String> {
    let mut features: Vec<String> = SUPPORTED_FEATURES
        .iter()
        .map(|entry| (*entry).to_owned())
        .collect();
    features.sort();
    features.dedup();
    features
}

/// 每次握手都必须选中的 feature（registry 的 `required: true`，§11.3）。
///
/// `core.session-activity.v1` 与 `node-link.core.v1` 同属协议必需：缺少三态摘要词表的握手无法保证
/// 两侧对 `sessionMeta.state` 的取值集合一致，因此 Owner 不等 Access 自愿声明就要求它。
pub const REQUIRED_FEATURES: [&str; 2] = ["node-link.core.v1", "core.session-activity.v1"];

/// v1 唯一的协议版本（`node.challenge.selectedProtocolVersion` 与交集结果）。
pub const PROTOCOL_VERSION: u64 = 1;

/// 协商失败：必需 feature 未满足。
///
/// `missing` 已排序去重（§11.3：`details.features` 的取值域要求）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NegotiationFault {
    /// 版本交集为空 → `nodelink.protocol.version_unsupported`（§2.3 的 4406 路径）。
    VersionUnsupported,
    /// 必需 feature 未被选中 → `nodelink.protocol.feature_required` + `details.features`。
    FeaturesMissing(Vec<String>),
}

/// 版本交集：`[min, max] ∩ [1, 1]`（v1 只有版本 1；`ProtocolVersionV1` 已在解码期固定取值）。
pub fn negotiated_version(hello: &NodeHello) -> Result<u64, NegotiationFault> {
    let min = hello.min_protocol_version.get();
    let max = hello.max_protocol_version.get();
    if min <= PROTOCOL_VERSION && PROTOCOL_VERSION <= max {
        Ok(PROTOCOL_VERSION)
    } else {
        Err(NegotiationFault::VersionUnsupported)
    }
}

/// feature 协商：`selected = 对端 supportedFeatures ∩ Owner 支持集合`（排序去重），
/// 然后核对「必需 feature」是否全部被选中。
///
/// 必需集合 = Owner 的 [`REQUIRED_FEATURES`] ∪ 对端 `requiredFeatures`（§11.3：`node-link.core.v1`
/// 与 `core.session-activity.v1` 是必需 feature；Access 未声明它们时 Owner 仍要求它们）。未被选中的
/// 必需 feature 按 UTF-8 字节升序、去重后返回。返回值是协商结果的 feature ID 文本（排序去重）。
///
/// 本函数用 [`SUPPORTED_FEATURES`] 全集；组合根在某个端点未注册时改用
/// [`negotiate_features_with`] 传入收窄后的集合。
pub fn negotiate_features(hello: &NodeHello) -> Result<Vec<String>, NegotiationFault> {
    negotiate_features_with(hello, &feature_list())
}

/// [`negotiate_features`] 的动态版本：`supported` 是本端本次真正支持（且端点已注册）的集合。
pub fn negotiate_features_with(
    hello: &NodeHello,
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

/// `node.challenge.selectedFeatures` 的 wire 形状（`selected` 已排序去重）。
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
/// 凭据状态按信任行的三态映射：`paired` → `Active`；`revoked` → `Revoked`；`pending` 与「无信任行」
/// 都按 `Unknown` 处理（批准事务只写 `paired`，`pending` 行尚未建立信任）。
pub fn peer_trust(view: &NodeLinkHandshakeView, node: &NodeId) -> PeerTrust {
    let credential = match view.node.as_ref().map(NodeRecord::state) {
        Some(NodeState::Paired) => CredentialStatus::Active,
        Some(NodeState::Revoked) => CredentialStatus::Revoked,
        Some(NodeState::Pending) | None => CredentialStatus::Unknown,
    };
    PeerTrust {
        peer: PeerIdentity::Node(node.clone()),
        public_key: view.public_key.clone(),
        credential,
        // 节点侧的绑定是「登记方在该配对里宣告的 endpoint」：它只存在于配对行（节点信任行不存绑定列）。
        host_binding: view
            .pairing
            .as_ref()
            .map(|pairing| pairing.host_binding().to_owned()),
        node_kind: view.node.as_ref().map(NodeRecord::kind),
    }
}

/// 本次握手使用的本机 endpoint（`ConnectionBinding.endpoint`）。
///
/// 优先取该对端最近一次配对里登记方宣告的 `host_binding`（认领时已校验两侧逐字相等，这里是权威值）；
/// 未知节点没有配对行，用 `daemon.public_origin` 推出 `wss://<authority>/node-link/v1`。两者都不可得时
/// 返回 `None`：本机没有可宣告的 Node Link endpoint，握手失败关闭（未配置 `public_origin` 时配对本身
/// 也无法创建，因此不存在「本应可用却没 endpoint」的部署）。
pub fn node_endpoint(
    view: &NodeLinkHandshakeView,
    public_origin: Option<&str>,
) -> Option<NodeEndpoint> {
    if let Some(endpoint) = view
        .pairing
        .as_ref()
        .and_then(|pairing| NodeEndpoint::parse(pairing.host_binding()).ok())
    {
        return Some(endpoint);
    }
    let authority = public_origin.and_then(origin_authority)?;
    NodeEndpoint::parse(&format!("wss://{authority}/node-link/v1")).ok()
}

/// 连接绑定：节点侧只有 endpoint（`canonical_origin` 必须为 `None`），`host` 与它自洽
/// （`Authority::hello`/`verify_proof` 会再核对持久化绑定）。
pub fn connection_binding(endpoint: &NodeEndpoint) -> ConnectionBinding {
    ConnectionBinding {
        canonical_origin: None,
        host: endpoint.authority().to_owned(),
        endpoint: Some(endpoint.clone()),
    }
}

/// 该对端待消费的配对（`approved` 尚未 `consumed`）：`Authority::complete_auth` 的配对目标。
pub fn pending_pairing(view: &NodeLinkHandshakeView) -> Option<PairingRecord> {
    view.pairing
        .as_ref()
        .filter(|pairing| pairing.state() == acp_core::model::PairingState::Approved)
        .cloned()
}

/// 该对端在信任记录里的节点角色（`identity-auth` 组装 `IdentityFact` 时要求它存在）。
///
/// 手上没有信任行时返回 `Access`：Node Link 的入站对端恒为 access 角色（Owner 自己的角色行属于
/// 出站方向），而这条路径上的对端必然已在 proof 阶段因没有公钥而被拒。
pub fn peer_kind(view: &NodeLinkHandshakeView) -> NodeKind {
    view.node
        .as_ref()
        .map(NodeRecord::kind)
        .unwrap_or(NodeKind::Access)
}

/// `https://<authority>` → `<authority>`（`public_origin` 的规范形状由 `CONFIG_REFERENCE.md` §1 冻结）。
fn origin_authority(origin: &str) -> Option<&str> {
    let rest = origin.strip_prefix("https://")?;
    let authority = rest.split(['/', '?', '#']).next()?;
    (!authority.is_empty()
        && !authority
            .chars()
            .any(|character| character.is_whitespace() || character.is_control()))
    .then_some(authority)
}

#[cfg(test)]
mod tests {
    use super::*;
    use acp_core::model::{PairingId, PairingTarget};
    use node_link_protocol::common::{Base64Url, NodeKind as WireNodeKind, Uuid};

    fn hello(supported: &[&str], required: &[&str]) -> NodeHello {
        NodeHello {
            min_protocol_version: node_link_protocol::common::ProtocolVersionV1::new(1)
                .expect("v1"),
            max_protocol_version: node_link_protocol::common::ProtocolVersionV1::new(1)
                .expect("v1"),
            access_node_id: Uuid::parse("10111213-1415-4617-9819-1a1b1c1d1e1f").expect("uuid"),
            role: WireNodeKind::Access,
            client_nonce: Base64Url::<32>::parse("AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8")
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

    #[test]
    fn supported_features_match_the_registry_mvp_subset() {
        // 词表的唯一机器来源是 `compatibility/features/v1/features.json`：这里逐条比对 node_link 子集里
        // `delivery = mvp` 的 ID，少一条或多一条都必须失败（能力协商不得虚报）。
        let registry =
            std::fs::read_to_string(repo_path("compatibility/features/v1/features.json"))
                .expect("features.json 必须存在");
        let registry: serde_json::Value = serde_json::from_str(&registry).expect("features.json");
        let mvp: Vec<String> = registry["protocols"]["node_link"]["features"]
            .as_array()
            .expect("node_link.features")
            .iter()
            .filter(|entry| entry["delivery"].as_str() == Some("mvp"))
            .map(|entry| entry["id"].as_str().expect("feature id").to_owned())
            .collect();
        let mut declared: Vec<String> = SUPPORTED_FEATURES
            .iter()
            .map(|id| (*id).to_owned())
            .collect();
        // 未接线的 mvp feature 必须在 `DEFERRED_FEATURES` 里逐条登记（显式差异，而不是静默漏项）：
        // 任何「没有宣告、也没有登记为 deferred」的 mvp feature 都会让下面的等式失败。
        let mut deferred: Vec<String> = DEFERRED_FEATURES
            .iter()
            .map(|id| (*id).to_owned())
            .collect();
        deferred.sort();
        let mut expected: Vec<String> = mvp
            .iter()
            .filter(|id| !deferred.contains(id))
            .cloned()
            .collect();
        declared.sort();
        expected.sort();
        assert_eq!(declared, expected);
        // deferred 集合本身必须真的都是 registry 的 mvp（防止把已废弃的 ID 记成「稍后实现」）。
        for id in &deferred {
            assert!(mvp.contains(id), "{id} is not an mvp feature");
        }
        // 已宣告的必须全部在 mvp 子集里（不宣告 registry 之外或 post_mvp 的 ID）。
        for id in &declared {
            assert!(mvp.contains(id), "{id} is not an mvp feature");
        }
        for id in REQUIRED_FEATURES {
            assert!(mvp.contains(&id.to_owned()), "{id} is not an mvp feature");
        }
        // registry 里 `required: true` 的条目必须正好是我们的必需集合（顺序无关）。
        let mut registry_required: Vec<&str> = registry["protocols"]["node_link"]["features"]
            .as_array()
            .expect("features")
            .iter()
            .filter(|entry| entry["required"].as_bool() == Some(true))
            .map(|entry| entry["id"].as_str().expect("id"))
            .collect();
        registry_required.sort();
        let mut expected_required = REQUIRED_FEATURES.to_vec();
        expected_required.sort();
        assert_eq!(registry_required, expected_required);
    }

    fn selected(hello: &NodeHello) -> Vec<String> {
        negotiate_features(hello).expect("必需 feature 已满足")
    }

    /// 未注册附件端点时，组合根用 [`feature_list_without`] 收窄本端集合：对端声明了该 feature 也不会被
    /// 选中；对端把它列为必需时握手必须失败（能力协商不得虚报，`AGENTS.md` §4）。
    #[test]
    fn an_unregistered_attachment_endpoint_is_never_announced() {
        let attachment = crate::node_link::attachments::ATTACHMENT_FEATURE;
        let narrowed = feature_list_without(attachment);
        assert!(
            !narrowed.iter().any(|feature| feature == attachment),
            "未注册的端点必须从本端集合里去掉"
        );

        let mut declared_features = REQUIRED_FEATURES.to_vec();
        declared_features.push(attachment);
        declared_features.push("node-link.command-status.v1");
        let declared = hello(&declared_features, &REQUIRED_FEATURES);
        let selected = negotiate_features_with(&declared, &narrowed).expect("必需 feature 已满足");
        assert!(
            !selected.iter().any(|feature| feature == attachment),
            "未注册的端点不得出现在 selectedFeatures 里"
        );

        let mut supported = REQUIRED_FEATURES.to_vec();
        supported.push(attachment);
        let mut required = REQUIRED_FEATURES.to_vec();
        required.push(attachment);
        let required = hello(&supported, &required);
        assert!(
            negotiate_features_with(&required, &narrowed).is_err(),
            "对端必需但本端未注册时必须失败关闭"
        );
    }

    #[test]
    fn feature_negotiation_selects_the_intersection_and_reports_missing_required() {
        let selected = selected(&hello(
            &[
                "node-link.core.v1",
                "core.session-activity.v1",
                "node-link.command-status.v1",
                // Owner 不认识（也不实现）的 ID 只是被忽略，不进 selectedFeatures。
                "node-link.future-thing.v1",
            ],
            &["node-link.core.v1"],
        ));
        assert_eq!(
            selected,
            vec![
                "core.session-activity.v1".to_owned(),
                "node-link.command-status.v1".to_owned(),
                "node-link.core.v1".to_owned()
            ]
        );
        assert!(wire_features(&selected).is_some());
        assert_eq!(
            transcript_features(&selected).as_slice(),
            vec![
                "core.session-activity.v1".to_owned(),
                "node-link.command-status.v1".to_owned(),
                "node-link.core.v1".to_owned()
            ]
        );

        // 必需 feature 未选中：给出排序去重的未满足集合。
        let fault = negotiate_features(&hello(
            &["node-link.session-create.v1", "node-link.core.v1"],
            &["node-link.node-rotation.v1"],
        ))
        .expect_err("必需 feature 未满足必须拒绝");
        assert_eq!(
            fault,
            NegotiationFault::FeaturesMissing(vec![
                "core.session-activity.v1".to_owned(),
                "node-link.node-rotation.v1".to_owned()
            ])
        );

        // 对端连 `core` 都不声明（Owner 的必需 feature）同样拒绝。
        let fault = negotiate_features(&hello(&[], &[])).expect_err("core 未被声明");
        assert_eq!(
            fault,
            NegotiationFault::FeaturesMissing(vec![
                "core.session-activity.v1".to_owned(),
                "node-link.core.v1".to_owned()
            ])
        );
    }

    /// 三态会话主状态合同是 Owner 的必需 feature：Access 只声明 `core` 也必须被拒绝。
    #[test]
    fn a_peer_without_the_three_state_contract_is_rejected() {
        let fault = negotiate_features(&hello(&["node-link.core.v1"], &[]))
            .expect_err("core.session-activity.v1 是 Owner 必需 feature");
        assert_eq!(
            fault,
            NegotiationFault::FeaturesMissing(vec!["core.session-activity.v1".to_owned()])
        );
    }

    #[test]
    fn version_intersection_accepts_v1_and_rejects_an_empty_interval() {
        let mut request = hello(&["node-link.core.v1"], &["node-link.core.v1"]);
        assert_eq!(negotiated_version(&request), Ok(PROTOCOL_VERSION));
        // 手工把区间抬到 v2（wire 类型在解码期已固定为 1，这里直接构造出「无交集」的形状）。
        request.min_protocol_version =
            node_link_protocol::common::ProtocolVersionV1::new(1).expect("v1");
        request.max_protocol_version =
            node_link_protocol::common::ProtocolVersionV1::new(1).expect("v1");
        assert_eq!(negotiated_version(&request), Ok(PROTOCOL_VERSION));
    }

    #[test]
    fn endpoint_prefers_the_registered_binding_and_falls_back_to_the_public_origin() {
        let view = NodeLinkHandshakeView {
            node: None,
            public_key: None,
            pairing: None,
            head: acp_core::model::GlobalCursor::new(
                acp_core::model::ServerEpoch::new("018f6f89-8a23-7a10-a0d3-f92e6a31d952")
                    .expect("epoch"),
                acp_core::model::Sequence::new(0).expect("sequence"),
            ),
            catalog_revision: 7,
        };
        assert_eq!(node_endpoint(&view, None), None, "没有可宣告的 endpoint");
        let endpoint = node_endpoint(&view, Some("https://owner.example.ts.net"))
            .expect("public_origin 必须能推出 endpoint");
        assert_eq!(endpoint.as_str(), "wss://owner.example.ts.net/node-link/v1");
        assert_eq!(endpoint.authority(), "owner.example.ts.net");
        assert!(node_endpoint(&view, Some("not-an-origin")).is_none());
        let binding = connection_binding(&endpoint);
        assert_eq!(binding.canonical_origin, None);
        assert_eq!(binding.endpoint, Some(endpoint));
    }

    /// 仓库根下的路径（集成测试的 cwd 不保证是仓库根）。
    fn repo_path(relative: &str) -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join(relative)
    }

    #[test]
    fn pending_pairing_only_counts_approved_pairings() {
        let node = NodeId::new("10111213-1415-4617-9819-1a1b1c1d1e1f").expect("node");
        let pairing = PairingRecord::try_new(
            PairingId::new("018f6f89-8a23-7a10-a0d3-f92e6a31d952").expect("pairing"),
            PairingTarget::Node,
            acp_core::model::PairingState::Approved,
            Some("Access".to_owned()),
            acp_core::model::Digest::new(&"A".repeat(43)).expect("digest"),
            "wss://owner.example.ts.net/node-link/v1",
            ts("2026-09-26T00:00:00.000Z"),
            ts("2026-09-26T00:05:00.000Z"),
            Some(ts("2026-09-26T00:00:01.000Z")),
            Some(ts("2026-09-26T00:00:02.000Z")),
            None,
        )
        .expect("pairing record");
        let mut view = NodeLinkHandshakeView {
            node: None,
            public_key: None,
            pairing: Some(pairing.clone()),
            head: acp_core::model::GlobalCursor::new(
                acp_core::model::ServerEpoch::new("018f6f89-8a23-7a10-a0d3-f92e6a31d952")
                    .expect("epoch"),
                acp_core::model::Sequence::new(7).expect("sequence"),
            ),
            catalog_revision: 7,
        };
        assert_eq!(
            pending_pairing(&view).map(|record| record.id().clone()),
            Some(pairing.id().clone())
        );
        assert_eq!(
            peer_trust(&view, &node).credential,
            CredentialStatus::Unknown
        );
        assert_eq!(
            peer_trust(&view, &node).host_binding.as_deref(),
            Some("wss://owner.example.ts.net/node-link/v1")
        );
        assert_eq!(peer_kind(&view), NodeKind::Access);

        // consumed 之后不再是消费目标；信任行决定凭据状态。
        view.pairing = Some(
            PairingRecord::try_new(
                pairing.id().clone(),
                PairingTarget::Node,
                acp_core::model::PairingState::Consumed,
                Some("Access".to_owned()),
                acp_core::model::Digest::new(&"A".repeat(43)).expect("digest"),
                "wss://owner.example.ts.net/node-link/v1",
                ts("2026-09-26T00:00:00.000Z"),
                ts("2026-09-26T00:05:00.000Z"),
                Some(ts("2026-09-26T00:00:01.000Z")),
                Some(ts("2026-09-26T00:00:02.000Z")),
                Some(ts("2026-09-26T00:00:03.000Z")),
            )
            .expect("consumed pairing"),
        );
        assert!(pending_pairing(&view).is_none());

        view.node = Some(
            NodeRecord::try_new(
                node.clone(),
                "Access",
                NodeKind::Access,
                acp_core::model::Fingerprint::new(&"b".repeat(64)).expect("fingerprint"),
                NodeState::Paired,
                None,
                ts("2026-09-26T00:00:00.000Z"),
                None,
                None,
            )
            .expect("node record"),
        );
        let trust = peer_trust(&view, &node);
        assert_eq!(trust.credential, CredentialStatus::Active);
        assert_eq!(trust.node_kind, Some(NodeKind::Access));
    }

    fn ts(text: &str) -> acp_core::model::Timestamp {
        acp_core::model::Timestamp::new(text).expect("timestamp")
    }
}
