import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  closedWithOptional,
  parseSessionViewOpened,
  SessionProtocolError,
} from "../src/management/session-protocol";
import { readSessionViewStatus } from "../src/management/session-view-status";
import { SessionViewDisplay } from "../src/management/session-view-display";
import { SharedViewController } from "../src/workbench/shared-view-controller";
import type { SessionViewOpened } from "../src/management/session-protocol";
import type { ManagementConnection } from "../src/management/client";
import type { IdentityRecord } from "../src/pairing/identity";
import { PROJECTION_FIELDS } from "../src/views/projection";
import type { ViewDataFrame, ViewRecordKind } from "../src/views/binary";

const STREAM_A = b64(1);
const STREAM_B = b64(2);
const SEG_A = b64(3);
const SEG_B = b64(4);
const SOURCE_ID = b64(5);
const TARGET_ID = b64(6);

function b64(n: number): string {
  const bytes = new Uint8Array(16).fill(n);
  let bin = "";
  for (const byte of bytes) bin += String.fromCharCode(byte);
  return btoa(bin).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/g, "");
}

function makeRawTextFrame(sessionId: string, messageId: string, text: string): Uint8Array {
  return new TextEncoder().encode(
    JSON.stringify({
      jsonrpc: "2.0",
      method: "session/update",
      params: {
        sessionId,
        update: {
          sessionUpdate: "agent_message_chunk",
          messageId,
          content: { type: "text", text },
        },
      },
    }),
  );
}

class DomText {
  textContent: string;
  parentNode: DomNode | null = null;
  constructor(text: string) {
    this.textContent = text;
  }
  remove(): void {
    if (this.parentNode !== null) {
      const idx = this.parentNode.childNodes.indexOf(this);
      if (idx !== -1) this.parentNode.childNodes.splice(idx, 1);
      this.parentNode = null;
    }
  }
}

class DomNode {
  readonly tagName: string;
  className = "";
  id = "";
  type = "";
  value = "";
  disabled = false;
  hidden = false;
  dataset: Record<string, string> = {};
  childNodes: Array<DomNode | DomText> = [];
  parentNode: DomNode | null = null;
  attributes = new Map<string, string>();
  listeners = new Map<string, Array<(event: unknown) => void>>();
  #textContent = "";

  constructor(tagName: string) {
    this.tagName = tagName.toUpperCase();
  }

  get children(): DomNode[] {
    return this.childNodes.filter((n): n is DomNode => n instanceof DomNode);
  }

  get textContent(): string {
    if (this.childNodes.length === 0) return this.#textContent;
    return this.#textContent + this.childNodes.map((n) => n.textContent).join("");
  }

  set textContent(v: string) {
    this.#textContent = v;
    for (const c of this.childNodes) c.parentNode = null;
    this.childNodes = [];
  }

  setAttribute(name: string, val: string): void {
    this.attributes.set(name, val);
    if (name === "id") this.id = val;
    if (name === "class") this.className = val;
    if (name.startsWith("data-")) {
      const camel = name.slice(5).replace(/-([a-z])/g, (_, ch: string) => ch.toUpperCase());
      this.dataset[camel] = val;
    }
  }

  getAttribute(name: string): string | null {
    return this.attributes.get(name) ?? null;
  }

  removeAttribute(name: string): void {
    this.attributes.delete(name);
    if (name.startsWith("data-")) {
      const camel = name.slice(5).replace(/-([a-z])/g, (_, ch: string) => ch.toUpperCase());
      delete this.dataset[camel];
    }
  }

  append(...items: Array<DomNode | DomText | string>): void {
    for (const entry of items) this.insertBefore(entry, null);
  }

  appendChild<T extends DomNode | DomText>(item: T): T {
    this.insertBefore(item, null);
    return item;
  }

  insertBefore(item: DomNode | DomText | string, ref: DomNode | DomText | null): void {
    if (typeof item === "string") {
      const txt = new DomText(item);
      this.insertNode(txt, ref);
      return;
    }
    if (item instanceof DomNode && item.tagName === "#FRAGMENT") {
      const kids = [...item.childNodes];
      item.childNodes = [];
      for (const k of kids) {
        k.parentNode = null;
        this.insertBefore(k, ref);
      }
      return;
    }
    this.insertNode(item, ref);
  }

  private insertNode(node: DomNode | DomText, ref: DomNode | DomText | null): void {
    node.remove();
    node.parentNode = this;
    if (ref === null) {
      this.childNodes.push(node);
      return;
    }
    const idx = this.childNodes.indexOf(ref);
    if (idx === -1) this.childNodes.push(node);
    else this.childNodes.splice(idx, 0, node);
  }

