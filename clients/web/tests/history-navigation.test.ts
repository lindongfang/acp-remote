import { describe, expect, it, vi } from "vitest";
import { ManagementRequestError } from "../src/management/client";
import { HistoryPageController } from "../src/workbench/history-page-controller";
import { SessionViewDisplay } from "../src/management/session-view-display";
import { encodeSessionCommand, parseSessionCommand, parseSessionViewOpened, SessionProtocolError,
  type SessionViewOpened, type HistoryRequest, type HistoryPage } from "../src/management/session-protocol";
import { FakeDocument, FakeElement } from "./support/fake-dom";
import { STREAM, SEGMENT } from "./support/timeline-fixtures";

const target = { kind: "target", targetId: STREAM } as const;
const page: HistoryPage = {
  generationId: "g1", totalTurns: "25", firstTurn: "6", lastTurn: "15", liveTail: false,
  prelude: false, continuity: "same_generation", relocated: false, order: "ascending",
  turnStatus: "provable", source: "transcript_cache", olderCursor: "before", newerCursor: "after",
  turns: Array.from({ length: 10 }, (_, i) => ({ turn: String(i+6), firstPosition: String(10*i+1) })),
};
function opened(streamId = STREAM, metadata = page): SessionViewOpened {
  return { v: 1, type: "view.opened", requestId: STREAM, streamId, viewEpoch: STREAM, snapshotRevision: "100",
    capabilities: { opaqueChunks: 1 }, status: { mode: "replay_complete", freshness: "current_observation",
      projection: "available", join: "proven_native_ids", historyIncomplete: false, cacheRecoverable: true }, page: metadata };
}
const wire = (value: unknown) => JSON.parse(JSON.stringify(value));
function command(payload: unknown, action = "history.page") {
  return JSON.stringify({ v: 1, type: "command", requestId: STREAM, action, payload });
}
function setup(metadata = page) {
  const doc = new FakeDocument() as unknown as Document;
  const transport = { credit: vi.fn(), detach: vi.fn() };
  const port = { readHistoryPage: vi.fn<(_request: HistoryRequest) => Promise<SessionViewOpened>>(),
    closeSessionView: vi.fn(async () => undefined),
    detachSessionStream: vi.fn() };
  const active = { current: true };
  const controller = new HistoryPageController(doc, target, port, transport, undefined, undefined, () => active.current);
  const initial = opened(STREAM, metadata);
  controller.attach(initial, new SessionViewDisplay(doc, initial, transport));
  return { controller, port, active };
}

describe("history command and view contracts", () => {
  it("preserves legacy encoding and validates optional page and tagged modes", () => {
    const old = command({ targetId: STREAM, reason: "first_open" }, "view.open");
    expect(encodeSessionCommand(parseSessionCommand(old))).toBe(old);
    const { page: ignored, ...legacy } = opened();
    expect(ignored).toBeDefined();
    expect(parseSessionViewOpened(wire(legacy))).toEqual(legacy);
    for (const mode of [{ kind: "first" }, { kind: "continue", cursor: "abc", direction: "older" },
      { kind: "locate", fromTurn: "41", direction: "newer" }]) {
      const payload = { target, mode, turns: "50", child: "child" };
      expect(parseSessionCommand(command(payload)).payload).toEqual(payload);
    }
    for (const turns of [0, null, "", "0", "01", "-1", "1.5", "51"]) {
      expect(() => parseSessionCommand(command({ target, turns, mode: { kind: "first" } }))).toThrow();
    }
    for (const source of ["transcript_cache", "replay", "live"] as const) {
      expect(parseSessionViewOpened(wire(opened(STREAM, { ...page, source }))).page?.source).toBe(source);
    }
    for (const patch of [{ extra: "x" }, { order: "descending" }, { totalTurns: 25 }, { continuity: "certain" },
      { turns: [{ turn: "1", firstPosition: "1", extra: 1 }] }]) {
      expect(() => parseSessionViewOpened(wire({ ...opened(), page: { ...page, ...patch } }))).toThrow(SessionProtocolError);
    }
  });
});

