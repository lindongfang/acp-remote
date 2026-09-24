//! WP2 的测试基座：fake keystore、定序熵源、可推进 `Clock` 与固定向量常量。
//!
//! 设计要点（`plan.md` D1/D8）：
//!
//! - **fake 只实现端口形状**，不做任何持久化：用它把「状态机不访问存储」这件事变成可断言的
//!   （崩溃/重启用「新建 `Authority`、复用同一组端口」表达）；
//! - **密钥材料固定在 fake 里**：签名用一把固定的 P-256 私钥，公钥由它导出，因此「自选公钥不通过」
//!   这类负例可以直接构造；
//! - **熵源是定序的**：同一序列在重跑时给出同样字节，测试因此不需要随机性也能覆盖 nonce/标识路径。

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use acp_core::model::{
    DeviceId, GrantSet, NodeId, Nonce, PairingId, PeerPublicKey, ScopeSet, Timestamp,
};
use acp_core::ports::Clock;
use async_trait::async_trait;
use identity_auth::{
    Authority, ChallengeId, EntropyError, EntropySource, IdentityKeystore, KeyHandle, KeyPurpose,
    KeystoreError, P1363Signature, PairingRequestId, SecretBytes, SecretPurpose,
};

/// 固定向量里的 host（本节点）标识。
pub const HOST: &str = "bdb2ec20-f98c-4d87-b789-e540d527ef87";
/// 固定向量里的设备标识。
pub const DEVICE: &str = "2ae1c07c-9242-46e9-a9d2-4ec58c130f49";
/// 固定向量里的配对标识。
pub const PAIRING: &str = "2bc8b944-2a4f-46a7-8c31-b2c40923f60a";
/// 固定向量里的另一台设备标识（并发/冲突用例）。
pub const OTHER_DEVICE: &str = "9c8f6b1d-7a35-4f0b-9b6a-2f6d5c4e3b1a";
/// 固定向量里的 canonical origin。
pub const ORIGIN: &str = "https://work-pc.example.ts.net";
/// 基准时刻（挑战 TTL 与配对过期都以它为起点）。
pub const AT: &str = "2026-01-01T00:00:00.000Z";

/// 从 16 进制文本构造一个 65 字节 SEC1 公钥（Node Link 向量用 `*Hex` 形态）。
pub fn key_from_hex(text: &str) -> PeerPublicKey {
    let bytes = hex(text);
    assert_eq!(bytes.len(), 65, "测试用公钥必须是 65 字节未压缩点");
    PeerPublicKey::try_from_bytes(&bytes).expect("测试用公钥必须是 P-256 上的合法点")
}

/// 从规范 base64url 构造 65 字节 SEC1 公钥。
pub fn key_from_base64url(text: &str) -> PeerPublicKey {
    let bytes = acpr_transcript::decode_base64url(text).expect("测试用公钥必须是规范 base64url");
    PeerPublicKey::try_from_bytes(&bytes).expect("测试用公钥必须是 65 字节 SEC1 点")
}

/// 解析一个 `PairingRequestId`。
pub fn request_id(text: &str) -> PairingRequestId {
    PairingRequestId::parse(text).expect("测试用 PairingRequestId 文本必须规范")
}

/// 解析一个 `ChallengeId`。
pub fn challenge_id(text: &str) -> ChallengeId {
    ChallengeId::parse(text).expect("测试用 ChallengeId 文本必须规范")
}

/// 测试用对端私钥：设备/节点侧自行持有的签名密钥（不是本机 keystore 里的那把）。
pub struct PeerKey {
    signing: p256::ecdsa::SigningKey,
    public_key: PeerPublicKey,
}

impl PeerKey {
    /// 由固定标量构造（测试专用值）。
    pub fn new() -> Self {
        Self::from_seed("0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b")
    }

    /// 由指定 32 字节标量构造。
    pub fn from_seed(seed_hex: &str) -> Self {
        let bytes = hex(seed_hex);
        let signing = p256::ecdsa::SigningKey::from_slice(&bytes).expect("合法标量");
        let point = p256::ecdsa::VerifyingKey::from(&signing).to_encoded_point(false);
        let public_key = PeerPublicKey::try_from_bytes(point.as_bytes()).expect("合法公钥");
        Self {
            signing,
            public_key,
        }
    }

