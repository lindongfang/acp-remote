//! 内容快照、类型分类与有界字节块（design D5/D7）。
//!
//! 快照只存在于内存：`open_content` 在**同一打开句柄**上取得该用途需要的字节（预览按文本/图片预算，
//! 下载按 64 MiB 预算），随后注册为连接期版本。后续 `read_content` 每次发出字节块前都会重新解析同一
//! 根内路径并比较身份；文件一旦变化即废弃快照并要求重新读取，绝不拼接两个版本。`release_content`
//! 幂等释放；空闲到期在每次注册表访问时惰性清理，总量与单条快照都有上限。
//!
//! 正文永不写临时文件、永不进日志。错误文本只说明「不可用」，不包含路径、版本或文件内容。

use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use acp_core::model::PortError;
use acp_core::model::{
    ContentSnapshot, ContentVersion, FileReadPurpose, RESULT_TOO_LARGE,
    SESSION_FILE_CHUNK_MAX_BYTES, SESSION_FILE_DOWNLOAD_MAX_BYTES,
    SESSION_FILE_IMAGE_PREVIEW_MAX_BYTES, SESSION_FILE_TEXT_PREVIEW_MAX_BYTES, SessionFileChunk,
    SessionFileContentKind, SessionFilePath,
};

use super::root::{FileIdentity, SecureRoot, unavailable};

/// 判断类型前读取的有界头部（足够覆盖所有图片 magic 与常见二进制判定）。
pub(crate) const TYPE_HEADER_BYTES: usize = 8 * 1024;
/// 快照总量上限：两个 64 MiB 下载快照之外还有余量，但总量始终有界。
pub(crate) const MAX_SNAPSHOT_BYTES: usize = 2 * SESSION_FILE_DOWNLOAD_MAX_BYTES as usize;
/// 适配器最多同时保留的快照数（每连接/节点的更细上限由 WP3 在接线层执行）。
pub(crate) const MAX_SNAPSHOTS: usize = 4;
/// 默认空闲 TTL（design D5：60 秒）。
pub(crate) const DEFAULT_IDLE_TTL: Duration = Duration::from_secs(60);

/// 一次打开后尚未注册的快照数据。
pub(crate) struct SnapshotRecord {
    pub(crate) root: String,
    pub(crate) path: SessionFilePath,
    pub(crate) kind: SessionFileContentKind,
    pub(crate) size_bytes: u64,
    pub(crate) stored: Vec<u8>,
    pub(crate) preview_truncated: bool,
    pub(crate) identity: FileIdentity,
    pub(crate) version: ContentVersion,
}

/// 读取一个字节块所需的数据（注册表外的纯数据）。
pub(crate) struct ChunkData {
    pub(crate) path: SessionFilePath,
    pub(crate) kind: SessionFileContentKind,
    pub(crate) size_bytes: u64,
    pub(crate) version: ContentVersion,
    pub(crate) offset: u64,
    pub(crate) data: Vec<u8>,
    pub(crate) eof: bool,
    pub(crate) preview_truncated: bool,
}

impl ChunkData {
    pub(crate) fn into_chunk(self) -> Result<SessionFileChunk, PortError> {
        SessionFileChunk::try_new(
            self.path,
            self.kind,
            self.size_bytes,
            self.version,
            self.offset,
            self.data,
            self.eof,
            self.preview_truncated,
        )
        .map_err(|_| unavailable())
    }
}

/// 在内存中注册的内容快照。
struct Snapshot {
    root: String,
    path: SessionFilePath,
    kind: SessionFileContentKind,
    size_bytes: u64,
    stored: Vec<u8>,
    preview_truncated: bool,
    identity: FileIdentity,
    version: ContentVersion,
    last_used: Instant,
}

#[derive(Default)]
struct Registry {
    snapshots: HashMap<String, Snapshot>,
    order: Vec<String>,
    total_bytes: usize,
}

/// 有界快照注册表。所有方法都在阻塞上下文中调用；内部只用 `std::sync::Mutex`，不跨 await 持有。
pub(crate) struct SnapshotStore {
    registry: Mutex<Registry>,
    idle_ttl: Duration,
    max_entries: usize,
    max_bytes: usize,
}

