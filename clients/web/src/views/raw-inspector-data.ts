import type { RawJsonToken } from "./raw-json";

export const MEMBER_WINDOW = 100;
export const LITERAL_WINDOW = 4096;

export interface RawMember {
  readonly key: number | null;
  readonly value: number;
}
export interface RawIndex {
  readonly children: readonly (readonly RawMember[] | undefined)[];
}

function invalidIndex(): never {
  throw new Error("原始结构索引无效");
}

export function buildRawIndex(tokens: readonly RawJsonToken[]): RawIndex {
  const root = tokens[0];
  if (root === undefined || root.parent !== null || root.role !== "value") {
    invalidIndex();
  }
  const children: (RawMember[] | undefined)[] = new Array(tokens.length);
  const pendingKeys = new Map<number, number>();
  for (let index = 0; index < tokens.length; ++index) {
    const token = tokens[index];
    if (token === undefined) invalidIndex();
    if (token.kind === "object" || token.kind === "array") children[index] = [];
    if (token.parent === null) {
      if (index !== 0) invalidIndex();
      continue;
    }
    const parent = tokens[token.parent];
    const members = children[token.parent];
    if (token.parent >= index || parent === undefined || members === undefined) {
      invalidIndex();
    }
    if (token.role === "key") {
      if (parent.kind !== "object" || pendingKeys.has(token.parent)) invalidIndex();
      pendingKeys.set(token.parent, index);
    } else {
      const key = parent.kind === "object" ? pendingKeys.get(token.parent) : null;
      if (key === undefined) invalidIndex();
      members.push(Object.freeze({ key, value: index }));
      pendingKeys.delete(token.parent);
    }
  }
  if (pendingKeys.size !== 0) invalidIndex();
  for (const members of children) if (members !== undefined) Object.freeze(members);
  return Object.freeze({ children: Object.freeze(children) });
}

export interface LiteralPaging {
  readonly text: string;
  readonly starts: readonly number[];
  readonly codePoints: number;
}

export function indexLiteral(text: string): LiteralPaging {
  const starts = [0];
  let codePoints = 0;
  for (let offset = 0; offset < text.length; ++codePoints) {
    if (codePoints !== 0 && codePoints % LITERAL_WINDOW === 0) starts.push(offset);
    const point = text.codePointAt(offset);
    if (point === undefined) invalidIndex();
    offset += point > 0xffff ? 2 : 1;
  }
  return Object.freeze({ text, starts: Object.freeze(starts), codePoints });
}

export function literalWindow(paging: LiteralPaging, page: number): string {
  if (!Number.isSafeInteger(page) || page < 0) invalidIndex();
  const start = paging.starts[page];
  if (start === undefined) invalidIndex();
  return paging.text.slice(start, paging.starts[page + 1] ?? paging.text.length);
}

const INVISIBLE = /[\p{Cc}\p{Cf}\p{Default_Ignorable_Code_Point}\u2028\u2029]/u;

export function visibleRawText(text: string): string {
  const pieces: string[] = [];
  for (const character of text) {
    if (INVISIBLE.test(character)) {
      const point = character.codePointAt(0);
      if (point === undefined) invalidIndex();
      pieces.push(`⟦U+${point.toString(16).toUpperCase().padStart(4, "0")}⟧`);
    } else pieces.push(character);
  }
  return pieces.join("");
}

export function shortLiteral(text: string, maximum: number): string {
  let offset = 0;
  for (let count = 0; count < maximum && offset < text.length; ++count) {
    const point = text.codePointAt(offset);
    if (point === undefined) invalidIndex();
    offset += point > 0xffff ? 2 : 1;
  }
  return visibleRawText(text.slice(0, offset)) + (offset < text.length ? "…" : "");
}
