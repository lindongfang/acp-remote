//! 本节点身份：节点身份密钥（`identity-auth` 的 `KeyPurpose::NodeIdentity`）与由它派生的 `nodeId`。
//!
//! 边界与口径：
//!
//! - **密钥只在端口后面**：本模块只经 [`identity_auth::IdentityKeystore`] 取公钥；私钥不出端口、
//!   不进日志。条目缺失时**生成一次**（本机身份的唯一创建点），其余失败（keystore 不可用）一律失败关闭，
//!   绝不静默降级成「进程内临时身份」。
//! - **`nodeId` 由节点身份公钥确定性派生**（`app::identity::node_id_for`）：`nodeId` 没有任何权威存储
//!   位置（`CORE_PORTS_AND_STORAGE.md` 的 `meta` 只有 `server_epoch`/`created_at`/`last_prune_at`，
//!   `CONFIG_REFERENCE.md` 没有 `node_id` 配置键，keystore 条目也只保存密钥）。派生而不是新建一份持久
//!   文件，是为了让「`daemon.status` 的 `nodeId` 与 `nodePublicKey` 必然同源」：没有第二处可被替换、
//!   可与公钥不一致的身份记录。代价如实登记：**换密钥即换 `nodeId`**（`node.rotate-key.begin` 在
//!   本切片恒回 `local.unsupported`，轮换落地时必须连同「`nodeId` 是否随轮换变化」一起裁定）。
//! - 派生输入是域分离前缀 + 65 字节 SEC1 未压缩公钥，输出取 SHA-256 前 16 字节并按 RFC 4122 的
//!   形状置位（version 8 = 自定义派生、variant = 10xx），最终是 `core` 的 `NodeId` 要求的规范小写文本。

use identity_auth::{IdentityKeystore, KeyHandle, KeyPurpose, KeystoreError, PeerPublicKey};
use sha2::{Digest as _, Sha256};

use acp_core::model::NodeId;

/// 节点身份条目的标签（keystore 的条目引用是 `<purpose>/<label>`，见 `identity-keystore` 的 `handle_of`）。
const NODE_KEY_LABEL: &str = "primary";

/// `nodeId` 派生输入的域分离前缀（本机解析，不出现在任何 wire 上）。
const NODE_ID_DOMAIN: &[u8] = b"acp-remote/node-id/v1";

/// 本节点身份。
#[derive(Clone)]
pub struct NodeIdentity {
    node_id: NodeId,
    key: KeyHandle,
    public_key: PeerPublicKey,
}

impl std::fmt::Debug for NodeIdentity {
    /// 只打印标识与公钥指纹：`KeyHandle` 不进日志（`SECURITY_DESIGN.md` §14.1）。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NodeIdentity")
            .field("node_id", &self.node_id)
            .field("public_key_fingerprint", &self.public_key.fingerprint())
            .finish_non_exhaustive()
    }
}

/// 本节点身份就绪失败。全部失败关闭：调用方必须拒绝启动。
#[derive(Debug, thiserror::Error)]
pub enum IdentityUnavailable {
    /// 平台 keystore 不可用。
    #[error("平台安全存储不可用")]
    KeystoreUnavailable,
    /// 条目损坏、用途不符或引用非法等实现缺陷。
    #[error("节点身份条目不可用")]
    EntryUnusable,
}

impl NodeIdentity {
    /// 就绪：读节点身份密钥的公钥；条目缺失时生成一把新密钥。返回的身份在同一进程内不变。
    pub async fn ensure(keystore: &dyn IdentityKeystore) -> Result<Self, IdentityUnavailable> {
        let handle = KeyHandle::new(&format!(
            "{}/{NODE_KEY_LABEL}",
            KeyPurpose::NodeIdentity.as_str()
        ))
        .map_err(|_| IdentityUnavailable::EntryUnusable)?;
        let public_key = match keystore.public_key(&handle).await {
            Ok(public_key) => public_key,
            Err(KeystoreError::EntryMissing) => {
                // 首次启动：生成并读取公钥（keystore 决定条目的落点与包裹方式）。
                keystore
                    .generate(KeyPurpose::NodeIdentity, NODE_KEY_LABEL)
                    .await
                    .map_err(map_keystore_error)?;
                keystore
                    .public_key(&handle)
                    .await
                    .map_err(map_keystore_error)?
            }
            Err(error) => return Err(map_keystore_error(error)),
        };
        let node_id = node_id_for(&public_key);
        Ok(Self {
            node_id,
            key: handle,
            public_key,
        })
    }

