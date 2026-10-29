use acp_remote::jws::{JwsType, sign};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use p256::ecdsa::{Signature, SigningKey};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    if let Some(path) = std::env::args().nth(1) {
        acp_remote::claims::verify_auth_client(std::fs::read_to_string(path)?.trim())?;
        println!("PASS fresh Chromium proof verified by Rust claims");
        return Ok(());
    }
    let fixture: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(
        "clients/web/tests/fixtures/jws-v1.json",
    )?)?;
    let raw = URL_SAFE_NO_PAD.decode(
        fixture["rust"]["signatureSegment"]
            .as_str()
            .ok_or("signature")?,
    )?;
    let signature = Signature::from_slice(&raw)?;
    let client = SigningKey::from_slice(&[8; 32])?;
    let node = SigningKey::from_slice(&[7; 32])?;
    assert_eq!(
        URL_SAFE_NO_PAD.encode(client.verifying_key().to_sec1_point(false).as_bytes()),
        fixture["clientPublicKey"]
    );
    let payload: serde_json::Value =
        serde_json::from_str(fixture["payload"].as_str().ok_or("payload")?)?;
    assert_eq!(
        URL_SAFE_NO_PAD.encode(node.verifying_key().to_sec1_point(false).as_bytes()),
        payload["nodePublicKey"]
    );
    let mut unknown = payload.clone();
    unknown
        .as_object_mut()
        .ok_or("object")?
        .insert("unexpected".into(), serde_json::json!(true));
    let unknown_client = sign(JwsType::AuthClient, &serde_json::to_vec(&unknown)?, &client)?;
    assert!(acp_remote::claims::verify_auth_client(&unknown_client).is_err());
    let compact = |sig: &[u8]| {
        format!(
            "{}.{}.{}",
            fixture["headerSegment"].as_str().unwrap(),
            fixture["payloadSegment"].as_str().unwrap(),
            URL_SAFE_NO_PAD.encode(sig)
        )
    };
    for scalar in [
        vec![0; 32],
        URL_SAFE_NO_PAD.decode("_____wAAAAD__________7zm-q2nF56E87nKwvxjJVE")?,
        URL_SAFE_NO_PAD.decode("_____wAAAAD__________7zm-q2nF56E87nKwvxjJVI")?,
    ] {
        for offset in [0, 32] {
            let mut value = raw.clone();
            value[offset..offset + 32].copy_from_slice(&scalar);
            assert!(
                acp_remote::jws::ParsedJws::parse(&compact(&value), &[JwsType::AuthClient])
                    .is_err()
            );
        }
    }
    assert!(
        acp_remote::jws::ParsedJws::parse(
            &compact(signature.to_der().as_bytes()),
            &[JwsType::AuthClient]
        )
        .is_err()
    );
    for value in [
        signature.normalize_s().to_bytes().to_vec(),
        URL_SAFE_NO_PAD.decode(fixture["highS"]["signatureSegment"].as_str().unwrap())?,
    ] {
        acp_remote::claims::verify_auth_client(&compact(&value))?;
    }
    for point in [
        client
            .verifying_key()
            .to_sec1_point(true)
            .as_bytes()
            .to_vec(),
        vec![4; 64],
        vec![3; 65],
        {
            let mut b = vec![0; 65];
            b[0] = 4;
            b
        },
    ] {
        let mut p = payload.clone();
        p["clientPublicKey"] = URL_SAFE_NO_PAD.encode(point).into();
        assert!(
            acp_remote::claims::verify_auth_client(&sign(
                JwsType::AuthClient,
                &serde_json::to_vec(&p)?,
                &client
            )?)
            .is_err()
        );
    }
    for field in ["iat", "exp"] {
        let mut p = payload.clone();
        p[field] = serde_json::json!(9007199254740992_u64);
        assert!(
            acp_remote::claims::verify_auth_client(&sign(
                JwsType::AuthClient,
                &serde_json::to_vec(&p)?,
                &client
            )?)
            .is_err()
        );
    }
    use p256::ecdsa::signature::Signer;
    let raw_header = "{ \"typ\" : \"acpr-auth-client+jwt\", \"alg\" : \"ES256\" }";
    let raw_payload = serde_json::to_string_pretty(&payload)?;
    let input = format!(
        "{}.{}",
        URL_SAFE_NO_PAD.encode(raw_header),
        URL_SAFE_NO_PAD.encode(raw_payload)
    );
    let raw_sig: Signature = client.sign(input.as_bytes());
    let original_bytes = format!("{input}.{}", URL_SAFE_NO_PAD.encode(raw_sig.to_bytes()));
    acp_remote::claims::verify_auth_client(&original_bytes)?;
    let output = serde_json::json!({
     "fixture":true,
     "originalBytesProof":original_bytes,
     "validDerSignature":URL_SAFE_NO_PAD.encode(signature.to_der().as_bytes()),
     "validCompressedPoint":URL_SAFE_NO_PAD.encode(client.verifying_key().to_sec1_point(true).as_bytes()),
     "normalizedLowS":URL_SAFE_NO_PAD.encode(signature.normalize_s().to_bytes()),
     "validHostProof":sign(JwsType::AuthHost,&serde_json::to_vec(&payload)?,&node)?,
     "unknownHostProof":sign(JwsType::AuthHost,&serde_json::to_vec(&unknown)?,&node)?,
     "unknownClientPayloadRejectedByRust":true
    });
    std::fs::write(
        ".tmp/independent-jws-20261030/extra-vectors.json",
        serde_json::to_vec_pretty(&output)?,
    )?;
    println!(
        "PASS: library-generated DER, valid compressed point, normalized low-S and signed payload-closure controls generated from public fixture"
    );
    Ok(())
}
