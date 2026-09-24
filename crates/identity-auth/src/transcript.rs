//! 12 个 transcript domain 的输入类型与装配（`docs/SYNC_PROTOCOL.md` §6.3、
//! `docs/NODE_LINK_PROTOCOL.md` §9.3/§9.4，机器登记见 `compatibility/transcripts/v1/transcripts.json`）。
//!
//! 设计（`design.md` D2）：每个 domain 一个输入结构，`transcript()` 按**协议 crate 导出的表**装配
//! 字段字节；domain 字符串与 field tag 一律不写死在本 crate。装配时逐条校验 tag 顺序与定长宽度，
//! 因此「少一个字段」「顺序写错」「宽度写错」都会在本地失败，而不是发出一个语义错误的签名输入。
//!
//! 签名（P1363）与 HMAC 的入口都在本模块：本 crate **只验签**，签名由 `Authority` 经 keystore 完成。

use node_link_protocol::domains::DOMAINS as NODE_LINK_DOMAINS;
use sync_protocol::domains::DOMAINS as SYNC_DOMAINS;

use acp_core::model::{DeviceId, NodeId, NodeKind, Nonce, PairingId, PeerPublicKey, Timestamp};
use acpr_transcript::table::{DomainSpec, domain_spec, encode_transcript};

use crate::error::{ProofError, TranscriptError};
use crate::types::{
    CanonicalOrigin, ChallengeId, ClientKind, NodeEndpoint, P1363Signature, PROTOCOL_VERSION,
    PairingProof, PairingRequestId, PairingSecret, Sas,
};

// ---------------------------------------------------------------------------------------------
// 字段值 → 字节
// ---------------------------------------------------------------------------------------------

/// 字段值的 wire 编码（每种类型只有一种合法编码，避免调用方各自拼字节）。
trait FieldValue {
    fn field_bytes(&self) -> Result<Vec<u8>, TranscriptError>;
}

impl FieldValue for NodeId {
    fn field_bytes(&self) -> Result<Vec<u8>, TranscriptError> {
        Ok(crate::types::uuid_bytes(self.as_str()).to_vec())
    }
}

impl FieldValue for DeviceId {
    fn field_bytes(&self) -> Result<Vec<u8>, TranscriptError> {
        Ok(crate::types::uuid_bytes(self.as_str()).to_vec())
    }
}

impl FieldValue for PairingId {
    fn field_bytes(&self) -> Result<Vec<u8>, TranscriptError> {
        Ok(crate::types::uuid_bytes(self.as_str()).to_vec())
    }
}

impl FieldValue for ChallengeId {
    fn field_bytes(&self) -> Result<Vec<u8>, TranscriptError> {
        Ok(crate::types::uuid_bytes(self.as_str()).to_vec())
    }
}

impl FieldValue for PairingRequestId {
    fn field_bytes(&self) -> Result<Vec<u8>, TranscriptError> {
        Ok(crate::types::uuid_bytes(self.as_str()).to_vec())
    }
}

impl FieldValue for Timestamp {
    fn field_bytes(&self) -> Result<Vec<u8>, TranscriptError> {
        Ok(acpr_transcript::encode_u64be(unix_seconds(self)?).to_vec())
    }
}

impl FieldValue for Nonce {
    fn field_bytes(&self) -> Result<Vec<u8>, TranscriptError> {
        let bytes = acpr_transcript::decode_base64url(self.as_str())
            .map_err(|_| TranscriptError::Base64Url("nonce"))?;
        if bytes.len() != 32 {
            return Err(TranscriptError::Base64Url("nonce"));
        }
        Ok(bytes)
    }
}

impl FieldValue for PeerPublicKey {
    fn field_bytes(&self) -> Result<Vec<u8>, TranscriptError> {
        Ok(self.as_bytes().to_vec())
    }
}

impl FieldValue for CanonicalOrigin {
    fn field_bytes(&self) -> Result<Vec<u8>, TranscriptError> {
        Ok(self.as_str().as_bytes().to_vec())
    }
}

impl FieldValue for NodeEndpoint {
    fn field_bytes(&self) -> Result<Vec<u8>, TranscriptError> {
        Ok(self.as_str().as_bytes().to_vec())
    }
}

impl FieldValue for ClientKind {
    fn field_bytes(&self) -> Result<Vec<u8>, TranscriptError> {
        Ok(self.as_str().as_bytes().to_vec())
    }
}