    /// 对端公钥（写进 `ClaimFields` 或 `PeerTrust`）。
    pub fn public_key(&self) -> PeerPublicKey {
        self.public_key.clone()
    }

    /// 对已装配 transcript 签名（测试扮演对端）。
    pub fn sign(&self, transcript: &[u8]) -> P1363Signature {
        use p256::ecdsa::signature::Signer as _;
        let signature: p256::ecdsa::Signature = self.signing.sign(transcript);
        P1363Signature::try_from_bytes(signature.to_bytes().as_slice()).expect("64 字节 P1363")
    }

    /// 同一 transcript 的 high-S 形态（`s' = n - s`）；P-256 的阶是公开常量。
    pub fn sign_high_s(&self, transcript: &[u8]) -> P1363Signature {
        let base = self.sign(transcript);
        let order = hex("ffffffff00000000ffffffffffffffffbce6faada7179e84f3b9cac2fc632551");
        let mut out = *base.as_bytes();
        let (r, s) = out.split_at_mut(32);
        let mut borrow = 0i16;
        for index in (0..32).rev() {
            let value = i16::from(order[index]) - i16::from(s[index]) - borrow;
            if value < 0 {
                s[index] = (value + 256) as u8;
                borrow = 1;
            } else {
                s[index] = value as u8;
                borrow = 0;
            }
        }
        assert_eq!(r.len(), 32);
        P1363Signature::try_from_bytes(&out).expect("64 字节 P1363")
    }
}

impl Default for PeerKey {
    fn default() -> Self {
        Self::new()
    }
}

/// 扮演对端：用配对 secret 计算 HMAC 证明（对端是另一个实现，这里用同一原语直接算）。
pub fn client_hmac(
    secret: &identity_auth::PairingSecret,
    transcript: &[u8],
) -> identity_auth::PairingProof {
    use hmac::{KeyInit as _, Mac as _};
    let mut mac =
        hmac::Hmac::<sha2::Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC key 长度合法");
    mac.update(transcript);
    identity_auth::PairingProof::try_from_bytes(mac.finalize().into_bytes().as_slice())
        .expect("HMAC-SHA256 输出是 32 字节")
}

/// 解析一个 `NodeId`。
pub fn node(text: &str) -> NodeId {
    NodeId::new(text).expect("测试用 NodeId 文本必须规范")
}

/// 解析一个设备 Id。
pub fn device(text: &str) -> DeviceId {
    DeviceId::new(text).expect("测试用 DeviceId 文本必须规范")
}

/// 解析一个配对 Id。
pub fn pairing(text: &str) -> PairingId {
    PairingId::new(text).expect("测试用 PairingId 文本必须规范")
}

/// 解析一个时间戳。
pub fn ts(text: &str) -> Timestamp {
    Timestamp::new(text).expect("测试用时间戳文本必须规范")
}

/// 16 进制文本 → 定长字节（nonce/密钥测试数据用）。
pub fn hex(text: &str) -> Vec<u8> {
    (0..text.len() / 2)
        .map(|index| u8::from_str_radix(&text[index * 2..index * 2 + 2], 16).expect("合法 hex"))
        .collect()
}

/// 从 16 进制文本构造 32 字节 nonce 的 base64url（左侧补零到 32 字节，便于测试里用小种子）。
pub fn nonce_from_hex(text: &str) -> Nonce {
    let seed = hex(text);
    assert!(seed.len() <= 32, "测试用 nonce 种子不得超过 32 字节");
    let mut bytes = vec![0u8; 32 - seed.len()];
    bytes.extend_from_slice(&seed);
    Nonce::new(&acpr_transcript::encode_base64url(&bytes)).expect("nonce 文本必须规范")
}

/// 不可用的 `ScopeSet`/`GrantSet` 快捷构造。
pub fn scopes(names: &[&str]) -> ScopeSet {
    ScopeSet::try_from_iter(names.iter().copied()).expect("测试用 scope 名必须规范")
}

/// 快捷构造 grant 集合。
pub fn grants(names: &[&str]) -> GrantSet {
    GrantSet::try_from_iter(names.iter().copied()).expect("测试用 grant 名必须规范")
}

/// 可推进的注入时钟。
#[derive(Debug, Default)]
pub struct FakeClock(Mutex<Option<Timestamp>>);

