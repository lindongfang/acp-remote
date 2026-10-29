use std::{fs, path::Path, sync::Arc};

use rustls::{
    ServerConfig,
    pki_types::{
        CertificateDer, PrivateKeyDer, PrivatePkcs1KeyDer, PrivatePkcs8KeyDer, PrivateSec1KeyDer,
    },
};
use zeroize::Zeroize;

use crate::error::AppError;

/// Loads a PEM certificate chain and private key into a direct-TLS server configuration.
///
/// # Errors
///
/// Returns an error when either file is missing, malformed, does not contain exactly the expected
/// PEM entries, or cannot be used by the selected TLS provider.
pub fn load_server_config(
    certificate_path: &Path,
    private_key_path: &Path,
) -> Result<Arc<ServerConfig>, AppError> {
    let certificate_pem = read_file(certificate_path, "reading the TLS certificate")?;
    let certificates = parse_certificates(&certificate_pem)?;
    let mut private_key_pem = read_file(private_key_path, "reading the TLS private key")?;
    let private_key = parse_private_key(&private_key_pem);
    private_key_pem.zeroize();
    let private_key = private_key?;

    let provider = rustls::crypto::ring::default_provider();
    let mut config = ServerConfig::builder_with_provider(Arc::new(provider))
        .with_safe_default_protocol_versions()
        .map_err(|error| AppError::Tls(format!("unsupported TLS protocol configuration: {error}")))?
        .with_no_client_auth()
        .with_single_cert(certificates, private_key)
        .map_err(|error| AppError::Tls(format!("certificate and key are unusable: {error}")))?;
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(Arc::new(config))
}

fn read_file(path: &Path, context: &'static str) -> Result<Vec<u8>, AppError> {
    fs::read(path).map_err(|source| AppError::Io { context, source })
}

fn parse_certificates(bytes: &[u8]) -> Result<Vec<CertificateDer<'static>>, AppError> {
    let blocks = pem::parse_many(bytes)
        .map_err(|error| AppError::Tls(format!("certificate PEM is invalid: {error}")))?;
    let mut certificates = Vec::with_capacity(blocks.len());
    for block in blocks {
        if block.tag() != "CERTIFICATE" {
            return Err(AppError::Tls(
                "certificate file contains a non-certificate PEM entry".to_owned(),
            ));
        }
        certificates.push(CertificateDer::from(block.into_contents()));
    }
    if certificates.is_empty() {
        return Err(AppError::Tls(
            "certificate file contains no certificates".to_owned(),
        ));
    }
    Ok(certificates)
}

fn parse_private_key(bytes: &[u8]) -> Result<PrivateKeyDer<'static>, AppError> {
    let mut blocks = pem::parse_many(bytes)
        .map_err(|error| AppError::Tls(format!("private key PEM is invalid: {error}")))?
        .into_iter();
    let Some(block) = blocks.next() else {
        return Err(AppError::Tls(
            "private key file contains no private key".to_owned(),
        ));
    };
    if blocks.next().is_some() {
        return Err(AppError::Tls(
            "private key file contains more than one PEM entry".to_owned(),
        ));
    }
    let tag = block.tag().to_owned();
    let contents = block.into_contents();
    match tag.as_str() {
        "PRIVATE KEY" => Ok(PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(contents))),
        "RSA PRIVATE KEY" => Ok(PrivateKeyDer::Pkcs1(PrivatePkcs1KeyDer::from(contents))),
        "EC PRIVATE KEY" => Ok(PrivateKeyDer::Sec1(PrivateSec1KeyDer::from(contents))),
        "ENCRYPTED PRIVATE KEY" => Err(AppError::Tls(
            "encrypted TLS private keys are not supported; provide an unencrypted key".to_owned(),
        )),
        _ => Err(AppError::Tls(
            "private key file does not contain a supported private key".to_owned(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;

    struct TestFiles {
        directory: PathBuf,
    }

    impl TestFiles {
        fn new() -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos();
            let directory =
                std::env::temp_dir().join(format!("acp-remote-tls-{}-{nonce}", std::process::id()));
            std::fs::create_dir(&directory).expect("test directory");
            Self { directory }
        }
    }

    impl Drop for TestFiles {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.directory);
        }
    }

    fn self_signed(directory: &Path) -> (PathBuf, PathBuf) {
        let certified = rcgen::generate_simple_self_signed(vec!["localhost".to_owned()])
            .expect("test certificate");
        let certificate = directory.join("server.pem");
        let private_key = directory.join("server-key.pem");
        std::fs::write(&certificate, certified.cert.pem()).expect("certificate file");
        std::fs::write(&private_key, certified.signing_key.serialize_pem())
            .expect("private key file");
        (certificate, private_key)
    }

    #[test]
    fn loads_a_valid_certificate_chain_and_key() {
        let files = TestFiles::new();
        let (certificate, private_key) = self_signed(&files.directory);
        let config = load_server_config(&certificate, &private_key).expect("TLS configuration");
        assert_eq!(config.alpn_protocols, vec![b"http/1.1".to_vec()]);
    }

    #[test]
    fn rejects_missing_malformed_and_encrypted_keys() {
        let files = TestFiles::new();
        let (certificate, private_key) = self_signed(&files.directory);
        std::fs::write(&private_key, b"encrypted").expect("malformed key");
        assert!(matches!(
            load_server_config(&certificate, &private_key),
            Err(AppError::Tls(_))
        ));

        std::fs::write(
            &private_key,
            "-----BEGIN ENCRYPTED PRIVATE KEY-----\nAA==\n-----END ENCRYPTED PRIVATE KEY-----\n",
        )
        .expect("encrypted key");
        assert!(matches!(
            load_server_config(&certificate, &private_key),
            Err(AppError::Tls(_))
        ));

        assert!(matches!(
            load_server_config(&files.directory.join("missing.pem"), &private_key),
            Err(AppError::Io {
                context: "reading the TLS certificate",
                ..
            })
        ));
    }

    #[test]
    fn rejects_a_private_key_that_does_not_match_the_certificate() {
        let files = TestFiles::new();
        let (certificate, _) = self_signed(&files.directory);
        let other = rcgen::generate_simple_self_signed(vec!["other.example.test".to_owned()])
            .expect("second test certificate");
        let unrelated_key = files.directory.join("unrelated-key.pem");
        std::fs::write(&unrelated_key, other.signing_key.serialize_pem()).expect("unrelated key");
        assert!(matches!(
            load_server_config(&certificate, &unrelated_key),
            Err(AppError::Tls(_))
        ));
    }
}
