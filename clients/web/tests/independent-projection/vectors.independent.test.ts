import { createHash } from "node:crypto";
import { describe, expect, it, vi } from "vitest";
import { ProjectionCollection } from "../../src/views/projection-collection";
import { decodeProjectionRecord, type ProjectedItem } from "../../src/views/projection";
import { decodeViewFrame, type ViewDataFrame } from "../../src/views/binary";
import { ViewRecordAssembler } from "../../src/views/records";
import { bytes, decodedItem, fixture, item, record, reject, SEGMENT, STREAM, type ItemWire } from "./support";

interface MetadataReference { rawRecordId: string; byteStart: string; byteEnd: string; append: boolean }
interface RecordInfo { path: string; recordId: string; recordKind: "ItemProjection" | "OriginalRaw" | "Status" | "SnapshotEnd"; segmentId: string; sha256: string; bytes: number }
interface Manifest { stages: { label: string; records: RecordInfo[] }[] }
interface ConsumerStage { label: string; tool: string; toolKey: { segmentId: string; anchorId: string }; firstPosition: string; revision: string; messageItems: string[] }
interface Consumer { versions: { version: string; manifest: string; stages: ConsumerStage[] }[] }
const text = new TextDecoder("utf-8", { fatal: true });
const index = JSON.parse(text.decode(fixture("consumer-index.json"))) as Consumer;
const manifests = index.versions.map((v) => ({ ...v, data: JSON.parse(text.decode(fixture(v.manifest))) as Manifest }));
const projections = manifests.flatMap((v) => v.data.stages.flatMap((s, stage) => s.records.filter((r) => r.recordKind === "ItemProjection").map((r) => ({ ...r, version: v.version, stage }))));
function oracleRef(raw: MetadataReference | null) {
  return raw === null ? null : { rawRecordId: BigInt(raw.rawRecordId), byteStart: BigInt(raw.byteStart), byteEnd: BigInt(raw.byteEnd), append: raw.append };
}
function expectedMetadata(payload: Uint8Array): unknown {
  // This parses the independent platform-wire oracle, never OriginalRaw/native content.
  const wire = JSON.parse(text.decode(payload)) as Record<string, unknown>;
  if (wire.type === "item_projection") {
    const known = wire as unknown as ItemWire;
    return { type: known.type, streamId: STREAM, segmentId: known.segmentId, anchorId: BigInt(known.anchorId), firstPosition: BigInt(known.firstPosition), revision: BigInt(known.revision), rawRecordId: BigInt(known.rawRecordId), kind: known.kind, fields: known.fields.map((f) => ({ field: f.field, knowledge: f.knowledge, raw: oracleRef(f.raw), enumCode: f.enumCode, evidenceCount: BigInt(f.evidenceCount) })) };
  }
  return { type: wire.type, streamId: STREAM, segmentId: wire.segmentId, anchorId: BigInt(wire.anchorId as string), revision: BigInt(wire.revision as string), field: wire.field, index: BigInt(wire.index as string), raw: oracleRef(wire.raw as MetadataReference) };
}