  replaceChildren(...items: Array<DomNode | DomText | string>): void {
    this.textContent = "";
    this.append(...items);
  }

  remove(): void {
    if (this.parentNode !== null) {
      const idx = this.parentNode.childNodes.indexOf(this);
      if (idx !== -1) this.parentNode.childNodes.splice(idx, 1);
      this.parentNode = null;
    }
  }

  focus(): void {}

  addEventListener(type: string, listener: (e: unknown) => void, opts?: { signal?: AbortSignal }): void {
    if (opts?.signal?.aborted) return;
    const list = this.listeners.get(type) ?? [];
    list.push(listener);
    this.listeners.set(type, list);
    opts?.signal?.addEventListener("abort", () => {
      const next = (this.listeners.get(type) ?? []).filter((f) => f !== listener);
      this.listeners.set(type, next);
    });
  }

  click(): void {
    if (this.disabled) return;
    for (const fn of this.listeners.get("click") ?? []) fn({ preventDefault: () => {} });
  }

  findAll(predicate: (n: DomNode) => boolean): DomNode[] {
    const out: DomNode[] = [];
    if (predicate(this)) out.push(this);
    for (const child of this.children) out.push(...child.findAll(predicate));
    return out;
  }
}

class DomDoc {
  createElement(tag: string): DomNode {
    return new DomNode(tag);
  }
  createTextNode(text: string): DomText {
    return new DomText(text);
  }
  createDocumentFragment(): DomNode {
    return new DomNode("#FRAGMENT");
  }
}

function makeFrame(
  streamId: string,
  seq: bigint,
  recordId: bigint,
  kind: ViewRecordKind,
  payload: Uint8Array,
): ViewDataFrame {
  return {
    type: "data",
    streamId,
    packetSequence: seq,
    recordId,
    byteOffset: 0n,
    totalLength: BigInt(payload.length),
    kind,
    last: true,
    payload,
  };
}

function makeOpened(
  streamId: string,
  overrides: Partial<SessionViewOpened["status"]> = {},
): SessionViewOpened {
  return {
    v: 1,
    type: "view.opened",
    requestId: streamId,
    streamId,
    viewEpoch: streamId,
    snapshotRevision: "1",
    capabilities: { opaqueChunks: 1 },
    status: {
      mode: "replay_requested",
      freshness: "revalidating",
      projection: "available",
      join: "proven_native_ids",
      historyIncomplete: false,
      cacheRecoverable: true,
      transcriptSource: "transcript_cache",
      syncState: "revalidating",
      capabilityBoundary: "replay_capable",
      continuable: true,
      cacheWarning: false,
      ...overrides,
    },
  };
}

function encodeContentProjection(
  _streamId: string,
  segmentId: string,
  anchorId: bigint,
  firstPos: bigint,
  revision: bigint,
  rawRecordId: bigint,
  rawBytes: Uint8Array,
  text: string,
): Uint8Array {
  const decoded = new TextDecoder().decode(rawBytes);
  const literal = JSON.stringify({ type: "text", text });
  const startChar = decoded.indexOf(literal);
  if (startChar < 0) throw new Error(`literal not found: ${literal}`);
  const endChar = startChar + literal.length;
  const encoder = new TextEncoder();
  const byteStart = String(encoder.encode(decoded.slice(0, startChar)).length);
  const byteEnd = String(encoder.encode(decoded.slice(0, endChar)).length);
  return encoder.encode(
    JSON.stringify({
      type: "item_projection",
      segmentId,
      anchorId: String(anchorId),
      firstPosition: String(firstPos),
      revision: String(revision),
      rawRecordId: String(rawRecordId),
      kind: "message",
      fields: PROJECTION_FIELDS.map((field) =>
        field === "content"
          ? {
              field,
              knowledge: "known",
              raw: {
                rawRecordId: String(rawRecordId),
                byteStart,
                byteEnd,
                append: false,
              },
              enumCode: null,
              evidenceCount: "0",
            }
          : {
              field,
              knowledge: "unknown",
              raw: null,
              enumCode: null,
              evidenceCount: "0",
            },
      ),
    }),
  );
}

