use std::{
    fs,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

use serde::Deserialize;
use url::{Host, Url};

use crate::error::AppError;

pub const DEFAULT_TRANSCRIPT_CACHE_RETENTION_DAYS: u64 = 30;
pub const DEFAULT_TRANSCRIPT_CACHE_MAX_BYTES: u64 = 10 * 1024 * 1024 * 1024;
const MILLIS_PER_DAY: i64 = 86_400_000;
const MAX_CONFIG_OVERRIDES: usize = 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TranscriptCacheConfig {
    pub retention_days: u64,
    pub max_bytes: u64,
}

impl Default for TranscriptCacheConfig {
    fn default() -> Self {
        Self {
            retention_days: DEFAULT_TRANSCRIPT_CACHE_RETENTION_DAYS,
            max_bytes: DEFAULT_TRANSCRIPT_CACHE_MAX_BYTES,
        }
    }
}

impl TranscriptCacheConfig {
    /// Creates and validates a transcript cache configuration.
    ///
    /// # Errors
    ///
    /// Returns `AppError::Config` when `retention_days` or `max_bytes` is zero or overflows
    /// millisecond retention arithmetic.
    pub fn new(retention_days: u64, max_bytes: u64) -> Result<Self, AppError> {
        if retention_days == 0 {
            return Err(AppError::Config(
                "transcript_cache.retention_days must be at least 1".to_owned(),
            ));
        }
        let days_i64 = i64::try_from(retention_days).map_err(|_| {
            AppError::Config(
                "transcript_cache.retention_days overflows millisecond window".to_owned(),
            )
        })?;
        if days_i64.checked_mul(MILLIS_PER_DAY).is_none() {
            return Err(AppError::Config(
                "transcript_cache.retention_days overflows millisecond window".to_owned(),
            ));
        }
        if max_bytes == 0 {
            return Err(AppError::Config(
                "transcript_cache.max_bytes must be at least 1".to_owned(),
            ));
        }
        Ok(Self {
            retention_days,
            max_bytes,
        })
    }

    #[must_use]
    #[allow(
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss,
        reason = "Checked against i64::MAX before cast in const fn"
    )]
    pub const fn retention_millis(&self) -> i64 {
        if self.retention_days > i64::MAX as u64 {
            return i64::MAX;
        }
        let days = self.retention_days as i64;
        match days.checked_mul(MILLIS_PER_DAY) {
            Some(millis) => millis,
            None => i64::MAX,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct TranscriptCacheConfigFile {
    retention_days: Option<i64>,
    max_bytes: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigFile {
    data_dir: PathBuf,
    listen: String,
    canonical_origin: String,
    tls_cert: PathBuf,
    tls_key: PathBuf,
    #[serde(default)]
    transcript_cache: Option<TranscriptCacheConfigFile>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NodeConfigKey {
    data_dir: PathBuf,
    listen: SocketAddr,
    canonical_origin: String,
    tls_cert: PathBuf,
    tls_key: PathBuf,
}

fn config_overrides() -> &'static Mutex<Vec<(NodeConfigKey, TranscriptCacheConfig)>> {
    static OVERRIDES: OnceLock<Mutex<Vec<(NodeConfigKey, TranscriptCacheConfig)>>> =
        OnceLock::new();
    OVERRIDES.get_or_init(|| Mutex::new(Vec::new()))
}

fn record_transcript_cache_config(config: &NodeConfig, transcript_cache: TranscriptCacheConfig) {
    let key = config.override_key();
    let mut guard = config_overrides()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(pos) = guard.iter().position(|(existing, _)| *existing == key) {
        guard.remove(pos);
    }
    if transcript_cache != TranscriptCacheConfig::default() {
        if guard.len() >= MAX_CONFIG_OVERRIDES {
            guard.remove(0);
        }
        guard.push((key, transcript_cache));
    }
}

#[derive(Clone, Debug)]
pub struct NodeConfig {
    pub data_dir: PathBuf,
    pub listen: SocketAddr,
    pub canonical_origin: String,
    pub tls_cert: PathBuf,
    pub tls_key: PathBuf,
}

impl NodeConfig {
    /// Loads and validates a node configuration file.
    ///
    /// # Errors
    ///
    /// Returns an error when the file cannot be read, parsed, or does not satisfy the node
    /// configuration contract.
    pub fn load(path: &Path) -> Result<Self, AppError> {
        let source = fs::read_to_string(path).map_err(|source| AppError::Io {
            context: "reading node configuration",
            source,
        })?;
        Self::parse_toml(&source)
    }

    /// Parses and validates a node configuration from TOML text.
    ///
    /// # Errors
    ///
    /// Returns `AppError::Config` when TOML syntax, unknown keys, or field constraints fail.
    pub fn parse_toml(source: &str) -> Result<Self, AppError> {
        let file: ConfigFile = toml::from_str(source)
            .map_err(|error| AppError::Config(format!("cannot parse TOML: {error}")))?;
        Self::from_parts(file)
    }

    fn from_parts(file: ConfigFile) -> Result<Self, AppError> {
        let canonical_origin = normalize_origin(&file.canonical_origin)?;
        let listen = file
            .listen
            .parse::<SocketAddr>()
            .map_err(|_| AppError::Config("listen must be a socket address".to_owned()))?;
        if listen.ip().is_unspecified() {
            return Err(AppError::Config(
                "listen must name an explicit interface".to_owned(),
            ));
        }
        if file.tls_cert.as_os_str().is_empty() || file.tls_key.as_os_str().is_empty() {
            return Err(AppError::Config(
                "TLS certificate and key paths are required".to_owned(),
            ));
        }
        let transcript_cache = parse_transcript_cache_section(file.transcript_cache)?;
        let config = Self {
            data_dir: file.data_dir,
            listen,
            canonical_origin,
            tls_cert: file.tls_cert,
            tls_key: file.tls_key,
        };
        record_transcript_cache_config(&config, transcript_cache);
        Ok(config)
    }

    fn override_key(&self) -> NodeConfigKey {
        NodeConfigKey {
            data_dir: self.data_dir.clone(),
            listen: self.listen,
            canonical_origin: self.canonical_origin.clone(),
            tls_cert: self.tls_cert.clone(),
            tls_key: self.tls_key.clone(),
        }
    }

    /// Returns the configured `[transcript_cache]` policy, defaulting to 30 days and 10 GiB.
    #[must_use]
    pub fn transcript_cache(&self) -> TranscriptCacheConfig {
        let key = self.override_key();
        let guard = config_overrides()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        guard
            .iter()
            .rev()
            .find_map(|(existing, value)| (*existing == key).then_some(*value))
            .unwrap_or_default()
    }

    /// Attaches a custom validated `TranscriptCacheConfig` to this `NodeConfig`.
    #[must_use]
    pub fn with_transcript_cache(self, transcript_cache: TranscriptCacheConfig) -> Self {
        record_transcript_cache_config(&self, transcript_cache);
        self
    }

    /// Rejects non-HTTPS, non-origin URLs and returns the normalized ASCII origin.
    ///
    /// # Errors
    ///
    /// Returns an error for non-HTTPS URLs, user information, paths, queries, fragments, or a
    /// missing host.
    pub fn normalize_origin(value: &str) -> Result<String, AppError> {
        normalize_origin(value)
    }
}

fn parse_transcript_cache_section(
    section: Option<TranscriptCacheConfigFile>,
) -> Result<TranscriptCacheConfig, AppError> {
    let Some(section) = section else {
        return Ok(TranscriptCacheConfig::default());
    };
    let retention_days = match section.retention_days {
        None => DEFAULT_TRANSCRIPT_CACHE_RETENTION_DAYS,
        Some(days) if days <= 0 => {
            return Err(AppError::Config(
                "transcript_cache.retention_days must be a positive integer".to_owned(),
            ));
        }
        Some(days) => {
            let Ok(unsigned) = u64::try_from(days) else {
                return Err(AppError::Config(
                    "transcript_cache.retention_days must be a positive integer".to_owned(),
                ));
            };
            unsigned
        }
    };
    let max_bytes = match section.max_bytes {
        None => DEFAULT_TRANSCRIPT_CACHE_MAX_BYTES,
        Some(bytes) if bytes <= 0 => {
            return Err(AppError::Config(
                "transcript_cache.max_bytes must be a positive integer".to_owned(),
            ));
        }
        Some(bytes) => {
            let Ok(unsigned) = u64::try_from(bytes) else {
                return Err(AppError::Config(
                    "transcript_cache.max_bytes must be a positive integer".to_owned(),
                ));
            };
            unsigned
        }
    };
    TranscriptCacheConfig::new(retention_days, max_bytes)
}

fn normalize_origin(value: &str) -> Result<String, AppError> {
    let url = Url::parse(value)
        .map_err(|_| AppError::Config("canonical_origin must be an absolute URL".to_owned()))?;
    if url.scheme() != "https" {
        return Err(AppError::Config(
            "canonical_origin must use HTTPS".to_owned(),
        ));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(AppError::Config(
            "canonical_origin must not contain user information".to_owned(),
        ));
    }
    if url.path() != "/" || url.query().is_some() || url.fragment().is_some() {
        return Err(AppError::Config(
            "canonical_origin must not contain a path, query, or fragment".to_owned(),
        ));
    }
    let host = match url.host() {
        Some(Host::Domain(domain)) => domain.to_ascii_lowercase(),
        Some(Host::Ipv4(address)) => address.to_string(),
        Some(Host::Ipv6(address)) => format!("[{address}]"),
        None => {
            return Err(AppError::Config(
                "canonical_origin must contain a host".to_owned(),
            ));
        }
    };
    let port = match url.port() {
        None | Some(443) => String::new(),
        Some(port) => format!(":{port}"),
    };
    Ok(format!("https://{host}{port}"))
}

#[cfg(test)]
mod tests {
    use std::{
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;

    static TEST_SEQ: AtomicU64 = AtomicU64::new(1);

    fn unique_nonce() -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let seq = TEST_SEQ.fetch_add(1, Ordering::Relaxed);
        format!("{}-{nanos}-{seq}", std::process::id())
    }

    #[test]
    fn origin_normalizes_default_port_and_host_case() {
        assert_eq!(
            NodeConfig::normalize_origin("https://NODE.Example.TEST:443/").expect("valid origin"),
            "https://node.example.test"
        );
    }

    #[test]
    fn origin_rejects_non_origin_urls() {
        for value in [
            "http://node.example.test",
            "https://node.example.test/path",
            "https://user@node.example.test",
            "https://node.example.test?x=1",
        ] {
            assert!(NodeConfig::normalize_origin(value).is_err(), "{value}");
        }
    }

    #[test]
    fn origin_preserves_ipv6_brackets_and_non_default_ports() {
        assert_eq!(
            NodeConfig::normalize_origin("https://[2001:DB8::1]:443/").expect("valid IPv6 origin"),
            "https://[2001:db8::1]"
        );
        assert_eq!(
            NodeConfig::normalize_origin("https://[2001:db8::1]:8443")
                .expect("valid IPv6 origin with port"),
            "https://[2001:db8::1]:8443"
        );
    }

    #[test]
    fn loads_a_valid_file_and_rejects_an_invalid_origin() {
        let nonce = unique_nonce();
        let path = std::env::temp_dir().join(format!("acp-remote-config-{nonce}.toml"));
        let valid = format!(
            r#"
            data_dir = "data-{nonce}"
            listen = "127.0.0.1:8443"
            canonical_origin = "https://NODE.example.test:443/"
            tls_cert = "server.pem"
            tls_key = "server-key.pem"
        "#
        );
        std::fs::write(&path, &valid).expect("valid config");
        let config = NodeConfig::load(&path).expect("load valid config");
        assert_eq!(config.canonical_origin, "https://node.example.test");
        assert_eq!(config.listen, "127.0.0.1:8443".parse().expect("listen"));
        assert_eq!(
            config.transcript_cache(),
            TranscriptCacheConfig {
                retention_days: 30,
                max_bytes: 10_737_418_240,
            }
        );
        assert_eq!(config.transcript_cache().retention_millis(), 2_592_000_000);

        std::fs::write(
            &path,
            valid.replace("https://NODE.example.test:443/", "http://node.example.test"),
        )
        .expect("invalid config");
        assert!(matches!(NodeConfig::load(&path), Err(AppError::Config(_))));
        std::fs::remove_file(path).expect("cleanup");
    }

    #[test]
    fn transcript_cache_accepts_explicit_positive_values_and_resets_to_defaults() {
        let nonce = unique_nonce();
        let custom_toml = format!(
            r#"
            data_dir = "data-tc-{nonce}"
            listen = "127.0.0.1:8443"
            canonical_origin = "https://node.example.test"
            tls_cert = "server.pem"
            tls_key = "server-key.pem"

            [transcript_cache]
            retention_days = 14
            max_bytes = 52428800
        "#
        );
        let config = NodeConfig::parse_toml(&custom_toml).expect("valid custom transcript_cache");
        assert_eq!(
            config.transcript_cache(),
            TranscriptCacheConfig {
                retention_days: 14,
                max_bytes: 52_428_800,
            }
        );

        let reverted_toml = format!(
            r#"
            data_dir = "data-tc-{nonce}"
            listen = "127.0.0.1:8443"
            canonical_origin = "https://node.example.test"
            tls_cert = "server.pem"
            tls_key = "server-key.pem"
        "#
        );
        let reverted = NodeConfig::parse_toml(&reverted_toml).expect("reverted config");
        assert_eq!(
            reverted.transcript_cache(),
            TranscriptCacheConfig::default()
        );
    }

    #[test]
    fn transcript_cache_rejects_zero_negative_overflow_and_unknown_fields() {
        let nonce = unique_nonce();
        let base = format!(
            r#"
            data_dir = "data-bad-{nonce}"
            listen = "127.0.0.1:8443"
            canonical_origin = "https://node.example.test"
            tls_cert = "server.pem"
            tls_key = "server-key.pem"
        "#
        );
        for invalid_section in [
            "[transcript_cache]\nretention_days = 0\n",
            "[transcript_cache]\nretention_days = -5\n",
            "[transcript_cache]\nretention_days = 106751991168\n",
            "[transcript_cache]\nmax_bytes = 0\n",
            "[transcript_cache]\nmax_bytes = -1024\n",
            "[transcript_cache]\nunknown_key = true\n",
            "unexpected_root_key = 1\n",
        ] {
            let candidate = format!("{base}\n{invalid_section}");
            assert!(
                matches!(NodeConfig::parse_toml(&candidate), Err(AppError::Config(_))),
                "expected AppError::Config for section:\n{invalid_section}"
            );
        }
    }
}
