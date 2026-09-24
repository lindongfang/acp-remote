//! 条目仓库：目录布局、原子写、读/删与端口实现。
//!
//! 布局（`design.md` D6）：`<root>/<purpose>/<label>.<version>`；`<root>` 的父子目录由调用方给出
//! （Daemon 用配置里的数据目录），本 crate 只在**包裹成功之后**才创建目录与文件。
//!
//! 原子写：同目录临时文件 → `fsync` → `rename`。写失败的临时文件会被删除，不会留下半成品条目
//! （对照合同 §7 的「条目损坏时失败而不覆盖」）。
//!
//! **持久性口径（如实登记，不夸大）**：`write_atomic` 只对文件本身做 `sync_all`，**不**对父目录
//! fsync。因此「原子替换」保证的是「要么旧内容、要么新内容，不会半截」，**不**保证掉电后新条目
//! 一定可见（可能仍看到旧条目）。keystore 语义没有崩溃持久性要求；需要时应在此处补父目录 fsync
//! （unix）或 write-through 替换（Windows）。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use identity_auth::{
    EntropySource, IdentityKeystore, KeyHandle, KeyPurpose, KeystoreError, P1363Signature,
    PeerPublicKey, SecretBytes, SecretPurpose,
};
use p256::elliptic_curve::sec1::ToEncodedPoint as _;

use crate::entry::{EntryHeader, EntryPurpose, FORMAT_VERSION, PRIVATE_KEY_LEN, SALT_LEN};
use crate::error::StoreError;
use crate::platform::{unwrap_secret, wrap_secret};

/// 生成私钥标量时的最大重试次数（熵源给出零值/超范围的标量时必须换一个，而不是接受它）。
const MAX_SCALAR_ATTEMPTS: usize = 8;

/// 后端可用性：由平台决定，**测试与审计脚本可以显式覆盖**。
///
/// 为什么把它做成显式值而不是到处写 `cfg!`：非 Windows 的失败关闭路径是本 crate 最重要的行为，
/// 但它恰好是「本机跑不到」的那条路径。把可用性变成构造参数后，**同一个用例可以在任何平台执行
/// 两条路径**（[`FileKeystore::with_availability`]），Linux 上真实的 `platform_supported() == false`
/// 与 Windows 上强制 `Unavailable` 走同一批断言。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Availability {
    /// 平台提供可用后端（Windows）：读写真正落到 DPAPI + 文件系统。
    Platform,
    /// 平台没有后端（非 Windows）：**所有**入口一律返回 `KeystoreError::Unavailable`，不触碰文件系统。
    Unavailable,
}

impl Availability {
    /// 不可用时在入口直接失败（读取/删除都不能「看起来像条目缺失」）。
    fn require(self) -> Result<(), KeystoreError> {
        match self {
            Self::Platform => Ok(()),
            Self::Unavailable => Err(KeystoreError::Unavailable),
        }
    }

    /// 由编译期平台决定。
    const fn detected() -> Self {
        if cfg!(windows) {
            Self::Platform
        } else {
            Self::Unavailable
        }
    }
}

/// 私有目录的模式位（unix）。抽成常量是为了让**任何平台**都能断言它的取值：
/// unix 上的真实 `chmod` 行为只能在 Linux runner 上执行，常量写错却能在本机被立刻发现。
#[cfg_attr(
    not(unix),
    allow(
        dead_code,
        reason = "Windows 构建走 DPAPI 包裹，模式位常量只由 Unix 路径与测试使用"
    )
)]
pub(crate) const PRIVATE_DIRECTORY_MODE: u32 = 0o700;

/// 条目文件的模式位（unix）。理由同上。
#[cfg_attr(
    not(unix),
    allow(
        dead_code,
        reason = "Windows 构建走 DPAPI 包裹，模式位常量只由 Unix 路径与测试使用"
    )
)]
pub(crate) const PRIVATE_FILE_MODE: u32 = 0o600;

/// 平台安全存储的目录实现。
pub struct FileKeystore {
    root: PathBuf,
    entropy: Arc<dyn EntropySource>,
    availability: Availability,
}

