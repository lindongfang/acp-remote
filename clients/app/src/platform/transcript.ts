/**
 * `acpr-transcript-v1` 的 Web 侧编码器（`docs/SYNC_PROTOCOL.md` §6.2/§6.3）。
 *
 * 归属 WP5b：设备证明（device proof）的签名输入必须由**客户端**按同一份规范编码，
 * 才能与 Rust 侧 `crates/acpr-transcript` 的产物逐字节相同。字段表以
 * `compatibility/transcripts/v1/transcripts.json` 为唯一机器权威——本模块**不复制**那份表，
 * 也不复制任何 fixture：向量只在测试里从仓库根读取（见 `transcript.test.ts`）。
 *
 * 与 Rust 侧同一份语义（`crates/acpr-transcript/src/lib.rs`）：
 *
 * ```text
 * magic(4 ASCII "ACPR") || codecVersion(u8=1) || domainLength(u16be) || domain(UTF-8)
 *   || fieldCount(u16be) || 逐字段 { fieldTag(u16be) || byteLength(u32be) || rawBytes }
 * ```
 *
 * 规则（逐条对应 §6.2 与 registry 的 `codec.description`）：
 *
 * - 字段按 `fieldTag` 严格递增，只能出现一次；重复先于乱序判定（与 Rust 侧一致）；
 * - 字符串是**不带 NUL** 的 UTF-8 原始字节；
 * - UUID 解码为 16 个原始字节后进入 transcript；
 * - protocol version 用 `u16be`，Unix 时间用 `u64be`；
 * - 公钥、nonce、hash 用原始字节，不用 base64url 文本；
 * - `negotiatedFeatures` 是**排序后**以单个 `0x00` 连接且**不带尾随 NUL**的 UTF-8 字节。
 *
 * 本模块是纯计算，不触碰任何平台 API（WebCrypto/IndexedDB 只在 `secure-storage.web.ts`），
 * 因此可在 Node 测试与浏览器 bundle 中同样求值。
 */

/** transcript 前缀，固定 4 字节 ASCII（`ACPR`）。 */
export const TRANSCRIPT_MAGIC = "ACPR";

/**
 * 明确以普通 `ArrayBuffer` 为底的字节视图。
 *
 * TypeScript 5.9 起 `Uint8Array` 泛型化：默认的 `Uint8Array<ArrayBufferLike>` 与
 * WebCrypto 的 `BufferSource` 不兼容（`SharedArrayBuffer` 分支无法排除）。本模块产出的
 * 字节都以 `new Uint8Array(...)` 构造，底层必然是普通 `ArrayBuffer`，因此显式声明该形状，
 * 让调用方（含 `crypto.subtle.sign`）无需在每处强转。
 */
export type ByteArray = Uint8Array<ArrayBuffer>;

/** v1 codec 版本，紧随 magic 的单个字节。 */
export const TRANSCRIPT_CODEC_VERSION = 1;

/** 设备证明的 domain（`docs/SYNC_PROTOCOL.md` §6.3；与 `host-challenge` 同字段、不同 domain）。 */
export const DEVICE_PROOF_DOMAIN = "acp-remote/device-proof/v1";

/**
 * 单个字段：tag 与已编码好的原始字节。
 *
 * 调用方负责把值编码成原始字节（见本模块的 `encodeU16be`/`encodeUuid16`/… 辅助），
 * 编码器只负责结构、顺序与长度前缀。这样「值怎么编」与「结构怎么拼」各自独立可测。
 */
export interface TranscriptField {
  readonly tag: number;
  readonly bytes: ByteArray;
}

/**
 * transcript 结构错误。
 *
 * 与 Rust 侧 `TranscriptError` 同名同义，便于跨语言对照；**不**包含任何字段值，
 * 避免把可能敏感的材料带进错误与日志（`AGENTS.md` §3）。
 */
export class TranscriptError extends Error {
  readonly kind:
    | "invalid_domain"
    | "too_many_fields"
    | "duplicate_tag"
    | "field_order"
    | "field_too_long";

  constructor(kind: TranscriptError["kind"], message: string) {
    super(message);
    this.name = "TranscriptError";
    this.kind = kind;
  }
}

/** `u16be` 字段值（protocol version 等）。 */
export function encodeU16be(value: number): ByteArray {
  if (!Number.isInteger(value) || value < 0 || value > 0xffff) {
    throw new TranscriptError("field_too_long", "u16be 字段超出取值域");
  }
  const out = new Uint8Array(2);
  new DataView(out.buffer).setUint16(0, value, false);
  return out;
}

/** `u64be` 字段值（Unix 秒）。 */
export function encodeU64be(value: bigint): ByteArray {
  if (value < 0n || value > 0xffffffffffffffffn) {
    throw new TranscriptError("field_too_long", "u64be 字段超出取值域");
  }
  const out = new Uint8Array(8);
  new DataView(out.buffer).setBigUint64(0, value, false);
  return out;
}

/**
 * `uuid16` 字段值：UUID 文本解码为 16 个原始字节。
 *
 * 只接受带连字符的 canonical 小写/大写混合 hex 形状（与 `schemas/sync/v1/common.schema.json`
 * 的 `$defs.uuid` 一致），拒绝其它写法——UUID 的**字节**进入 transcript，而不是文本。
 */
export function encodeUuid16(uuid: string): ByteArray {
  const hex = uuid.replaceAll("-", "").toLowerCase();
  if (!/^[0-9a-f]{32}$/.test(hex)) {
    throw new TranscriptError("field_too_long", "uuid16 字段不是 UUID");
  }
  const out = new Uint8Array(16);
  for (let index = 0; index < 16; index += 1) {
    out[index] = Number.parseInt(hex.slice(index * 2, index * 2 + 2), 16);
  }
  return out;
}

