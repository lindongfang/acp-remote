//! 新设备短二维码（`docs/SYNC_PROTOCOL.md` §7.1）。
//!
//! 设备二维码只承载**引用**，不再内嵌完整配对资料：
//!
//! ```text
//! https://<canonical-origin>/pair#q2=<payload>
//! payload = base64url-no-pad( 0x02 || qrToken(32) || hostKeyFingerprint(32) )
//! ```
//!
//! 外层 URL 必须是无 userinfo、无 query、路径恰为 `/pair` 的 HTTPS 地址；fragment 只允许一个
//! `q2=` 键。旧设备 `#data=`（完整 JSON）、未知版本、错误长度与非规范 base64url 一律明确拒绝，
//! **不做降级解析**；节点二维码使用独立的 `node-link-protocol`，不在本模块。
//!
//! 指纹是主机公钥的 32 字节 SHA-256 摘要；本模块只保存与校验它的**长度**，摘要计算与 P-256
//! 曲线合法性属于持有公钥的消费方（`core`/`identity-auth`），协议层不反向依赖它们。

use std::fmt;

use crate::pairing::CanonicalOrigin;

/// 设备二维码载荷的格式版本（第一个字节）。
pub const DEVICE_QR_VERSION: u8 = 2;

/// 解码后的载荷长度：1 字节版本 + 32 字节取回凭据 + 32 字节指纹。
pub const DEVICE_QR_PAYLOAD_BYTES: usize = 65;

/// 无填充 base64url 编码后的载荷长度（65 字节 → 87 个 ASCII 字符）。
pub const DEVICE_QR_PAYLOAD_ASCII_LENGTH: usize = 87;

/// fragment 键前缀；设备二维码只接受 `q2=`。
pub const DEVICE_QR_FRAGMENT_PREFIX: &str = "q2=";

/// 旧设备完整资料 fragment 的前缀；收到即明确拒绝，不尝试 JSON 解析。
pub const LEGACY_DEVICE_QR_FRAGMENT_PREFIX: &str = "data=";

/// 固定测试 origin（`SYNC_PROTOCOL.md` §7.1 的长度预算向量）。
pub const DEVICE_QR_TEST_ORIGIN: &str = "https://faraday.tail74842e.ts.net:8765";

/// 固定测试地址的完整长度：43（`…/pair`）+ 4（`#q2=`）+ 87（payload）。
pub const DEVICE_QR_TEST_URL_LENGTH: usize = 134;

/// 测试地址承诺的总长度上界。
pub const DEVICE_QR_MAX_TEST_URL_LENGTH: usize = 150;

/// L 级纠错 + 四模块白边后的矩阵列数预算（按实心半块字符计）。
pub const DEVICE_QR_MATRIX_MAX_COLUMNS: usize = 58;

/// L 级纠错 + 四模块白边后的矩阵行数预算（按实心半块字符计）。
pub const DEVICE_QR_MATRIX_MAX_ROWS: usize = 29;

/// 设备短二维码的解码错误。
///
/// 只暴露**稳定的机器码**（`code()`），不携带任何输入内容：载荷里的 `qrToken` 是取回已存在
/// 配对资料的凭据，错误文本可能进入日志。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DeviceQrError {
    /// 收到旧设备的 `#data=` 完整资料地址。
    #[error("旧设备二维码 #data= 格式已停用")]
    LegacyDataFormat,
    /// 外层 URL 不是 `https://<origin>/pair#q2=…`。
    #[error("设备二维码地址不是 https://<origin>/pair#q2=…")]
    MalformedPairingUrl,
    /// 载荷版本不是 [`DEVICE_QR_VERSION`]。
    #[error("不支持的设备二维码版本")]
    UnsupportedVersion,
    /// 载荷解码后不是 65 字节。
    #[error("设备二维码载荷长度不是 65 字节")]
    WrongLength,
    /// 载荷不是规范的无填充 base64url（含填充、标准字母表或非规范尾字节）。
    #[error("设备二维码载荷不是规范的无填充 base64url")]
    NonCanonicalBase64Url,
}

impl DeviceQrError {
    /// 跨语言固定向量使用的稳定错误码（Rust 与 TypeScript 必须一致）。
    pub fn code(&self) -> &'static str {
        match self {
            DeviceQrError::LegacyDataFormat => "legacy_data_format",
            DeviceQrError::MalformedPairingUrl => "malformed_pairing_url",
            DeviceQrError::UnsupportedVersion => "unsupported_version",
            DeviceQrError::WrongLength => "wrong_length",
            DeviceQrError::NonCanonicalBase64Url => "non_canonical_base64url",
        }
    }
}

/// 新设备短二维码的 fragment 载荷。
///
/// `Debug` 对 `qrToken` 脱敏且不实现 `Display`：它只应活在内存与 QR 画布里，不得经格式化落入
/// 日志。指纹是公开摘要，按原样显示。
#[derive(Clone, PartialEq, Eq)]
pub struct DeviceQrPayload {
    qr_token: [u8; 32],
    host_key_fingerprint: [u8; 32],
}

impl DeviceQrPayload {
    /// 由两个独立字段构造（版本固定为 2）。
    pub fn from_parts(qr_token: [u8; 32], host_key_fingerprint: [u8; 32]) -> Self {
        Self {
            qr_token,
            host_key_fingerprint,
        }
    }

