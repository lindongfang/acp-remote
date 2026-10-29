use std::{
    fs,
    net::SocketAddr,
    path::{Path, PathBuf},
};

use serde::Deserialize;
use url::{Host, Url};

use crate::error::AppError;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigFile {
    data_dir: PathBuf,
    listen: String,
    canonical_origin: String,
    tls_cert: PathBuf,
    tls_key: PathBuf,
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
        let file: ConfigFile = toml::from_str(&source)
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
        Ok(Self {
            data_dir: file.data_dir,
            listen,
            canonical_origin,
            tls_cert: file.tls_cert,
            tls_key: file.tls_key,
        })
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
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

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
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "acp-remote-config-{}-{nonce}.toml",
            std::process::id()
        ));
        let valid = r#"
            data_dir = "data"
            listen = "127.0.0.1:8443"
            canonical_origin = "https://NODE.example.test:443/"
            tls_cert = "server.pem"
            tls_key = "server-key.pem"
        "#;
        std::fs::write(&path, valid).expect("valid config");
        let config = NodeConfig::load(&path).expect("load valid config");
        assert_eq!(config.canonical_origin, "https://node.example.test");
        assert_eq!(config.listen, "127.0.0.1:8443".parse().expect("listen"));

        std::fs::write(
            &path,
            valid.replace("https://NODE.example.test:443/", "http://node.example.test"),
        )
        .expect("invalid config");
        assert!(matches!(NodeConfig::load(&path), Err(AppError::Config(_))));
        std::fs::remove_file(path).expect("cleanup");
    }
}
