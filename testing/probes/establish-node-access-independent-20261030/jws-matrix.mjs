const task = await taskSpace('independent-jws-20261030');
const page = task.page("p1");
const result = await page.evaluate(async () => {
  const jws = await import("/src/protocol/jws.ts");
  const fixture = await (await fetch("/tests/fixtures/jws-v1.json")).json();
  const accepted = ["acpr-auth-client+jwt"];
  const encode = text => jws.encodeBase64url(new TextEncoder().encode(text));
  const compactPayload = bytes => `${fixture.headerSegment}.${jws.encodeBase64url(bytes)}.${fixture.rust.signatureSegment}`;
  const compactHeader = header => `${encode(JSON.stringify(header))}.${fixture.payloadSegment}.${fixture.rust.signatureSegment}`;
  const checks = [];
  const reject = async (name, action, expected) => {
    let error;
    try { await action(); } catch (caught) { error = caught; }
    if (!(error instanceof jws.JwsError) || (expected && error.code !== expected)) throw new Error(`${name}: expected ${expected ?? "JwsError"}, got ${error?.code ?? "acceptance"}`);
    checks.push({name, code:error.code, passed:true});
  };
  for (const [name, text, code] of [
    ["duplicate_top", '{"v":1,"v":1}', "duplicate_field"],
    ["duplicate_nested", '{"v":1,"x":{"a":1,"a":2}}', "duplicate_field"],
    ["non_object", '[]', "non_object_payload"],
    ["unpaired_surrogate", '{"v":1,"s":"\\uD800"}', "invalid_json"],
    ["fraction_encoding", '{"v":1.0}', "invalid_json"],
    ["exponent_encoding", '{"v":1e3}', "invalid_json"],
    ["unsafe_integer", '{"v":9007199254740992}', "invalid_json"],
  ]) await reject(name, () => jws.parseCompactJws(compactPayload(new TextEncoder().encode(text)), accepted), code);
  await reject("malformed_utf8", () => jws.parseCompactJws(compactPayload(Uint8Array.of(0xc3, 0x28)), accepted), "invalid_json");
  for (const [name, header] of [
    ["unknown_header", {alg:"ES256",typ:accepted[0],extra:1}],
    ["untrusted_jku", {alg:"ES256",typ:accepted[0],jku:"https://example.invalid"}],
    ["critical_header", {alg:"ES256",typ:accepted[0],crit:["exp"]}],
    ["no_algorithm", {alg:"none",typ:accepted[0]}],
    ["cross_purpose", {alg:"ES256",typ:"acpr-pair-claim+jwt"}],
  ]) await reject(name, () => jws.parseCompactJws(compactHeader(header), accepted));
  for (const value of ["AA=", "AA+", "AA/", "AB", "AA\n"]) await reject(`base64_${JSON.stringify(value)}`, () => jws.decodeBase64urlStrict(value), "invalid_base64");
  for (const [name, key] of [
    ["compressed_key", new Uint8Array(33)],
    ["truncated_key", new Uint8Array(64)],
    ["wrong_key_prefix", new Uint8Array(65).fill(3)],
    ["off_curve", Uint8Array.from([4,...new Uint8Array(64)])],
  ]) await reject(name, () => jws.verifyCompactJws(`${fixture.headerSegment}.${fixture.payloadSegment}.${fixture.rust.signatureSegment}`, key, accepted), "invalid_public_key");
  for (const [name, signature] of [
    ["der_encoding",new Uint8Array(70)],
    ["zero_scalars",new Uint8Array(64)],
    ["r_out_of_range",Uint8Array.from([...new Uint8Array(32).fill(255),...new Uint8Array(31),1])],
  ]) await reject(name, () => jws.parseCompactJws(`${fixture.headerSegment}.${fixture.payloadSegment}.${jws.encodeBase64url(signature)}`, accepted), "invalid_signature_encoding");
  const key = await crypto.subtle.importKey("jwk", {kty:"EC",crv:"P-256",d:fixture.privateJwk.d,x:fixture.privateJwk.x,y:fixture.privateJwk.y,ext:true,key_ops:["sign"]}, {name:"ECDSA",namedCurve:"P-256"}, false, ["sign"]);
  const signed = await jws.signCompactJws(accepted[0], new TextEncoder().encode(fixture.payload), key);
  await jws.verifyCompactJws(signed, jws.decodeBase64urlStrict(fixture.clientPublicKey), accepted);
  return {secureContext:isSecureContext, userAgent:navigator.userAgent, existing:JSON.parse(document.querySelector("#result").textContent), checks, signed};
});
const fs = await import("node:fs/promises");
await fs.writeFile("D:/Project/acp-remote/.tmp/independent-jws-20261030/browser-proof.txt", result.signed);
delete result.signed;
await fs.writeFile("D:/Project/acp-remote/.tmp/independent-jws-20261030/matrix.json", JSON.stringify(result,null,2));
console.log({negativeChecks:result.checks.length, passed:result.checks.every(item=>item.passed), secureContext:result.secureContext, fixedChecks:result.existing.checks});

