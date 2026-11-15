import { describe, expect, it } from "vitest";
import { RawJsonDocument } from "../../src/views/raw-json";
import { buildRawIndex, indexLiteral, literalWindow, visibleRawText, shortLiteral } from "../../src/views/raw-inspector-data";
const parse = (text: string) => RawJsonDocument.parse(new TextEncoder().encode(text));

describe("independent raw inspector data auxiliaries; no DOM or parent L5 claim", () => {
  it("WD03 original ordered duplicate keys and prototype names retain different value tokens", () => {
    const raw = parse('{"x":1,"__proto__":2,"x":3}'), index = buildRawIndex(raw.tokens);
    expect(index.children[0]?.map(m => [raw.tokenLexeme(m.key!), raw.tokenLexeme(m.value)])).toEqual([['"x"', "1"], ['"__proto__"', "2"], ['"x"', "3"]]);
    expect(Object.isFrozen(index)).toBe(true); expect(Object.isFrozen(index.children)).toBe(true); expect(Object.isFrozen(index.children[0])).toBe(true);
  });
  it("WD04 array indices include identical values and nested containers", () => {
    const raw = parse('[1,1,{"n":-0},[],null]'), indexed = buildRawIndex(raw.tokens);
    expect(indexed.children[0]?.map(m => [m.key, raw.tokenLexeme(m.value)])).toEqual([[null, "1"], [null, "1"], [null, '{"n":-0}'], [null, "[]"], [null, "null"]]);
  });
  it("WD05 huge numbers and exponent spelling are lexemes", () => {
    const raw = parse('[-0,123456789012345678901234567890123456789,1e+99999,1.2300e-0003]');
    expect(buildRawIndex(raw.tokens).children[0]?.map(m => raw.tokenLexeme(m.value))).toEqual(["-0", "123456789012345678901234567890123456789", "1e+99999", "1.2300e-0003"]);
  });
  it("WD06 original string escape spelling never round trips through decoded values", () => {
    const raw = parse('{"\\u0061":"\\/\\u0062"}'), member = buildRawIndex(raw.tokens).children[0]![0]!;
    expect(raw.tokenLexeme(member.key!)).toBe('"\\u0061"'); expect(raw.tokenLexeme(member.value)).toBe('"\\/\\u0062"');
  });
  for (const size of [4095, 4096, 4097, 8193]) it(`WD10 ${size} raw codepoints are completely accessible`, () => {
    const text = "q".repeat(size), indexed = indexLiteral(text), pages = indexed.starts.map((_, i) => literalWindow(indexed, i));
    expect(pages.join("")).toBe(text); expect(pages.every(p => [...p].length <= 4096)).toBe(true); expect(indexed.codePoints).toBe(size); expect(Object.isFrozen(indexed.starts)).toBe(true);
  });
  it("WD11 supplementary scalar at a codepoint boundary is not split", () => {
    const text = "q".repeat(4095) + "😀" + "tail", indexed = indexLiteral(text);
    expect(literalWindow(indexed, 0)).toBe("q".repeat(4095) + "😀"); expect(literalWindow(indexed, 1)).toBe("tail");
    expect(indexed.starts).toEqual([0, 4097]);
  });
  for (const page of [-1, 0.5, NaN, Infinity, 99]) it(`WD10 invalid window ${String(page)} is rejected`, () => { expect(() => literalWindow(indexLiteral("a"), page)).toThrow(); });
  it("WD15 actual invisible Unicode markers are visible without changing the source", () => {
    const original = "A" + String.fromCodePoint(0x9b, 0xad, 0x202e, 0x2067, 0x200d, 0xfe0f, 0xe0100) + "B";
    expect(visibleRawText(original)).toBe("A⟦U+009B⟧⟦U+00AD⟧⟦U+202E⟧⟦U+2067⟧⟦U+200D⟧⟦U+FE0F⟧⟦U+E0100⟧B"); expect([...original].length).toBe(9);
  });
  it("WD16 a literal escape sequence stays visible text; lower-level actual controls are labeled auxiliary only", () => {
    expect(visibleRawText('"\\u001B[31m"')).toBe('"\\u001B[31m"');
    expect(visibleRawText("\u001b\n\0")).toBe("⟦U+001B⟧⟦U+000A⟧⟦U+0000⟧");
    expect(() => parse('"\u001b"')).toThrow();
  });
  it("WD09 an actual twenty thousand deep scanner tree indexes iteratively", () => {
    const raw = parse("[".repeat(20000) + "0" + "]".repeat(20000)), index = buildRawIndex(raw.tokens);
    let at = 0; for (let depth = 0; depth < 20000; ++depth) { const members = index.children[at]!; expect(members.length).toBe(1); at = members[0]!.value; }
    expect(raw.tokenLexeme(at)).toBe("0"); expect(raw.tokens.length).toBe(20001);
  });
  it("WD12 long key preview is a window and original full lexeme remains", () => {
    const text = '"' + "x".repeat(40000) + 'KEY_END"', raw = parse("{" + text + ":1}"), key = buildRawIndex(raw.tokens).children[0]![0]!.key!;
    const original = raw.tokenLexeme(key); expect(shortLiteral(original, 160)).toBe('"' + "x".repeat(159) + "…");
    const indexed = indexLiteral(original); expect(literalWindow(indexed, indexed.starts.length - 1).endsWith('KEY_END"')).toBe(true); expect(raw.tokenLexeme(key)).toBe(text);
  });
});