    /// 本节点标识。
    pub fn node_id(&self) -> &NodeId {
        &self.node_id
    }

    /// 节点身份密钥的端口引用（交给 `identity_auth::Authority` 签名用）。
    pub fn key(&self) -> &KeyHandle {
        &self.key
    }

    /// 65 字节 SEC1 未压缩公钥。
    pub fn public_key(&self) -> &PeerPublicKey {
        &self.public_key
    }
}

/// 由节点身份公钥派生 `nodeId`（见模块头注释的口径与代价）。
pub fn node_id_for(public_key: &PeerPublicKey) -> NodeId {
    let mut hasher = Sha256::new();
    hasher.update(NODE_ID_DOMAIN);
    hasher.update(public_key.as_bytes());
    let digest = hasher.finalize();
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    // RFC 4122 形状：version nibble = 8（自定义派生），variant = 10xx。
    bytes[6] = (bytes[6] & 0x0f) | 0x80;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let text = format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    );
    // 形状由上面的格式化保证（36 字符、小写 hex、四个连字符）；不合法只可能是本函数被改坏。
    NodeId::new(&text).unwrap_or_else(|_| {
        tracing::error!(
            event = "identity.node_id_invalid",
            "派生的 nodeId 不是规范 UUID"
        );
        NodeId::new("00000000-0000-0000-0000-000000000008")
            .expect("常量文本必然通过 NodeId 的形状校验")
    })
}

/// keystore 失败 → 启动失败：只有「不可用」是可重试的部署问题，其余都是实现/数据缺陷。
fn map_keystore_error(error: KeystoreError) -> IdentityUnavailable {
    match error {
        KeystoreError::Unavailable => IdentityUnavailable::KeystoreUnavailable,
        KeystoreError::EntryMissing
        | KeystoreError::EntryCorrupt
        | KeystoreError::EntryInvalid
        | KeystoreError::PurposeMismatch
        | KeystoreError::SecretMissing => IdentityUnavailable::EntryUnusable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 基点 G 的 SEC1 未压缩编码（曲线上的确定点，测试不需要随机源）。
    const BASE_POINT_HEX: &str = concat!(
        "04",
        "6b17d1f2e12c4247f8bce6e563a440f277037d812deb33a0f4a13945d898c296",
        "4fe342e2fe1a7f9b8ee7eb4a7c0f9e162bce33576b315ececbb6406837bf51f5"
    );

    fn public_key(hex: &str) -> PeerPublicKey {
        let bytes: Vec<u8> = hex
            .as_bytes()
            .chunks(2)
            .map(|pair| {
                let hi = (pair[0] as char).to_digit(16).expect("hex") as u8;
                let lo = (pair[1] as char).to_digit(16).expect("hex") as u8;
                (hi << 4) | lo
            })
            .collect();
        PeerPublicKey::try_from_bytes(&bytes).expect("合法的 P-256 未压缩点")
    }

    #[test]
    fn the_derived_node_id_is_a_canonical_uuid_and_bound_to_the_key() {
        let key = public_key(BASE_POINT_HEX);
        let first = node_id_for(&key);
        let second = node_id_for(&key);
        assert_eq!(first, second, "同一公钥必须派生同一 nodeId");
        let text = first.as_str();
        assert_eq!(text.len(), 36);
        assert_eq!(
            text.split('-').map(str::len).collect::<Vec<_>>(),
            vec![8, 4, 4, 4, 12]
        );
        assert!(
            text.chars()
                .all(|character| character.is_ascii_hexdigit() || character == '-')
        );
        assert_eq!(&text[14..15], "8", "version nibble 标识自定义派生");
        assert!(matches!(&text[19..20], "8" | "9" | "a" | "b"), "variant 位");

        // 另一把密钥 → 另一个 nodeId（2G 是 P-256 上的另一个确定点，坐标取自 NIST 测试向量）。
        let other = public_key(
            "04\
             7cf27b188d034f7e8a52380304b51ac3c08969e277f21b35a60b48fc47669978\
             07775510db8ed040293d9ac69f7430dbba7dade63ce982299e04b79d227873d1",
        );
        assert_ne!(node_id_for(&other), first);
    }
}
