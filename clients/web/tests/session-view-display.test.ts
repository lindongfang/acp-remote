import { beforeEach, describe, expect, it, vi } from "vitest";
import { SessionViewDisplay } from "../src/management/session-view-display";
import type { SessionViewOpened } from "../src/management/session-protocol";
import type { ViewDataFrame, ViewRecordKind } from "../src/views/binary";
import { item, STREAM, SEGMENT } from "./support/timeline-fixtures";

// Component ownership/flow-control tests: this small DOM double is not browser or main-page evidence.
class Element {
  textContent = "";
  className = "";
  type = "";
  dataset: Record<string, string> = {};
  children: Element[] = [];
  attributes = new Map<string, string>();
  removed = false;
  setAttribute(key: string, value: string): void { this.attributes.set(key, value); }
  append(...elements: Element[]): void { this.children.push(...elements); }
  replaceChildren(...elements: Element[]): void { this.children = elements; }
  remove(): void { this.removed = true; }
  addEventListener(): void {}
}
const { timelines, inspectors } = vi.hoisted(() => ({ timelines: vi.fn(), inspectors: vi.fn() }));
vi.mock("../src/views/timeline", () => ({ createStructuredTimeline: (...args: unknown[]) => timelines(...args) }));
vi.mock("../src/views/raw-inspector", () => ({ createRawInspector: (...args: unknown[]) => inspectors(...args) }));
const owner = { createElement: () => new Element() } as unknown as Document;
const opened: SessionViewOpened = {
  v: 1, type: "view.opened", requestId: STREAM, streamId: STREAM, viewEpoch: STREAM,
  snapshotRevision: "0", capabilities: { opaqueChunks: 1 }, status: {
    mode: "current_only", freshness: "current_observation", projection: "available",
    join: "unassociated_segments", historyIncomplete: true, cacheRecoverable: false,
  },
};
function frame(sequence: bigint, source: string, kind: ViewRecordKind = "original_raw",
  overrides: Partial<ViewDataFrame> = {}): ViewDataFrame {
  const payload = new TextEncoder().encode(source);
  return { type: "data", streamId: STREAM, packetSequence: sequence, recordId: sequence,
    byteOffset: 0n, totalLength: BigInt(payload.length), kind, last: true, payload, ...overrides };
}
function setup(credit = vi.fn()) {
  const detach = vi.fn(), snapshotReady = vi.fn();
  const display = new SessionViewDisplay(owner, opened, { credit, detach, snapshotReady });
  return { display, credit, detach, snapshotReady, node: display.element as unknown as Element };
}
beforeEach(() => {
  timelines.mockReset().mockImplementation(() => ({ element: new Element(), refresh: vi.fn(), dispose: vi.fn() }));
  inspectors.mockReset();
});

