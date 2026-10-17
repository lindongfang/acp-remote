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
//!   `delivery = mvp` 的十二条（§5.2 的 v1 baseline），逐条常量在此登记并有契约测试比对。

use acp_core::model::{
    DeviceId, DeviceRecord, DeviceState, PairingRecord, PairingState, PeerIdentity,
};
use acp_core::use_cases::SyncDeviceHandshakeView;
use identity_auth::{CanonicalOrigin, ConnectionBinding, CredentialStatus, FeatureList, PeerTrust};
use sync_protocol::auth::ClientHello;
use sync_protocol::common::{FeatureId, FeatureList as WireFeatureList};

/// 本切片实现的 Sync feature（`features.json` 的 `sync` 子集里 `delivery = mvp` 的十二条，§5.2）。
///
/// 十二条全部 `required: false`：Sync v1 没有「协议要求每次协商都必须选中」的 feature（必需性由客户端按
/// 自身需要声明），因此本模块的必需集合只来自对端。
///
/// 协商是**交集**（见 [`negotiate_features`]），因此这里漏登记一条 feature 会让它**静默失效**：客户端
/// 声明了却被交集丢掉，服务端随后的门控（快照的 `path` 键、`workspace.browse`、`file.changed` 的
/// 差异行）一律按「未协商」处置。新增 feature 时必须同时把它加进本表。
pub const SUPPORTED_FEATURES: [&str; 11] = [
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
    "core.file-diff.v1",
];

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
/// 然后核对客户端必需 feature 是否全部被选中（§5.2）。
///
/// 必需集合只来自对端（十二条 baseline 全部 `required: false`）。未被选中的必需 feature 按 UTF-8 字节
/// 升序、去重后返回。返回值是协商结果的 feature ID 文本（排序去重）。
pub fn negotiate_features(hello: &ClientHello) -> Result<Vec<String>, NegotiationFault> {
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
    // `SUPPORTED_FEATURES` 的登记顺序不是字典序（按 §5.2 的表格顺序），这里统一排序，
    // 使 `auth.server_challenge.selectedFeatures` 与逐字节升序的 transcript 输入一致。
    selected.sort();

    let mut required: Vec<String> = hello
        .required_features
        .as_slice()
        .iter()
        .map(|feature| feature.as_str().to_owned())
        .collect();
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
        grants: acp_core::model::GrantSet::empty(),
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
    use acp_core::model::{GlobalCursor, Sequence, ServerEpoch};
    use sync_protocol::auth::ClientKind;
    use sync_protocol::common::{Base64Url, UIntAtLeast};

    fn hello(supported: &[&str], required: &[&str]) -> ClientHello {
        ClientHello {
            min_protocol_version: UIntAtLeast::<1>::new(1).expect("v1"),
            max_protocol_version: UIntAtLeast::<1>::new(1).expect("v1"),
            host_id: sync_protocol::common::Uuid::parse("bdb2ec20-f98c-4d87-b789-e540d527ef87")
                .expect("uuid"),
            device_id: sync_protocol::common::Uuid::parse("2ae1c07c-9242-46e9-a9d2-4ec58c130f49")
                .expect("uuid"),
            client_kind: ClientKind::Pwa,
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
        // 词表的唯一机器来源是 `compatibility/features/v1/features.json`：逐条比对 sync 子集里
        // `delivery = mvp` 的 ID，少一条或多一条都必须失败（能力协商不得虚报）。
        let registry =
            std::fs::read_to_string(repo_path("compatibility/features/v1/features.json"))
                .expect("features.json 必须存在");
        let registry: serde_json::Value = serde_json::from_str(&registry).expect("features.json");
        let features = registry["protocols"]["sync"]["features"]
            .as_array()
            .expect("sync.features");
        let mut mvp: Vec<String> = features
            .iter()
            .filter(|entry| entry["delivery"].as_str() == Some("mvp"))
            .map(|entry| entry["id"].as_str().expect("feature id").to_owned())
            .collect();
        let mut declared: Vec<String> = SUPPORTED_FEATURES
            .iter()
            .map(|id| (*id).to_owned())
            .collect();
        declared.sort();
        mvp.sort();
        assert_eq!(declared, mvp, "支持集合必须与 registry 的 mvp 子集逐条相等");
        // §5.2：Sync v1 没有 `required: true` 的 feature，必需性只由客户端声明。
        assert!(
            features
                .iter()
                .all(|entry| entry["required"].as_bool() == Some(false)),
            "sync 子集不得有必需 feature"
        );
    }

    #[test]
    fn feature_negotiation_selects_the_intersection_and_reports_missing_required() {
        let selected = negotiate_features(&hello(
            &[
                "core.snapshot.v1",
                "core.event-ack.v1",
                // 服务端不认识（也不实现）的 ID 只是被忽略，不进 selectedFeatures。
                "core.future-thing.v1",
            ],
            &[],
        ))
        .expect("无需满足的必需集合");
        assert_eq!(
            selected,
            vec![
                "core.event-ack.v1".to_owned(),
                "core.snapshot.v1".to_owned()
            ],
            "交集必须排序去重"
        );
        assert!(wire_features(&selected).is_some());
        assert_eq!(
            transcript_features(&selected).as_slice(),
            vec![
                "core.event-ack.v1".to_owned(),
                "core.snapshot.v1".to_owned()
            ]
        );

        // 客户端必需 feature 未选中：给出排序去重的未满足集合。
        let fault = negotiate_features(&hello(
            &["core.snapshot.v1"],
            &["core.session-history.v1", "core.snapshot.v1"],
        ))
        .expect_err("必需 feature 未满足必须拒绝");
        assert_eq!(
            fault,
            NegotiationFault::FeaturesMissing(vec!["core.session-history.v1".to_owned()])
        );

        // 客户端什么都不声明：交集为空但是合法（Sync 没有服务端必需 feature）。
        assert_eq!(negotiate_features(&hello(&[], &[])), Ok(Vec::new()));
    }

    #[test]
    fn version_intersection_accepts_v1_and_rejects_an_empty_interval() {
        let mut request = hello(&["core.snapshot.v1"], &[]);
        assert_eq!(negotiated_version(&request), Ok(PROTOCOL_VERSION));
        request.min_protocol_version = UIntAtLeast::<1>::new(2).expect("v2 下限");
        request.max_protocol_version = UIntAtLeast::<1>::new(3).expect("v3 上限");
        assert_eq!(
            negotiated_version(&request),
            Err(NegotiationFault::VersionUnsupported)
        );
    }

    #[test]
    fn peer_trust_comes_from_the_persisted_row_only() {
        let device = DeviceId::new("2ae1c07c-9242-46e9-a9d2-4ec58c130f49").expect("device");
        // 未知设备：结构完整的空视图（第三个 None 是 pairing）。
        let unknown = view(None, None, None);
        let trust = peer_trust(&unknown, &device);
        assert_eq!(trust.credential, CredentialStatus::Unknown);
        assert!(trust.public_key.is_none());
        assert!(trust.host_binding.is_none());
        assert_eq!(trust.node_kind, None);
        assert_eq!(trust.grants.iter().count(), 0);
        assert_eq!(trust.peer, PeerIdentity::Device(device.clone()));

        // 已撤销设备：凭据状态来自信任行。
        let revoked = view(
            Some(record(DeviceState::Revoked)),
            None,
            Some(pairing(PairingState::Consumed)),
        );
        assert_eq!(
            peer_trust(&revoked, &device).credential,
            CredentialStatus::Revoked
        );
        assert_eq!(
            peer_trust(&revoked, &device).host_binding.as_deref(),
            Some("https://work-pc.example.test")
        );

        // `pending` 行尚未建立信任 → 与未知设备同类（不区分存在性）。
        let pending = view(Some(record(DeviceState::Pending)), None, None);
        assert_eq!(
            peer_trust(&pending, &device).credential,
            CredentialStatus::Unknown
        );

        // active 设备：凭据状态为 `Active`，且**没有**任何授权集合可填。
        let active = view(
            Some(record(DeviceState::Active)),
            None,
            Some(pairing(PairingState::Approved)),
        );
        let trust = peer_trust(&active, &device);
        assert_eq!(trust.credential, CredentialStatus::Active);
        assert_eq!(
            pending_pairing(&active)
                .map(|record| record.id().as_str().to_owned())
                .as_deref(),
            Some("018f6f89-8a23-7a10-a0d3-f92e6a31d952")
        );
        assert!(pending_pairing(&unknown).is_none());
        assert!(pending_pairing(&revoked).is_none());
    }

    #[test]
    fn connection_binding_uses_the_origin_authority_including_the_port() {
        let origin = CanonicalOrigin::parse("https://pc.example.test:8443").expect("canonical");
        let binding = connection_binding(&origin);
        assert_eq!(binding.canonical_origin, Some(origin.clone()));
        assert_eq!(binding.host, "pc.example.test:8443");
        assert_eq!(binding.host, origin.authority());
        assert_eq!(binding.endpoint, None);
        // 默认端口已由来源归一化省略：authority 与它一致，不带端口。
        let default_port = CanonicalOrigin::parse("https://pc.example.test").expect("canonical");
        assert_eq!(connection_binding(&default_port).host, "pc.example.test");
    }

    fn view(
        device: Option<DeviceRecord>,
        public_key: Option<acp_core::model::PeerPublicKey>,
        pairing: Option<PairingRecord>,
    ) -> SyncDeviceHandshakeView {
        SyncDeviceHandshakeView {
            device,
            public_key,
            pairing,
            head: GlobalCursor::new(
                ServerEpoch::new("018f6f89-8a23-7a10-a0d3-f92e6a31d952").expect("epoch"),
                Sequence::new(0).expect("sequence"),
            ),
        }
    }

    fn record(state: DeviceState) -> DeviceRecord {
        use acp_core::model::{Fingerprint, Timestamp};
        DeviceRecord::try_new(
            DeviceId::new("2ae1c07c-9242-46e9-a9d2-4ec58c130f49").expect("device"),
            "Test Phone",
            Fingerprint::new(&"a".repeat(64)).expect("fingerprint"),
            state,
            Timestamp::new("2026-09-18T09:12:03.412Z").expect("timestamp"),
            None,
            (state == DeviceState::Revoked)
                .then(|| Timestamp::new("2026-09-18T09:12:04.412Z").expect("timestamp")),
        )
        .expect("device record")
    }

    fn pairing(state: PairingState) -> PairingRecord {
        use acp_core::model::{Digest, GrantSet, PairingId, PairingTarget, ScopeSet, Timestamp};
        let terminal = state
            .is_terminal()
            .then(|| Timestamp::new("2026-09-18T09:12:05.412Z").expect("timestamp"));
        PairingRecord::try_new(
            PairingId::new("018f6f89-8a23-7a10-a0d3-f92e6a31d952").expect("pairing"),
            PairingTarget::Device,
            state,
            Some("Test Phone".to_owned()),
            ScopeSet::try_from_iter(["session.read"]).expect("scopes"),
            GrantSet::empty(),
            Digest::new(&"A".repeat(43)).expect("digest"),
            "https://work-pc.example.test",
            Timestamp::new("2026-09-18T09:12:03.412Z").expect("timestamp"),
            Timestamp::new("2026-09-18T09:17:03.412Z").expect("timestamp"),
            Some(Timestamp::new("2026-09-18T09:12:03.412Z").expect("timestamp")),
            Some(Timestamp::new("2026-09-18T09:12:04.412Z").expect("timestamp")),
            terminal,
        )
        .expect("pairing record")
    }

    /// 仓库根下的路径（集成测试的 cwd 不保证是仓库根）。
    fn repo_path(relative: &str) -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join(relative)
    }
}