impl FileKeystore {
    /// 构造。`root` 由组合根决定；本方法**不**创建任何目录（避免「不可用平台也留下目录」）。
    pub fn new(root: impl Into<PathBuf>, entropy: Arc<dyn EntropySource>) -> Self {
        Self::with_availability(root, entropy, Availability::detected())
    }

    /// 显式指定可用性构造：**仅供测试与审计脚本**，生产装配用 [`FileKeystore::new`]。
    ///
    /// 用途是让「平台不可用 → 失败关闭且零写入」这条本机跑不到的路径可以在任何平台上被真实执行。
    pub fn with_availability(
        root: impl Into<PathBuf>,
        entropy: Arc<dyn EntropySource>,
        availability: Availability,
    ) -> Self {
        Self {
            root: root.into(),
            entropy,
            availability,
        }
    }

    /// 当前可用性（测试用来断言 `new` 取自平台检测）。
    pub fn availability(&self) -> Availability {
        self.availability
    }

    /// 根目录。
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// 条目路径：`<root>/<purpose>/<label>.<version>`。
    ///
    /// **大小写语义**：本 crate 不做大小写归一，标签按**文件系统语义**唯一——在大小写不敏感的文件系统
    /// （Windows/macOS）上 `Primary` 与 `primary` 是同一条目，后写者会覆盖前者（旧引用随后报
    /// `IdentityMismatch`）。调用方必须保证同一用途下的标签大小写不冲突（`entry.rs` 的
    /// `is_valid_label` 有同一说明）。
    ///
    /// **调用方必须先校验标签**：[`EntryHeader::new`] 与 [`EntryHeader::decode`] 都拒绝分隔符与控制字符，
    /// 本方法只做拼接（不返回 `Result`）——把未校验的文本传进来就会得到目录穿越。
    pub fn entry_path(&self, purpose: EntryPurpose, label: &str) -> PathBuf {
        self.root
            .join(purpose.directory())
            .join(format!("{label}.{FORMAT_VERSION}"))
    }