impl SnapshotStore {
    pub(crate) fn new(idle_ttl: Duration) -> Self {
        Self {
            registry: Mutex::new(Registry::default()),
            idle_ttl,
            max_entries: MAX_SNAPSHOTS,
            max_bytes: MAX_SNAPSHOT_BYTES,
        }
    }

    /// 打开并注册一个快照。`root` 是会话根的绝对文本，`path` 是根内相对路径。
    pub(crate) fn open(
        &self,
        root: &str,
        path: &SessionFilePath,
        purpose: FileReadPurpose,
    ) -> Result<ContentSnapshot, PortError> {
        let secure = SecureRoot::open(root)?;
        let resolved = secure.resolve(Some(path))?;
        if !resolved.metadata().is_file() {
            return Err(unavailable());
        }
        let size_bytes = resolved.metadata().len();

        if purpose == FileReadPurpose::Download && size_bytes > SESSION_FILE_DOWNLOAD_MAX_BYTES {
            return Err(PortError::InvalidRequest(RESULT_TOO_LARGE));
        }

        let (mut file, metadata) = resolved.open_file()?;
        if metadata.len() != size_bytes {
            return Err(unavailable());
        }
        let (kind, stored) = match purpose {
            FileReadPurpose::Download => {
                let stored = read_bounded(&mut file, size_bytes)?;
                if stored.len() as u64 != size_bytes {
                    return Err(unavailable());
                }
                (classify_bytes(&stored), stored)
            }
            FileReadPurpose::Preview => classify_preview(&mut file, size_bytes)?,
        };

        // 读取期间文件被改写时，身份会变化；此时不注册任何快照。
        let after = file.metadata().map_err(|_| unavailable())?;
        if FileIdentity::of(&after) != resolved.identity() {
            return Err(unavailable());
        }

        let preview_truncated =
            purpose == FileReadPurpose::Preview && (stored.len() as u64) < size_bytes;
        let version = self.unique_version();
        let snapshot = ContentSnapshot::try_new(
            path.clone(),
            kind,
            purpose,
            size_bytes,
            version.clone(),
            preview_truncated,
        )
        .map_err(|_| PortError::InvalidRequest(RESULT_TOO_LARGE))?;

        let record = SnapshotRecord {
            root: root.to_owned(),
            path: path.clone(),
            kind,
            size_bytes,
            stored,
            preview_truncated,
            identity: resolved.identity(),
            version,
        };
        self.insert(record)?;
        Ok(snapshot)
    }

    fn unique_version(&self) -> ContentVersion {
        let raw = uuid::Uuid::new_v4().simple().to_string();
        // uuid 的 simple 形式是 32 位十六进制，必然通过 ContentVersion 的形状校验。
        match ContentVersion::parse(&raw) {
            Ok(version) => version,
            Err(_) => ContentVersion::parse("v0").expect("常量版本必然合法"),
        }
    }

    fn insert(&self, record: SnapshotRecord) -> Result<(), PortError> {
        let now = Instant::now();
        let bytes = record.stored.len();
        let mut registry = self.lock();
        self.expire(&mut registry, now);
        while registry.order.len() >= self.max_entries
            || registry.total_bytes + bytes > self.max_bytes
        {
            let Some(oldest) = registry.order.first().cloned() else {
                return Err(unavailable());
            };
            remove(&mut registry, &oldest);
        }
        registry.total_bytes += bytes;
        registry.order.push(record.version.as_str().to_owned());
        let key = record.version.as_str().to_owned();
        registry.snapshots.insert(
            key,
            Snapshot {
                root: record.root,
                path: record.path,
                kind: record.kind,
                size_bytes: record.size_bytes,
                stored: record.stored,
                preview_truncated: record.preview_truncated,
                identity: record.identity,
                version: record.version,
                last_used: now,
            },
        );
        Ok(())
    }

