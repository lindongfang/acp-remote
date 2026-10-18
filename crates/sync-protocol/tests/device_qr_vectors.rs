//! 新设备短二维码：Rust 与 TypeScript 共享的固定向量（`fixtures/sync/v1/device-qr/vectors.json`）。
//!
//! 本文件同时钉住四件事：
//!
//! 1. 65 字节布局（`0x02 || qrToken(32) || hostKeyFingerprint(32)`）与 87 个 ASCII 字符的规范编码；
//! 2. 旧设备 `#data=`、未知版本、错误长度、非规范 base64url 必须被拒，且不降级；
//! 3. 固定测试地址的 134 字节总长与 58×29 的矩阵预算；
//! 4. 主机公钥（65 字节未压缩 P-256）的 SHA-256 与二维码里的 32 字节指纹一致；形状非法与指纹
//!    不符都不能进入认领路径。
//!
//! 曲线点合法性由 `core` 的既有公钥类型负责，本测试只固定协议层可见的「65 字节 + `0x04` 前缀 +
//! 完整 SHA-256」判据，避免在协议 crate 复制 P-256 实现。

mod support;

use sha2::{Digest, Sha256};
use sync_protocol::device_qr::{
    DEVICE_QR_MATRIX_MAX_COLUMNS, DEVICE_QR_MATRIX_MAX_ROWS, DEVICE_QR_MAX_TEST_URL_LENGTH,
    DEVICE_QR_PAYLOAD_ASCII_LENGTH, DEVICE_QR_PAYLOAD_BYTES, DEVICE_QR_TEST_URL_LENGTH,
    DeviceQrError, DeviceQrPayload, decode_pairing_url,
};

const MANIFEST: &str = "fixtures/sync/v1/manifest.json";

fn vectors() -> serde_json::Value {
    let manifest = support::read_json(&support::repo_path(MANIFEST));
    let files = manifest["deviceQrVectors"]
        .as_array()
        .expect("manifest.deviceQrVectors 必须是数组");
    assert_eq!(files.len(), 1, "设备二维码向量文件应恰好一份");
    let relative = files[0].as_str().expect("向量路径必须是字符串");
    support::read_json(&support::repo_path(&format!("fixtures/sync/v1/{relative}")))
}

fn bytes(text: &str) -> Vec<u8> {
    acpr_transcript::decode_base64url(text).expect("固定向量的 base64url 必须合法")
}

fn fingerprint(public_key: &[u8]) -> [u8; 32] {
    let digest = Sha256::digest(public_key);
    let mut output = [0_u8; 32];
    output.copy_from_slice(&digest);
    output
}

#[test]
fn canonical_payload_encodes_the_documented_layout() {
    let vectors = vectors();
    let canonical = &vectors["canonical"];

    let payload_text = canonical["payload"].as_str().expect("payload");
    let payload = DeviceQrPayload::decode(payload_text).expect("规范载荷必须可解码");
    assert_eq!(payload_text.len(), DEVICE_QR_PAYLOAD_ASCII_LENGTH);
    assert_eq!(payload.to_bytes().len(), DEVICE_QR_PAYLOAD_BYTES);
    assert_eq!(
        payload.qr_token().as_slice(),
        bytes(canonical["qrToken"].as_str().expect("qrToken")).as_slice()
    );
    assert_eq!(
        payload.host_key_fingerprint().as_slice(),
        bytes(
            canonical["hostKeyFingerprint"]
                .as_str()
                .expect("hostKeyFingerprint")
        )
        .as_slice()
    );
    assert_eq!(
        payload.encode(),
        payload_text,
        "解码后必须重新编码出同一个规范字符串"
    );

    let origin = canonical["canonicalOrigin"].as_str().expect("origin");
    let pairing_url = payload.pairing_url(
        &sync_protocol::pairing::CanonicalOrigin::parse(origin).expect("canonical origin"),
    );
    assert_eq!(
        pairing_url,
        canonical["pairingUrl"].as_str().expect("pairingUrl")
    );
    assert_eq!(
        pairing_url.len(),
        DEVICE_QR_TEST_URL_LENGTH,
        "固定测试地址必须是 134 ASCII 字节"
    );
    assert!(pairing_url.len() <= DEVICE_QR_MAX_TEST_URL_LENGTH);

    let parsed = decode_pairing_url(&pairing_url).expect("规范地址必须可解析");
    assert_eq!(parsed.canonical_origin().as_str(), origin);
    assert_eq!(parsed.payload(), &payload, "URL 解码必须回到同一载荷");
}