    /// 列出某用途下已存在的标签（用于孤儿回收与诊断；不返回秘密材料）。
    ///
    /// 与 7 个端口入口同样先过可用性闸门：不可用平台上返回 [`StoreError::PlatformUnavailable`]，
    /// 而不是「空仓库」——否则调用方会把「后端不可用」误读成「没有条目」。
    pub fn list(&self, purpose: EntryPurpose) -> Result<Vec<String>, StoreError> {
        self.availability
            .require()
            .map_err(|_| StoreError::PlatformUnavailable)?;
        let directory = self.root.join(purpose.directory());
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(StoreError::Io(error.kind().to_string())),
        };
        let suffix = format!(".{FORMAT_VERSION}");
        let mut labels = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| StoreError::Io(error.kind().to_string()))?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if let Some(label) = name.strip_suffix(&suffix) {
                labels.push(label.to_owned());
            }
        }
        labels.sort();
        Ok(labels)
    }

    /// 读取条目：校验头部与请求一致，返回头部与被包裹的秘密值。
    fn read_entry(
        &self,
        purpose: EntryPurpose,
        label: &str,
    ) -> Result<(EntryHeader, Vec<u8>), StoreError> {
        let path = self.entry_path(purpose, label);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(StoreError::Missing);
            }
            Err(error) => return Err(StoreError::Io(error.kind().to_string())),
        };
        let (header, _, wrapped) = EntryHeader::decode(&bytes)?;
        header.matches(purpose, label)?;
        Ok((header, wrapped.to_vec()))
    }

    /// 原子写入条目：先包裹、再建目录、再临时文件 + rename。任一步失败都不留半成品。
    fn write_entry(
        &self,
        purpose: EntryPurpose,
        label: &str,
        secret: &[u8],
    ) -> Result<(), StoreError> {
        if !purpose.accepts_len(secret.len()) {
            return Err(StoreError::TooLong);
        }
        let mut salt = [0u8; SALT_LEN];
        self.entropy
            .fill(&mut salt)
            .map_err(|_| StoreError::PlatformUnavailable)?;
        let header = EntryHeader::new(purpose, label, FORMAT_VERSION, salt)?;
        let entropy = header.additional_entropy();
        // 先包裹：非 Windows 平台在这里失败，因此**不会**创建目录或文件。
        let wrapped = wrap_secret(secret, &entropy)?;

        let path = self.entry_path(purpose, label);
        let directory = path
            .parent()
            .ok_or(StoreError::HeaderInvalid)?
            .to_path_buf();
        create_private_directory(&directory)?;

        let mut bytes = header.encode();
        bytes.extend_from_slice(&wrapped);
        write_atomic(&path, &bytes)
    }

    /// 解开条目并返回明文秘密值。
    ///
    /// 返回 [`SecretBytes`]（析构清零）：签名与读公钥路径上的明文副本同样被清零，
    /// 而不是在堆上留下未清零的 `Vec<u8>`。
    fn read_secret(&self, purpose: EntryPurpose, label: &str) -> Result<SecretBytes, StoreError> {
        let (header, wrapped) = self.read_entry(purpose, label)?;
        let entropy = header.additional_entropy();
        let mut secret = unwrap_secret(&wrapped, &entropy)?;
        if !purpose.accepts_len(secret.len()) {
            secret.fill(0);
            return Err(StoreError::Corrupt);
        }
        let copy = SecretBytes::new(&secret);
        // 平台解包返回的中间 `Vec<u8>` 必须清零后再释放，否则签名/读公钥的每次调用都会在堆上
        // 留下未清零的私钥副本（`SecretBytes` 的析构清零只覆盖它自己那份）。平台 wrapper 内部
        // 仍有一份自己的缓冲区（第三方实现，登记在 platform/windows.rs 的 `unwrap_secret` 文档里）。
        secret.fill(0);
        drop(secret);
        Ok(copy)
    }

    /// 删除条目；不存在时返回明确错误（不静默成功）。
    fn remove_entry(&self, purpose: EntryPurpose, label: &str) -> Result<(), StoreError> {
        let path = self.entry_path(purpose, label);
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(StoreError::Missing),
            Err(error) => Err(StoreError::Io(error.kind().to_string())),
        }
    }

    /// 生成一个新的节点身份私钥标量（拒绝零值与非曲线标量）。
    ///
    /// 返回 [`SecretBytes`]（析构清零）而不是裸 `[u8; 32]`：栈上不留未清零的私钥副本
    /// （**已知残余**：本函数内部的 `candidate` 是栈数组，出错轮次无法被 `SecretBytes` 覆盖，
    /// 退出前显式清零；`p256::SecretKey` 自身在 `zeroize` feature 下会清零）。
    fn generate_scalar(&self) -> Result<SecretBytes, StoreError> {
        for _ in 0..MAX_SCALAR_ATTEMPTS {
            let mut candidate = [0u8; PRIVATE_KEY_LEN];
            self.entropy.fill(&mut candidate).map_err(|_| {
                candidate.fill(0);
                StoreError::PlatformUnavailable
            })?;
            if p256::SecretKey::from_slice(&candidate).is_ok() {
                let scalar = SecretBytes::new(&candidate);
                candidate.fill(0);
                return Ok(scalar);
            }
            candidate.fill(0);
        }
        Err(StoreError::PlatformUnavailable)
    }
}

#[async_trait]
impl IdentityKeystore for FileKeystore {
    async fn generate(&self, purpose: KeyPurpose, label: &str) -> Result<KeyHandle, KeystoreError> {
        self.availability.require()?;
        let entry_purpose = EntryPurpose::from_key_purpose(purpose);
        let scalar = self.generate_scalar()?;
        self.write_entry(entry_purpose, label, scalar.as_bytes())?;
        handle_of(entry_purpose, label)
    }

    async fn public_key(&self, handle: &KeyHandle) -> Result<PeerPublicKey, KeystoreError> {
        self.availability.require()?;
        let (purpose, label) = parse_handle(handle)?;
        let secret = self.read_secret(purpose, &label)?;
        let secret_key = p256::SecretKey::from_slice(secret.as_bytes())
            .map_err(|_| KeystoreError::EntryCorrupt)?;
        let point = secret_key.public_key().to_encoded_point(false);
        PeerPublicKey::try_from_bytes(point.as_bytes()).map_err(|_| KeystoreError::EntryCorrupt)
    }