    /// 从 65 字节规范布局解码。
    pub fn from_bytes(bytes: [u8; DEVICE_QR_PAYLOAD_BYTES]) -> Result<Self, DeviceQrError> {
        if bytes[0] != DEVICE_QR_VERSION {
            return Err(DeviceQrError::UnsupportedVersion);
        }
        let mut qr_token = [0_u8; 32];
        let mut host_key_fingerprint = [0_u8; 32];
        qr_token.copy_from_slice(&bytes[1..33]);
        host_key_fingerprint.copy_from_slice(&bytes[33..]);
        Ok(Self {
            qr_token,
            host_key_fingerprint,
        })
    }

    /// 规范 65 字节布局：`0x02 || qrToken || hostKeyFingerprint`。
    pub fn to_bytes(&self) -> [u8; DEVICE_QR_PAYLOAD_BYTES] {
        let mut bytes = [0_u8; DEVICE_QR_PAYLOAD_BYTES];
        bytes[0] = DEVICE_QR_VERSION;
        bytes[1..33].copy_from_slice(&self.qr_token);
        bytes[33..].copy_from_slice(&self.host_key_fingerprint);
        bytes
    }

    /// 32 字节扫码取回凭据。
    pub fn qr_token(&self) -> &[u8; 32] {
        &self.qr_token
    }

    /// 32 字节主机公钥指纹（SHA-256 原始摘要，不是 hex）。
    pub fn host_key_fingerprint(&self) -> &[u8; 32] {
        &self.host_key_fingerprint
    }

    /// 编码为 87 个 ASCII 字符的无填充 base64url。
    pub fn encode(&self) -> String {
        acpr_transcript::encode_base64url(&self.to_bytes())
    }

    /// 严格解码 fragment 载荷：只接受规范的新格式。
    pub fn decode(text: &str) -> Result<Self, DeviceQrError> {
        let bytes = acpr_transcript::decode_base64url(text)
            .map_err(|_| DeviceQrError::NonCanonicalBase64Url)?;
        let layout = <[u8; DEVICE_QR_PAYLOAD_BYTES]>::try_from(bytes.as_slice())
            .map_err(|_| DeviceQrError::WrongLength)?;
        Self::from_bytes(layout)
    }

    /// 构造 `https://<canonical-origin>/pair#q2=…`。
    pub fn pairing_url(&self, origin: &CanonicalOrigin) -> String {
        format!(
            "{}/pair#{}{}",
            origin.as_str(),
            DEVICE_QR_FRAGMENT_PREFIX,
            self.encode()
        )
    }
}

impl fmt::Debug for DeviceQrPayload {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DeviceQrPayload")
            .field("version", &DEVICE_QR_VERSION)
            .field("qr_token", &"<redacted>")
            .field("host_key_fingerprint", &self.host_key_fingerprint)
            .finish()
    }
}

/// 一条已解析的设备配对地址：canonical origin 与 fragment 载荷。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DevicePairingUrl {
    canonical_origin: CanonicalOrigin,
    payload: DeviceQrPayload,
}

impl DevicePairingUrl {
    pub fn canonical_origin(&self) -> &CanonicalOrigin {
        &self.canonical_origin
    }

    pub fn payload(&self) -> &DeviceQrPayload {
        &self.payload
    }

    pub fn into_parts(self) -> (CanonicalOrigin, DeviceQrPayload) {
        (self.canonical_origin, self.payload)
    }
}

/// 严格解析设备配对地址；旧 `#data=` 与其他 fragment 键都拒绝。
pub fn decode_pairing_url(url: &str) -> Result<DevicePairingUrl, DeviceQrError> {
    let Some((base, fragment)) = url.split_once('#') else {
        return Err(DeviceQrError::MalformedPairingUrl);
    };
    if fragment.contains('#') {
        return Err(DeviceQrError::MalformedPairingUrl);
    }
    if fragment.starts_with(LEGACY_DEVICE_QR_FRAGMENT_PREFIX) {
        return Err(DeviceQrError::LegacyDataFormat);
    }
    let Some(payload_text) = fragment.strip_prefix(DEVICE_QR_FRAGMENT_PREFIX) else {
        return Err(DeviceQrError::MalformedPairingUrl);
    };

    // 外层地址必须是 `https://<authority>/pair`，无 userinfo、query 或额外路径。
    if url.contains('?') {
        return Err(DeviceQrError::MalformedPairingUrl);
    }
    let Some(authority) = base
        .strip_prefix("https://")
        .and_then(|rest| rest.strip_suffix("/pair"))
    else {
        return Err(DeviceQrError::MalformedPairingUrl);
    };
    if authority.is_empty()
        || authority.contains(['/', '@', '?', '#'])
        || authority
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
    {
        return Err(DeviceQrError::MalformedPairingUrl);
    }

    let canonical_origin = CanonicalOrigin::parse(&format!("https://{authority}"))
        .map_err(|_| DeviceQrError::MalformedPairingUrl)?;
    let payload = DeviceQrPayload::decode(payload_text)?;
    Ok(DevicePairingUrl {
        canonical_origin,
        payload,
    })
}