describe("original shared-stream display owner", () => {
  it("does not credit in constructor, starts original stream once, and exposes limitations", () => {
    const { display, credit, node } = setup();
    expect(credit).not.toHaveBeenCalled();
    expect(node.children[0]?.textContent).toContain("历史不完整");
    expect(node.children[0]?.textContent).toContain("缓存丢失");
    display.start(); display.start();
    expect(credit.mock.calls).toEqual([[STREAM, 0n]]);
  });
  it("credits each accepted partial packet and accepts only its own stream", () => {
    const { display, credit, detach } = setup();
    display.start();
    display.accept(frame(1n, "{", "status", { totalLength: 2n, last: false }));
    expect(timelines).not.toHaveBeenCalled();
    display.accept(frame(2n, "}", "status", { recordId: 1n, byteOffset: 1n, totalLength: 2n }));
    display.accept(frame(3n, "{}", "status", { streamId: SEGMENT }));
    expect(credit.mock.calls).toEqual([[STREAM, 0n], [STREAM, 1n], [STREAM, 2n]]);
    expect(detach).not.toHaveBeenCalled();
  });
  it("treats snapshot end as a boundary and keeps consuming tail packets", () => {
    const { display, credit, snapshotReady, node } = setup();
    display.accept(frame(1n, "", "snapshot_end"));
    display.accept(frame(2n, '{"type":"unknown"}', "status"));
    expect(snapshotReady).toHaveBeenCalledOnce();
    expect(credit.mock.calls).toEqual([[STREAM, 1n], [STREAM, 2n]]);
    expect(node.children[0]?.textContent).toContain("不代表原生历史重放已完整");
  });
  it("reuses one segment timeline for a later tool revision and disposes it once", () => {
    const { display, detach, node } = setup();
    display.accept(frame(1n, '{}'));
    display.accept(frame(2n, new TextDecoder().decode(item(0n, 1n, 1n, 1n).bytes()), "item_projection"));
    display.accept(frame(3n, '{}'));
    display.accept(frame(4n, new TextDecoder().decode(item(0n, 1n, 2n, 3n).bytes()), "item_projection"));
    expect(timelines).toHaveBeenCalledOnce();
    const timeline = timelines.mock.results[0]?.value;
    expect(timeline.refresh).toHaveBeenCalledTimes(3);
    const store = timelines.mock.calls[0]?.[1];
    expect(store.items(SEGMENT)[0].firstPosition).toBe(1n);
    expect(store.items(SEGMENT)[0].revision).toBe(2n);
    expect(detach).not.toHaveBeenCalled();
    display.dispose(); display.dispose();
    expect(timeline.dispose).toHaveBeenCalledOnce();
    expect(node.removed).toBe(true);
    expect(() => store.items(SEGMENT)).toThrow();
  });
  it("stops and detaches on invalid status without confirming the failed packet", () => {
    const { display, credit, detach, node } = setup();
    display.accept(frame(1n, '{"type":"unknown"}', "status"));
    display.accept(frame(2n, '{"type":"view_segment"}', "status"));
    display.accept(frame(3n, '{}'));
    expect(credit.mock.calls).toEqual([[STREAM, 1n]]);
    expect(detach).toHaveBeenCalledWith(STREAM);
    expect(node.dataset.stale).toBe("true");
    expect(node.children[0]?.textContent).toContain("未取消原生执行");
    expect(node.children[1]?.children).toHaveLength(1);
  });
  it("keeps old DOM but stops credit on sequence failure and later packets", () => {
    const { display, credit, detach, node } = setup();
    display.accept(frame(2n, '{}'));
    display.accept(frame(1n, '{}'));
    expect(credit).not.toHaveBeenCalled();
    expect(detach).toHaveBeenCalledOnce();
    expect(node.removed).toBe(false);
  });
  it("credits accepted module-only rebound on old stream before detach without minting a binding", () => {
    const { display, credit, detach, node } = setup();
    const order: string[] = [];
    credit.mockImplementation((stream, sequence) => order.push(`credit:${stream}:${sequence}`));
    detach.mockImplementation((stream) => order.push(`detach:${stream}`));
    display.accept(frame(1n, JSON.stringify({ type: "stream_rebound", streamId: SEGMENT,
      previousStreamId: STREAM, viewEpoch: SEGMENT, snapshotRevision: "2" }), "status"));
    display.accept(frame(2n, '{}'));
    expect(order).toEqual([`credit:${STREAM}:1`, `detach:${STREAM}`]);
    expect(credit.mock.calls).toEqual([[STREAM, 1n]]);
    expect(detach).toHaveBeenCalledWith(STREAM);
    expect(node.children[0]?.textContent).toContain("重新打开");
  });
  it("detaches old stream without accepting later data when rebound credit fails", () => {
    const credit = vi.fn(() => { throw new Error("synthetic-credit"); });
    const { display, detach, node } = setup(credit);
    display.accept(frame(1n, '{"type":"stream_rebound"}', "status"));
    display.accept(frame(2n, '{}'));
    expect(credit).toHaveBeenCalledExactlyOnceWith(STREAM, 1n);
    expect(detach).toHaveBeenCalledExactlyOnceWith(STREAM);
    expect(node.dataset.stale).toBe("true");
    expect(node.children[0]?.textContent).toContain("未取消原生执行");
  });
  it("seals partial data on suspension and never acknowledges a late packet", () => {
    const { display, credit, node } = setup();
    display.accept(frame(1n, "{", "status", { totalLength: 2n, last: false }));
    display.suspend();
    display.accept(frame(2n, "}", "status", { recordId: 1n, byteOffset: 1n, totalLength: 2n }));
    expect(credit.mock.calls).toEqual([[STREAM, 1n]]);
    expect(node.children[0]?.textContent).toContain("旧视图");
    expect(node.children[1]?.children).toHaveLength(0);
  });
  it("propagates initial credit failure and stops accepting", () => {
    const credit = vi.fn(() => { throw new Error("synthetic-send"); });
    const { display, node } = setup(credit);
    expect(() => display.start()).toThrow("synthetic-send");
    display.accept(frame(1n, '{}'));
    expect(credit).toHaveBeenCalledOnce();
    expect(node.dataset.stale).toBe("true");
  });
});