    async fn sign(
        &self,
        handle: &KeyHandle,
        transcript: &[u8],
    ) -> Result<P1363Signature, KeystoreError> {
        use p256::ecdsa::signature::Signer as _;

        self.availability.require()?;
        let (purpose, label) = parse_handle(handle)?;
        if purpose != EntryPurpose::NodeIdentity {
            return Err(KeystoreError::PurposeMismatch);
        }
        let secret = self.read_secret(purpose, &label)?;
        let secret_key = p256::SecretKey::from_slice(secret.as_bytes())
            .map_err(|_| KeystoreError::EntryCorrupt)?;
        let signing_key = p256::ecdsa::SigningKey::from(secret_key);
        let signature: p256::ecdsa::Signature = signing_key.sign(transcript);
        P1363Signature::try_from_bytes(signature.to_bytes().as_slice())
            .map_err(|_| KeystoreError::EntryCorrupt)
    }

    async fn delete(&self, handle: &KeyHandle) -> Result<(), KeystoreError> {
        // 可用性先于句柄形状：平台不可用时连「条目是否存在」都不该被探测。
        self.availability.require()?;
        let (purpose, label) = parse_handle(handle)?;
        self.remove_entry(purpose, &label)?;
        Ok(())
    }

    async fn get_secret(
        &self,
        purpose: SecretPurpose,
        key: &str,
    ) -> Result<Option<SecretBytes>, KeystoreError> {
        self.availability.require()?;
        let entry_purpose = EntryPurpose::from_secret_purpose(purpose);
        match self.read_secret(entry_purpose, key) {
            Ok(secret) => Ok(Some(secret)),
            Err(StoreError::Missing) => Ok(None),
            Err(error) => Err(error.into_port()),
        }
    }

    async fn put_secret(
        &self,
        purpose: SecretPurpose,
        key: &str,
        value: &SecretBytes,
    ) -> Result<(), KeystoreError> {
        self.availability.require()?;
        let entry_purpose = EntryPurpose::from_secret_purpose(purpose);
        self.write_entry(entry_purpose, key, value.as_bytes())?;
        Ok(())
    }

    async fn delete_secret(&self, purpose: SecretPurpose, key: &str) -> Result<(), KeystoreError> {
        self.availability.require()?;
        let entry_purpose = EntryPurpose::from_secret_purpose(purpose);
        match self.remove_entry(entry_purpose, key) {
            Ok(()) => Ok(()),
            Err(StoreError::Missing) => Err(KeystoreError::SecretMissing),
            Err(error) => Err(error.into_port()),
        }
    }
}

/// 由用途与标签构造端口引用（文本形状 `<purpose>/<label>`）。
fn handle_of(purpose: EntryPurpose, label: &str) -> Result<KeyHandle, KeystoreError> {
    KeyHandle::new(&format!("{}/{label}", purpose.directory()))
        .map_err(|_| KeystoreError::EntryInvalid)
}

/// 解析端口引用；用途不匹配返回 `PurposeMismatch`，标签形状非法返回 `EntryInvalid`。
///
/// 标签校验与写入/读取路径**共用** [`EntryPurpose` 同级的 `is_valid_label`]（`entry.rs`）：
/// 若这里只做「非空 + 长度 + 分隔符」，含 Windows 保留字符或首尾空白的引用会落到 fs 层报
/// `Io`/`NotFound`，被端口层映射成 `Unavailable`/`EntryMissing`——把「引用非法」误报成
/// 「后端不可用」或「条目缺失」。
fn parse_handle(handle: &KeyHandle) -> Result<(EntryPurpose, String), KeystoreError> {
    let text = handle.as_str();
    let (purpose_text, label) = text.split_once('/').ok_or(KeystoreError::EntryInvalid)?;
    let purpose = match purpose_text {
        "node-identity" => EntryPurpose::NodeIdentity,
        "provider-credential" => EntryPurpose::ProviderCredential,
        _ => return Err(KeystoreError::PurposeMismatch),
    };
    if !crate::entry::is_valid_label(label) {
        return Err(KeystoreError::EntryInvalid);
    }
    Ok((purpose, label.to_owned()))
}