impl FieldValue for NodeKind {
    fn field_bytes(&self) -> Result<Vec<u8>, TranscriptError> {
        Ok(self.as_str().as_bytes().to_vec())
    }
}

impl FieldValue for String {
    fn field_bytes(&self) -> Result<Vec<u8>, TranscriptError> {
        Ok(self.as_bytes().to_vec())
    }
}

impl FieldValue for u16 {
    fn field_bytes(&self) -> Result<Vec<u8>, TranscriptError> {
        Ok(acpr_transcript::encode_u16be(*self).to_vec())
    }
}

impl FieldValue for u64 {
    fn field_bytes(&self) -> Result<Vec<u8>, TranscriptError> {
        Ok(acpr_transcript::encode_u64be(*self).to_vec())
    }
}

impl FieldValue for FeatureList {
    fn field_bytes(&self) -> Result<Vec<u8>, TranscriptError> {
        let parts: Vec<&str> = self.0.iter().map(String::as_str).collect();
        acpr_transcript::encode_nul_joined(&parts).map_err(TranscriptError::from)
    }
}

/// `negotiatedFeatures`：排序（UTF-16 code unit 序，即 Rust 的 `String` 序）后以 NUL 连接。
///
/// 排序在装配层完成：协议表把该字段定义为「排序且以 NUL 分隔的 UTF-8 bytes」，参考实现
/// （`scripts/check-contract-assets.mjs`）同样先排序再连接。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FeatureList(Vec<String>);

impl FeatureList {
    /// 从名称构造（顺序无关）。
    pub fn new(items: impl IntoIterator<Item = String>) -> Self {
        let mut items: Vec<String> = items.into_iter().collect();
        items.sort();
        Self(items)
    }

    /// 名称（已排序）。
    pub fn as_slice(&self) -> &[String] {
        &self.0
    }

    /// 项数。
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// 按登记表装配 transcript：先校验字段数量、tag 顺序与定长宽度，再交给 `acpr-transcript` 编码。
fn assemble(
    domains: &[DomainSpec],
    domain: &'static str,
    pairs: &[(u16, &dyn FieldValue)],
) -> Result<Vec<u8>, TranscriptError> {
    let spec = domain_spec(domains, domain).ok_or(TranscriptError::UnknownDomain(domain))?;
    if pairs.len() != spec.fields.len() {
        return Err(TranscriptError::FieldCount {
            expected: spec.fields.len(),
            got: pairs.len(),
        });
    }
    let mut values: Vec<Vec<u8>> = Vec::with_capacity(pairs.len());
    for (position, (field, (tag, value))) in spec.fields.iter().zip(pairs.iter()).enumerate() {
        if field.tag != *tag {
            return Err(TranscriptError::FieldOrder {
                position,
                expected: field.tag,
                got: *tag,
            });
        }
        let bytes = value.field_bytes()?;
        if let Some(width) = field.ty.fixed_width() {
            if bytes.len() != width as usize {
                return Err(TranscriptError::FieldWidth {
                    tag: *tag,
                    expected: width,
                    got: bytes.len() as u32,
                });
            }
        }
        values.push(bytes);
    }
    let refs: Vec<&[u8]> = values.iter().map(Vec::as_slice).collect();
    encode_transcript(spec, &refs).map_err(TranscriptError::from)
}

// ---------------------------------------------------------------------------------------------
// 时间换算与密码学原语
// ---------------------------------------------------------------------------------------------

/// `Timestamp`（`YYYY-MM-DDTHH:MM:SS.mmmZ`）→ Unix 秒。
///
/// 协议把 `pairingExpiresAt` 定义为 u64be 秒数，而持久记录与 wire 载荷用 RFC 3339 文本，因此这里
/// 需要一个确定性换算。算法是标准 civil-from-days 的逆运算，不引入日期库（`AGENTS.md` §7）。
pub(crate) fn unix_seconds(timestamp: &Timestamp) -> Result<u64, TranscriptError> {
    let text = timestamp.as_str().as_bytes();
    if text.len() != 24 {
        return Err(TranscriptError::Timestamp(timestamp.as_str().to_owned()));
    }
    let digits = |range: std::ops::Range<usize>| -> Result<i64, TranscriptError> {
        let slice = text
            .get(range)
            .ok_or_else(|| TranscriptError::Timestamp(timestamp.as_str().to_owned()))?;
        if !slice.iter().all(u8::is_ascii_digit) {
            return Err(TranscriptError::Timestamp(timestamp.as_str().to_owned()));
        }
        let mut value = 0i64;
        for byte in slice {
            value = value * 10 + i64::from(byte - b'0');
        }
        Ok(value)
    };
    let year = digits(0..4)?;
    let month = digits(5..7)?;
    let day = digits(8..10)?;
    let hour = digits(11..13)?;
    let minute = digits(14..16)?;
    let second = digits(17..19)?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return Err(TranscriptError::Timestamp(timestamp.as_str().to_owned()));
    }
    let days = days_from_civil(year, month, day);
    let seconds = days * 86_400 + hour * 3_600 + minute * 60 + second;
    u64::try_from(seconds).map_err(|_| TranscriptError::Timestamp(timestamp.as_str().to_owned()))
}

