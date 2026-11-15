import { expect } from "vitest";
import { decodeViewFrame, type ViewDataFrame } from "../../src/views/binary";
import { ViewDecodeError, type ViewDecodeErrorCode } from "../../src/views/errors";

export const STREAM = "AAECAwQFBgcICQoLDA0ODw";
export const OTHER = "Dw4NDAsKCQgHBgUEAwIBAA";
export const MAX = 0xffff_ffff_ffff_ffffn;
export const encoder = new TextEncoder();
export async function sha256(bytes: Uint8Array): Promise<string> {
  const digest = await globalThis.crypto.subtle.digest("SHA-256", new Uint8Array(bytes));
  return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("");
}
export function dataWire(options: {
  packet?: bigint; record?: bigint; offset?: bigint; total?: bigint;
  kind?: number; payload?: Uint8Array; flags?: number; stream?: Uint8Array;
} = {}): Uint8Array {
  const payload = options.payload ?? encoder.encode("x");
  const offset = options.offset ?? 0n;
  const total = options.total ?? BigInt(payload.length);
  const wire = new Uint8Array(72 + payload.length);
  wire.set([0x41, 0x43, 0x50, 0x56, 1, 1]);
  const header = new DataView(wire.buffer);
  header.setUint16(6, options.flags ?? (offset + BigInt(payload.length) === total ? 1 : 0));
  wire.set(options.stream ?? Uint8Array.from({ length: 16 }, (_, i) => i), 8);
  for (const [at, value] of [[24, options.packet ?? 1n], [32, options.record ?? 7n], [40, offset], [48, total]] as const) {
    header.setBigUint64(at, value);
  }
  wire[56] = options.kind ?? 1;
  header.setUint32(64, payload.length);
  wire.set(payload, 72);
  return wire;
}
export function controlWire(type: 2 | 3, packet: bigint): Uint8Array {
  const bytes = new Uint8Array(72);
  bytes.set([0x41, 0x43, 0x50, 0x56, 1, type]);
  bytes.set(Uint8Array.from({ length: 16 }, (_, i) => i), 8);
  new DataView(bytes.buffer).setBigUint64(24, packet);
  return bytes;
}
export function frame(options: Parameters<typeof dataWire>[0] = {}): ViewDataFrame {
  const decoded = decodeViewFrame(dataWire(options));
  if (decoded.type !== "data") throw new Error("fixture expects Data");
  return decoded;
}
export function reject(action: () => unknown, code?: ViewDecodeErrorCode): ViewDecodeError {
  let thrown: unknown;
  try { action(); } catch (error) { thrown = error; }
  expect(thrown).toBeInstanceOf(ViewDecodeError);
  const error = thrown as ViewDecodeError;
  if (code !== undefined) expect(error.code).toBe(code);
  expect(error.name).toBe("ViewDecodeError");
  expect(error.message).toBe(error.code);
  expect(Object.hasOwn(error, "cause")).toBe(false);
  return error;
}
