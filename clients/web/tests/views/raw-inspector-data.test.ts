import { describe, expect, it } from "vitest";
import { RawJsonDocument } from "../../src/views/raw-json";
import {
  buildRawIndex, indexLiteral, literalWindow, shortLiteral, visibleRawText,
} from "../../src/views/raw-inspector-data";

function parse(text: string): RawJsonDocument {
  return RawJsonDocument.parse(new TextEncoder().encode(text));
}

describe("raw inspector pure data helpers (not browser/DOM acceptance)", () => {
  it("indexes actual object tokens without collapsing duplicate or special keys", () => {
    const raw = parse('{"b":1,"a":2,"b":3,"__proto__":4,"constructor":5,"toString":6}');
    const members = buildRawIndex(raw.tokens).children[0];
    expect(members?.map(member => member.key === null ? null : raw.tokenLexeme(member.key)))
      .toEqual(['"b"', '"a"', '"b"', '"__proto__"', '"constructor"', '"toString"']);
    expect(members?.map(member => raw.tokenLexeme(member.value)))
      .toEqual(["1", "2", "3", "4", "5", "6"]);
    expect(Object.getPrototypeOf({})).toBe(Object.prototype);
  });

  it("separates nested members and retains array order and numeric lexemes", () => {
    const raw = parse('[{"x":[1,1]},[],null,-0,1e+99999,1.2300e-0003,123456789012345678901234567890]');
    const index = buildRawIndex(raw.tokens);
    const root = index.children[0];
    expect(root).toHaveLength(7);
    expect(root?.slice(2).map(member => raw.tokenLexeme(member.value)))
      .toEqual(["null", "-0", "1e+99999", "1.2300e-0003", "123456789012345678901234567890"]);
    const object = root?.[0];
    expect(object).toBeDefined();
    const nested = index.children[object!.value]?.[0];
    expect(nested).toBeDefined();
    expect(index.children[nested!.value]?.map(member => raw.tokenLexeme(member.value)))
      .toEqual(["1", "1"]);
  });

  it("indexes a real 20000-level document iteratively to its terminal scalar", () => {
    const raw = parse("[".repeat(20_000) + '"deep-end"' + "]".repeat(20_000));
    const index = buildRawIndex(raw.tokens);
    let token = 0;
    for (let depth = 0; depth < 20_000; ++depth) {
      const members = index.children[token];
      expect(members).toHaveLength(1);
      token = members![0]!.value;
    }
    expect(raw.tokenLexeme(token)).toBe('"deep-end"');
    expect(index.children[token]).toBeUndefined();
  });

  it("does not cap a direct-member index at its display window", () => {
    const raw = parse("[" + Array.from({ length: 201 }, (_, i) => i.toString()).join(",") + "]");
    const members = buildRawIndex(raw.tokens).children[0];
    expect(members).toHaveLength(201);
    expect(raw.tokenLexeme(members![200]!.value)).toBe("200");
    expect(Object.isFrozen(members)).toBe(true);
  });

  it.each([4095, 4096, 4097, 8193])("pages all %i original lexeme codepoints", length => {
    const text = "x".repeat(length);
    const paging = indexLiteral(text);
    expect(paging.codePoints).toBe(length);
    expect(paging.starts).toHaveLength(Math.ceil(length / 4096));
    expect(paging.starts.map((_, i) => literalWindow(paging, i)).join("")).toBe(text);
  });

  it("keeps supplementary scalars whole across a text window", () => {
    const text = "a".repeat(4095) + "😀" + "b";
    const paging = indexLiteral(text);
    expect(paging.codePoints).toBe(4097);
    expect(literalWindow(paging, 0)).toBe("a".repeat(4095) + "😀");
    expect(literalWindow(paging, 1)).toBe("b");
    expect(paging.starts[1]).toBe(4097);
  });

  it("retains JSON escapes and quotes instead of decoding their meaning", () => {
    const raw = parse(String.raw`"\u001B[31m\n\u0000\/\uD83D\uDE00"`);
    const lexeme = raw.tokenLexeme(0);
    expect(visibleRawText(lexeme)).toBe(String.raw`"\u001B[31m\n\u0000\/\uD83D\uDE00"`);
    expect(literalWindow(indexLiteral(lexeme), 0)).toBe(lexeme);
  });

  it("makes control and default-ignorable codepoints visible without changing raw", () => {
    const points = [0x7f, 0x9b, 0xad, 0x61c, 0x200b, 0x200c, 0x200d,
      0x202a, 0x202e, 0x2066, 0x2069, 0xfe0e, 0xfe0f, 0xe0100];
    const text = points.map(point => String.fromCodePoint(point)).join("");
    const source = '"' + text + '"';
    const raw = parse(source);
    expect(visibleRawText(raw.tokenLexeme(0))).toBe('"' + points.map(point =>
      `⟦U+${point.toString(16).toUpperCase().padStart(4, "0")}⟧`).join("") + '"');
    expect(raw.tokenLexeme(0)).toBe(source);
  });

  it("keeps ordinary text, quotes and HTML-looking data as text", () => {
    const text = '"<svg onload=alert(1)> &amp; https://example.invalid/"';
    expect(visibleRawText(text)).toBe(text);
    expect(shortLiteral("a😀b", 2)).toBe("a😀…");
  });

  it("can directly address the last window beyond an old one-MiB boundary", () => {
    const raw = parse('"' + "x".repeat(1024 * 1024 + 17) + 'end"');
    const paging = indexLiteral(raw.tokenLexeme(0));
    expect(paging.codePoints).toBe(1024 * 1024 + 22);
    expect(literalWindow(paging, paging.starts.length - 1).endsWith('end"')).toBe(true);
    expect(() => literalWindow(paging, paging.starts.length)).toThrow("原始结构索引无效");
  });
});
