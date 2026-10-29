import { beforeAll, describe, expect, it } from "vitest";

import fixture from "./fixtures/jws-v1.json";
import {
  JwsError,
  type JwsType,
  decodeBase64urlStrict,
  encodeBase64url,
  parseCompactJws,
  signCompactJws,
  verifyCompactJws,
} from "../src/protocol/jws";

const ACCEPTED: readonly JwsType[] = ["acpr-auth-client+jwt"];

function compactWithSignature(signatureSegment: string): string {
  return `${fixture.headerSegment}.${fixture.payloadSegment}.${signatureSegment}`;
}

function errorFrom(action: () => unknown): JwsError {
  try {
    action();
  } catch (error) {
    if (error instanceof JwsError) {
      return error;
    }
    throw error;
  }
  throw new Error("expected JwsError");
}

async function asyncErrorFrom(action: () => Promise<unknown>): Promise<JwsError> {
  try {
    await action();
  } catch (error) {
    if (error instanceof JwsError) {
      return error;
    }
    throw error;
  }
  throw new Error("expected JwsError");
}

describe("strict ES256 JWS", () => {
  let signingKey: CryptoKey;

  beforeAll(async () => {
    const jwk: JsonWebKey = {
      kty: "EC",
      crv: "P-256",
      d: fixture.privateJwk.d,
      x: fixture.privateJwk.x,
      y: fixture.privateJwk.y,
      ext: true,
      key_ops: ["sign"],
    };
    signingKey = await crypto.subtle.importKey(
      "jwk",
      jwk,
      { name: "ECDSA", namedCurve: "P-256" },
      false,
      ["sign"],
    );
  });

  it("verifies the fixed Rust and WebCrypto vectors over original bytes", async () => {
    expect(
      new TextDecoder().decode(decodeBase64urlStrict(fixture.headerSegment)),
    ).toBe(fixture.header);
    expect(
      new TextDecoder().decode(decodeBase64urlStrict(fixture.payloadSegment)),
    ).toBe(fixture.payload);

    const publicKey = decodeBase64urlStrict(fixture.clientPublicKey);
    for (const vector of [fixture.rust, fixture.webcrypto]) {
      const parsed = await verifyCompactJws(
        compactWithSignature(vector.signatureSegment),
        publicKey,
        ACCEPTED,
      );
      expect(parsed.typ).toBe(fixture.type);
      expect(parsed.payload.v).toBe(1);
    }
  });

  it("accepts a legal fixed high-S signature", async () => {
    expect(fixture.highS.signatureSegment).not.toBe(
      fixture.rust.signatureSegment,
    );
    const parsed = await verifyCompactJws(
      compactWithSignature(fixture.highS.signatureSegment),
      decodeBase64urlStrict(fixture.clientPublicKey),
      ACCEPTED,
    );
    expect(parsed.payload.origin).toBe("https://node.example.test");
  });

  it("signs with WebCrypto and verifies the produced compact value", async () => {
    const compact = await signCompactJws(
      "acpr-auth-client+jwt",
      new TextEncoder().encode(fixture.payload),
      signingKey,
    );
    const parsed = await verifyCompactJws(
      compact,
      decodeBase64urlStrict(fixture.clientPublicKey),
      ACCEPTED,
    );
    expect(parsed.payload.clientId).toBeNull();
    expect(compact.split(".")).toHaveLength(3);
  });

  it("rejects tampering and cross-purpose use", async () => {
    const publicKey = decodeBase64urlStrict(fixture.clientPublicKey);
    const changedPayload = fixture.payload.replace(
      "1700000015",
      "1700000016",
    );
    const changed = `${fixture.headerSegment}.${encodeBase64url(
      new TextEncoder().encode(changedPayload),
    )}.${fixture.rust.signatureSegment}`;
    const error = await asyncErrorFrom(() =>
      verifyCompactJws(changed, publicKey, ACCEPTED),
    );
    expect(error.code).toBe("invalid_signature");

    const wrongType = errorFrom(() =>
      parseCompactJws(compactWithSignature(fixture.rust.signatureSegment), [
        "acpr-auth-host+jwt",
      ]),
    );
    expect(wrongType.code).toBe("unsupported_type");
  });

  it("rejects noncanonical and malformed envelopes", () => {
    const valid = compactWithSignature(fixture.rust.signatureSegment);
    expect(errorFrom(() => parseCompactJws(`${valid}=`, ACCEPTED)).code).toBe(
      "invalid_base64",
    );
    expect(errorFrom(() => parseCompactJws(`${valid}.extra`, ACCEPTED)).code).toBe(
      "invalid_compact",
    );

    const duplicateHeader = encodeBase64url(
      new TextEncoder().encode(
        '{"alg":"ES256","alg":"ES256","typ":"acpr-auth-client+jwt"}',
      ),
    );
    expect(
      errorFrom(() =>
        parseCompactJws(
          `${duplicateHeader}.${fixture.payloadSegment}.${fixture.rust.signatureSegment}`,
          ACCEPTED,
        ),
      ).code,
    ).toBe("duplicate_field");

    const unexpectedHeader = encodeBase64url(
      new TextEncoder().encode(
        '{"alg":"ES256","typ":"acpr-auth-client+jwt","crit":[]}',
      ),
    );
    expect(
      errorFrom(() =>
        parseCompactJws(
          `${unexpectedHeader}.${fixture.payloadSegment}.${fixture.rust.signatureSegment}`,
          ACCEPTED,
        ),
      ).code,
    ).toBe("invalid_header");
  });

  it("rejects invalid payload JSON, Unicode, and non-objects", () => {
    const header = fixture.headerSegment;
    const signature = fixture.rust.signatureSegment;
    const cases: Array<[Uint8Array, string]> = [
      [new TextEncoder().encode('{"v":1,"v":1}'), "duplicate_field"],
      [
        new TextEncoder().encode('{"v":1,"nested":{"x":1,"x":2}}'),
        "duplicate_field",
      ],
      [new TextEncoder().encode("[]"), "non_object_payload"],
      [new TextEncoder().encode('{"v":1,"x":"\\uD800"}'), "invalid_json"],
      [new TextEncoder().encode('{"v":1.0}'), "invalid_json"],
      [new TextEncoder().encode('{"v":1e3}'), "invalid_json"],
      [new TextEncoder().encode('{"v":9007199254740992}'), "invalid_json"],
      [Uint8Array.of(0xff), "invalid_json"],
    ];
    for (const [payload, code] of cases) {
      const compact = `${header}.${encodeBase64url(payload)}.${signature}`;
      expect(errorFrom(() => parseCompactJws(compact, ACCEPTED)).code).toBe(code);
    }
  });

  it("enforces the eight-level protocol JSON boundary", () => {
    const header = fixture.headerSegment;
    const signature = fixture.rust.signatureSegment;
    const accepted = new TextEncoder().encode(
      '{"a":{"b":{"c":{"d":{"e":{"f":{"g":{"h":1}}}}}}}}',
    );
    const tooDeep = new TextEncoder().encode(
      '{"a":{"b":{"c":{"d":{"e":{"f":{"g":{"h":{"i":1}}}}}}}}}',
    );
    expect(
      parseCompactJws(
        `${header}.${encodeBase64url(accepted)}.${signature}`,
        ACCEPTED,
      ).payload.v,
    ).toBeUndefined();
    expect(
      errorFrom(() =>
        parseCompactJws(
          `${header}.${encodeBase64url(tooDeep)}.${signature}`,
          ACCEPTED,
        ),
      ).code,
    ).toBe("invalid_json");
  });

  it("rejects DER, zero scalars, and an oversized compact value", () => {
    const der = `${fixture.headerSegment}.${fixture.payloadSegment}.${encodeBase64url(
      new Uint8Array(70),
    )}`;
    expect(errorFrom(() => parseCompactJws(der, ACCEPTED)).code).toBe(
      "invalid_signature_encoding",
    );

    const zeroSignature = `${fixture.headerSegment}.${fixture.payloadSegment}.${encodeBase64url(
      new Uint8Array(64),
    )}`;
    expect(errorFrom(() => parseCompactJws(zeroSignature, ACCEPTED)).code).toBe(
      "invalid_signature_encoding",
    );

    const orderHex =
      "ffffffff00000000ffffffffffffffffbce6faada7179e84f3b9cac2fc632551";
    const order = new Uint8Array(orderHex.length / 2);
    for (let index = 0; index < order.length; index += 1) {
      order[index] = Number.parseInt(orderHex.slice(index * 2, index * 2 + 2), 16);
    }
    const outOfRangeSignature = new Uint8Array(64);
    outOfRangeSignature.set(order);
    outOfRangeSignature[63] = 1;
    const outOfRange = `${fixture.headerSegment}.${fixture.payloadSegment}.${encodeBase64url(
      outOfRangeSignature,
    )}`;
    expect(errorFrom(() => parseCompactJws(outOfRange, ACCEPTED)).code).toBe(
      "invalid_signature_encoding",
    );

    expect(errorFrom(() => parseCompactJws("x".repeat(16 * 1024 + 1), ACCEPTED)).code).toBe(
      "too_large",
    );
  });

  it("rejects malformed SEC1 public keys", async () => {
    const valid = compactWithSignature(fixture.rust.signatureSegment);
    const shortKey = new Uint8Array(64);
    shortKey[0] = 0x04;
    const badPrefix = new Uint8Array(65);
    badPrefix[0] = 0x05;
    const offCurve = new Uint8Array(65);
    offCurve[0] = 0x04;
    offCurve[64] = 1;

    for (const publicKey of [shortKey, badPrefix, offCurve]) {
      const error = await asyncErrorFrom(() =>
        verifyCompactJws(valid, publicKey, ACCEPTED),
      );
      expect(error.code).toBe("invalid_public_key");
    }
  });
});
