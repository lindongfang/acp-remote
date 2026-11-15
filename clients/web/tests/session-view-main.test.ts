import { readFileSync } from "node:fs";
import { stripTypeScriptTypes } from "node:module";
import { describe, expect, it, vi } from "vitest";

const main = new TextDecoder().decode(readFileSync(new URL("../src/main.ts", import.meta.url)));
function actualFunction(name: string, nextName: string, bindings: Record<string, unknown>) {
  const start = main.indexOf(`async function ${name}(`);
  const end = main.indexOf(`\n${nextName}`, start);
  if (start < 0 || end < 0) throw new Error("Production function boundary missing");
  const source = stripTypeScriptTypes(main.slice(start, end));
  return new Function(...Object.keys(bindings), `${source}; return ${name};`)(...Object.values(bindings));
}

describe("production shared-view open wiring", () => {
  it("registers an original display and grants the original stream initial credit", async () => {
    const opened = { streamId: "AAECAwQFBgcICQoLDA0ODw" };
    const credit = vi.fn();
    const connection = { openSessionView: vi.fn(async () => opened), sendSessionViewCredit: credit,
      detachSessionStream: vi.fn(), closeSessionView: vi.fn(async () => ({})) };
    const establish = vi.fn((_owner, _opened, transport) => ({ element: {}, streamId: opened.streamId,
      start: () => transport.credit(opened.streamId, 0n), dispose: vi.fn() }));
    const bindings = {
      management: connection, managementGeneration: 1, inspectedQueueTargetId: "target",
      openedSessionView: null, sharedSessionDisplay: null, sharedViewAttempt: 0,
      sharedSessionConnection: null, sharedViewTargetId: null,
      sharedViewHost: { append: vi.fn() }, staleSharedDisplays: new Set(),
      SessionViewDisplay: class {
        constructor(...args: Parameters<typeof establish>) { return establish(...args); }
      },
      document: {}, showManagementDashboard: vi.fn(), managementErrorMessage: String,
      releaseSharedDisplay: vi.fn(), disposeSharedViews: vi.fn(),
    };
    const open = actualFunction("openSharedSessionView", "async function closeSharedSessionView", bindings);
    await open({}, "target", { disabled: true });
    expect(connection.openSessionView).toHaveBeenCalledOnce();
    expect(establish).toHaveBeenCalledOnce();
    expect(credit).toHaveBeenCalledWith(opened.streamId, 0n);
  });

  it("does not route every shared-view packet exclusively through the queued-body reader", () => {
    const start = main.indexOf("function handleSessionViewData(");
    const end = main.indexOf("\nfunction failQueueBodyRead", start);
    const branch = main.slice(start, end);
    expect(branch).toContain("sharedSessionDisplay");
    expect(branch).toContain(".accept(frame)");
  });

  it("owns and disposes shared content on real identity replacement", () => {
    const start = main.indexOf("async function resetLocalIdentity(");
    const end = main.indexOf("\nasync function ", start + 1);
    expect(main.slice(start, end < 0 ? undefined : end)).toContain("disposeSharedViews()");
  });
});

function lifecycle() {
  const source = stripTypeScriptTypes(main.slice(main.indexOf("async function openSharedSessionView("),
    main.indexOf("\nfunction sessionActionErrorMessage(")));
  const displays: Array<{ streamId: string; start: ReturnType<typeof vi.fn>;
    suspend: ReturnType<typeof vi.fn>; dispose: ReturnType<typeof vi.fn> }> = [];
  const connection = {
    openSessionView: vi.fn<(target: string, reason: string) => Promise<{ streamId: string }>>(),
    sendSessionViewCredit: vi.fn(), detachSessionStream: vi.fn(),
    closeSessionView: vi.fn(async (_stream: string) => ({ state: "closed", lastCommittedPacket: "0" })),
  };
  const dashboard = vi.fn(), append = vi.fn(), construct = vi.fn();
  const make = new Function("connection", "SessionViewDisplay", "document", "sharedViewHost",
    "showManagementDashboard", "managementErrorMessage", `
      let management = connection, managementGeneration = 1, inspectedQueueTargetId = "target";
      let sharedSessionDisplay = null, sharedSessionConnection = null, sharedViewTargetId = null;
      let openedSessionView = null, sharedViewAttempt = 0;
      const staleSharedDisplays = new Set();
      ${source}
      return { open: openSharedSessionView, close: closeSharedSessionView,
        suspend: suspendSharedView, dispose: disposeSharedViews,
        target(value) { inspectedQueueTargetId = value; },
        generation(value) { managementGeneration = value; },
        connection(value) { management = value; },
        state() { return { display: sharedSessionDisplay, opened: openedSessionView,
          target: sharedViewTargetId, stale: [...staleSharedDisplays] }; } };
    `);
  const api = make(connection, class {
    element = {};
    streamId: string;
    start: ReturnType<typeof vi.fn>;
    suspend = vi.fn();
    dispose = vi.fn();
    constructor(_owner: unknown, opened: { streamId: string }, transport: {
      credit: (id: string, sequence: bigint) => void;
    }) {
      construct();
      this.streamId = opened.streamId;
      this.start = vi.fn(() => transport.credit(opened.streamId, 0n));
      displays.push(this);
    }
  }, {}, { append }, dashboard, String);
  return { api, connection, displays, dashboard, append, construct };
}
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