impl FakeClock {
    /// 以基准时刻启动。
    pub fn new() -> Arc<Self> {
        Arc::new(Self(Mutex::new(Some(ts(AT)))))
    }

    /// 推进到给定时刻（既不读系统时间，也不等待真实时间流逝）。
    pub fn set(&self, at: &str) {
        *self.0.lock().expect("测试时钟锁") = Some(ts(at));
    }
}

impl Clock for FakeClock {
    fn now(&self) -> Timestamp {
        self.0
            .lock()
            .expect("测试时钟锁")
            .clone()
            .expect("测试时钟必须已初始化")
    }
}

/// 定序熵源：每次调用返回递增的字节序列，因此可重跑且不需要系统随机源。
#[derive(Debug, Default)]
pub struct SequenceEntropy {
    counter: Mutex<u8>,
    fail: Mutex<bool>,
}

impl SequenceEntropy {
    /// 新建。
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// 让后续 `fill` 失败（覆盖「熵源不可用 → 失败关闭」）。
    pub fn fail_next(&self) {
        *self.fail.lock().expect("熵源锁") = true;
    }
}

impl EntropySource for SequenceEntropy {
    fn fill(&self, out: &mut [u8]) -> Result<(), EntropyError> {
        if *self.fail.lock().expect("熵源锁") {
            return Err(EntropyError::Unavailable);
        }
        let mut counter = self.counter.lock().expect("熵源锁");
        for byte in out.iter_mut() {
            *counter = counter.wrapping_add(1);
            *byte = *counter;
        }
        Ok(())
    }
}

/// 同步入口直通：/ 一类入口不返回 future，测试里用它与 [] 配对书写。
pub fn sync_result<T>(value: T) -> T {
    value
}

/// 无 runtime 的最小 executor。
///
/// `hello` 里唯一的 `await` 是 keystore/熵源调用，测试实现立即返回；因此本 executor 只需轮询到 `Ready`。
/// 长时间 `Pending` 时以明确消息失败：那意味着有人引入了真正需要 runtime 的调用，而不是「再等一会」。
pub fn block_on<F: std::future::Future>(future: F) -> F::Output {
    use std::task::{Context, Poll, Waker};

    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    let mut future = std::pin::pin!(future);
    for _ in 0..1_000_000 {
        if let Poll::Ready(output) = future.as_mut().poll(&mut context) {
            return output;
        }
    }
    panic!("future 长时间未就绪：测试里的端口必须立即返回（本 executor 不驱动真实 IO）");
}

/// 固定私钥的 fake keystore：公钥由私钥导出，签名在进程内完成，secret 存在内存里。
pub struct FakeKeystore {
    signing: p256::ecdsa::SigningKey,
    public_key: PeerPublicKey,
    signed: Mutex<Vec<Vec<u8>>>,
    fail_sign: Mutex<bool>,
    secrets: Mutex<BTreeMap<(String, String), Vec<u8>>>,
}

impl FakeKeystore {
    /// 用固定标量构造（测试专用值，绝不用于生产）。
    pub fn new() -> Arc<Self> {
        let bytes = hex("0707070707070707070707070707070707070707070707070707070707070707");
        let signing =
            p256::ecdsa::SigningKey::from_slice(&bytes).expect("固定标量必须是合法 P-256 私钥");
        let verifying = p256::ecdsa::VerifyingKey::from(&signing);
        let point = verifying.to_encoded_point(false);
        let public_key =
            PeerPublicKey::try_from_bytes(point.as_bytes()).expect("导出的公钥必须合法");
        Arc::new(Self {
            signing,
            public_key,
            signed: Mutex::new(Vec::new()),
            fail_sign: Mutex::new(false),
            secrets: Mutex::new(BTreeMap::new()),
        })
    }

    /// 本节点身份公钥（测试断言「挑战中的宿主证明可由它验证」）。
    ///
    /// 名字刻意不叫 `public_key`：否则会遮蔽 `IdentityKeystore::public_key` 端口方法。
    pub fn node_public_key(&self) -> PeerPublicKey {
        self.public_key.clone()
    }

    /// 已签名 transcript 的历史（断言「签名的是装配后的 transcript」）。
    pub fn signed(&self) -> Vec<Vec<u8>> {
        self.signed.lock().expect("签名记录锁").clone()
    }