/// 公历 `(year, month, day)` → 自 1970-01-01 起的天数（Howard Hinnant 的 civil-from-days 逆式）。
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let shifted_month = if month > 2 { month - 3 } else { month + 9 };
    let day_of_year = (153 * shifted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// 自 1970-01-01 起的天数 → 公历 `(year, month, day)`（Howard Hinnant 的 civil-from-days）。
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let shifted = days + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = if month <= 2 { year + 1 } else { year };
    (year, month, day)
}

/// Unix 秒 → `Timestamp`（`YYYY-MM-DDTHH:MM:SS.mmmZ`）。
///
/// 与 [`unix_seconds`] 互为逆运算，两边都有单元测试（含闰年与边界）。适配器可用它把 wire 上的
/// 秒数换算成持久层/记录层使用的时间戳，避免各自实现日期算法。
pub fn timestamp_from_unix_seconds(seconds: u64) -> Result<Timestamp, TranscriptError> {
    let seconds =
        i64::try_from(seconds).map_err(|_| TranscriptError::Timestamp(format!("{seconds}")))?;
    let days = seconds.div_euclid(86_400);
    let time_of_day = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    if !(0..=9999).contains(&year) {
        return Err(TranscriptError::Timestamp(format!("{seconds}")));
    }
    let hour = time_of_day / 3_600;
    let minute = (time_of_day % 3_600) / 60;
    let second = time_of_day % 60;
    let text = format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.000Z");
    Timestamp::new(&text).map_err(|_| TranscriptError::Timestamp(text))
}

/// `Timestamp` 加若干秒（挑战 TTL、过期判定都在固定格式文本上做，无需其它时间库）。
pub(crate) fn add_seconds(
    timestamp: &Timestamp,
    seconds: u64,
) -> Result<Timestamp, TranscriptError> {
    let base = unix_seconds(timestamp)?;
    timestamp_from_unix_seconds(base + seconds)
}

/// 由 SAS transcript 与 pairing secret 派生 6 位短验证码（合同 §4.4 的唯一入口）。
///
/// 双方各自计算：这里只接受**已装配的 transcript 字节**，不提供「取对端结果」的路径。
pub fn derive_sas(transcript: &[u8], secret: &PairingSecret) -> Result<Sas, ProofError> {
    let output = hmac_sha256(secret.as_bytes(), transcript)?;
    Sas::from_hmac_output(&output).map_err(ProofError::Transcript)
}

/// HMAC-SHA256（常量时间比较由 [`verify_hmac`] 承担）。
pub(crate) fn hmac_sha256(key: &[u8], transcript: &[u8]) -> Result<[u8; 32], ProofError> {
    use hmac::{KeyInit as _, Mac as _};
    let mut mac = hmac::Hmac::<sha2::Sha256>::new_from_slice(key).map_err(|_| ProofError::Hmac)?;
    mac.update(transcript);
    let bytes = mac.finalize().into_bytes();
    let mut out = [0u8; 32];
    out.copy_from_slice(bytes.as_slice());
    Ok(out)
}

/// 校验 HMAC（`verify_slice` 是常量时间比较）。
pub(crate) fn verify_hmac(
    key: &[u8],
    transcript: &[u8],
    proof: &PairingProof,
) -> Result<(), ProofError> {
    use hmac::{KeyInit as _, Mac as _};
    let mut mac = hmac::Hmac::<sha2::Sha256>::new_from_slice(key).map_err(|_| ProofError::Hmac)?;
    mac.update(transcript);
    mac.verify_slice(proof.as_bytes())
        .map_err(|_| ProofError::Hmac)
}