    /// 读取一个字节块。文件身份变化或快照过期/释放时返回明确的不可用。
    pub(crate) fn read(
        &self,
        version: &ContentVersion,
        offset: u64,
    ) -> Result<ChunkData, PortError> {
        let now = Instant::now();
        let (root, path, identity, size_bytes) = {
            let mut registry = self.lock();
            self.expire(&mut registry, now);
            let snapshot = registry
                .snapshots
                .get_mut(version.as_str())
                .ok_or_else(unavailable)?;
            snapshot.last_used = now;
            (
                snapshot.root.clone(),
                snapshot.path.clone(),
                snapshot.identity,
                snapshot.size_bytes,
            )
        };

        // 每块发出前重新验证源文件身份；变化即废弃快照，不续传旧字节。
        let revalidated = SecureRoot::open(&root).and_then(|secure| {
            let resolved = secure.resolve(Some(&path))?;
            if resolved.identity() != identity {
                return Err(unavailable());
            }
            resolved.open_file().map(|_| ())
        });
        if revalidated.is_err() {
            let mut registry = self.lock();
            registry.snapshots.remove(version.as_str());
            registry.order.retain(|key| key != version.as_str());
            return Err(unavailable());
        }

        let mut registry = self.lock();
        let snapshot = registry
            .snapshots
            .get_mut(version.as_str())
            .ok_or_else(unavailable)?;
        if snapshot.identity != identity || snapshot.size_bytes != size_bytes {
            remove(&mut registry, version.as_str());
            return Err(unavailable());
        }
        if offset > snapshot.stored.len() as u64 {
            return Err(unavailable());
        }
        let start = offset as usize;
        let end = start
            .saturating_add(SESSION_FILE_CHUNK_MAX_BYTES)
            .min(snapshot.stored.len());
        snapshot.last_used = now;
        let data = snapshot.stored[start..end].to_vec();
        let eof = end >= snapshot.stored.len();
        Ok(ChunkData {
            path: snapshot.path.clone(),
            kind: snapshot.kind,
            size_bytes: snapshot.size_bytes,
            version: snapshot.version.clone(),
            offset,
            data,
            eof,
            preview_truncated: snapshot.preview_truncated,
        })
    }

    /// 幂等释放。未知版本视为已释放。
    pub(crate) fn release(&self, version: &ContentVersion) {
        let mut registry = self.lock();
        remove(&mut registry, version.as_str());
    }

    /// 释放全部快照（Daemon 关闭或连接断开时由接线层调用）。
    pub(crate) fn release_all(&self) {
        let mut registry = self.lock();
        registry.snapshots.clear();
        registry.order.clear();
        registry.total_bytes = 0;
    }

