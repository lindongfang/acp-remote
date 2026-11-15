import { describe, expect, it, vi } from "vitest";
import { ViewDecodeError } from "../../src/views/errors";
import { RawJsonDocument } from "../../src/views/raw-json";

const utf8 = (value: string): Uint8Array => new TextEncoder().encode(value);

describe("flat original JSON tokens", () => {
  it("keeps numbers, duplicate keys, key order and unknown structure as exact lexemes", () => {
    const source = '{"z":9007199254740993,"a":-0,"z":1.2300e+003,"u":[1e999999,true,false,null]}';
    const document = RawJsonDocument.parse(utf8(source));
    expect(document.tokenLexeme(0)).toBe(source);
    expect(document.tokens.filter((token) => token.kind === "number")
      .map((token) => source.slice(token.startByte, token.endByte)))
      .toEqual(["9007199254740993", "-0", "1.2300e+003", "1e999999"]);
    expect(document.tokens.flatMap((token, index) =>
      token.role === "key" ? [document.stringValue(index)] : []))
      .toEqual(["z", "a", "z", "u"]);
    expect(document.tokens[1]).toMatchObject({ role: "key", parent: 0 });
    expect(document.tokens[2]).toMatchObject({ role: "value", parent: 0 });
    expect(document.tokens.at(-1)).toMatchObject({ kind: "null", parent: 8 });
    expect(() => document.stringValue(2)).toThrowError(new ViewDecodeError("invalid_token"));
  });

  it("never parses a whole raw object; explicitly decoded literals alone use JSON.parse", () => {
    const spy = vi.spyOn(JSON, "parse");
    try {
      const document = RawJsonDocument.parse(utf8('{"k":"a\\n\\uD83D\\uDE42","n":1e99999}'));
      expect(spy).not.toHaveBeenCalled();
      expect(document.tokenLexeme(2)).toBe('"a\\n\\uD83D\\uDE42"');
      expect(document.stringValue(2)).toBe("a\n🙂");
      expect(spy).toHaveBeenCalledExactlyOnceWith('"a\\n\\uD83D\\uDE42"');
    } finally {
      spy.mockRestore();
    }
  });

  it("keeps byte addresses across Unicode and escaped keys", () => {
    const source = '{"é":"🙂","\\u006b":0,"k":1}';
    const document = RawJsonDocument.parse(utf8(source));
    expect(document.tokens[2]).toMatchObject({ startByte: 6, endByte: 12 });
    expect(document.stringValue(2)).toBe("🙂");
    expect(document.tokenLexeme(3)).toBe('"\\u006b"');
    expect(document.stringValue(3)).toBe("k");
    expect(document.stringValue(5)).toBe("k");
    expect(document.tokenLexeme(0)).toBe(source);
  });

  it("handles 20000-level arrays and objects without recursive parsing", () => {
    const levels = 20_000;
    const array = "[".repeat(levels) + "0" + "]".repeat(levels);
    const arrayDocument = RawJsonDocument.parse(utf8(array));
    expect(arrayDocument.tokens).toHaveLength(levels + 1);
    expect(arrayDocument.tokens[0]?.endByte).toBe(array.length);
    expect(arrayDocument.tokens.at(-1)).toMatchObject({
      kind: "number", parent: levels - 1,
    });
    const object = '{"k":'.repeat(levels) + "null" + "}".repeat(levels);
    const objectDocument = RawJsonDocument.parse(utf8(object));
    expect(objectDocument.tokens).toHaveLength(levels * 2 + 1);
    expect(objectDocument.tokens[0]?.endByte).toBe(object.length);
    expect(objectDocument.tokens.at(-1)?.kind).toBe("null");
  });

  it("handles many leaves without a leaf quota", () => {
    const document = RawJsonDocument.parse(utf8("[" + Array(50_000).fill("1").join(",") + "]"));
    expect(document.tokens).toHaveLength(50_001);
    expect(document.tokenLexeme(50_000)).toBe("1");
  });

  it("keeps input, byte copies and frozen token metadata independent", () => {
    const input = utf8('{"s":"original"}');
    const document = RawJsonDocument.parse(input);
    input.fill(0);
    document.tokenBytes(2).fill(0);
    expect(document.stringValue(2)).toBe("original");
    expect(Object.isFrozen(document)).toBe(true);
    expect(Object.isFrozen(document.tokens)).toBe(true);
    expect(document.tokens.every(Object.isFrozen)).toBe(true);
    for (const index of [-1, 0.5, NaN, Infinity, Number.MAX_SAFE_INTEGER + 1, 99]) {
      expect(() => document.tokenBytes(index)).toThrowError(
        new ViewDecodeError("invalid_token"),
      );
    }
  });

  it.each([
    "", " ", "[", "{", "[1,]", '{"a":1,}', '{"a" 1}', '{"a":}',
    "{1:2}", '{"a":1 "b":2}', "[1 2]", "true false", "nullx", "false0",
    "+1", ".1", "-.1", "01", "-01", "1.", "1e", "1e+", "1e-", "--1",
    "NaN", "Infinity", "0x12", '"unterminated', '"\\q"', '"\\u12"',
    '"\\uZZZZ"', '"raw\nline"', '"raw\tline"', "{}[]", "[}", "{]",
    "\ufeff{}", "\u00a0{}",
  ])("rejects malformed JSON without echoing it: %j", (source) => {
    expect(() => RawJsonDocument.parse(utf8(source)))
      .toThrowError(new ViewDecodeError("invalid_json"));
  });

  it.each([
    "0", "-0", "1.0e-003", "true", "false", "null", '""',
    " \r\n\t [ {}, [], \"a\", 0, true, null ] \t\n",
    '"\\\"\\\\\\/\\b\\f\\n\\r\\t\\u0000"',
    '"\\uD800"',
  ])("accepts complete JSON grammar while retaining the original root: %j", (source) => {
    const document = RawJsonDocument.parse(utf8(source));
    expect(document.tokenLexeme(0)).toBe(source.trim());
  });

  it.each([
    [0x22, 0x80, 0x22],
    [0x22, 0xc0, 0xaf, 0x22],
    [0x22, 0xed, 0xa0, 0x80, 0x22],
    [0x22, 0xf4, 0x90, 0x80, 0x80, 0x22],
    [0x22, 0xf0, 0x9f, 0x92],
  ])("rejects malformed UTF8 independently from JSON syntax", (...values) => {
    expect(() => RawJsonDocument.parse(new Uint8Array(values)))
      .toThrowError(new ViewDecodeError("invalid_utf8"));
  });

  it("validates Unicode spanning its local UTF8 validation batch boundary", () => {
    const value = "x".repeat(16_382) + "🙂" + "é";
    const document = RawJsonDocument.parse(utf8('"' + value + '"'));
    expect(document.stringValue(0)).toBe(value);
  });

  it("treats prototype keys, markup, URI and action strings only as data", () => {
    const source = '{"__proto__":{"polluted":true},"constructor":"<script>approve()</script>","uri":"http://127.0.0.1/private","action":"approval.decide"}';
    const document = RawJsonDocument.parse(utf8(source));
    expect(document.tokenLexeme(0)).toBe(source);
    expect(document.stringValue(1)).toBe("__proto__");
    expect(Object.prototype).not.toHaveProperty("polluted");
  });
});
