/**
 * transcript 编码的字节级验证（R11 / AC3）。
 *
 * 判据来自 `plan.md` 的 WP5b Contract Freeze：`fixtures/sync/v1/transcripts/device-proof.json@WP1`。
 * 本测试**从仓库根读取**该向量（经 `src/protocol/paths.ts` 的 `repoRoot` 解析），
 * **不复制**它——`AGENTS.md` §5「Rust 和 TypeScript 必须消费同一 manifest，不能各自复制一套测试样例」。
 *
 * 字段表同样不复制：从 `compatibility/transcripts/v1/transcripts.json` 读入并按 domain 查表，
 * 因此实现的 tag/宽度/顺序与**机器权威**逐项对应，而不是与测试里的手抄副本对应。
 */

import { readFileSync } from "node:fs";
import { join } from "node:path";
import { createHash } from "node:crypto";

import { describe, expect, it } from "vitest";

import { repoRoot } from "../protocol/paths";
import {
  DEVICE_PROOF_DOMAIN,
  TRANSCRIPT_CODEC_VERSION,
  TRANSCRIPT_MAGIC,
  TranscriptError,
  encodeBase64Url,
  encodeNulJoinedUtf8,
  encodeTranscript,
  encodeU16be,
  encodeU64be,
  encodeUtf8,
  encodeUuid16,
  type ByteArray,
  type TranscriptField,
} from "./transcript";

/** registry 里一条字段登记。 */
interface RegistryField {
  readonly tag: number;
  readonly name: string;
  readonly type: string;
  readonly inputKey: string;
}

/** registry 里一个 domain 的登记。 */
interface RegistryDomain {
  readonly domain: string;
  readonly protocol: string;
  readonly fields: readonly RegistryField[];
}

const registry = JSON.parse(
  readFileSync(join(repoRoot, "compatibility", "transcripts", "v1", "transcripts.json"), "utf8"),
) as { readonly domains: readonly RegistryDomain[] };

const deviceProof = JSON.parse(
  readFileSync(
    join(repoRoot, "fixtures", "sync", "v1", "transcripts", "device-proof.json"),
    "utf8",
  ),
) as {
  readonly domain: string;
  readonly input: Readonly<Record<string, unknown>>;
  readonly expected: {
    readonly transcriptBase64url: string;
    readonly transcriptSha256Hex: string;
    readonly publicKey: string;
  };
};

/** 按 registry 的字段类型把 fixture 的 `input` 编成原始字节。 */
function encodeRegistryField(field: RegistryField, input: Readonly<Record<string, unknown>>): ByteArray {
  const value = input[field.inputKey];
  if (value === undefined) throw new Error(`fixture 缺少字段 ${field.inputKey}`);
  switch (field.type) {
    case "u16be":
      return encodeU16be(Number(value));
    case "u64be":
      return encodeU64be(BigInt(value as string | number));
    case "uuid16":
      return encodeUuid16(String(value));
    case "utf8":
      return encodeUtf8(String(value));
    case "nul-joined-utf8":
      return encodeNulJoinedUtf8(value as readonly string[]);
    case "bytes32": {
      const text = String(value);
      // registry 约定：以 `Hex` 结尾的 inputKey 是十六进制，其余是无填充 base64url。
      if (field.inputKey.endsWith("Hex")) {
        const bytes: ByteArray = new Uint8Array(text.length / 2);
        for (let index = 0; index < bytes.length; index += 1) {
          bytes[index] = Number.parseInt(text.slice(index * 2, index * 2 + 2), 16);
        }
        return bytes;
      }
      return decodeBase64Url(text);
    }
    case "sec1-65":
      return decodeBase64Url(String(value));
    default:
      throw new Error(`未覆盖的字段类型 ${field.type}`);
  }
}

/** 无填充 base64url 解码（测试侧，仅用于把 fixture 文本还原成字节）。 */
function decodeBase64Url(text: string): ByteArray {
  const padded = text.replaceAll("-", "+").replaceAll("_", "/");
  const binary = atob(padded + "=".repeat((4 - (padded.length % 4)) % 4));
  const bytes: ByteArray = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) {
    bytes[index] = binary.charCodeAt(index);
  }
  return bytes;
}

/** 用 registry 的字段表把 `input` 编码成 transcript。 */
function encodeFromRegistry(
  domain: RegistryDomain,
  input: Readonly<Record<string, unknown>>,
): ByteArray {
  const fields: TranscriptField[] = domain.fields.map((field) => ({
    tag: field.tag,
    bytes: encodeRegistryField(field, input),
  }));
  return encodeTranscript(domain.domain, fields);
}