    fn expire(&self, registry: &mut Registry, now: Instant) {
        let expired: Vec<String> = registry
            .snapshots
            .values()
            .filter(|snapshot| now.duration_since(snapshot.last_used) >= self.idle_ttl)
            .map(|snapshot| snapshot.version.as_str().to_owned())
            .collect();
        for key in expired {
            remove(registry, &key);
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Registry> {
        match self.registry.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    #[cfg(test)]
    pub(crate) fn count(&self) -> usize {
        self.lock().snapshots.len()
    }

    #[cfg(test)]
    pub(crate) fn total_bytes(&self) -> usize {
        self.lock().total_bytes
    }
}

fn remove(registry: &mut Registry, key: &str) {
    if let Some(snapshot) = registry.snapshots.remove(key) {
        registry.total_bytes = registry.total_bytes.saturating_sub(snapshot.stored.len());
    }
    registry.order.retain(|entry| entry != key);
}

/// 下载用途：对已经完整读入的字节做类型判定。
fn classify_bytes(bytes: &[u8]) -> SessionFileContentKind {
    if image_magic(bytes) {
        SessionFileContentKind::Image
    } else if is_text_prefix(bytes, true) {
        SessionFileContentKind::Text
    } else {
        SessionFileContentKind::Binary
    }
}

/// 预览用途：返回类型与已经取得的内存内容（文本 256 KiB / 图片 4 MiB；二进制不带正文）。
fn classify_preview(
    file: &mut File,
    size_bytes: u64,
) -> Result<(SessionFileContentKind, Vec<u8>), PortError> {
    let header = read_bounded(file, TYPE_HEADER_BYTES.min(size_bytes as usize) as u64)?;
    if image_magic(&header) {
        let budget = (size_bytes).min(SESSION_FILE_IMAGE_PREVIEW_MAX_BYTES);
        let mut stored = header;
        let remaining = budget.saturating_sub(stored.len() as u64);
        if remaining > 0 {
            stored.extend(read_bounded(file, remaining)?);
        }
        return Ok((SessionFileContentKind::Image, stored));
    }
    // 文本判定需要看到预览预算内的完整 UTF-8；多读 4 字节以判断预算边界上的码点是否被截断。
    let read_limit = SESSION_FILE_TEXT_PREVIEW_MAX_BYTES.saturating_add(4);
    let mut data = header;
    let remaining = (size_bytes as usize).saturating_sub(data.len());
    let more = remaining.min(read_limit.saturating_sub(data.len() as u64) as usize);
    if more > 0 {
        data.extend(read_bounded(file, more as u64)?);
    }
    if is_text_prefix(&data, data.len() as u64 == size_bytes) {
        // 截断到预算内的合法 UTF-8 边界：最多回退 3 字节。
        let kept = trim_to_char_boundary(&data, SESSION_FILE_TEXT_PREVIEW_MAX_BYTES as usize);
        data.truncate(kept);
        Ok((SessionFileContentKind::Text, data))
    } else {
        // 二进制预览不携带正文（客户端会走下载用途）。
        Ok((SessionFileContentKind::Binary, Vec::new()))
    }
}

fn read_bounded(file: &mut File, limit: u64) -> Result<Vec<u8>, PortError> {
    let mut buffer = Vec::with_capacity((limit as usize).min(SESSION_FILE_CHUNK_MAX_BYTES));
    let mut scratch = [0_u8; 8 * 1024];
    let mut remaining = limit as usize;
    while remaining > 0 {
        let want = remaining.min(scratch.len());
        match file.read(&mut scratch[..want]) {
            Ok(0) => break,
            Ok(read) => {
                buffer.extend_from_slice(&scratch[..read]);
                remaining -= read;
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(unavailable()),
        }
    }
    Ok(buffer)
}

/// 只把 PNG/JPEG/GIF/WebP 识别为图片；按实际 magic bytes，不看扩展名。
pub(crate) fn image_magic(header: &[u8]) -> bool {
    header.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A])
        || header.starts_with(&[0xFF, 0xD8, 0xFF])
        || header.starts_with(b"GIF8")
        || (header.len() >= 12 && header.starts_with(b"RIFF") && &header[8..12] == b"WEBP")
}

/// 判定一段前缀是否是 UTF-8 文本：不含 NUL；完整输入要求整体可解码，截断输入允许结尾的不完整
/// 码点（预览/头部本来就可能切在码点中间）。
pub(crate) fn is_text_prefix(bytes: &[u8], complete: bool) -> bool {
    if bytes.contains(&0) {
        return false;
    }
    match std::str::from_utf8(bytes) {
        Ok(_) => true,
        Err(error) if error.error_len().is_none() => !complete,
        Err(_) => false,
    }
}

/// 截取不超过 `limit` 的最大 UTF-8 字符边界前缀长度。
fn trim_to_char_boundary(bytes: &[u8], limit: usize) -> usize {
    let mut end = limit.min(bytes.len());
    while end > 0 && std::str::from_utf8(&bytes[..end]).is_err() {
        end -= 1;
    }
    end
}

/// 供状态视图判定一个根内文件是否为二进制（读取有界头部，不读取整个文件）。
pub(crate) fn looks_binary(secure: &SecureRoot, path: &SessionFilePath) -> bool {
    let Ok(resolved) = secure.resolve(Some(path)) else {
        return false;
    };
    if !resolved.metadata().is_file() {
        return false;
    }
    let limit = TYPE_HEADER_BYTES.min(resolved.metadata().len() as usize);
    let Ok(header) = resolved.read_head(limit) else {
        return false;
    };
    if image_magic(&header) {
        return true;
    }
    !is_text_prefix(&header, header.len() as u64 == resolved.metadata().len())
}

/// 根文本版本（Git 状态视图使用；解析失败时保守地按非二进制处理）。
pub(crate) fn looks_binary_root(root: &str, path: &SessionFilePath) -> bool {
    SecureRoot::open(root)
        .map(|secure| looks_binary(&secure, path))
        .unwrap_or(false)
}
