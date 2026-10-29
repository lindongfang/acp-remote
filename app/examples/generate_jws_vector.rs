use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use p256::ecdsa::{SigningKey, signature::Signer};
use serde_json::json;

use acp_remote::{
    claims::{Id128, NodeId, Nonce256, PublicKeySec1},
    jws::{JwsType, sign},
};

fn public_key(key: &SigningKey) -> PublicKeySec1 {
    let point = key.verifying_key().to_sec1_point(false);
    PublicKeySec1::new(point.as_bytes().try_into().expect("65-byte SEC1 point"))
        .expect("valid SEC1 point")
}

fn main() {
    let node_key = SigningKey::from_slice(&[7_u8; 32]).expect("test node scalar");
    let client_key = SigningKey::from_slice(&[8_u8; 32]).expect("test client scalar");
    let node_public_key = public_key(&node_key);
    let client_public_key = public_key(&client_key);
    let node_id = NodeId::from_public_key(&node_public_key).expect("node id");

    let payload = serde_json::to_vec(&json!({
        "v": 1,
        "nodeId": node_id,
        "nodePublicKey": node_public_key,
        "origin": "https://node.example.test",
        "connectionId": Id128::new([0; 16]),
        "challengeId": Id128::new([1; 16]),
        "nonce": Nonce256::new([2; 32]),
        "iat": 1_700_000_000_u64,
        "exp": 1_700_000_015_u64,
        "clientId": null,
        "clientPublicKey": client_public_key,
        "clientNonce": Nonce256::new([3; 32])
    }))
    .expect("serialize payload");
    let compact = sign(JwsType::AuthClient, &payload, &client_key).expect("sign vector");
    let segments: Vec<&str> = compact.split('.').collect();
    let header = br#"{"alg":"ES256","typ":"acpr-auth-client+jwt"}"#;
    let signature: p256::ecdsa::Signature =
        client_key.sign(format!("{}.{}", segments[0], segments[1]).as_bytes());
    let public_bytes = client_public_key.as_bytes();

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "nodeId": node_id,
            "nodePublicKey": node_public_key,
            "clientPublicKey": client_public_key,
            "privateScalarHex": hex::encode([8_u8; 32]),
            "privateJwk": {
                "kty": "EC",
                "crv": "P-256",
                "d": URL_SAFE_NO_PAD.encode([8_u8; 32]),
                "x": URL_SAFE_NO_PAD.encode(&public_bytes[1..33]),
                "y": URL_SAFE_NO_PAD.encode(&public_bytes[33..65])
            },
            "payload": String::from_utf8(payload).expect("utf8 payload"),
            "header": String::from_utf8(header.to_vec()).expect("utf8 header"),
            "headerSegment": segments[0],
            "payloadSegment": segments[1],
            "rustSignatureSegment": segments[2],
            "rustCompact": compact,
            "rustSignatureHex": hex::encode(signature.to_bytes())
        }))
        .expect("serialize vector")
    );
}