/// 创建目录（Unix 上限制为 `0700`）。
fn create_private_directory(directory: &Path) -> Result<(), StoreError> {
    if !directory.exists() {
        fs::create_dir_all(directory).map_err(|error| StoreError::Io(error.kind().to_string()))?;
    }
    #[cfg(unix)]
    {
        // 即使目录已经存在也收紧权限：否则「目录先被别的工具建成 0755」会永久削弱条目保护。
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(
            directory,
            fs::Permissions::from_mode(PRIVATE_DIRECTORY_MODE),
        )
        .map_err(|error| StoreError::Io(error.kind().to_string()))?;
    }
    // Windows 没有对应的模式位；DPAPI 的保护由条目包裹承担。
    let _ = directory;
    Ok(())
}

/// 原子写：临时文件 → （Unix `0600`）→ `rename`；**任何**失败都不留临时文件。
///
/// 临时名带进程 id + 进程内单调计数器：同一条目的两次并发写不会复用同一个临时路径，
/// 因此不会出现「一个 writer rename 到另一个 writer 的半截文件」。
/// 整个写入过程由 [`TempFileGuard`] 包住：只有成功 rename 后才解除守卫，其余任何返回路径都会删掉临时文件。
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), StoreError> {
    static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    let parent = path.parent().ok_or(StoreError::HeaderInvalid)?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(StoreError::HeaderInvalid)?;
    let sequence = SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let temporary = parent.join(format!(".{name}.tmp-{}-{sequence}", std::process::id()));
    let mut guard = TempFileGuard {
        path: temporary.clone(),
        armed: true,
    };

    {
        use std::io::Write as _;
        let mut file = fs::File::create(&temporary)
            .map_err(|error| StoreError::Io(error.kind().to_string()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            file.set_permissions(fs::Permissions::from_mode(PRIVATE_FILE_MODE))
                .map_err(|error| StoreError::Io(error.kind().to_string()))?;
        }
        file.write_all(bytes)
            .map_err(|error| StoreError::Io(error.kind().to_string()))?;
        file.sync_all()
            .map_err(|error| StoreError::Io(error.kind().to_string()))?;
    }

    fs::rename(&temporary, path).map_err(|error| StoreError::Io(error.kind().to_string()))?;
    guard.armed = false;
    Ok(())
}

/// 临时文件守卫：析构时若仍处于「已武装」状态就删除临时文件（失败路径不留残留）。
struct TempFileGuard {
    path: PathBuf,
    armed: bool,
}

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
mod mode_tests {
    //! 与平台无关的模式位断言：常量一旦被改成宽松值（例如 `0o644`），任何平台的测试都会失败。
    //! unix 上的**真实** `chmod` 行为另有 `unix_modes` 模块（只在 Unix 构建里执行）。

    use super::{PRIVATE_DIRECTORY_MODE, PRIVATE_FILE_MODE};

    #[test]
    fn private_modes_are_restrictive() {
        assert_eq!(PRIVATE_DIRECTORY_MODE, 0o700, "私有目录模式必须是 0700");
        assert_eq!(PRIVATE_FILE_MODE, 0o600, "条目文件模式必须是 0600");
        assert_eq!(
            PRIVATE_DIRECTORY_MODE & 0o077,
            0,
            "目录不得给组/其他用户任何权限"
        );
        assert_eq!(
            PRIVATE_FILE_MODE & 0o077,
            0,
            "文件不得给组/其他用户任何权限"
        );
    }
}

#[cfg(all(test, unix))]
mod unix_modes {
    //! Unix 权限位的**真实**断言（随 lib 单测二进制在 Linux CI 上执行，不需要平台后端）。
    //!
    //! 为什么放在这里而不是 `tests/`：```tests/permissions.rs``` 这类集成测试在非 Windows 上会因为
    //! 「平台后端不可用 → 入口失败关闭」而**必然早退**，从而变成「0 断言但仍算通过」。本模块直接调用
    //! 私有的目录/原子写辅助函数，因此模式位逻辑在任何 Unix 构建里都真被执行。
    //!
    //! 残余限制（如实登记）：在非 Windows 构建里，`write_entry` 之前的 `wrap_secret` 一定会失败，
    //! 因此 0700/0600 这两行在真实「写条目」路径上尚未被完整路径覆盖——它由本模块按辅助函数粒度覆盖。

    use super::*;
    use std::os::unix::fs::PermissionsExt as _;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn mode_of(path: &Path) -> u32 {
        fs::metadata(path)
            .expect("路径必须存在")
            .permissions()
            .mode()
            & 0o777
    }

    #[test]
    fn private_directory_and_entry_file_modes_are_restrictive() {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "acpr-keystore-modes-{}-{sequence}",
            std::process::id()
        ));
        let directory = root.join("node-identity");

        create_private_directory(&directory).expect("创建私有目录必须成功");
        assert_eq!(
            mode_of(&directory),
            PRIVATE_DIRECTORY_MODE,
            "私有目录必须是 0700"
        );

        let path = directory.join(format!("primary.{FORMAT_VERSION}"));
        write_atomic(&path, b"wrapped-bytes").expect("原子写必须成功");
        assert_eq!(mode_of(&path), PRIVATE_FILE_MODE, "条目文件必须是 0600");
        assert_eq!(fs::read(&path).expect("可读回"), b"wrapped-bytes");

        // 已存在的宽松目录会被收紧（而不是「已存在就放过」）。
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o755))
            .expect("构造宽松目录必须成功");
        create_private_directory(&directory).expect("重复调用必须成功");
        assert_eq!(
            mode_of(&directory),
            PRIVATE_DIRECTORY_MODE,
            "已存在的目录也必须被收紧到 0700"
        );

        // 成功的原子写不留临时文件。
        let leftovers: Vec<String> = fs::read_dir(&directory)
            .expect("目录可读")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.contains(".tmp-"))
            .collect();
        assert!(leftovers.is_empty(), "不得残留临时文件：{leftovers:?}");

        fs::remove_dir_all(&root).ok();
    }
}

