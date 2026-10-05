/**
 * 无填充 base64url 的编解码（`docs/SYNC_PROTOCOL.md` §6.1）。
 *
 * ## 为什么本层自带而不复用 `src/platform/transcript.ts`
 *
 * `encodeBase64Url` 在平台层确实存在，但分层门禁禁止内层 import `src/platform/`；
 * 更关键的是**解码**方向（把 wire 上的 nonce 解成 32 字节原始字节喂给 transcript）在平台层
 * 根本没有这个函数，而解码是握手与 digest 校验的必需环节。这里的编解码是 1:1 的字母表变换，
 * 不含协议判定，因此按「纯计算、可单测」的规模重写是可接受的；协议语义仍只有一份
 * （§6.1 的无填充 base64url）。
 */

import type { ByteArray } from "./ports";

/** 无填充 base64url 字母表。 */
const ALPHABET = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/** 字符 → 6 位值的查表；非字母表字符映射为 `-1`。 */
const REVERSE = (() => {
  const table = new Int8Array(128).fill(-1);
  for (let index = 0; index < ALPHABET.length; index += 1) {
    table[ALPHABET.charCodeAt(index)] = index;
  }
  return table;
})();

/** base64url 编解码错误：wire 上出现非字母表字符或长度非法（§6.1 固定宽度）。 */
export class Base64UrlError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "Base64UrlError";
  }
}

/** 字节 → 无填充 base64url。 */
export function encodeBase64Url(bytes: Uint8Array): string {
  let out = "";
  for (let index = 0; index < bytes.length; index += 3) {
    const b0 = bytes[index] ?? 0;
    const b1 = bytes[index + 1] ?? 0;
    const b2 = bytes[index + 2] ?? 0;
    const hasTwo = index + 1 < bytes.length;
    const hasThree = index + 2 < bytes.length;
    out += ALPHABET[b0 >> 2] ?? "A";
    out += ALPHABET[((b0 & 0x03) << 4) | (b1 >> 4)] ?? "A";
    if (!hasTwo) break;
    out += ALPHABET[((b1 & 0x0f) << 2) | (b2 >> 6)] ?? "A";
    if (!hasThree) break;
    out += ALPHABET[b2 & 0x3f] ?? "A";
  }
  return out;
}

/**
 * 无填充 base64url → 字节。
 *
 * 长度必须合法（`4n` 字节对应 `4n/4*3` 字节，即文本长度 `%4` 不得为 1），出现 `=`、`+`、`/`
 * 一律拒绝：wire 上的编码是固定的，宽容解码会把协议漂移藏起来。
 */
export function decodeBase64Url(text: string, expectedBytes?: number): ByteArray {
  if (text.length % 4 === 1) {
    throw new Base64UrlError("base64url 文本长度非法");
  }
  if (expectedBytes !== undefined && text.length !== Math.ceil((expectedBytes * 4) / 3)) {
    throw new Base64UrlError(`base64url 文本长度与 ${expectedBytes} 字节不符`);
  }
  const byteLength = Math.floor((text.length * 3) / 4);
  const out = new Uint8Array(byteLength);
  let outIndex = 0;
  let buffer = 0;
  let bits = 0;
  for (let index = 0; index < text.length; index += 1) {
    const code = text.charCodeAt(index);
    const value = code < 128 ? (REVERSE[code] ?? -1) : -1;
    if (value < 0) {
      throw new Base64UrlError("base64url 文本含非字母表字符");
    }
    buffer = (buffer << 6) | value;
    bits += 6;
    if (bits >= 8) {
      bits -= 8;
      out[outIndex] = (buffer >> bits) & 0xff;
      outIndex += 1;
    }
  }
  return out;
}

/** 两个字节序列逐字节相等（长度不同直接判否）。 */
export function bytesEqual(left: Uint8Array, right: Uint8Array): boolean {
  if (left.length !== right.length) return false;
  for (let index = 0; index < left.length; index += 1) {
    if (left[index] !== right[index]) return false;
  }
  return true;
}