describe("WP08 frozen actual Rust projection payloads", () => {
  it("includes exactly 90 payloads, with 56 items and 34 evidence records", () => {
    expect(projections).toHaveLength(90);
    expect(projections.filter((r) => (JSON.parse(text.decode(fixture(`${r.version}/${r.path}`))) as { type: string }).type === "item_projection")).toHaveLength(56);
  });
  it.each(projections)("$version stage $stage $path: original bytes and complete platform DTO", (entry) => {
    const payload = fixture(`${entry.version}/${entry.path}`);
    expect(createHash("sha256").update(payload).digest("hex")).toBe(entry.sha256);
    expect(payload.length).toBe(entry.bytes);
    const output = decodeProjectionRecord(record(payload, { recordId: BigInt(entry.recordId) }));
    expect(output).toEqual(expectedMetadata(payload)); expect(Object.isFrozen(output)).toBe(true);
  });
  for (const version of manifests) it(`${version.version}: six stages preserve one T at first position, references, raw order and message order`, () => {
    const collections = new Map<string, ProjectionCollection>();
    const frozenRaw = new Map<string, string>();
    const firstMessages = new Map<string, { first: bigint; value: ProjectedItem }>();
    const expectedRevisions = version.version === "v1" ? [5n, 8n, 10n, 11n, 11n, 11n] : [6n, 9n, 11n, 12n, 12n, 12n];
    const expectedPosition = version.version === "v1" ? 5n : 6n;
    let evidenceCount = 0;
    for (let stageIndex = 0; stageIndex < version.data.stages.length; ++stageIndex) {
      const stage = version.data.stages[stageIndex]!; const consumer = version.stages[stageIndex]!;
      const rawOrder: string[] = [];
      for (const entry of stage.records) {
        const payload = fixture(`${version.version}/${entry.path}`);
        const digest = createHash("sha256").update(payload).digest("hex"); expect(digest).toBe(entry.sha256);
        if (entry.recordKind === "OriginalRaw") {
          const identity = `${entry.segmentId}/${entry.recordId}`; rawOrder.push(identity);
          const prior = frozenRaw.get(identity); if (prior !== undefined) expect(digest).toBe(prior); frozenRaw.set(identity, digest);
          continue;
        }
        if (entry.recordKind !== "ItemProjection") continue; // Caller retains status/snapshot_end routing.
        const output = decodeProjectionRecord(record(payload, { recordId: BigInt(entry.recordId) }));
        const references = output.type === "projection_evidence" ? [output.raw] : output.fields.flatMap((f) => f.raw === null ? [] : [f.raw]);
        for (const reference of references) {
          const rawInfo = stage.records.find((r) => r.recordKind === "OriginalRaw" && r.segmentId === output.segmentId && r.recordId === String(reference.rawRecordId));
          expect(rawInfo).toBeDefined();
          const original = fixture(`${version.version}/${rawInfo!.path}`);
          expect(reference.byteStart).toBeLessThanOrEqual(reference.byteEnd); expect(reference.byteEnd).toBeLessThanOrEqual(BigInt(original.length));
          const span = original.subarray(Number(reference.byteStart), Number(reference.byteEnd));
          expect(span.length).toBe(Number(reference.byteEnd - reference.byteStart));
          // Native bytes are compared directly; no JSON.parse/stringify or text coalescing here.
        }
        if (output.type === "projection_evidence") { ++evidenceCount; continue; }
        let collection = collections.get(output.segmentId);
        if (!collection) { collection = new ProjectionCollection(STREAM, output.segmentId); collections.set(output.segmentId, collection); }
        const accepted = collection.accept(output);
        if (stageIndex === 5) expect(accepted).toBe(false);
      }
      expect(rawOrder).toEqual(stage.records.filter((r) => r.recordKind === "OriginalRaw").map((r) => `${r.segmentId}/${r.recordId}`));
      const collection = collections.get(consumer.toolKey.segmentId)!;
      const tools = collection.items().filter((v) => v.anchorId === BigInt(consumer.toolKey.anchorId));
      expect(tools).toHaveLength(1); const tool = tools[0]!;
      expect(tool.kind).toBe("tool"); expect(tool.firstPosition).toBe(expectedPosition); expect(tool.revision).toBe(expectedRevisions[stageIndex]); expect(tool.fields[5]!.enumCode).toBe([0, 1, 1, 2, 2, 2][stageIndex]);
      const messages = consumer.messageItems.map((p) => JSON.parse(text.decode(fixture(`${version.version}/${p}`))) as ItemWire);
      const actualMessages = collection.items().filter((v) => v.kind === "message");
      expect(actualMessages.map((v) => v.anchorId)).toEqual(messages.map((v) => BigInt(v.anchorId)));
      for (const message of actualMessages) {
        const key = `${message.segmentId}/${message.anchorId}`; const prior = firstMessages.get(key);
        if (prior) { expect(message.firstPosition).toBe(prior.first); expect(message).toBe(prior.value); }
        else firstMessages.set(key, { first: message.firstPosition, value: message });
      }
      expect(collection.items().findIndex((v) => v.anchorId === tool.anchorId)).toBe(0);
      if (stageIndex >= 4) expect(collection.items().some((v) => v.kind === "unknownStructured")).toBe(true);
    }
    expect(evidenceCount).toBeGreaterThan(0);
    console.info(`WP08 ${version.version}: six stages; first=${expectedPosition}; revisions=${expectedRevisions.join(",")}; evidence routed=${evidenceCount}`);
  });
});