    /// 让后续签名失败（覆盖「keystore 不可用 → 失败关闭」）。
    pub fn fail_next_sign(&self) {
        *self.fail_sign.lock().expect("签名锁") = true;
    }

    /// 另一个独立私钥导出的公钥（覆盖「自选公钥不能通过」）。
    pub fn other_public_key() -> PeerPublicKey {
        let bytes = hex("0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a");
        let signing =
            p256::ecdsa::SigningKey::from_slice(&bytes).expect("固定标量必须是合法 P-256 私钥");
        let point = p256::ecdsa::VerifyingKey::from(&signing).to_encoded_point(false);
        PeerPublicKey::try_from_bytes(point.as_bytes()).expect("导出的公钥必须合法")
    }

    /// 用另一把私钥对给定 transcript 签名（覆盖「自选公钥签名」）。
    pub fn sign_with_other(transcript: &[u8]) -> P1363Signature {
        use p256::ecdsa::signature::Signer as _;
        let bytes = hex("0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a0a");
        let signing =
            p256::ecdsa::SigningKey::from_slice(&bytes).expect("固定标量必须是合法 P-256 私钥");
        let signature: p256::ecdsa::Signature = signing.sign(transcript);
        P1363Signature::try_from_bytes(signature.to_bytes().as_slice()).expect("64 字节 P1363")
    }

    /// 直接对给定 transcript 用固定私钥签名（构造合法证明用）。
    pub fn sign_direct(&self, transcript: &[u8]) -> P1363Signature {
        use p256::ecdsa::signature::Signer as _;
        let signature: p256::ecdsa::Signature = self.signing.sign(transcript);
        P1363Signature::try_from_bytes(signature.to_bytes().as_slice()).expect("64 字节 P1363")
    }
}

#[async_trait]
impl IdentityKeystore for FakeKeystore {
    async fn generate(&self, purpose: KeyPurpose, label: &str) -> Result<KeyHandle, KeystoreError> {
        KeyHandle::new(&format!("{}/{label}", purpose.as_str()))
    }

    async fn public_key(&self, _handle: &KeyHandle) -> Result<PeerPublicKey, KeystoreError> {
        Ok(self.public_key.clone())
    }

    async fn sign(
        &self,
        _handle: &KeyHandle,
        transcript: &[u8],
    ) -> Result<P1363Signature, KeystoreError> {
        if *self.fail_sign.lock().expect("签名锁") {
            *self.fail_sign.lock().expect("签名锁") = false;
            return Err(KeystoreError::Unavailable);
        }
        self.signed
            .lock()
            .expect("签名记录锁")
            .push(transcript.to_vec());
        Ok(self.sign_direct(transcript))
    }

    async fn delete(&self, _handle: &KeyHandle) -> Result<(), KeystoreError> {
        Ok(())
    }

    async fn get_secret(
        &self,
        purpose: SecretPurpose,
        key: &str,
    ) -> Result<Option<SecretBytes>, KeystoreError> {
        Ok(self
            .secrets
            .lock()
            .expect("secret 锁")
            .get(&(purpose.as_str().to_owned(), key.to_owned()))
            .map(|value| SecretBytes::new(value)))
    }

    async fn put_secret(
        &self,
        purpose: SecretPurpose,
        key: &str,
        value: &SecretBytes,
    ) -> Result<(), KeystoreError> {
        self.secrets.lock().expect("secret 锁").insert(
            (purpose.as_str().to_owned(), key.to_owned()),
            value.as_bytes().to_vec(),
        );
        Ok(())
    }

    async fn delete_secret(&self, purpose: SecretPurpose, key: &str) -> Result<(), KeystoreError> {
        self.secrets
            .lock()
            .expect("secret 锁")
            .remove(&(purpose.as_str().to_owned(), key.to_owned()));
        Ok(())
    }
}

/// 测试用 `Authority` 装配（新实例 = 进程重启语义）。
pub fn authority(
    keystore: &Arc<FakeKeystore>,
    entropy: &Arc<SequenceEntropy>,
    clock: &Arc<FakeClock>,
) -> Authority {
    Authority::new(
        node(HOST),
        KeyHandle::new("node-identity/primary").expect("合法引用"),
        keystore.clone(),
        entropy.clone(),
        clock.clone(),
    )
}
