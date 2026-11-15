import { describe, expect, it } from "vitest";
import { encodeBase64url } from "../src/protocol/jws";
import type { ViewDataFrame } from "../src/views/binary";
import {
  QueuedBodyReadError,
  QueuedBodyReader,
} from "../src/management/session-body-read";

const STREAM = encodeBase64url(new Uint8Array(16).fill(1));
const ENCODER = new TextEncoder();

function data(
  packet: bigint,
  offset: bigint,
  total: bigint,
  bytes: Uint8Array,
  overrides: Partial<ViewDataFrame> = {},
): ViewDataFrame {
  return {
    type: "data",
    streamId: STREAM,
    packetSequence: packet,
    recordId: 7n,
    byteOffset: offset,
    totalLength: total,
    kind: "original_raw",
    last: offset + BigInt(bytes.length) === total,
    payload: bytes,
    ...overrides,
  };
}

function snapshotEnd(packet: bigint): ViewDataFrame {
  return data(packet, 0n, 0n, new Uint8Array(), {
    recordId: 1n,
    kind: "snapshot_end",
  });
}

describe("queued input body reader", () => {
  it("assembles split UTF-8 and returns a stable JSON preview at snapshot end", () => {
    const raw = ENCODER.encode('{"text":"\\u4f60\\u597d \\ud83d\\ude42"}');
    const reader = new QueuedBodyReader(STREAM, BigInt(raw.length));
    expect(reader.accept(data(1n, 0n, BigInt(raw.length), raw.slice(0, 5))))
      .toEqual({ state: "pending" });
    expect(reader.accept(data(2n, 5n, BigInt(raw.length), raw.slice(5))))
      .toEqual({ state: "pending" });
    expect(reader.accept(snapshotEnd(3n))).toEqual({
      state: "complete",
      preview: '{\n  "text": "你好 🙂"\n}',
    });
    expect(() => reader.accept(snapshotEnd(4n))).toThrowError(
      new QueuedBodyReadError("invalid_stream"),
    );
  });

  it("keeps non-JSON UTF-8 as the exact displayed text", () => {
    const raw = ENCODER.encode("plain text");
    const reader = new QueuedBodyReader(STREAM, BigInt(raw.length));
    reader.accept(data(1n, 0n, BigInt(raw.length), raw));
    expect(reader.accept(snapshotEnd(2n))).toEqual({
      state: "complete",
      preview: "plain text",
    });
  });

  it("rejects invalid UTF-8 instead of displaying replacement characters", () => {
    const reader = new QueuedBodyReader(STREAM, 1n);
    reader.accept(data(1n, 0n, 1n, Uint8Array.of(0xff)));
    expect(() => reader.accept(snapshotEnd(2n))).toThrowError(
      new QueuedBodyReadError("invalid_stream"),
    );
  });

  it("rejects an unrepresentable declared body before receiving chunks", () => {
    expect(() => new QueuedBodyReader(
      STREAM,
      BigInt(Number.MAX_SAFE_INTEGER) + 1n,
    )).toThrowError(new QueuedBodyReadError("body_too_large"));
  });

  it("rejects descriptor and record length disagreement at completion", () => {
    const raw = ENCODER.encode("body");
    const reader = new QueuedBodyReader(STREAM, 5n);
    reader.accept(data(1n, 0n, BigInt(raw.length), raw));
    expect(() => reader.accept(snapshotEnd(2n))).toThrowError(
      new QueuedBodyReadError("invalid_stream"),
    );
  });

  it("completes a body larger than one MiB without a platform total quota", () => {
    const raw = new Uint8Array(1_048_577).fill(120);
    const reader = new QueuedBodyReader(STREAM, BigInt(raw.length));
    let packet = 1n;
    for (let offset = 0; offset < raw.length; offset += 16_384) {
      const bytes = raw.subarray(offset, offset + 16_384);
      const update = reader.accept(data(
        packet++,
        BigInt(offset),
        BigInt(raw.length),
        bytes,
      ));
      if (offset + bytes.length < raw.length) {
        expect(update).toEqual({ state: "pending" });
      }
    }
    expect(reader.accept(snapshotEnd(packet))).toEqual({
      state: "complete",
      preview: "x".repeat(raw.length),
    });
  });

  it("seals after a packet gap and never resumes partial content", () => {
    const raw = ENCODER.encode("hello");
    const reader = new QueuedBodyReader(STREAM, BigInt(raw.length));
    reader.accept(data(1n, 0n, BigInt(raw.length), raw.slice(0, 2)));
    expect(() => reader.accept(
      data(3n, 2n, BigInt(raw.length), raw.slice(2)),
    )).toThrowError(new QueuedBodyReadError("invalid_stream"));
    expect(() => reader.accept(
      data(2n, 2n, BigInt(raw.length), raw.slice(2)),
    )).toThrowError(new QueuedBodyReadError("invalid_stream"));
  });
});