function frame(payload: Uint8Array, offset: number, total: number, packet: bigint): ViewDataFrame {
  const wire = new Uint8Array(72 + payload.length); wire.set([65, 67, 80, 86, 1, 1]); wire.set(Uint8Array.from({ length: 16 }, (_, i) => i), 8);
  const header = new DataView(wire.buffer); header.setUint16(6, offset + payload.length === total ? 1 : 0); header.setBigUint64(24, packet); header.setBigUint64(32, 19n); header.setBigUint64(40, BigInt(offset)); header.setBigUint64(48, BigInt(total)); wire[56] = 2; header.setUint32(64, payload.length); wire.set(payload, 72);
  const decoded = decodeViewFrame(wire); if (decoded.type !== "data") throw new Error(); return decoded;
}

describe("WP09 composed existing functions, synthetic outer framing", () => {
  it("does not decode a partial record and accepts the complete metadata above 16KiB", () => {
    const payload = new TextEncoder().encode(" ".repeat(20_000) + JSON.stringify(item({ revision: "2" })));
    const collection = new ProjectionCollection(STREAM, SEGMENT); const old = decodedItem(); collection.accept(old);
    const assembler = new ViewRecordAssembler(STREAM);
    expect(assembler.accept(frame(payload.subarray(0, 16_384), 0, payload.length, 1n))).toBeNull(); expect(collection.items()).toEqual([old]);
    const completed = assembler.accept(frame(payload.subarray(16_384), 16_384, payload.length, 2n)); expect(completed).not.toBeNull();
    const projected = decodeProjectionRecord(completed!); if (projected.type !== "item_projection") throw new Error();
    expect(collection.accept(projected)).toBe(true); expect(collection.items()[0]!.revision).toBe(2n);
  });
  it("cancels only a partial display assembly and preserves previously accepted items", () => {
    const payload = bytes(item({ revision: "3" })); const collection = new ProjectionCollection(STREAM, SEGMENT); const old = decodedItem(); collection.accept(old);
    const assembler = new ViewRecordAssembler(STREAM); expect(assembler.accept(frame(payload.subarray(0, 10), 0, payload.length, 1n))).toBeNull();
    assembler.seal(); reject(() => assembler.accept(frame(payload.subarray(10), 10, payload.length, 2n)), "sealed"); expect(collection.items()[0]).toBe(old);
    const fresh = new ViewRecordAssembler(STREAM); const completed = fresh.accept(frame(payload, 0, payload.length, 1n)); const output = decodeProjectionRecord(completed!); if (output.type !== "item_projection") throw new Error(); expect(collection.accept(output)).toBe(true); expect(collection.items()[0]!.revision).toBe(3n);
  });
  it("has no network, raw-value resolution or execution side effect", () => {
    let calls = 0; vi.stubGlobal("fetch", () => { ++calls; throw new Error("synthetic no-network guard"); }); vi.stubGlobal("WebSocket", class { constructor() { ++calls; throw new Error("synthetic no-network guard"); } });
    try {
      const value = item({ kind: "unknownStructured" }); value.fields[8]!.raw = { rawRecordId: "99999", byteStart: "0", byteEnd: "18446744073709551615", append: true };
      const collection = new ProjectionCollection(STREAM, SEGMENT); expect(collection.accept(decodedItem(value))).toBe(true);
      expect(collection.items()[0]!.fields[8]!.raw!.byteEnd).toBe(0xffff_ffff_ffff_ffffn); expect(calls).toBe(0);
    } finally { vi.unstubAllGlobals(); }
  });
});