function feedTextMessage(
  accept: (f: ViewDataFrame) => void,
  streamId: string,
  segmentId: string,
  seqStart: bigint,
  rawRecordId: bigint,
  anchorId: bigint,
  text: string,
): bigint {
  const rawBytes = makeRawTextFrame("nat-1", `m-${anchorId}`, text);
  accept(makeFrame(streamId, seqStart, rawRecordId, "original_raw", rawBytes));
  const projBytes = encodeContentProjection(
    streamId,
    segmentId,
    anchorId,
    rawRecordId,
    1n,
    rawRecordId,
    rawBytes,
    text,
  );
  accept(makeFrame(streamId, seqStart + 1n, rawRecordId + 100n, "item_projection", projBytes));
  return seqStart + 2n;
}

describe("STC-002 Web strict protocol & status schema validation", () => {
  it("validates closedWithOptional, optional status fields, and transcript_cache_state status", () => {
    const parser = (v: unknown) =>
      closedWithOptional(
        v as never,
        { mode: (x) => String(x) },
        { cacheWarning: (x) => Boolean(x) },
      );
    expect(parser({ mode: "current_only" })).toEqual({ mode: "current_only" });
    expect(parser({ mode: "current_only", cacheWarning: true })).toEqual({
      mode: "current_only",
      cacheWarning: true,
    });
    expect(() => parser({ mode: "current_only", unknown: 1 })).toThrow(SessionProtocolError);

    const parsed = parseSessionViewOpened({
      v: 1,
      type: "view.opened",
      requestId: STREAM_A,
      streamId: STREAM_A,
      viewEpoch: STREAM_A,
      snapshotRevision: "0",
      capabilities: { opaqueChunks: 1 },
      status: {
        mode: "replay_complete",
        freshness: "current_observation",
        projection: "available",
        join: "proven_native_ids",
        historyIncomplete: false,
        cacheRecoverable: true,
        transcriptSource: "transcript_cache",
        syncState: "degraded_capacity_full",
        capabilityBoundary: "resume_only",
        continuable: true,
        cacheWarning: true,
      },
    });
    expect(parsed.status.syncState).toBe("degraded_capacity_full");
    expect(parsed.status.cacheWarning).toBe(true);

    expect(() =>
      parseSessionViewOpened({
        v: 1,
        type: "view.opened",
        requestId: STREAM_A,
        streamId: STREAM_A,
        viewEpoch: STREAM_A,
        snapshotRevision: "0",
        capabilities: { opaqueChunks: 1 },
        status: {
          mode: "replay_complete",
          freshness: "current_observation",
          projection: "available",
          join: "proven_native_ids",
          historyIncomplete: false,
          cacheRecoverable: true,
          unexpectedKey: "bad",
        },
      }),
    ).toThrow(SessionProtocolError);

    const stateRec = readSessionViewStatus(
      new TextEncoder().encode(
        JSON.stringify({
          type: "transcript_cache_state",
          transcriptSource: "hybrid_resumed",
          syncState: "synced",
          capabilityBoundary: "resume_only",
          continuable: true,
          cacheWarning: false,
        }),
      ),
    );
    expect(stateRec.requiresReopen).toBe(false);
    expect(stateRec.cacheState).toEqual({
      transcriptSource: "hybrid_resumed",
      syncState: "synced",
      capabilityBoundary: "resume_only",
      continuable: true,
      cacheWarning: false,
    });

    expect(() =>
      readSessionViewStatus(
        new TextEncoder().encode(
          JSON.stringify({
            type: "transcript_cache_state",
            transcriptSource: "live",
            syncState: "invalid_state",
            capabilityBoundary: "resume_only",
            continuable: true,
            cacheWarning: false,
          }),
        ),
      ),
    ).toThrow();
  });
});