/** `utf8` 字段值：不含 NUL 的 UTF-8 原始字节。 */
export function encodeUtf8(value: string): ByteArray {
  if (value.includes("\u0000")) {
    throw new TranscriptError("field_too_long", "utf8 字段不得含 NUL");
  }
  return new TextEncoder().encode(value);
}

/**
 * `nul-joined-utf8` 字段值：各段以**单个** `0x00` 连接，**无尾随 NUL**，段内不得含 NUL，
 * 且输入必须已按 UTF-8 字节升序排列（`docs/SYNC_PROTOCOL.md` §5.2「列表去重后按 UTF-8 字节升序排列」）。
 *
 * 排序在这里**校验而非重排**：静默重排会让「调用方顺序写错」变成看不出来的行为，
 * 而不排序的 wire 又与规范不符。两者都不可接受，因此顺序错误是硬错误。
 */
export function encodeNulJoinedUtf8(parts: readonly string[]): ByteArray {
  for (const part of parts) {
    if (part.includes("\u0000")) {
      throw new TranscriptError("field_too_long", "nul-joined-utf8 字段的段不得含 NUL");
    }
  }
  const encoded = parts.map((part) => new TextEncoder().encode(part));
  for (let index = 1; index < encoded.length; index += 1) {
    const previous = encoded[index - 1];
    const current = encoded[index];
    if (previous === undefined || current === undefined) continue;
    if (compareBytes(previous, current) >= 0) {
      throw new TranscriptError(
        "field_order",
        "nul-joined-utf8 字段必须按 UTF-8 字节升序、无重复",
      );
    }
  }
  return joinBytes(encoded, 0x00);
}

/**
 * 按 `domain` 与 `fields` 编码 transcript。
 *
 * `fields` 必须按 `tag` 严格升序且唯一；重复先于乱序判定（与 Rust 侧 `encode` 一致）。
 * 结构非法时抛 `TranscriptError`，不产出半成品字节。
 */
export function encodeTranscript(
  domain: string,
  fields: readonly TranscriptField[],
): ByteArray {
  if (domain.length === 0 || domain.includes("\u0000") || domain.length > 0xffff) {
    throw new TranscriptError("invalid_domain", "domain 为空、含 NUL 或超过 u16 上限");
  }
  if (fields.length > 0xffff) {
    throw new TranscriptError("too_many_fields", "字段数量超过 u16 上限");
  }

  const domainBytes = new TextEncoder().encode(domain);
  // magic(4) + codecVersion(1) + domainLength(2) + domain + fieldCount(2)
  const header: ByteArray = new Uint8Array(4 + 1 + 2 + domainBytes.length + 2);
  const headerView = new DataView(header.buffer);
  header[0] = 0x41; // 'A'
  header[1] = 0x43; // 'C'
  header[2] = 0x50; // 'P'
  header[3] = 0x52; // 'R'
  header[4] = TRANSCRIPT_CODEC_VERSION;
  headerView.setUint16(5, domainBytes.length, false);
  header.set(domainBytes, 7);
  headerView.setUint16(7 + domainBytes.length, fields.length, false);

  const chunks: ByteArray[] = [header];
  let previousTag: number | undefined;
  for (const field of fields) {
    if (previousTag !== undefined) {
      if (field.tag === previousTag) {
        throw new TranscriptError("duplicate_tag", `字段 tag ${field.tag} 重复`);
      }
      if (field.tag < previousTag) {
        throw new TranscriptError(
          "field_order",
          `字段 tag ${field.tag} 小于前一个 tag ${previousTag}`,
        );
      }
    }
    previousTag = field.tag;

    if (!Number.isInteger(field.tag) || field.tag < 0 || field.tag > 0xffff) {
      throw new TranscriptError("field_too_long", "字段 tag 超出 u16 取值域");
    }
    if (field.bytes.length > 0xffffffff) {
      throw new TranscriptError("field_too_long", `字段 tag ${field.tag} 超过 u32 长度上限`);
    }
    const fieldHeader: ByteArray = new Uint8Array(6);
    const fieldView = new DataView(fieldHeader.buffer);
    fieldView.setUint16(0, field.tag, false);
    fieldView.setUint32(2, field.bytes.length, false);
    chunks.push(fieldHeader, field.bytes);
  }

  return joinBytes(chunks, null);
}

/** 无填充 base64url 编码（`docs/SYNC_PROTOCOL.md` §6.1）。 */
export function encodeBase64Url(bytes: ByteArray): string {
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replaceAll("+", "-").replaceAll("/", "_").replaceAll("=", "");
}

/** 逐字节比较（UTF-8 字节升序判定用）。 */
function compareBytes(left: ByteArray, right: ByteArray): number {
  const length = Math.min(left.length, right.length);
  for (let index = 0; index < length; index += 1) {
    const a = left[index] ?? 0;
    const b = right[index] ?? 0;
    if (a !== b) return a - b;
  }
  return left.length - right.length;
}

/** 拼接若干字节块；`separator` 非 null 时在块之间插入该字节。 */
function joinBytes(chunks: readonly ByteArray[], separator: number | null): ByteArray {
  let total = 0;
  for (let index = 0; index < chunks.length; index += 1) {
    total += chunks[index]?.length ?? 0;
    if (separator !== null && index > 0) total += 1;
  }
  const out: ByteArray = new Uint8Array(total);
  let offset = 0;
  for (let index = 0; index < chunks.length; index += 1) {
    if (separator !== null && index > 0) {
      out[offset] = separator;
      offset += 1;
    }
    const chunk = chunks[index];
    if (chunk !== undefined) {
      out.set(chunk, offset);
      offset += chunk.length;
    }
  }
  return out;
}