describe("R11/AC3：device-proof transcript 逐字节对齐固定向量", () => {
  const domain = registry.domains.find((candidate) => candidate.domain === DEVICE_PROOF_DOMAIN);

  it("registry 里存在 device-proof domain，且与 fixture 的 domain 一致", () => {
    expect(domain, "registry 缺少 device-proof domain").toBeDefined();
    expect(deviceProof.domain).toBe(DEVICE_PROOF_DOMAIN);
    expect(domain?.protocol).toBe("sync");
  });

  it("编码结果的 base64url 与 SHA-256 与向量逐字节相同", () => {
    expect(domain).toBeDefined();
    if (domain === undefined) return;

    const encoded = encodeFromRegistry(domain, deviceProof.input);

    expect(encodeBase64Url(encoded)).toBe(deviceProof.expected.transcriptBase64url);
    expect(createHash("sha256").update(encoded).digest("hex")).toBe(
      deviceProof.expected.transcriptSha256Hex,
    );
  });

  it("结构头逐字段符合 codec：magic/版本/domain 长度/字段数/tag 升序/长度前缀", () => {
    expect(domain).toBeDefined();
    if (domain === undefined) return;

    const encoded = encodeFromRegistry(domain, deviceProof.input);
    const view = new DataView(encoded.buffer, encoded.byteOffset, encoded.byteLength);
    const text = new TextDecoder();

    expect(text.decode(encoded.subarray(0, 4))).toBe(TRANSCRIPT_MAGIC);
    expect(encoded[4]).toBe(TRANSCRIPT_CODEC_VERSION);
    const domainLength = view.getUint16(5, false);
    expect(text.decode(encoded.subarray(7, 7 + domainLength))).toBe(DEVICE_PROOF_DOMAIN);

    let offset = 7 + domainLength;
    const fieldCount = view.getUint16(offset, false);
    expect(fieldCount).toBe(domain.fields.length);
    offset += 2;

    let previousTag = 0;
    for (const field of domain.fields) {
      const tag = view.getUint16(offset, false);
      const byteLength = view.getUint32(offset + 2, false);
      offset += 6;
      expect(tag).toBe(field.tag);
      expect(tag).toBeGreaterThan(previousTag); // 严格递增且唯一
      previousTag = tag;
      const body = encoded.subarray(offset, offset + byteLength);
      expect(body.length).toBe(byteLength);
      offset += byteLength;
    }
    // 没有尾随字节。
    expect(offset).toBe(encoded.length);
  });

  it("同一 domain 的 host-challenge 向量也逐字节对齐（避免只对一个向量调参）", () => {
    const hostChallenge = JSON.parse(
      readFileSync(
        join(repoRoot, "fixtures", "sync", "v1", "transcripts", "host-challenge.json"),
        "utf8",
      ),
    ) as {
      readonly domain: string;
      readonly input: Readonly<Record<string, unknown>>;
      readonly expected: { readonly transcriptBase64url: string; readonly transcriptSha256Hex: string };
    };
    const hostDomain = registry.domains.find(
      (candidate) => candidate.domain === hostChallenge.domain,
    );
    expect(hostDomain, "registry 缺少 host-challenge domain").toBeDefined();
    if (hostDomain === undefined) return;

    const encoded = encodeFromRegistry(hostDomain, hostChallenge.input);
    expect(encodeBase64Url(encoded)).toBe(hostChallenge.expected.transcriptBase64url);
    expect(createHash("sha256").update(encoded).digest("hex")).toBe(
      hostChallenge.expected.transcriptSha256Hex,
    );
  });
});

describe("R11：transcript 编码器的结构性拒绝", () => {
  it("重复 tag 与乱序 tag 被拒绝（重复先于乱序判定）", () => {
    expect(() =>
      encodeTranscript(DEVICE_PROOF_DOMAIN, [
        { tag: 1, bytes: encodeU16be(1) },
        { tag: 1, bytes: encodeU16be(1) },
      ]),
    ).toThrow(/重复/);
    expect(() =>
      encodeTranscript(DEVICE_PROOF_DOMAIN, [
        { tag: 2, bytes: encodeU16be(1) },
        { tag: 1, bytes: encodeU16be(1) },
      ]),
    ).toThrow(/小于前一个/);
  });

  it("空 domain 与含 NUL 的字符串被拒绝", () => {
    expect(() => encodeTranscript("", [])).toThrow(/domain/);
    expect(() => encodeUtf8("a\u0000b")).toThrow(/NUL/);
  });

  it("UTF-8 字节数超过 u16 上限的 domain 被拒绝（不以码元数判定、不静默截断）", () => {
    // 30000 个码元（String.length 远低于 65535）但 90000 UTF-8 字节。
    const oversized = "\u4e2d".repeat(30000);
    expect(oversized.length).toBeLessThan(0xffff);
    expect(new TextEncoder().encode(oversized).length).toBeGreaterThan(0xffff);
    expect(() => encodeTranscript(oversized, [])).toThrow(TranscriptError);
    // 边界以内（65535 字节）仍然接受，且写出的 u16be 前缀等于真实字节数。
    const atLimit = "a".repeat(0xffff);
    const encoded = encodeTranscript(atLimit, []);
    const view = new DataView(encoded.buffer, encoded.byteOffset, encoded.byteLength);
    expect(view.getUint16(5, false)).toBe(0xffff);
  });

  it("negotiatedFeatures 必须按 UTF-8 字节升序且无重复", () => {
    expect(() =>
      encodeNulJoinedUtf8(["core.snapshot.v1", "acp.raw-payload.v1"]),
    ).toThrow(/升序/);
    expect(encodeNulJoinedUtf8(["acp.raw-payload.v1", "core.snapshot.v1"])).toEqual(
      new TextEncoder().encode("acp.raw-payload.v1\u0000core.snapshot.v1"),
    );
  });
});
