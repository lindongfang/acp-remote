import fixture from "../fixtures/jws-v1.json";
import {
  JwsError,
  type JwsType,
  decodeBase64urlStrict,
  encodeBase64url,
  parseCompactJws,
  signCompactJws,
  verifyCompactJws,
} from "../../src/protocol/jws";

const acceptedTypes: readonly JwsType[] = ["acpr-auth-client+jwt"];
const status = document.querySelector<HTMLDivElement>("#status");
const output = document.querySelector<HTMLPreElement>("#result");

if (status === null || output === null) {
  throw new Error("browser contract elements are missing");
}

function compactWith(signatureSegment: string): string {
  return `${fixture.headerSegment}.${fixture.payloadSegment}.${signatureSegment}`;
}

async function expectError(code: JwsError["code"], action: () => unknown): Promise<void> {
  try {
    await action();
    throw new Error(`expected ${code}`);
  } catch (error) {
    if (!(error instanceof JwsError) || error.code !== code) {
      throw error;
    }
  }
}

async function run(): Promise<Record<string, boolean>> {
  const checks: Record<string, boolean> = {};
  const publicKey = decodeBase64urlStrict(fixture.clientPublicKey);
  const signingKey = await crypto.subtle.importKey(
    "jwk",
    {
      kty: "EC",
      crv: "P-256",
      d: fixture.privateJwk.d,
      x: fixture.privateJwk.x,
      y: fixture.privateJwk.y,
      ext: true,
      key_ops: ["sign"],
    },
    { name: "ECDSA", namedCurve: "P-256" },
    false,
    ["sign"],
  );

  for (const name of ["rust", "webcrypto", "highS"] as const) {
    const parsed = await verifyCompactJws(
      compactWith(fixture[name].signatureSegment),
      publicKey,
      acceptedTypes,
    );
    checks[`fixed_${name}`] = parsed.typ === "acpr-auth-client+jwt";
  }

  const signed = await signCompactJws(
    "acpr-auth-client+jwt",
    new TextEncoder().encode(fixture.payload),
    signingKey,
  );
  checks.webcrypto_sign_verify = signed.split(".").length === 3;
  await verifyCompactJws(signed, publicKey, acceptedTypes);

  const tamperedPayload = fixture.payload.replace("1700000015", "1700000016");
  await expectError("invalid_signature", () =>
    verifyCompactJws(
      `${fixture.headerSegment}.${encodeBase64url(
        new TextEncoder().encode(tamperedPayload),
      )}.${fixture.rust.signatureSegment}`,
      publicKey,
      acceptedTypes,
    ),
  );
  checks.tamper_rejected = true;

  await expectError("invalid_json", () =>
    parseCompactJws(
      `${fixture.headerSegment}.${encodeBase64url(
        new TextEncoder().encode('{"v":9007199254740992}'),
      )}.${fixture.rust.signatureSegment}`,
      acceptedTypes,
    ),
  );
  checks.unsafe_integer_rejected = true;

  await expectError("invalid_public_key", () =>
    verifyCompactJws(
      compactWith(fixture.rust.signatureSegment),
      new Uint8Array(65),
      acceptedTypes,
    ),
  );
  checks.invalid_sec1_rejected = true;

  return checks;
}

try {
  const checks = await run();
  const passed = Object.values(checks).every(Boolean);
  document.documentElement.dataset.result = passed ? "pass" : "fail";
  status.textContent = passed ? "PASS" : "FAIL";
  output.textContent = JSON.stringify(
    {
      userAgent: navigator.userAgent,
      secureContext: window.isSecureContext,
      checks,
    },
    null,
    2,
  );
} catch (error) {
  document.documentElement.dataset.result = "fail";
  status.textContent = "FAIL";
  output.textContent = error instanceof Error ? error.stack ?? error.message : String(error);
}
