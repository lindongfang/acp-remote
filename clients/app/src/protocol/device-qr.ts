/**
 * 新设备短二维码（`docs/SYNC_PROTOCOL.md` §7.1）的 TypeScript 编解码。
 *
 * ```text
 * https://<canonical-origin>/pair#q2=<payload>
 * payload = base64url-no-pad( 0x02 || qrToken(32) || hostKeyFingerprint(32) )
 * ```
 *
 * 与 Rust `sync_protocol::device_qr` 逐条对应：只接受版本 2、65 字节布局、规范无填充
 * base64url，以及无 userinfo/query、路径恰为 `/pair` 的 HTTPS 外层地址。旧设备 `#data=`、
 * 未知版本、错误长度和非规范编码一律**明确拒绝**，不降级解析。
 *
 * 本模块是纯计算，不读文件、不碰 DOM 存储；`qrToken` 是取回配对资料的凭据，调用方不得把它
 * 写进 UI 文本、日志或持久化状态。主机公钥的 P-256 曲线合法性与 SHA-256 指纹校验属于消费方
 * （本层只固定指纹的 32 字节宽度与载荷编解码）。
 */

/** 载荷格式版本（第一个字节）。 */
export const DEVICE_QR_VERSION = 2;

/** 解码后的载荷长度：1 字节版本 + 32 字节取回凭据 + 32 字节指纹。 */
export const DEVICE_QR_PAYLOAD_BYTES = 65;

/** 无填充 base64url 编码后的载荷长度。 */
export const DEVICE_QR_PAYLOAD_ASCII_LENGTH = 87;

/** 设备二维码只接受的 fragment 键前缀。 */
export const DEVICE_QR_FRAGMENT_PREFIX = "q2=";

/** 旧设备完整资料 fragment 前缀；收到即明确拒绝。 */
export const LEGACY_DEVICE_QR_FRAGMENT_PREFIX = "data=";

/** 固定测试地址（共享向量 `fixtures/sync/v1/device-qr/vectors.json`）。 */
export const DEVICE_QR_TEST_ORIGIN = "https://faraday.tail74842e.ts.net:8765";

/** 固定测试地址的完整长度：43 + 4 + 87。 */
export const DEVICE_QR_TEST_URL_LENGTH = 134;

/** 测试地址承诺的总长度上界。 */
export const DEVICE_QR_MAX_TEST_URL_LENGTH = 150;

/** L 级纠错 + 四模块白边后的矩阵列数预算（实心半块字符）。 */
export const DEVICE_QR_MATRIX_MAX_COLUMNS = 58;

/** L 级纠错 + 四模块白边后的矩阵行数预算（实心半块字符）。 */
export const DEVICE_QR_MATRIX_MAX_ROWS = 29;

/** 与 Rust `DeviceQrError::code()` 逐字相同的稳定错误码。 */
export type DeviceQrErrorCode =
  | "legacy_data_format"
  | "malformed_pairing_url"
  | "unsupported_version"
  | "wrong_length"
  | "non_canonical_base64url";

/** 解码失败。消息就是机器码，绝不回显载荷或凭据。 */
export class DeviceQrError extends Error {
  readonly code: DeviceQrErrorCode;

  constructor(code: DeviceQrErrorCode) {
    super(code);
    this.name = "DeviceQrError";
    this.code = code;
  }
}

/** 解码后的 fragment 载荷。 */
export interface DeviceQrPayload {
  readonly version: typeof DEVICE_QR_VERSION;
  /** 32 字节扫码取回凭据（敏感材料，只进内存）。 */
  readonly qrToken: Uint8Array;
  /** 32 字节主机公钥指纹（SHA-256 原始摘要，不是 hex）。 */
  readonly hostKeyFingerprint: Uint8Array;
}

/** 已解析的设备配对地址。 */
export interface DevicePairingUrl {
  readonly canonicalOrigin: string;
  readonly payload: DeviceQrPayload;
}

const BASE64URL_ALPHABET =
  "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
const BASE64URL_PATTERN = /^[A-Za-z0-9_-]+$/;

function encodeBase64Url(bytes: Uint8Array): string {
  let out = "";
  for (let index = 0; index < bytes.length; index += 3) {
    const first = bytes[index];
    const second = bytes[index + 1];
    const third = bytes[index + 2];
    if (first === undefined) break;
    out += BASE64URL_ALPHABET.charAt(first >> 2);
    out += BASE64URL_ALPHABET.charAt(((first & 0x03) << 4) | ((second ?? 0) >> 4));
    if (second === undefined) break;
    out += BASE64URL_ALPHABET.charAt(((second & 0x0f) << 2) | ((third ?? 0) >> 6));
    if (third === undefined) break;
    out += BASE64URL_ALPHABET.charAt(third & 0x3f);
  }
  return out;
}

