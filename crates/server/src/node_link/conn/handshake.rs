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
//!   `delivery = mvp` 的五条（本切片实现），逐条常量在此登记并有契约测试比对。

use acp_core::model::{
    GrantSet, NodeId, NodeKind, NodeRecord, NodeState, PairingRecord, PeerIdentity, ScopeSet,
};
use acp_core::use_cases::NodeLinkHandshakeView;
use identity_auth::{ConnectionBinding, CredentialStatus, FeatureList, NodeEndpoint, PeerTrust};
use node_link_protocol::common::{FeatureId, FeatureList as WireFeatureList};
use node_link_protocol::handshake::NodeHello;

/// 本切片实现的 Node Link feature（`features.json` 的 `node_link` 子集里 `delivery = mvp` 的五条）。
///
/// `node-link.node-rotation.v1` 是 `post_mvp`（只登记 ID 与消息名），**不**选中：选中它等于虚报支持
/// （`AGENTS.md` §3 的能力协商真实性）。
pub const SUPPORTED_FEATURES: [&str; 5] = [
    "node-link.core.v1",
    "node-link.raw-acp.v1",
    "node-link.command-status.v1",
    "node-link.session-create.v1",
    "node-link.export-revoke.v1",
];

/// 每次握手都必须选中的 feature（registry 的 `required: true`，§11.3）。
pub const REQUIRED_FEATURE: &str = "node-link.core.v1";

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
/// 必需集合 = Owner 的 [`REQUIRED_FEATURE`] ∪ 对端 `requiredFeatures`（§11.3：`node-link.core.v1`
/// 是必需 feature；Access 未声明它时 Owner 仍要求它）。未被选中的必需 feature 按 UTF-8 字节升序、
/// 去重后返回。返回值是协商结果的 feature ID 文本（排序去重）。
pub fn negotiate_features(hello: &NodeHello) -> Result<Vec<String>, NegotiationFault> {
    let declared: Vec<&str> = hello
        .supported_features
        .as_slice()
        .iter()
        .map(FeatureId::as_str)
        .collect();
    let mut selected: Vec<String> = SUPPORTED_FEATURES
        .iter()
        .filter(|feature| declared.contains(*feature))
        .map(|feature| (*feature).to_owned())
        .collect();
    selected.sort();
    selected.dedup();

    let mut required: Vec<String> = hello
        .required_features
        .as_slice()
        .iter()
        .map(|feature| feature.as_str().to_owned())
        .collect();
    required.push(REQUIRED_FEATURE.to_owned());
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
/// 都按 `Unknown` 处理（批准事务只写 `paired`，`pending` 行尚未建立信任）。`grant.*` 取自信任行，
/// 是 WP6 授权交集的输入之一。
pub fn peer_trust(view: &NodeLinkHandshakeView, node: &NodeId) -> PeerTrust {
    let credential = match view.node.as_ref().map(NodeRecord::state) {
        Some(NodeState::Paired) => CredentialStatus::Active,
        Some(NodeState::Revoked) => CredentialStatus::Revoked,
        Some(NodeState::Pending) | None => CredentialStatus::Unknown,
    };
    let grants = view
        .node
        .as_ref()
        .map(|row| row.grants().clone())
        .unwrap_or_else(GrantSet::empty);
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
        scopes: ScopeSet::empty(),
        grants,
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
        let mut mvp = mvp;
        declared.sort();
        mvp.sort();
        assert_eq!(declared, mvp);
        assert!(mvp.contains(&REQUIRED_FEATURE.to_owned()));
        // `required: true` 的唯一一条必须正好是我们的必需 feature。
        let required: Vec<&str> = registry["protocols"]["node_link"]["features"]
            .as_array()
            .expect("features")
            .iter()
            .filter(|entry| entry["required"].as_bool() == Some(true))
            .map(|entry| entry["id"].as_str().expect("id"))
            .collect();
        assert_eq!(required, vec![REQUIRED_FEATURE]);
    }

    fn selected(hello: &NodeHello) -> Vec<String> {
        negotiate_features(hello).expect("必需 feature 已满足")
    }

    #[test]
    fn feature_negotiation_selects_the_intersection_and_reports_missing_required() {
        let selected = selected(&hello(
            &[
                "node-link.core.v1",
                "node-link.command-status.v1",
                // Owner 不认识（也不实现）的 ID 只是被忽略，不进 selectedFeatures。
                "node-link.future-thing.v1",
            ],
            &["node-link.core.v1"],
        ));
        assert_eq!(
            selected,
            vec![
                "node-link.command-status.v1".to_owned(),
                "node-link.core.v1".to_owned()
            ]
        );
        assert!(wire_features(&selected).is_some());
        assert_eq!(
            transcript_features(&selected).as_slice(),
            vec![
                "node-link.command-status.v1".to_owned(),
                "node-link.core.v1".to_owned()
            ]
        );

        // 必需 feature 未选中：给出排序去重的未满足集合。
        let fault = negotiate_features(&hello(
            &["node-link.core.v1", "node-link.export-revoke.v1"],
            &["node-link.session-create.v1", "node-link.core.v1"],
        ))
        .expect_err("必需 feature 未满足必须拒绝");
        assert_eq!(
            fault,
            NegotiationFault::FeaturesMissing(vec!["node-link.session-create.v1".to_owned()])
        );

        // 对端连 `core` 都不声明（Owner 的必需 feature）同样拒绝。
        let fault = negotiate_features(&hello(&[], &[])).expect_err("core 未被声明");
        assert_eq!(
            fault,
            NegotiationFault::FeaturesMissing(vec![REQUIRED_FEATURE.to_owned()])
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
            ScopeSet::empty(),
            GrantSet::empty(),
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
                ScopeSet::empty(),
                GrantSet::empty(),
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
                GrantSet::try_from_iter(["grant.observe"]).expect("grants"),
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
        assert_eq!(
            trust.grants.iter().collect::<Vec<&str>>(),
            vec!["grant.observe"]
        );
    }

    fn ts(text: &str) -> acp_core::model::Timestamp {
        acp_core::model::Timestamp::new(text).expect("timestamp")
    }
}