describe("history page controller", () => {
  it.each([["older", "5"], ["newer", "16"]] as const)("relocates %s using the requested boundary", async (direction, coordinate) => {
    const { controller, port } = setup();
    controller.view.turns.value = "5";
    port.readHistoryPage.mockRejectedValueOnce(new ManagementRequestError("stale_cursor"))
      .mockResolvedValueOnce(opened(SEGMENT, { ...page, relocated: true, continuity: "unproven" }));
    await controller.navigate(direction);
    expect(port.readHistoryPage.mock.calls[1]?.[0]).toEqual({ target, turns: "5",
      mode: { kind: "locate", direction, fromTurn: coordinate } });
    expect(controller.view.notice.textContent).toContain("连续性未经证明");
  });
  it("keeps old content when the coordinate disappeared and rejects invalid sizes locally", async () => {
    const { controller, port } = setup();
    const old = (controller.view.body as unknown as FakeElement).children[0];
    for (const size of ["0", "-1", "1.5", "51"]) {
      controller.view.turns.value = size;
      await controller.navigate("older");
      expect(controller.view.notice.textContent).toContain("整数");
    }
    expect(port.readHistoryPage).not.toHaveBeenCalled();
    controller.view.turns.value = "";
    port.readHistoryPage.mockRejectedValueOnce(new ManagementRequestError("stale_cursor"))
      .mockRejectedValueOnce(new ManagementRequestError("not_found"));
    await controller.navigate("older");
    expect(port.readHistoryPage.mock.calls[0]?.[0]).not.toHaveProperty("turns");
    expect((controller.view.body as unknown as FakeElement).children[0]).toBe(old);
    expect(controller.element.dataset.stale).toBe("true");
    expect(controller.view.notice.textContent).toContain("不存在");
  });
  it.each(["superseded", "disconnect", "switch_target"])("closes a late stream after %s", async reason => {
    const { controller, port, active } = setup();
    let resolve!: (v: SessionViewOpened) => void;
    port.readHistoryPage.mockReturnValueOnce(new Promise(r => { resolve = r; }));
    const pending = controller.navigate("older");
    if (reason === "disconnect") controller.suspend();
    if (reason === "switch_target") active.current = false;
    if (reason === "superseded") {
      port.readHistoryPage.mockResolvedValueOnce(opened(SEGMENT));
      await controller.navigate("newer");
    }
    const content = controller.view.body.textContent;
    resolve(opened(STREAM));
    await pending;
    expect(port.closeSessionView).toHaveBeenCalledWith(STREAM);
    expect(controller.view.body.textContent).toBe(content);
  });
  it("reopens child first independently and scopes errors to its container", async () => {
    const { controller, port } = setup();
    const root = controller.element as unknown as FakeElement;
    const [label, toggle] = root.children.slice(-3) as FakeElement[];
    (label!.children[0] as FakeElement).value = "child";
    const parentBody = controller.view.body;
    port.readHistoryPage.mockResolvedValueOnce(opened(SEGMENT));
    toggle!.click();
    await vi.waitFor(() => expect(port.readHistoryPage).toHaveBeenCalledOnce());
    toggle!.click();
    expect(port.closeSessionView).toHaveBeenCalledWith(SEGMENT);
    port.readHistoryPage.mockRejectedValueOnce(new ManagementRequestError("invalid_target"));
    toggle!.click();
    await vi.waitFor(() => expect(port.readHistoryPage).toHaveBeenCalledTimes(2));
    expect(port.readHistoryPage.mock.calls.map(c => c[0])).toEqual(Array(2).fill({ target, child: "child", mode: { kind: "first" } }));
    expect(controller.view.body).toBe(parentBody);
    expect(controller.element.dataset.stale).toBeUndefined();
  });
  it("renders child and prelude headings in source order and marks interrupted pages", () => {
    const doc = new FakeDocument() as unknown as Document;
    for (const prelude of [false, true]) {
      const turns = Array.from({ length: prelude ? 5 : 10 }, (_, i) => ({ turn: String(i+1), firstPosition: String(i+2) }));
      const initial = opened(STREAM, { ...page, prelude, firstTurn: "1", lastTurn: String(turns.length), turns });
      const display = new SessionViewDisplay(doc, initial, { credit: vi.fn(), detach: vi.fn() });
      const root = display.element as unknown as FakeElement;
      const groups = (root.children.at(-1) as FakeElement).children;
      expect(groups.map(node => node.textContent)).toEqual([
        ...(prelude ? ["前导记录"] : []), ...turns.map(t => "第 " + t.turn + " 轮"),
      ]);
      display.suspend();
      expect(display.element.textContent).toContain("未加载部分");
    }
  });
  it("shows replay traversal and unprovable history limitations", () => {
    for (const source of ["live", "transcript_cache", "replay"] as const) {
      const { controller } = setup({ ...page, source, firstTurn: null, lastTurn: null, turns: [], turnStatus: "unprovable" });
      expect(controller.view.notice.textContent).toContain("无法证明");
      expect(controller.view.notice.textContent?.includes("遍历")).toBe(source === "replay");
      expect(((controller.element as unknown as FakeElement).children[0] as FakeElement).hidden).toBe(true);
    }
  });
});

describe("independent history component checks", () => {
  it("imports the history protocol, controller and view with no document", async () => {
    expect(typeof document).toBe("undefined");
    vi.resetModules();
    const [protocol, controller, view] = await Promise.all([
      import("../src/management/session-protocol/history"),
      import("../src/workbench/history-page-controller"),
      import("../src/management/history-page-view"),
    ]);
    expect(protocol.historyTurns("10")).toBe("10");
    expect(typeof controller.HistoryPageController).toBe("function");
    expect(typeof view.HistoryPageView).toBe("function");
  });

  it("preserves every parent turn node across a child's collapse, reopen, and failure", async () => {
    const { controller, port } = setup();
    const body = controller.view.body as unknown as FakeElement;
    const groups = () => ((body.children[0] as FakeElement).children.at(-1) as FakeElement).children;
    const original = [...groups()];
    expect(original).toHaveLength(10);
    const root = controller.element as unknown as FakeElement;
    const [label, toggle, host] = root.children.slice(-3) as FakeElement[];
    (label!.children[0] as FakeElement).value = "child";
    const childPage = { ...page, firstTurn: "1", lastTurn: "10", olderCursor: null,
      turns: Array.from({ length: 10 }, (_, i) => ({ turn: String(i + 1), firstPosition: String(i + 1) })) };
    port.readHistoryPage.mockResolvedValueOnce(opened(SEGMENT, childPage));
    toggle!.click();
    await vi.waitFor(() => expect(host!.textContent).toContain("第 1–10 轮"));
    toggle!.click();
    expect(port.closeSessionView).toHaveBeenCalledWith(SEGMENT);
    port.readHistoryPage.mockRejectedValueOnce(new ManagementRequestError("invalid_target"));
    toggle!.click();
    await vi.waitFor(() => expect(port.readHistoryPage).toHaveBeenCalledTimes(2));
    expect(host!.textContent).not.toContain("第 1–10 轮");
    expect(port.readHistoryPage.mock.calls.map(([request]) => request.mode)).toEqual([{kind: "first"}, {kind: "first"}]);
    expect(groups()).toHaveLength(original.length);
    original.forEach((node, i) => expect(groups()[i]).toBe(node));
    expect(controller.view.notice.textContent).toContain("第 6–15 轮");
  });
});