/// 校验 P1363 签名：先断言 `r`/`s` 非零，再解析公钥与签名后验签（`verify` 对 transcript 做 SHA-256）。
///
/// 只接受 64 字节 P1363；DER 形态在 [`P1363Signature`] 构造时就已被长度断言拒绝。
pub(crate) fn verify_signature(
    public_key: &PeerPublicKey,
    transcript: &[u8],
    signature: &P1363Signature,
) -> Result<(), ProofError> {
    use p256::ecdsa::signature::Verifier as _;
    if !signature.components_nonzero() {
        return Err(ProofError::Signature);
    }
    let key = p256::ecdsa::VerifyingKey::from_sec1_bytes(public_key.as_bytes())
        .map_err(|_| ProofError::PublicKey)?;
    let parsed = p256::ecdsa::Signature::from_slice(signature.as_bytes())
        .map_err(|_| ProofError::Signature)?;
    key.verify(transcript, &parsed)
        .map_err(|_| ProofError::Signature)
}

/// 从 HMAC 输出派生 6 位 SAS（合同 §4.4）。
pub(crate) fn sas_from_hmac(output: &[u8]) -> Result<Sas, TranscriptError> {
    Sas::from_hmac_output(output)
}

// ---------------------------------------------------------------------------------------------
// 输入结构
// ---------------------------------------------------------------------------------------------