describe("actual main lifecycle functions (component execution, not browser evidence)", () => {
  it("ignores an out-of-order open in the same generation and closes only its original stream", async () => {
    const { api, connection, displays } = lifecycle();
    const first = deferred<{ streamId: string }>(), second = deferred<{ streamId: string }>();
    connection.openSessionView.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
    const a = api.open({}, "target", null), b = api.open({}, "target", null);
    second.resolve({ streamId: "new" }); await b;
    first.resolve({ streamId: "old" }); await a;
    expect(displays.map((d) => d.streamId)).toEqual(["new"]);
    expect(connection.sendSessionViewCredit.mock.calls).toEqual([["new", 0n]]);
    expect(connection.detachSessionStream).toHaveBeenCalledWith("old");
    expect(connection.closeSessionView).toHaveBeenCalledWith("old");
    expect(api.state().display.streamId).toBe("new");
  });
  it("ignores open after target replacement and does not credit it", async () => {
    const { api, connection, displays } = lifecycle();
    const pending = deferred<{ streamId: string }>();
    connection.openSessionView.mockReturnValueOnce(pending.promise);
    const task = api.open({}, "target", null);
    api.target("other"); api.dispose();
    pending.resolve({ streamId: "stale" }); await task;
    expect(displays).toHaveLength(0);
    expect(connection.sendSessionViewCredit).not.toHaveBeenCalled();
    expect(connection.closeSessionView).toHaveBeenCalledWith("stale");
  });
  it("uses the original connection to release a stale response after reconnect", async () => {
    const { api, connection, displays } = lifecycle();
    const pending = deferred<{ streamId: string }>();
    connection.openSessionView.mockReturnValueOnce(pending.promise);
    const task = api.open({}, "target", null);
    api.generation(2); api.connection({});
    pending.resolve({ streamId: "old-generation" }); await task;
    expect(displays).toHaveLength(0);
    expect(connection.closeSessionView).toHaveBeenCalledWith("old-generation");
  });
  it("rolls back registration and disposes owner when initial credit fails", async () => {
    const { api, connection, displays, dashboard } = lifecycle();
    connection.openSessionView.mockResolvedValueOnce({ streamId: "failed" });
    connection.sendSessionViewCredit.mockImplementationOnce(() => { throw new Error("synthetic-send"); });
    await api.open({}, "target", null);
    expect(api.state().display).toBeNull(); expect(api.state().opened).toBeNull();
    expect(displays[0]?.dispose).toHaveBeenCalledOnce();
    expect(connection.detachSessionStream).toHaveBeenCalledWith("failed");
    expect(dashboard.mock.calls[0]?.[2]).toContain("未打开");
  });
  for (const fault of ["constructor", "append", "initial-credit"] as const) {
    it(`releases newly acquired stream and preserves original owner after ${fault} failure`, async () => {
      const { api, connection, displays, append, construct } = lifecycle();
      connection.openSessionView.mockResolvedValueOnce({ streamId: "original" });
      await api.open({}, "target", null);
      const original = api.state();
      const fail = () => { throw new Error(`synthetic-${fault}`); };
      if (fault === "constructor") construct.mockImplementationOnce(fail);
      if (fault === "append") append.mockImplementationOnce(fail);
      if (fault === "initial-credit") connection.sendSessionViewCredit.mockImplementationOnce(fail);
      connection.openSessionView.mockResolvedValueOnce({ streamId: "failed-candidate" });
      await api.open({}, "target", null);
      expect(api.state().display).toBe(original.display);
      expect(api.state().opened).toBe(original.opened);
      expect(displays[0]?.dispose).not.toHaveBeenCalled();
      expect(displays[0]?.suspend).not.toHaveBeenCalled();
      expect(connection.detachSessionStream.mock.calls).toEqual([["failed-candidate"]]);
      expect(connection.closeSessionView.mock.calls).toEqual([["failed-candidate"]]);
      if (fault !== "constructor") expect(displays[1]?.dispose).toHaveBeenCalledOnce();
      if (fault !== "initial-credit") {
        expect(connection.sendSessionViewCredit.mock.calls).toEqual([["original", 0n]]);
      }
    });
  }
  it("releases local display immediately even if remote close rejects", async () => {
    const { api, connection, displays, dashboard } = lifecycle();
    connection.openSessionView.mockResolvedValueOnce({ streamId: "one" });
    await api.open({}, "target", null);
    const close = deferred<{ state: string; lastCommittedPacket: string }>();
    connection.closeSessionView.mockReturnValueOnce(close.promise);
    const task = api.close({}, api.state().opened, { disabled: true });
    expect(displays[0]?.dispose).toHaveBeenCalledOnce();
    expect(api.state().display).toBeNull();
    close.reject(new Error("synthetic-close")); await task;
    expect(dashboard.mock.calls.at(-1)?.[2]).toContain("关闭未完成");
  });
  it("does not clear the new display when old close returns late", async () => {
    const { api, connection, displays, dashboard } = lifecycle();
    connection.openSessionView.mockResolvedValueOnce({ streamId: "old" });
    await api.open({}, "target", null);
    const close = deferred<{ state: string; lastCommittedPacket: string }>();
    connection.closeSessionView.mockReturnValueOnce(close.promise);
    const task = api.close({}, api.state().opened, { disabled: true });
    connection.openSessionView.mockResolvedValueOnce({ streamId: "new" });
    await api.open({}, "target", null);
    const rendered = dashboard.mock.calls.length;
    close.resolve({ state: "closed", lastCommittedPacket: "1" }); await task;
    expect(api.state().display.streamId).toBe("new");
    expect(displays[1]?.dispose).not.toHaveBeenCalled();
    expect(dashboard.mock.calls).toHaveLength(rendered);
  });
  it("preserves a stale owner through read-only reconnect without reusing its assembler", async () => {
    const { api, connection, displays } = lifecycle();
    connection.openSessionView.mockResolvedValueOnce({ streamId: "before" });
    await api.open({}, "target", null);
    api.suspend();
    expect(displays[0]?.suspend).toHaveBeenCalledOnce();
    expect(displays[0]?.dispose).not.toHaveBeenCalled();
    connection.openSessionView.mockResolvedValueOnce({ streamId: "after" });
    await api.open({}, "target", null, "reconnect");
    expect(connection.openSessionView.mock.calls.at(-1)).toEqual(["target", "reconnect"]);
    expect(api.state().display).toBe(displays[1]);
    expect(api.state().stale).toEqual([displays[0]]);
    api.dispose();
    expect(displays[0]?.dispose).toHaveBeenCalledOnce();
    expect(displays[1]?.dispose).toHaveBeenCalledOnce();
    expect(api.state().target).toBeNull();
  });
});