function decodeBase64Url(text: string): Uint8Array {
  if (text.length === 0 || text.length % 4 === 1 || !BASE64URL_PATTERN.test(text)) {
    throw new DeviceQrError("non_canonical_base64url");
  }
  const bytes: number[] = [];
  let buffer = 0;
  let bits = 0;
  for (const character of text) {
    const value = BASE64URL_ALPHABET.indexOf(character);
    if (value < 0) throw new DeviceQrError("non_canonical_base64url");
    buffer = (buffer << 6) | value;
    bits += 6;
    if (bits >= 8) {
      bits -= 8;
      bytes.push((buffer >> bits) & 0xff);
      buffer &= (1 << bits) - 1;
    }
  }
  const decoded = Uint8Array.from(bytes);
  // 非规范尾字节（低位有值）与填充会在这里被拒：只有规范编码能重新编码出同一字符串。
  if (encodeBase64Url(decoded) !== text) {
    throw new DeviceQrError("non_canonical_base64url");
  }
  return decoded;
}

function assertPayloadParts(payload: DeviceQrPayload): void {
  if (payload.qrToken.length !== 32 || payload.hostKeyFingerprint.length !== 32) {
    throw new DeviceQrError("wrong_length");
  }
}

/** 编码为 87 个 ASCII 字符的规范载荷。 */
export function encodeDeviceQrPayload(payload: DeviceQrPayload): string {
  assertPayloadParts(payload);
  const bytes = new Uint8Array(DEVICE_QR_PAYLOAD_BYTES);
  bytes[0] = DEVICE_QR_VERSION;
  bytes.set(payload.qrToken, 1);
  bytes.set(payload.hostKeyFingerprint, 33);
  return encodeBase64Url(bytes);
}

/** 严格解码 fragment 载荷；只接受版本 2 的 65 字节规范布局。 */
export function decodeDeviceQrPayload(text: string): DeviceQrPayload {
  const bytes = decodeBase64Url(text);
  if (bytes.length !== DEVICE_QR_PAYLOAD_BYTES) {
    throw new DeviceQrError("wrong_length");
  }
  const version = bytes[0];
  if (version !== DEVICE_QR_VERSION) {
    throw new DeviceQrError("unsupported_version");
  }
  return {
    version: DEVICE_QR_VERSION,
    qrToken: bytes.slice(1, 33),
    hostKeyFingerprint: bytes.slice(33, 65),
  };
}

function assertCanonicalOrigin(origin: string): void {
  if (!origin.startsWith("https://")) {
    throw new DeviceQrError("malformed_pairing_url");
  }
  const authority = origin.slice("https://".length);
  if (
    authority.length === 0 ||
    /[/@?#]/.test(authority) ||
    /[\s\u0000-\u001f\u007f]/.test(authority)
  ) {
    throw new DeviceQrError("malformed_pairing_url");
  }
}

/** 构造 `https://<canonical-origin>/pair#q2=…`。 */
export function buildDevicePairingUrl(origin: string, payload: DeviceQrPayload): string {
  assertCanonicalOrigin(origin);
  return `${origin}/pair#${DEVICE_QR_FRAGMENT_PREFIX}${encodeDeviceQrPayload(payload)}`;
}

/** 严格解析设备配对地址；旧 `#data=` 与其他 fragment 键都拒绝。 */
export function parseDevicePairingUrl(url: string): DevicePairingUrl {
  const hash = url.indexOf("#");
  if (hash < 0) throw new DeviceQrError("malformed_pairing_url");
  const base = url.slice(0, hash);
  const fragment = url.slice(hash + 1);
  if (fragment.includes("#")) throw new DeviceQrError("malformed_pairing_url");
  if (fragment.startsWith(LEGACY_DEVICE_QR_FRAGMENT_PREFIX)) {
    throw new DeviceQrError("legacy_data_format");
  }
  if (!fragment.startsWith(DEVICE_QR_FRAGMENT_PREFIX)) {
    throw new DeviceQrError("malformed_pairing_url");
  }
  const payloadText = fragment.slice(DEVICE_QR_FRAGMENT_PREFIX.length);

  if (url.includes("?")) throw new DeviceQrError("malformed_pairing_url");
  if (!base.startsWith("https://") || !base.endsWith("/pair")) {
    throw new DeviceQrError("malformed_pairing_url");
  }
  const authority = base.slice("https://".length, -"/pair".length);
  const canonicalOrigin = `https://${authority}`;
  assertCanonicalOrigin(canonicalOrigin);

  return { canonicalOrigin, payload: decodeDeviceQrPayload(payloadText) };
}
