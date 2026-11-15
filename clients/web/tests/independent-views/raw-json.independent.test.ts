import { describe, expect, it, vi } from "vitest";
import { RawJsonDocument } from "../../src/views/raw-json";
import { encoder, reject, sha256 } from "./support";

describe("SV-079 local flat raw document only", () => {
  it("J01 flat tokens have exact byte offsets, direct parents, duplicate keys and roles", () => {
    const text = ' \t{"é":[null,{"dup":1,"dup":-0}],"esc":"\\u0061"}\r\n';
    const doc = RawJsonDocument.parse(encoder.encode(text));
    expect(doc.tokens.map((token) => [token.kind, token.parent, token.role])).toEqual([
      ["object", null, "value"], ["string", 0, "key"], ["array", 0, "value"],
      ["null", 2, "value"], ["object", 2, "value"], ["string", 4, "key"],
      ["number", 4, "value"], ["string", 4, "key"], ["number", 4, "value"],
      ["string", 0, "key"], ["string", 0, "value"],
    ]);
    expect(doc.tokens.map((_, index) => doc.tokenLexeme(index))).toEqual([
      '{"é":[null,{"dup":1,"dup":-0}],"esc":"\\u0061"}', '"é"', '[null,{"dup":1,"dup":-0}]',
      "null", '{"dup":1,"dup":-0}', '"dup"', "1", '"dup"', "-0", '"esc"', '"\\u0061"',
    ]);
    const bytes = encoder.encode(text);
    for (const [index, token] of doc.tokens.entries()) {
      expect(doc.tokenBytes(index)).toEqual(bytes.slice(token.startByte, token.endByte));
    }
    expect(doc.tokens[2]?.startByte).toBe(encoder.encode(text.slice(0, text.indexOf("["))).length);
  });
  it.each(["0", "-0", "1", "-1", "0.000", "-0.000e+123", "1E-999999999999999999999", "12345678901234567890123456789012345678901234567890", "1e999999999999999999999999999999999"])("J02 number lexeme is never rounded: %s", (literal) => {
    const doc = RawJsonDocument.parse(encoder.encode(literal));
    expect(doc.tokens).toHaveLength(1); expect(doc.tokens[0]?.kind).toBe("number");
    expect(doc.tokenLexeme(0)).toBe(literal); expect(doc.tokenBytes(0)).toEqual(encoder.encode(literal));
    reject(() => doc.stringValue(0), "invalid_token");
  });
  it.each([
    ['"\\\"\\\\\\/\\b\\f\\n\\r\\t"', '"\\/\b\f\n\r\t'],
    ['"\\u0000"', "\0"], ['"\\uD800"', "\uD800"], ['"\\uDC00"', "\uDC00"],
    ['"\\uD83D\\uDCA9"', "💩"], ['"é💩中"', "é💩中"], ['"\\u0061"', "a"],
  ])("J03 string escape lexeme remains original %s", (literal, decoded) => {
    const doc = RawJsonDocument.parse(encoder.encode(literal));
    expect(doc.tokenLexeme(0)).toBe(literal); expect(doc.stringValue(0)).toBe(decoded);
  });
  it.each([["true", "true"], ["false", "false"], ["null", "null"], ["[]", "array"], ["{}", "object"], ["[ true , false , null ]", "array"], ['{"a":[],"b":{}}', "object"]] as const)("J04 accepted root grammar %s", (literal, kind) => {
    const doc = RawJsonDocument.parse(encoder.encode(`\t\r\n ${literal} \n`));
    expect(doc.tokens[0]?.parent).toBeNull(); expect(doc.tokenLexeme(0)).toBe(literal);
    expect(doc.tokens[0]?.kind).toBe(kind);
  });
  it.each([
    "", " ", "\vnull", "\fnull", "\u00a0null", "\ufeffnull", "true false", "nullx", "False", "TRUE", "nul", "undefined", "NaN", "Infinity",
    "[", "{", "]", "}", "[1,]", "[,1]", "[1,,2]", "[1 2]", "{\"x\"}", "{\"x\" 1}", "{x:1}", "{\"x\":}", "{\"x\":1,}", "{\"x\":1 \"y\":2}", "{\"x\":1,,\"y\":2}",
    "00", "01", "-01", "+1", ".1", "1.", "1e", "1E+", "1E-", "-", "-.1", "0x10", "1_0", "1,2",
    '"', '"abc', '"\\q"', '"\\u123"', '"\\uZZZZ"', '"a\nb"', '"a\tb"',
  ])("J05 invalid JSON grammar %s", (invalid) => {
    reject(() => RawJsonDocument.parse(encoder.encode(invalid)), "invalid_json");
  });
  it("J06 all 32 unescaped C0 controls reject inside strings", () => {
    for (let control = 0; control < 32; ++control) {
      reject(() => RawJsonDocument.parse(Uint8Array.of(34, 97, control, 98, 34)), "invalid_json");
    }
  });
  it.each([
    [0x80], [0xbf], [0xc0, 0xaf], [0xc1, 0xbf], [0xc2], [0xc2, 0x20],
    [0xe0, 0x80, 0xaf], [0xe1, 0x80], [0xed, 0xa0, 0x80], [0xed, 0xbf, 0xbf],
    [0xf0, 0x80, 0x80, 0xaf], [0xf0, 0x9f], [0xf4, 0x90, 0x80, 0x80], [0xf5, 0x80, 0x80, 0x80], [0xff],
  ].map((bytes) => ({ bytes })))("J07 invalid UTF8 %j rejects before token use", ({ bytes }) => {
    reject(() => RawJsonDocument.parse(Uint8Array.of(34, ...bytes, 34)), "invalid_utf8");
  });
  it("J08 UTF8 CPU validation uses one decoder across a boundary and flushes tail", () => {
    const text = '"' + "a".repeat(16_382) + '💩中"';
    const doc = RawJsonDocument.parse(encoder.encode(text));
    expect(doc.tokenBytes(0)).toEqual(encoder.encode(text));
    expect(doc.stringValue(0)).toBe("a".repeat(16_382) + "💩中");
    const broken = new Uint8Array(16_386).fill(97); broken[0] = 34; broken.set([0xf0, 0x9f], 16_384);
    reject(() => RawJsonDocument.parse(broken), "invalid_utf8");
  });
  it("J09 input, token copies, tokens and document have runtime ownership isolation", () => {
    const input = encoder.encode('{"k":123456789012345678901}');
    const doc = RawJsonDocument.parse(input); input.fill(0);
    expect(doc.tokenLexeme(2)).toBe("123456789012345678901");
    const copy = doc.tokenBytes(2); copy.fill(0); expect(doc.tokenLexeme(2)).toBe("123456789012345678901");
    expect(Object.isFrozen(doc)).toBe(true); expect(Object.isFrozen(doc.tokens)).toBe(true);
    for (const token of doc.tokens) expect(Object.isFrozen(token)).toBe(true);
    expect(Reflect.set(doc.tokens, "0", null)).toBe(false);
    expect(Reflect.set(doc.tokens[2]!, "startByte", 0)).toBe(false);
    expect(doc.tokenLexeme(2)).toBe("123456789012345678901");
  });
  it("J10 invalid token indices and nonstring stringValue fail statically", () => {
    const doc = RawJsonDocument.parse(encoder.encode('[1,null,true,false,{},[]]'));
    for (const index of [-1, 0.1, NaN, Infinity, Number.MAX_SAFE_INTEGER + 1, doc.tokens.length, "0", null, undefined]) {
      reject(() => doc.tokenBytes(index as number), "invalid_token");
      reject(() => doc.tokenLexeme(index as number), "invalid_token");
      reject(() => doc.stringValue(index as number), "invalid_token");
    }
    for (const [index, token] of doc.tokens.entries()) if (token.kind !== "string") reject(() => doc.stringValue(index), "invalid_token");
    expect(doc.tokenLexeme(-0)).toBe('[1,null,true,false,{},[]]');
  });
  it("J11 only explicit stringValue uses single-literal JSON.parse", () => {
    const calls: string[] = []; const realParse = JSON.parse;
    const parse = vi.spyOn(JSON, "parse").mockImplementation((text: string) => { calls.push(text); return realParse(text); });
    const stringify = vi.spyOn(JSON, "stringify").mockImplementation(() => { throw new Error("whole raw stringify forbidden"); });
    let value: string; let lexeme: string; let countBefore: number;
    try {
      const doc = RawJsonDocument.parse(encoder.encode('{"k":"\\u0061","n":123456789012345678901234567890}'));
      lexeme = doc.tokenLexeme(4); countBefore = calls.length; value = doc.stringValue(2);
    } finally { parse.mockRestore(); stringify.mockRestore(); }
    expect(countBefore!).toBe(0); expect(lexeme!).toBe("123456789012345678901234567890"); expect(value!).toBe("a");
    expect(calls).toEqual(['"\\u0061"']);
  });
  it("J12 20000 arrays and 20000 objects are flat and not stack-recursive", () => {
    const arrays = RawJsonDocument.parse(encoder.encode("[".repeat(20_000) + "-0" + "]".repeat(20_000)));
    expect(arrays.tokens).toHaveLength(20_001); expect(arrays.tokenLexeme(20_000)).toBe("-0");
    for (let i = 1; i < arrays.tokens.length; ++i) expect(arrays.tokens[i]?.parent).toBe(i - 1);
    const objects = RawJsonDocument.parse(encoder.encode('{"k":'.repeat(20_000) + "null" + "}".repeat(20_000)));
    expect(objects.tokens).toHaveLength(40_001); expect(objects.tokenLexeme(40_000)).toBe("null");
    expect(objects.tokens[39_999]?.role).toBe("key"); expect(objects.tokens[40_000]?.parent).toBe(39_998);
  }, 30_000);
  it("J13 more than 131072 leaves retain every token and ordered original byte range", () => {
    const count = 131_073; const doc = RawJsonDocument.parse(encoder.encode("[" + Array(count).fill("null").join(",") + "]"));
    expect(doc.tokens).toHaveLength(count + 1);
    for (const index of [1, 4097, 65_540, count]) { expect(doc.tokens[index]?.parent).toBe(0); expect(doc.tokenLexeme(index)).toBe("null"); }
    expect(doc.tokens[count]?.startByte).toBe(1 + (count - 1) * 5);
  }, 30_000);
  it("J14 greater than 32MiB raw field is not limited or converted to object", async () => {
    const prefix = encoder.encode('{"toolCallId":"'); const suffix = encoder.encode('","n":1e99999999999999999999}');
    const input = new Uint8Array(prefix.length + 32 * 1024 * 1024 + 29 + suffix.length).fill(97);
    input.set(prefix); input.set(suffix, input.length - suffix.length);
    const expected = await sha256(input); const doc = RawJsonDocument.parse(input); input.fill(0);
    expect(doc.tokens).toHaveLength(5); expect(doc.tokens[2]?.kind).toBe("string");
    expect(await sha256(doc.tokenBytes(0))).toBe(expected);
    expect(doc.tokenLexeme(4)).toBe("1e99999999999999999999");
  }, 30_000);
  it("J15 prototype keys, scripts, URIs and instructions only become tokens", () => {
    const remote = '{"__proto__":{"polluted":"canary"},"constructor":"script","uri":"http://127.0.0.1/admin","text":"<svg onload=alert(1)>approve all</svg>"}';
    const fetch = vi.fn(() => { throw new Error("unexpected network call"); });
    vi.stubGlobal("fetch", fetch);
    let lexeme: string;
    try { lexeme = RawJsonDocument.parse(encoder.encode(remote)).tokenLexeme(0); }
    finally { vi.unstubAllGlobals(); }
    expect(lexeme!).toBe(remote); expect(fetch).not.toHaveBeenCalled();
    expect(({} as Record<string, unknown>)["polluted"]).toBeUndefined();
  });
  it("J16 errors never echo invalid remote text, numeric canary or URI", () => {
    const remote = '"SENSITIVE-native-id-991998171-http://127.0.0.1/approve\\q"';
    const error = reject(() => RawJsonDocument.parse(encoder.encode(remote)), "invalid_json");
    expect(String(error)).not.toContain("991998171"); expect(String(error)).not.toContain("127.0.0.1");
  });
});