function bodyLifecycle() {
  const source = stripTypeScriptTypes(main.slice(main.indexOf("async function openQueueBody("),
    main.indexOf("\nfunction handleSessionViewData(")));
  const connection = { openSessionBody: vi.fn<(...args: unknown[]) => Promise<{
    streamId: string; totalBytes: string;
  }>>(), detachSessionStream: vi.fn(), sendSessionViewCredit: vi.fn() };
  const timers = { setTimeout: vi.fn(() => 1), clearTimeout: vi.fn() }, dashboard = vi.fn();
  const make = new Function("connection", "QueuedBodyReader", "QueuedBodyReadError", "globalThis",
    "showManagementDashboard", "managementErrorMessage", "failQueueBodyRead", `
      let management = connection, managementGeneration = 1, inspectedQueueTargetId = "A";
      let activeBodyRead = null, bodyPreview = null, bodyReadTimer = null, bodyReadAttempt = 0;
      ${source}
      return { open: openQueueBody, reset(value) { inspectedQueueTargetId = value;
        ++bodyReadAttempt; activeBodyRead?.reader.seal(); activeBodyRead = null; },
        active() { return activeBodyRead; } };
    `);
  const api = make(connection, class { seal = vi.fn(); }, class extends Error {}, timers,
    dashboard, String, vi.fn());
  return { api, connection, dashboard };
}
describe("actual queued-body open fencing during view target changes", () => {
  it("detaches an old target descriptor without registering or crediting it", async () => {
    const { api, connection, dashboard } = bodyLifecycle();
    const pending = deferred<{ streamId: string; totalBytes: string }>();
    connection.openSessionBody.mockReturnValueOnce(pending.promise);
    const task = api.open({}, "A", { itemId: "item", revision: "1" }, { disabled: true });
    api.reset("B"); pending.resolve({ streamId: "old-A", totalBytes: "2" }); await task;
    expect(api.active()).toBeNull();
    expect(connection.sendSessionViewCredit).not.toHaveBeenCalled();
    expect(connection.detachSessionStream).toHaveBeenCalledExactlyOnceWith("old-A");
    expect(dashboard).not.toHaveBeenCalled();
  });
  it("keeps only latest body-open in the same target when responses arrive in reverse order", async () => {
    const { api, connection } = bodyLifecycle();
    const first = deferred<{ streamId: string; totalBytes: string }>();
    const second = deferred<{ streamId: string; totalBytes: string }>();
    connection.openSessionBody.mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
    const a = api.open({}, "A", { itemId: "one", revision: "1" }, { disabled: true });
    const b = api.open({}, "A", { itemId: "two", revision: "1" }, { disabled: true });
    second.resolve({ streamId: "second", totalBytes: "2" }); await b;
    first.resolve({ streamId: "first", totalBytes: "2" }); await a;
    expect(api.active().streamId).toBe("second");
    expect(connection.sendSessionViewCredit.mock.calls).toEqual([["second", 0n]]);
    expect(connection.detachSessionStream).toHaveBeenCalledExactlyOnceWith("first");
  });
});