#[test]
fn invalid_payloads_are_rejected_with_stable_codes() {
    let vectors = vectors();
    for case in vectors["invalidPayloads"]
        .as_array()
        .expect("invalidPayloads")
    {
        let name = case["name"].as_str().expect("name");
        let payload = case["payload"].as_str().expect("payload");
        let expected = case["expectedError"].as_str().expect("expectedError");
        let error =
            DeviceQrPayload::decode(payload).expect_err(&format!("{name}：非法载荷不得被接受"));
        assert_eq!(error.code(), expected, "{name}：错误码必须与共享向量一致");
    }
}

#[test]
fn invalid_urls_are_rejected_with_stable_codes() {
    let vectors = vectors();
    for case in vectors["invalidUrls"].as_array().expect("invalidUrls") {
        let name = case["name"].as_str().expect("name");
        let url = case["url"].as_str().expect("url");
        let expected = case["expectedError"].as_str().expect("expectedError");
        let error = decode_pairing_url(url).expect_err(&format!("{name}：非法地址不得被接受"));
        assert_eq!(error.code(), expected, "{name}：错误码必须与共享向量一致");
    }

    // 旧格式必须给出独立错误，不能落进通用的 URL 形状错误：消费方据此提示「已停用」。
    let legacy = vectors["invalidUrls"]
        .as_array()
        .expect("invalidUrls")
        .iter()
        .find(|case| case["name"].as_str() == Some("legacy-device-data"))
        .expect("legacy-device-data 向量");
    assert_eq!(
        decode_pairing_url(legacy["url"].as_str().expect("url")),
        Err(DeviceQrError::LegacyDataFormat)
    );
}

#[test]
fn host_public_key_shape_and_sha256_fingerprint_are_fixed() {
    let vectors = vectors();
    for case in vectors["hostPublicKeys"]
        .as_array()
        .expect("hostPublicKeys")
    {
        let name = case["name"].as_str().expect("name");
        let public_key = bytes(case["hostPublicKey"].as_str().expect("hostPublicKey"));
        let shape_valid = public_key.len() == 65 && public_key[0] == 0x04;

        match case["expectedError"].as_str() {
            Some("host_public_key_invalid") => {
                assert!(
                    !shape_valid,
                    "{name}：非法公钥形状不得被当成 65 字节未压缩点"
                );
                continue;
            }
            Some("fingerprint_mismatch") => {
                assert!(shape_valid, "{name}：本向量只测指纹不符");
                let declared = bytes(case["hostKeyFingerprint"].as_str().expect("fingerprint"));
                assert_ne!(
                    fingerprint(&public_key).as_slice(),
                    declared.as_slice(),
                    "{name}：挂错指纹必须被消费方拒绝"
                );
                continue;
            }
            Some(other) => panic!("{name}：未知 expectedError {other}"),
            None => {}
        }

        assert!(shape_valid, "{name}：合法公钥必须是 65 字节未压缩 SEC1 点");
        let declared = bytes(case["hostKeyFingerprint"].as_str().expect("fingerprint"));
        assert_eq!(
            fingerprint(&public_key).as_slice(),
            declared.as_slice(),
            "{name}：主机公钥的 SHA-256 必须与二维码指纹逐字节一致"
        );
    }
}

#[test]
fn url_and_qr_matrix_budget_matches_the_shared_vector() {
    let vectors = vectors();
    let budget = &vectors["budget"];
    assert_eq!(
        budget["testPairingUrlLength"].as_u64(),
        Some(DEVICE_QR_TEST_URL_LENGTH as u64)
    );
    assert_eq!(
        budget["maxTestPairingUrlLength"].as_u64(),
        Some(DEVICE_QR_MAX_TEST_URL_LENGTH as u64)
    );
    assert_eq!(
        budget["maxMatrixColumns"].as_u64(),
        Some(DEVICE_QR_MATRIX_MAX_COLUMNS as u64)
    );
    assert_eq!(
        budget["maxMatrixRows"].as_u64(),
        Some(DEVICE_QR_MATRIX_MAX_ROWS as u64)
    );
    const {
        assert!(
            DEVICE_QR_TEST_URL_LENGTH <= DEVICE_QR_MAX_TEST_URL_LENGTH,
            "134 字节测试地址必须落在 150 字节预算内"
        );
    }
}