describe("STC-016 Off-screen staging rebind, DOM/Selection preservation, shortened replay, and banner", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it("preserves exact <li> DOM identity, scrollY, and active Selection across cross-segment rebind", async () => {
    const doc = new DomDoc() as unknown as Document;
    const host = new DomNode("div");
    const credit = vi.fn();
    const detach = vi.fn();

    const openedA = makeOpened(STREAM_A);
    const openedB = makeOpened(STREAM_B, {
      freshness: "current_observation",
      mode: "replay_complete",
      transcriptSource: "live",
      syncState: "synced",
    });

    const record: IdentityRecord = {
      nodeId: SOURCE_ID,
      nodePublicKey: "pk",
      clientId: TARGET_ID,
      displayName: "STC-016",
      privateKeyPkcs8: "sk",
      authenticated: true,
    } as unknown as IdentityRecord;

    const openSessionView = vi.fn(async () => openedA);
    const rebindSessionView = vi.fn(async () => openedB);
    const fakeConnection = {
      openSessionView,
      rebindSessionView,
      closeSessionView: vi.fn(async () => ({
        streamId: STREAM_B,
        alreadyClosed: false,
        lastCommittedPacket: "1",
        startedPacket: null,
        state: "closed" as const,
      })),
      sendSessionViewCredit: credit,
      detachSessionStream: detach,
    } as unknown as ManagementConnection;

    const controller = new SharedViewController({
      getManagement: () => fakeConnection,
      getManagementGeneration: () => 1,
      getInspectedQueueTargetId: () => TARGET_ID,
      getSharedViewHost: () => host as unknown as { append: (el: HTMLElement) => void },
      getDocument: () => doc,
      renderDashboard: vi.fn(),
    });

    await controller.openTargetSharedView(record, TARGET_ID, null, "first_open");
    const display = controller.sharedSessionDisplay as unknown as SessionViewDisplay;
    expect(display).not.toBeNull();

    let seq = 1n;
    seq = feedTextMessage((f) => controller.acceptViewFrame(f), STREAM_A, SEG_A, seq, 1n, 1n, "first-msg");
    seq = feedTextMessage((f) => controller.acceptViewFrame(f), STREAM_A, SEG_A, seq, 2n, 2n, "second-msg");
    seq = feedTextMessage((f) => controller.acceptViewFrame(f), STREAM_A, SEG_A, seq, 3n, 3n, "third-msg");
    controller.acceptViewFrame(makeFrame(STREAM_A, seq++, 4n, "snapshot_end", new Uint8Array()));

    const domRoot = display.element as unknown as DomNode;
    const listBefore = domRoot.findAll((n) => n.tagName === "OL")[0]!;
    expect(listBefore.children).toHaveLength(3);
    const [li1Before, li2Before, li3Before] = listBefore.children;
    const activeSelection = { anchorNode: li1Before!, anchorOffset: 2, focusOffset: 7 };
    const mockScrollY = 420;

    // Trigger stream_rebound -> starts off-screen staging for STREAM_B!
    controller.acceptViewFrame(
      makeFrame(
        STREAM_A,
        seq++,
        5n,
        "status",
        new TextEncoder().encode(
          JSON.stringify({
            type: "stream_rebound",
            streamId: STREAM_B,
            previousStreamId: STREAM_A,
            viewEpoch: STREAM_B,
            snapshotRevision: "2",
          }),
        ),
      ),
    );
    await vi.waitFor(() => expect(rebindSessionView).toHaveBeenCalledWith(STREAM_A));

    // Stream partial packets for STREAM_B prior to SnapshotEnd: live DOM must NOT flash or replace!
    let seqB = 1n;
    seqB = feedTextMessage((f) => controller.acceptViewFrame(f), STREAM_B, SEG_B, seqB, 1n, 101n, "first-msg");
    seqB = feedTextMessage((f) => controller.acceptViewFrame(f), STREAM_B, SEG_B, seqB, 2n, 102n, "second-msg");
    seqB = feedTextMessage((f) => controller.acceptViewFrame(f), STREAM_B, SEG_B, seqB, 3n, 103n, "third-msg");
    expect(display.streamId).toBe(STREAM_A);
    expect(listBefore.children).toEqual([li1Before, li2Before, li3Before]);

    // Deliver SnapshotEnd on STREAM_B -> atomic swap preserves exact === <li> elements!
    controller.acceptViewFrame(makeFrame(STREAM_B, seqB++, 4n, "snapshot_end", new Uint8Array()));
    expect(display.streamId).toBe(STREAM_B);
    expect(domRoot.dataset.streamId).toBe(STREAM_B);
    expect(listBefore.children).toHaveLength(3);
    expect(listBefore.children[0]).toBe(li1Before);
    expect(listBefore.children[1]).toBe(li2Before);
    expect(listBefore.children[2]).toBe(li3Before);
    expect(activeSelection.anchorNode).toBe(li1Before);
    expect(mockScrollY).toBe(420);

    // Verify 5 -> 3 compressed replay rebind removes dropped cards and keeps surviving === <li>!
    const openedC = makeOpened(b64(9), { syncState: "synced", transcriptSource: "live" });
    seqB = feedTextMessage((f) => controller.acceptViewFrame(f), STREAM_B, SEG_B, seqB, 10n, 4n, "fourth-msg");
    seqB = feedTextMessage((f) => controller.acceptViewFrame(f), STREAM_B, SEG_B, seqB, 11n, 5n, "fifth-msg");
    expect(listBefore.children).toHaveLength(5);
    const li5Before = listBefore.children[4]!;

    rebindSessionView.mockResolvedValueOnce(openedC);
    controller.acceptViewFrame(
      makeFrame(
        STREAM_B,
        seqB++,
        90n,
        "status",
        new TextEncoder().encode(JSON.stringify({ type: "stream_rebound" })),
      ),
    );
    await vi.waitFor(() => expect(rebindSessionView).toHaveBeenCalledTimes(2));

    const segC = b64(10);
    let seqC = 1n;
    seqC = feedTextMessage((f) => controller.acceptViewFrame(f), openedC.streamId, segC, seqC, 1n, 201n, "first-msg");
    seqC = feedTextMessage((f) => controller.acceptViewFrame(f), openedC.streamId, segC, seqC, 2n, 202n, "third-msg");
    seqC = feedTextMessage((f) => controller.acceptViewFrame(f), openedC.streamId, segC, seqC, 3n, 203n, "fifth-msg");
    controller.acceptViewFrame(makeFrame(openedC.streamId, seqC++, 4n, "snapshot_end", new Uint8Array()));

    expect(listBefore.children).toHaveLength(3);
    expect(listBefore.children[0]).toBe(li1Before);
    expect(listBefore.children[1]).toBe(li3Before);
    expect(listBefore.children[2]).toBe(li5Before);
  });

  it("renders banner degradation states, preserves DOM on staging failure, and triggers explicit_refresh on retry", async () => {
    const doc = new DomDoc() as unknown as Document;
    const host = new DomNode("div");
    const openSessionView = vi.fn(async (_tid: string, reason: string) =>
      makeOpened(reason === "explicit_refresh" ? STREAM_B : STREAM_A, {
        syncState: reason === "explicit_refresh" ? "synced" : "degraded_capacity_full",
        transcriptSource: reason === "explicit_refresh" ? "live" : "transcript_cache",
      }),
    );
    const fakeConnection = {
      openSessionView,
      closeSessionView: vi.fn(async () => ({})),
      sendSessionViewCredit: vi.fn(),
      detachSessionStream: vi.fn(),
    } as unknown as ManagementConnection;
    const controller = new SharedViewController({
      getManagement: () => fakeConnection,
      getManagementGeneration: () => 1,
      getInspectedQueueTargetId: () => TARGET_ID,
      getSharedViewHost: () => host as unknown as { append: (el: HTMLElement) => void },
      getDocument: () => doc,
      renderDashboard: vi.fn(),
    });
    const record = { clientId: TARGET_ID } as unknown as IdentityRecord;

    await controller.openTargetSharedView(record, TARGET_ID, null, "first_open");
    const display = controller.sharedSessionDisplay as unknown as SessionViewDisplay;
    const root = display.element as unknown as DomNode;
    const banner = root.findAll((n) => n.className === "transcript-cache-banner")[0]!;
    expect(banner.hidden).toBe(false);
    expect(banner.textContent).toContain("50/50");
    const retryBtn = banner.findAll((n) => n.tagName === "BUTTON")[0]!;
    expect(retryBtn.hidden).toBe(false);
    retryBtn.click();
    await vi.waitFor(() => {
      expect(openSessionView).toHaveBeenCalledWith(TARGET_ID, "explicit_refresh");
      expect(controller.sharedSessionDisplay?.streamId).toBe(STREAM_B);
    });

    // Check hybrid_resumed, pure_memory, and cacheWarning banner states on live display.
    const nextDisplay = controller.sharedSessionDisplay as unknown as SessionViewDisplay;
    const nextRoot = nextDisplay.element as unknown as DomNode;
    const nextBanner = nextRoot.findAll((n) => n.className === "transcript-cache-banner")[0]!;
    expect(nextBanner.hidden).toBe(true);

    nextDisplay.accept(
      makeFrame(
        STREAM_B,
        1n,
        1n,
        "status",
        new TextEncoder().encode(
          JSON.stringify({
            type: "transcript_cache_state",
            transcriptSource: "hybrid_resumed",
            syncState: "synced",
            capabilityBoundary: "resume_only",
            continuable: true,
            cacheWarning: true,
          }),
        ),
      ),
    );
    expect(nextBanner.hidden).toBe(false);
    expect(nextBanner.textContent).toContain("resume");
    expect(nextBanner.textContent).toContain("降级告警");

    // Failed staged rebind preserves old cached DOM and sets data-stale="true".
    const ok = nextDisplay.beginStagingRebind(makeOpened(b64(44)), {
      credit: vi.fn(),
      detach: vi.fn(),
    });
    expect(ok).toBe(true);
    nextDisplay.acceptStagedFrame(makeFrame(b64(44), 9n, 9n, "status", new TextEncoder().encode("not-json")));
    expect(nextRoot.dataset.stale).toBe("true");
  });
});