#[cfg(test)]
mod atomic_tests {
    //! `write_atomic` 的替换语义与**失败清理**（任何平台的 lib 单测二进制都会执行——
    //! 这条清理路径与模式位无关，因此不能放在 `unix_modes` 里）。

    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[test]
    fn write_atomic_replaces_content_without_leaving_temporaries_on_failure() {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "acpr-keystore-atomic-{}-{sequence}",
            std::process::id()
        ));
        let directory = root.join("provider-credential");
        create_private_directory(&directory).expect("创建目录必须成功");
        let path = directory.join(format!("token.{FORMAT_VERSION}"));

        write_atomic(&path, b"first").expect("首次写必须成功");
        write_atomic(&path, b"second").expect("替换必须成功");
        assert_eq!(
            fs::read(&path).expect("可读回"),
            b"second",
            "必须整体替换而不是追加"
        );

        // 失败路径 ①：临时名指向一个**已存在的目录** → `File::create` 失败，守卫必须清掉它
        // （这条路径才会真的产生「清理」这个动作；父目录不存在时按构造连临时文件都不会出现）。
        let blocked = directory.join(format!(".blocked.{FORMAT_VERSION}"));
        fs::create_dir(&blocked).expect("构造同名目录必须成功");
        assert!(
            write_atomic(&blocked, b"x").is_err(),
            "目标为目录时必须失败"
        );
        let leftovers: Vec<String> = fs::read_dir(&directory)
            .expect("目录可读")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.contains(".tmp-"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "失败路径不得残留临时文件：{leftovers:?}"
        );
        assert!(blocked.is_dir(), "守卫不得删除失败写入的目标目录本身");

        // 失败路径 ②：父目录不存在 → 返回错误，且不得顺手创建父目录。
        let missing = root.join("no-such-dir").join("entry");
        assert!(write_atomic(&missing, b"x").is_err(), "父目录缺失必须失败");
        assert!(!root.join("no-such-dir").exists(), "失败不得创建目标目录");

        fs::remove_dir_all(&root).ok();
    }
}
