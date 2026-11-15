import { describe, expect, it } from "vitest";
import { readSessionViewStatus } from "../src/management/session-view-status";

const ID = "AAECAwQFBgcICQoLDA0ODw";
const bytes = (source: string) => new TextEncoder().encode(source);
function source(overrides: Record<string, unknown> = {}): string {
  return JSON.stringify({
    type: "view_segment", phase: "start", segmentId: ID, segmentViewEpoch: ID,
    readEpoch: null, origin: "live_observation", readMode: "CurrentOnly",
    freshness: "CurrentObservation", join: "UnassociatedSegments", sourceBindingId: ID,
    sourceId: ID, beginSequence: "9007199254740993", throughSequence: null,
    failure: null, projection: "Available", completionResponseSequence: null,
    open: true, readContext: "outside_read_window", relation: "connection_observation_only",
    limitation: null, ...overrides,
  });
}

describe("shared-view platform status is read without native JSON normalization", () => {
  for (const phase of ["start", "update", "end"]) {
    it(`keeps complete producer ${phase} metadata and original bytes`, () => {
      const raw = source({ phase });
      const result = readSessionViewStatus(bytes(raw));
      expect(result.segmentId).toBe(ID);
      expect(result.requiresReopen).toBe(false);
      expect(result.summary).toContain("历史不完整");
      expect(result.document.tokenLexeme(0)).toBe(raw);
      expect(result.document.tokenLexeme(0)).toContain("9007199254740993");
    });
  }
  it("preserves unknown type, duplicate extension keys and huge numbers without interpreting them", () => {
    const raw = '{"type":"future","extra":{"x":1,"x":2},"n":999999999999999999999999999999}';
    const result = readSessionViewStatus(bytes(raw));
    expect(result.segmentId).toBeNull();
    expect(result.requiresReopen).toBe(false);
    expect(result.document.tokenLexeme(0)).toBe(raw);
    expect(result.summary).toContain("未识别");
  });
  it("does not assign semantics to future enum values or discard extensions", () => {
    const raw = source({ readMode: "FutureMode", projection: "FutureProjection", extension: [1, 2] });
    const result = readSessionViewStatus(bytes(raw));
    expect(result.document.tokenLexeme(0)).toBe(raw);
    expect(result.summary).not.toContain("历史不完整");
    expect(result.summary).not.toContain("结构投影不可用");
  });
  it("reports stale, projection unavailable and actual source failure separately", () => {
    const result = readSessionViewStatus(bytes(source({ freshness: "Stale",
      projection: "ProjectionUnavailable", failure: "read_failure" })));
    expect(result.summary).toContain("旧视图");
    expect(result.summary).toContain("结构投影不可用");
    expect(result.summary).toContain("报告失败");
  });
  it("retains module-only rebound but requires explicit reopen", () => {
    const raw = JSON.stringify({ type: "stream_rebound", previousStreamId: ID,
      streamId: ID, viewEpoch: ID, snapshotRevision: "18446744073709551615" });
    const result = readSessionViewStatus(bytes(raw));
    expect(result.requiresReopen).toBe(true);
    expect(result.segmentId).toBeNull();
    expect(result.document.tokenLexeme(0)).toBe(raw);
  });
  for (const override of [
    { segmentId: "bad" }, { segmentViewEpoch: "AQEBAQEBAQEBAQEBAQEBAQ" },
    { readEpoch: 2 }, { sourceId: null }, { sourceBindingId: "bad" },
    { throughSequence: "01" }, { completionResponseSequence: "18446744073709551616" },
    { beginSequence: 1 }, { open: "true" }, { failure: {} }, { limitation: false },
    { phase: null }, { join: [] },
  ]) {
    it(`rejects invalid known metadata ${JSON.stringify(override)}`, () => {
      expect(() => readSessionViewStatus(bytes(source(override)))).toThrow();
    });
  }
  for (const raw of [
    "[]", "null", '{"type":"future","type":"view_segment"}',
    source().replace('"segmentId":', `"segmentId":"${ID}","segmentId":`),
    source().replace('"open":true,', ""),
    '{"type":"stream_rebound","streamId":"invalid"}',
    '{"type":"stream_rebound","snapshotRevision":"01"}',
    '{"type":"stream_rebound","sourceBindingId":"bad"}',
    '{"type":"stream_rebound","sourceId":"bad"}',
    '{"type":"stream_rebound","readEpoch":"bad"}',
  ]) {
    it(`rejects malformed or ambiguous status ${raw.slice(0, 70)}`, () => {
      expect(() => readSessionViewStatus(bytes(raw))).toThrow();
    });
  }
});