macro_rules! domain_input {
    (
        $(#[$meta:meta])*
        pub struct $name:ident { $( $field:ident: $ty:ty ),+ $(,)? }
        domain $domains:expr, $domain:literal;
        tags [ $( $tag:literal => $source:ident ),+ $(,)? ];
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name {
            $(
                #[doc = concat!("字段 `", stringify!($source), "`。")]
                pub $field: $ty,
            )+
        }

        impl $name {
            /// 按登记表装配 transcript（字段顺序与宽度在装配时逐条校验）。
            pub fn transcript(&self) -> Result<Vec<u8>, TranscriptError> {
                // tag 1 固定是 protocolVersion（v1 常量），由宏注入，struct 里不重复存一份。
                let pairs: Vec<(u16, &dyn FieldValue)> =
                    vec![ (1u16, &PROTOCOL_VERSION), $( ($tag, &self.$source) ),+ ];
                assemble(&$domains, $domain, &pairs)
            }
        }
    };
}

domain_input! {
    /// Sync 配对认领证明（`acp-remote/pairing-proof/v1`，HMAC-SHA256）。
    pub struct SyncPairingProof {
        host_id: NodeId,
        device_id: DeviceId,
        pairing_id: PairingId,
        pairing_expires_at: Timestamp,
        canonical_origin: CanonicalOrigin,
        device_public_key: PeerPublicKey,
        client_nonce: Nonce,
        device_name: String,
        client_kind: ClientKind,
    }
    domain SYNC_DOMAINS, "acp-remote/pairing-proof/v1";
    tags [
        2 => host_id,
        3 => device_id,
        4 => pairing_id,
        5 => pairing_expires_at,
        6 => canonical_origin,
        8 => device_public_key,
        9 => client_nonce,
        14 => device_name,
        15 => client_kind,
    ];
}

impl SyncPairingProof {
    /// 计算认领证明。
    pub fn hmac(&self, secret: &PairingSecret) -> Result<PairingProof, ProofError> {
        let transcript = self.transcript()?;
        let output = hmac_sha256(secret.as_bytes(), &transcript)?;
        PairingProof::try_from_bytes(&output).map_err(ProofError::Transcript)
    }

    /// 校验认领证明（服务端；密钥是内存中的 pairing secret）。
    pub fn verify(&self, secret: &PairingSecret, proof: &PairingProof) -> Result<(), ProofError> {
        let transcript = self.transcript()?;
        verify_hmac(secret.as_bytes(), &transcript, proof)
    }
}

domain_input! {
    /// Sync 配对宿主证明（`acp-remote/pairing-host-proof/v1`，宿主签名）。
    pub struct SyncPairingHostProof {
        host_id: NodeId,
        device_id: DeviceId,
        pairing_id: PairingId,
        canonical_origin: CanonicalOrigin,
        host_public_key: PeerPublicKey,
        device_public_key: PeerPublicKey,
        client_nonce: Nonce,
        server_nonce: Nonce,
        pairing_request_id: PairingRequestId,
    }
    domain SYNC_DOMAINS, "acp-remote/pairing-host-proof/v1";
    tags [
        2 => host_id,
        3 => device_id,
        4 => pairing_id,
        6 => canonical_origin,
        7 => host_public_key,
        8 => device_public_key,
        9 => client_nonce,
        10 => server_nonce,
        13 => pairing_request_id,
    ];
}

impl SyncPairingHostProof {
    /// 校验宿主证明（设备侧；宿主公钥来自持久化信任）。
    pub fn verify(
        &self,
        host_public_key: &PeerPublicKey,
        signature: &P1363Signature,
    ) -> Result<(), ProofError> {
        let transcript = self.transcript()?;
        verify_signature(host_public_key, &transcript, signature)
    }
}

domain_input! {
    /// Sync 配对 SAS（`acp-remote/pairing-sas/v1`，HMAC-SHA256）。
    pub struct SyncPairingSas {
        host_id: NodeId,
        device_id: DeviceId,
        pairing_id: PairingId,
        canonical_origin: CanonicalOrigin,
        host_public_key: PeerPublicKey,
        device_public_key: PeerPublicKey,
        client_nonce: Nonce,
        server_nonce: Nonce,
        pairing_request_id: PairingRequestId,
    }
    domain SYNC_DOMAINS, "acp-remote/pairing-sas/v1";
    tags [
        2 => host_id,
        3 => device_id,
        4 => pairing_id,
        6 => canonical_origin,
        7 => host_public_key,
        8 => device_public_key,
        9 => client_nonce,
        10 => server_nonce,
        13 => pairing_request_id,
    ];
}

impl SyncPairingSas {
    /// HMAC 输出（固定向量直接断言该值）。
    pub fn hmac(&self, secret: &PairingSecret) -> Result<[u8; 32], ProofError> {
        let transcript = self.transcript()?;
        hmac_sha256(secret.as_bytes(), &transcript)
    }

    /// 6 位短验证码：由双方各自计算，宿主不下发本机结果冒充对端结果。
    pub fn sas(&self, secret: &PairingSecret) -> Result<Sas, ProofError> {
        let output = self.hmac(secret)?;
        sas_from_hmac(&output).map_err(ProofError::Transcript)
    }
}

domain_input! {
    /// Sync 配对状态查询（`acp-remote/pairing-status/v1`，HMAC-SHA256）。
    pub struct SyncPairingStatus {
        host_id: NodeId,
        device_id: DeviceId,
        pairing_id: PairingId,
        pairing_request_id: PairingRequestId,
        request_nonce: Nonce,
    }
    domain SYNC_DOMAINS, "acp-remote/pairing-status/v1";
    tags [
        2 => host_id,
        3 => device_id,
        4 => pairing_id,
        13 => pairing_request_id,
        16 => request_nonce,
    ];
}

impl SyncPairingStatus {
    /// 计算状态查询证明。
    pub fn hmac(&self, secret: &PairingSecret) -> Result<PairingProof, ProofError> {
        let transcript = self.transcript()?;
        let output = hmac_sha256(secret.as_bytes(), &transcript)?;
        PairingProof::try_from_bytes(&output).map_err(ProofError::Transcript)
    }

    /// 校验状态查询证明。
    pub fn verify(&self, secret: &PairingSecret, proof: &PairingProof) -> Result<(), ProofError> {
        let transcript = self.transcript()?;
        verify_hmac(secret.as_bytes(), &transcript, proof)
    }
}

domain_input! {
    /// Sync 连接挑战（`acp-remote/host-challenge/v1`，宿主签名）。
    pub struct SyncHostChallenge {
        host_id: NodeId,
        device_id: DeviceId,
        canonical_origin: CanonicalOrigin,
        client_nonce: Nonce,
        server_nonce: Nonce,
        connection_id: ChallengeId,
        negotiated_features: FeatureList,
    }
    domain SYNC_DOMAINS, "acp-remote/host-challenge/v1";
    tags [
        2 => host_id,
        3 => device_id,
        6 => canonical_origin,
        9 => client_nonce,
        10 => server_nonce,
        11 => connection_id,
        12 => negotiated_features,
    ];
}

impl SyncHostChallenge {
    /// 校验宿主证明（设备侧）。
    pub fn verify(
        &self,
        host_public_key: &PeerPublicKey,
        signature: &P1363Signature,
    ) -> Result<(), ProofError> {
        let transcript = self.transcript()?;
        verify_signature(host_public_key, &transcript, signature)
    }
}

domain_input! {
    /// Sync 设备证明（`acp-remote/device-proof/v1`，设备签名）。
    pub struct SyncDeviceProof {
        host_id: NodeId,
        device_id: DeviceId,
        canonical_origin: CanonicalOrigin,
        client_nonce: Nonce,
        server_nonce: Nonce,
        connection_id: ChallengeId,
        negotiated_features: FeatureList,
    }
    domain SYNC_DOMAINS, "acp-remote/device-proof/v1";
    tags [
        2 => host_id,
        3 => device_id,
        6 => canonical_origin,
        9 => client_nonce,
        10 => server_nonce,
        11 => connection_id,
        12 => negotiated_features,
    ];
}

impl SyncDeviceProof {
    /// 校验设备证明（宿主侧；公钥只来自持久化信任）。
    pub fn verify(
        &self,
        device_public_key: &PeerPublicKey,
        signature: &P1363Signature,
    ) -> Result<(), ProofError> {
        let transcript = self.transcript()?;
        verify_signature(device_public_key, &transcript, signature)
    }
}

domain_input! {
    /// Node Link 配对认领证明（`acp-remote/node-link-pairing-proof/v1`，HMAC-SHA256）。
    pub struct NodeLinkPairingProof {
        owner_node_id: NodeId,
        access_node_id: NodeId,
        pairing_id: PairingId,
        pairing_expires_at: Timestamp,
        access_public_key: PeerPublicKey,
        client_nonce: Nonce,
        node_name: String,
        node_kind: NodeKind,
    }
    domain NODE_LINK_DOMAINS, "acp-remote/node-link-pairing-proof/v1";
    tags [
        2 => owner_node_id,
        3 => access_node_id,
        4 => pairing_id,
        5 => pairing_expires_at,
        8 => access_public_key,
        9 => client_nonce,
        14 => node_name,
        15 => node_kind,
    ];
}

impl NodeLinkPairingProof {
    /// 计算认领证明。
    pub fn hmac(&self, secret: &PairingSecret) -> Result<PairingProof, ProofError> {
        let transcript = self.transcript()?;
        let output = hmac_sha256(secret.as_bytes(), &transcript)?;
        PairingProof::try_from_bytes(&output).map_err(ProofError::Transcript)
    }

    /// 校验认领证明。
    pub fn verify(&self, secret: &PairingSecret, proof: &PairingProof) -> Result<(), ProofError> {
        let transcript = self.transcript()?;
        verify_hmac(secret.as_bytes(), &transcript, proof)
    }
}

domain_input! {
    /// Node Link 配对 Owner 证明（`acp-remote/node-link-pairing-owner-proof/v1`，Owner 签名）。
    pub struct NodeLinkPairingOwnerProof {
        owner_node_id: NodeId,
        access_node_id: NodeId,
        pairing_id: PairingId,
        owner_public_key: PeerPublicKey,
        access_public_key: PeerPublicKey,
        client_nonce: Nonce,
        server_nonce: Nonce,
        pairing_request_id: PairingRequestId,
    }
    domain NODE_LINK_DOMAINS, "acp-remote/node-link-pairing-owner-proof/v1";
    tags [
        2 => owner_node_id,
        3 => access_node_id,
        4 => pairing_id,
        7 => owner_public_key,
        8 => access_public_key,
        9 => client_nonce,
        10 => server_nonce,
        13 => pairing_request_id,
    ];
}

impl NodeLinkPairingOwnerProof {
    /// 校验 Owner 证明（Access 侧）。
    pub fn verify(
        &self,
        owner_public_key: &PeerPublicKey,
        signature: &P1363Signature,
    ) -> Result<(), ProofError> {
        let transcript = self.transcript()?;
        verify_signature(owner_public_key, &transcript, signature)
    }
}

domain_input! {
    /// Node Link 配对 SAS（`acp-remote/node-link-pairing-sas/v1`，HMAC-SHA256）。
    pub struct NodeLinkPairingSas {
        owner_node_id: NodeId,
        access_node_id: NodeId,
        pairing_id: PairingId,
        owner_public_key: PeerPublicKey,
        access_public_key: PeerPublicKey,
        client_nonce: Nonce,
        server_nonce: Nonce,
        pairing_request_id: PairingRequestId,
    }
    domain NODE_LINK_DOMAINS, "acp-remote/node-link-pairing-sas/v1";
    tags [
        2 => owner_node_id,
        3 => access_node_id,
        4 => pairing_id,
        7 => owner_public_key,
        8 => access_public_key,
        9 => client_nonce,
        10 => server_nonce,
        13 => pairing_request_id,
    ];
}

impl NodeLinkPairingSas {
    /// HMAC 输出。
    pub fn hmac(&self, secret: &PairingSecret) -> Result<[u8; 32], ProofError> {
        let transcript = self.transcript()?;
        hmac_sha256(secret.as_bytes(), &transcript)
    }

    /// 6 位短验证码。
    pub fn sas(&self, secret: &PairingSecret) -> Result<Sas, ProofError> {
        let output = self.hmac(secret)?;
        sas_from_hmac(&output).map_err(ProofError::Transcript)
    }
}

domain_input! {
    /// Node Link 配对状态查询（`acp-remote/node-link-pairing-status/v1`，HMAC-SHA256）。
    pub struct NodeLinkPairingStatus {
        owner_node_id: NodeId,
        access_node_id: NodeId,
        pairing_id: PairingId,
        pairing_request_id: PairingRequestId,
        request_nonce: Nonce,
    }
    domain NODE_LINK_DOMAINS, "acp-remote/node-link-pairing-status/v1";
    tags [
        2 => owner_node_id,
        3 => access_node_id,
        4 => pairing_id,
        13 => pairing_request_id,
        16 => request_nonce,
    ];
}

impl NodeLinkPairingStatus {
    /// 计算状态查询证明。
    pub fn hmac(&self, secret: &PairingSecret) -> Result<PairingProof, ProofError> {
        let transcript = self.transcript()?;
        let output = hmac_sha256(secret.as_bytes(), &transcript)?;
        PairingProof::try_from_bytes(&output).map_err(ProofError::Transcript)
    }

    /// 校验状态查询证明。
    pub fn verify(&self, secret: &PairingSecret, proof: &PairingProof) -> Result<(), ProofError> {
        let transcript = self.transcript()?;
        verify_hmac(secret.as_bytes(), &transcript, proof)
    }
}

domain_input! {
    /// Node Link 连接挑战（`acp-remote/node-link-challenge/v1`，Owner 签名）。
    pub struct NodeLinkChallenge {
        owner_node_id: NodeId,
        access_node_id: NodeId,
        catalog_revision: u64,
        client_nonce: Nonce,
        server_nonce: Nonce,
        connection_id: ChallengeId,
        negotiated_features: FeatureList,
    }
    domain NODE_LINK_DOMAINS, "acp-remote/node-link-challenge/v1";
    tags [
        2 => owner_node_id,
        3 => access_node_id,
        6 => catalog_revision,
        9 => client_nonce,
        10 => server_nonce,
        11 => connection_id,
        12 => negotiated_features,
    ];
}

impl NodeLinkChallenge {
    /// 校验 Owner 挑战证明（Access 侧）。
    pub fn verify(
        &self,
        owner_public_key: &PeerPublicKey,
        signature: &P1363Signature,
    ) -> Result<(), ProofError> {
        let transcript = self.transcript()?;
        verify_signature(owner_public_key, &transcript, signature)
    }
}

domain_input! {
    /// Node Link 节点证明（`acp-remote/node-link-proof/v1`，Access 签名）。
    pub struct NodeLinkProof {
        owner_node_id: NodeId,
        access_node_id: NodeId,
        catalog_revision: u64,
        client_nonce: Nonce,
        server_nonce: Nonce,
        connection_id: ChallengeId,
        negotiated_features: FeatureList,
    }
    domain NODE_LINK_DOMAINS, "acp-remote/node-link-proof/v1";
    tags [
        2 => owner_node_id,
        3 => access_node_id,
        6 => catalog_revision,
        9 => client_nonce,
        10 => server_nonce,
        11 => connection_id,
        12 => negotiated_features,
    ];
}

impl NodeLinkProof {
    /// 校验节点证明（Owner 侧；公钥只来自持久化信任）。
    pub fn verify(
        &self,
        access_public_key: &PeerPublicKey,
        signature: &P1363Signature,
    ) -> Result<(), ProofError> {
        let transcript = self.transcript()?;
        verify_signature(access_public_key, &transcript, signature)
    }
}
