import { describe, expect, it, vi } from "vitest";
import type { ManagementConnection } from "../src/management/client";
import type { SessionViewOpened } from "../src/management/session-protocol";
import {
  QueuedBodyController,
  type QueuedBodyReaderLike,
} from "../src/workbench/queued-body-controller";
import {
  SharedViewController,
  type SharedViewDisplayLike,
} from "../src/workbench/shared-view-controller";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}

function createSharedViewHarness() {
  const displays: SharedViewDisplayLike[] = [];
  const connection = {
    openSessionView: vi.fn<
      (target: string, reason: string) => Promise<{ streamId: string }>
    >(),
    sendSessionViewCredit: vi.fn(),
    detachSessionStream: vi.fn(),
    closeSessionView: vi.fn(async (_stream: string) => ({
      state: "closed",
      lastCommittedPacket: "0",
    })),
  };
  const dashboard = vi.fn();
  const append = vi.fn();
  const construct = vi.fn();

  let currentManagement: ManagementConnection | null =
    connection as unknown as ManagementConnection;
  let currentGeneration = 1;
  let currentQueueTargetId: string | null = "target";

  const controller = new SharedViewController({
    getManagement: () => currentManagement,
    getManagementGeneration: () => currentGeneration,
    getInspectedQueueTargetId: () => currentQueueTargetId,
    getSharedViewHost: () => ({ append }),
    renderDashboard: dashboard,
    displayFactory: (_doc, opened, transport) => {
      construct();
      const d: SharedViewDisplayLike = {
        element: { dataset: {}, removeAttribute: vi.fn() },
        streamId: opened.streamId,
        start: vi.fn(() => transport.credit(opened.streamId, 0n)),
        suspend: vi.fn(),
        dispose: vi.fn(),
      };
      displays.push(d);
      return d;
    },
    formatError: String,
  });

  return {
    controller,
    connection,
    displays,
    dashboard,
    append,
    construct,
    setTarget: (val: string | null) => {
      currentQueueTargetId = val;
    },
    setGeneration: (val: number) => {
      currentGeneration = val;
    },
    setConnection: (val: ManagementConnection | null) => {
      currentManagement = val;
    },
  };
}

describe("production shared-view open wiring", () => {
  it("registers an original display and grants the original stream initial credit", async () => {
    const opened = {
      streamId: "AAECAwQFBgcICQoLDA0ODw",
    } as unknown as SessionViewOpened;
    const credit = vi.fn();
    const connection = {
      openSessionView: vi.fn(async () => opened),
      sendSessionViewCredit: credit,
      detachSessionStream: vi.fn(),
      closeSessionView: vi.fn(async () => ({})),
    };
    const establish = vi.fn((_doc, _opened, transport) => ({
      element: { dataset: {}, removeAttribute: vi.fn() },
      streamId: opened.streamId,
      start: () => transport.credit(opened.streamId, 0n),
      suspend: vi.fn(),
      dispose: vi.fn(),
    }));

    const controller = new SharedViewController({
      getManagement: () => connection as unknown as ManagementConnection,
      getManagementGeneration: () => 1,
      getInspectedQueueTargetId: () => "target",
      getSharedViewHost: () => ({ append: vi.fn() }),
      renderDashboard: vi.fn(),
      displayFactory: establish,
      formatError: String,
    });

    await controller.openSharedSessionView(
      {} as never,
      "target",
      { disabled: true } as HTMLButtonElement,
    );
    expect(connection.openSessionView).toHaveBeenCalledOnce();
    expect(establish).toHaveBeenCalledOnce();
    expect(credit).toHaveBeenCalledWith(opened.streamId, 0n);
  });

  it("does not route every shared-view packet exclusively through the queued-body reader", async () => {
    const harness = createSharedViewHarness();
    const acceptMock = vi.fn();
    harness.connection.openSessionView.mockResolvedValueOnce({
      streamId: "stream-1",
    });
    await harness.controller.openSharedSessionView(
      {} as never,
      "target",
      null,
    );
    const mockDisplay = harness.displays[0]!;
    mockDisplay.accept = acceptMock;

    const frame = {
      type: "data" as const,
      streamId: mockDisplay.streamId,
      packetSequence: 1n,
      recordId: 1n,
      byteOffset: 0n,
      totalLength: 10n,
      kind: "item_projection" as const,
      last: false,
      payload: new Uint8Array([1, 2, 3]),
    };

    const handled = harness.controller.acceptViewFrame(frame);
    expect(handled).toBe(true);
    expect(acceptMock).toHaveBeenCalledWith(frame);

    const otherFrame = { ...frame, streamId: "other-stream" };
    expect(harness.controller.acceptViewFrame(otherFrame)).toBe(false);
  });

  it("owns and disposes shared content on disposeSharedViews", async () => {
    const harness = createSharedViewHarness();
    harness.connection.openSessionView.mockResolvedValueOnce({
      streamId: "stream-1",
    });
    await harness.controller.openSharedSessionView(
      {} as never,
      "target",
      null,
    );
    const mockDisplay = harness.displays[0]!;

    harness.controller.disposeSharedViews();
    expect(mockDisplay.dispose).toHaveBeenCalledOnce();
    expect(harness.connection.closeSessionView).toHaveBeenCalledWith("stream-1");
    expect(harness.controller.sharedSessionDisplay).toBeNull();
  });
});

describe("actual main lifecycle functions (component execution, not browser evidence)", () => {
  it("ignores an out-of-order open in the same generation and closes only its original stream", async () => {
    const harness = createSharedViewHarness();
    const first = deferred<{ streamId: string }>();
    const second = deferred<{ streamId: string }>();
    harness.connection.openSessionView
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise);

    const a = harness.controller.openSharedSessionView(
      {} as never,
      "target",
      null,
    );
    const b = harness.controller.openSharedSessionView(
      {} as never,
      "target",
      null,
    );

    second.resolve({ streamId: "new" });
    await b;
    first.resolve({ streamId: "old" });
    await a;

    expect(harness.displays.map((d) => d.streamId)).toEqual(["new"]);
    expect(harness.connection.sendSessionViewCredit.mock.calls).toEqual([
      ["new", 0n],
    ]);
    expect(harness.connection.detachSessionStream).toHaveBeenCalledWith("old");
    expect(harness.connection.closeSessionView).toHaveBeenCalledWith("old");
    expect(harness.controller.sharedSessionDisplay?.streamId).toBe("new");
  });

  it("ignores open after target replacement and does not credit it", async () => {
    const harness = createSharedViewHarness();
    const pending = deferred<{ streamId: string }>();
    harness.connection.openSessionView.mockReturnValueOnce(pending.promise);

    const task = harness.controller.openSharedSessionView(
      {} as never,
      "target",
      null,
    );
    harness.setTarget("other");
    harness.controller.disposeSharedViews();
    pending.resolve({ streamId: "stale" });
    await task;

    expect(harness.displays).toHaveLength(0);
    expect(harness.connection.sendSessionViewCredit).not.toHaveBeenCalled();
    expect(harness.connection.closeSessionView).toHaveBeenCalledWith("stale");
  });

  it("uses the original connection to release a stale response after reconnect", async () => {
    const harness = createSharedViewHarness();
    const pending = deferred<{ streamId: string }>();
    harness.connection.openSessionView.mockReturnValueOnce(pending.promise);

    const task = harness.controller.openSharedSessionView(
      {} as never,
      "target",
      null,
    );
    harness.setGeneration(2);
    harness.setConnection({} as unknown as ManagementConnection);
    pending.resolve({ streamId: "old-generation" });
    await task;

    expect(harness.displays).toHaveLength(0);
    expect(harness.connection.closeSessionView).toHaveBeenCalledWith(
      "old-generation",
    );
  });

  it("rolls back registration and disposes owner when initial credit fails", async () => {
    const harness = createSharedViewHarness();
    harness.connection.openSessionView.mockResolvedValueOnce({
      streamId: "failed",
    });
    harness.connection.sendSessionViewCredit.mockImplementationOnce(() => {
      throw new Error("synthetic-send");
    });

    await harness.controller.openSharedSessionView(
      {} as never,
      "target",
      null,
    );
    expect(harness.controller.sharedSessionDisplay).toBeNull();
    expect(harness.controller.openedSessionView).toBeNull();
    expect(harness.displays[0]?.dispose).toHaveBeenCalledOnce();
    expect(harness.connection.detachSessionStream).toHaveBeenCalledWith(
      "failed",
    );
    expect(harness.dashboard.mock.calls[0]?.[2]).toContain("未打开");
  });

  for (const fault of ["constructor", "append", "initial-credit"] as const) {
    it(`releases newly acquired stream and preserves original owner after ${fault} failure`, async () => {
      const harness = createSharedViewHarness();
      harness.connection.openSessionView.mockResolvedValueOnce({
        streamId: "original",
      });
      await harness.controller.openSharedSessionView(
        {} as never,
        "target",
        null,
      );
      const originalDisplay = harness.controller.sharedSessionDisplay;
      const originalOpened = harness.controller.openedSessionView;

      const fail = () => {
        throw new Error(`synthetic-${fault}`);
      };
      if (fault === "constructor") harness.construct.mockImplementationOnce(fail);
      if (fault === "append") harness.append.mockImplementationOnce(fail);
      if (fault === "initial-credit") {
        harness.connection.sendSessionViewCredit.mockImplementationOnce(fail);
      }

      harness.connection.openSessionView.mockResolvedValueOnce({
        streamId: "failed-candidate",
      });
      await harness.controller.openSharedSessionView(
        {} as never,
        "target",
        null,
      );

      expect(harness.controller.sharedSessionDisplay).toBe(originalDisplay);
      expect(harness.controller.openedSessionView).toBe(originalOpened);
      expect(harness.displays[0]?.dispose).not.toHaveBeenCalled();
      expect(harness.displays[0]?.suspend).not.toHaveBeenCalled();
      expect(harness.connection.detachSessionStream.mock.calls).toEqual([
        ["failed-candidate"],
      ]);
      expect(harness.connection.closeSessionView.mock.calls).toEqual([
        ["failed-candidate"],
      ]);
      if (fault !== "constructor") {
        expect(harness.displays[1]?.dispose).toHaveBeenCalledOnce();
      }
      if (fault !== "initial-credit") {
        expect(harness.connection.sendSessionViewCredit.mock.calls).toEqual([
          ["original", 0n],
        ]);
      }
    });
  }

  it("releases local display immediately even if remote close rejects", async () => {
    const harness = createSharedViewHarness();
    harness.connection.openSessionView.mockResolvedValueOnce({
      streamId: "one",
    });
    await harness.controller.openSharedSessionView(
      {} as never,
      "target",
      null,
    );

    const close = deferred<{ state: string; lastCommittedPacket: string }>();
    harness.connection.closeSessionView.mockReturnValueOnce(close.promise);
    const opened = harness.controller.openedSessionView!;

    const task = harness.controller.closeSharedSessionView(
      {} as never,
      opened,
      { disabled: true } as HTMLButtonElement,
    );
    expect(harness.displays[0]?.dispose).toHaveBeenCalledOnce();
    expect(harness.controller.sharedSessionDisplay).toBeNull();

    close.reject(new Error("synthetic-close"));
    await task;
    expect(harness.dashboard.mock.calls.at(-1)?.[2]).toContain("关闭未完成");
  });

  it("does not clear the new display when old close returns late", async () => {
    const harness = createSharedViewHarness();
    harness.connection.openSessionView.mockResolvedValueOnce({
      streamId: "old",
    });
    await harness.controller.openSharedSessionView(
      {} as never,
      "target",
      null,
    );

    const close = deferred<{ state: string; lastCommittedPacket: string }>();
    harness.connection.closeSessionView.mockReturnValueOnce(close.promise);
    const opened = harness.controller.openedSessionView!;

    const task = harness.controller.closeSharedSessionView(
      {} as never,
      opened,
      { disabled: true } as HTMLButtonElement,
    );

    harness.connection.openSessionView.mockResolvedValueOnce({
      streamId: "new",
    });
    await harness.controller.openSharedSessionView(
      {} as never,
      "target",
      null,
    );
    const rendered = harness.dashboard.mock.calls.length;

    close.resolve({ state: "closed", lastCommittedPacket: "1" });
    await task;

    expect(harness.controller.sharedSessionDisplay?.streamId).toBe("new");
    expect(harness.displays[1]?.dispose).not.toHaveBeenCalled();
    expect(harness.dashboard.mock.calls).toHaveLength(rendered);
  });

  it("preserves a stale owner through read-only reconnect without reusing its assembler", async () => {
    const harness = createSharedViewHarness();
    harness.connection.openSessionView.mockResolvedValueOnce({
      streamId: "before",
    });
    await harness.controller.openSharedSessionView(
      {} as never,
      "target",
      null,
    );

    harness.controller.suspendSharedView();
    expect(harness.displays[0]?.suspend).toHaveBeenCalledOnce();
    expect(harness.displays[0]?.dispose).not.toHaveBeenCalled();

    harness.connection.openSessionView.mockResolvedValueOnce({
      streamId: "after",
    });
    await harness.controller.openSharedSessionView(
      {} as never,
      "target",
      null,
      "reconnect",
    );
    expect(harness.connection.openSessionView.mock.calls.at(-1)).toEqual([
      "target",
      "reconnect",
    ]);
    expect(harness.controller.sharedSessionDisplay).toBe(harness.displays[1]);
    expect([...harness.controller.staleSharedDisplays]).toEqual([
      harness.displays[0],
    ]);

    harness.controller.disposeSharedViews();
    expect(harness.displays[0]?.dispose).toHaveBeenCalledOnce();
    expect(harness.displays[1]?.dispose).toHaveBeenCalledOnce();
    expect(harness.controller.sharedViewTargetId).toBeNull();
  });
});

function createBodyHarness() {
  const connection = {
    openSessionBody: vi.fn<
      (...args: unknown[]) => Promise<{ streamId: string; totalBytes: string }>
    >(),
    detachSessionStream: vi.fn(),
    sendSessionViewCredit: vi.fn(),
  };
  let nextTimerId = 1;
  const timers: Array<{
    readonly id: number;
    readonly ms: number;
    readonly callback: () => void;
  }> = [];
  const timerApi = {
    setTimeout: vi.fn((callback: () => void, ms: number) => {
      const id = nextTimerId++;
      timers.push({
        id,
        ms,
        callback,
      });
      return id as unknown as ReturnType<typeof globalThis.setTimeout>;
    }),
    clearTimeout: vi.fn(),
  };
  const readers: Array<{
    readonly accept: ReturnType<typeof vi.fn>;
    readonly seal: ReturnType<typeof vi.fn>;
  }> = [];
  const dashboard = vi.fn();
  let currentTargetId: string | null = "A";

  const controller = new QueuedBodyController({
    getManagement: () => connection as unknown as ManagementConnection,
    getManagementGeneration: () => 1,
    getInspectedQueueTargetId: () => currentTargetId,
    renderDashboard: dashboard,
    readerFactory: () => {
      const reader = {
        accept: vi.fn(() => ({ state: "pending" as const })),
        seal: vi.fn(),
      };
      readers.push(reader);
      return reader as unknown as QueuedBodyReaderLike;
    },
    timerApi,
    formatError: String,
  });

  return {
    controller,
    connection,
    dashboard,
    timerApi,
    timers,
    readers,
    setTarget: (target: string | null) => {
      currentTargetId = target;
      controller.reset();
    },
  };
}

describe("actual queued-body open fencing during view target changes", () => {
  it("detaches an old target descriptor without registering or crediting it", async () => {
    const { controller, connection, dashboard, setTarget } =
      createBodyHarness();
    const pending = deferred<{ streamId: string; totalBytes: string }>();
    connection.openSessionBody.mockReturnValueOnce(pending.promise);

    const task = controller.openQueueBody(
      {} as never,
      "A",
      { itemId: "item", revision: "1" } as never,
      { disabled: true } as HTMLButtonElement,
    );
    setTarget("B");
    pending.resolve({ streamId: "old-A", totalBytes: "2" });
    await task;

    expect(controller.activeBodyRead).toBeNull();
    expect(connection.sendSessionViewCredit).not.toHaveBeenCalled();
    expect(connection.detachSessionStream).toHaveBeenCalledExactlyOnceWith(
      "old-A",
    );
    expect(dashboard).not.toHaveBeenCalled();
  });

  it("keeps only latest body-open in the same target when responses arrive in reverse order", async () => {
    const { controller, connection } = createBodyHarness();
    const first = deferred<{ streamId: string; totalBytes: string }>();
    const second = deferred<{ streamId: string; totalBytes: string }>();
    connection.openSessionBody
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise);

    const a = controller.openQueueBody(
      {} as never,
      "A",
      { itemId: "one", revision: "1" } as never,
      { disabled: true } as HTMLButtonElement,
    );
    const b = controller.openQueueBody(
      {} as never,
      "A",
      { itemId: "two", revision: "1" } as never,
      { disabled: true } as HTMLButtonElement,
    );

    second.resolve({ streamId: "second", totalBytes: "2" });
    await b;
    first.resolve({ streamId: "first", totalBytes: "2" });
    await a;

    expect(controller.activeBodyRead?.streamId).toBe("second");
    expect(connection.sendSessionViewCredit.mock.calls).toEqual([["second", 0n]]);
    expect(connection.detachSessionStream).toHaveBeenCalledExactlyOnceWith(
      "first",
    );
  });

  it("seals reader, detaches active stream, and renders timeout failure after 30_000ms", async () => {
    const { controller, connection, dashboard, timerApi, timers, readers } =
      createBodyHarness();
    connection.openSessionBody.mockResolvedValueOnce({
      streamId: "stream-timeout",
      totalBytes: "16",
    });

    await controller.openQueueBody(
      {} as never,
      "A",
      { itemId: "item-timeout", revision: "1" } as never,
      { disabled: true } as HTMLButtonElement,
    );

    expect(controller.activeBodyRead?.streamId).toBe("stream-timeout");
    expect(timers).toHaveLength(1);
    expect(timers[0]?.ms).toBe(30_000);
    expect(connection.sendSessionViewCredit).toHaveBeenCalledExactlyOnceWith(
      "stream-timeout",
      0n,
    );

    timers[0]!.callback();

    expect(readers[0]?.seal).toHaveBeenCalledOnce();
    expect(connection.detachSessionStream).toHaveBeenCalledExactlyOnceWith(
      "stream-timeout",
    );
    expect(timerApi.clearTimeout).toHaveBeenCalledWith(timers[0]?.id);
    expect(controller.activeBodyRead).toBeNull();
    expect(dashboard.mock.calls.at(-1)?.[2]).toBe("读取正文超时。");
  });

  it("seals and detaches an already-active body stream when switching targets or starting a new body read", async () => {
    const {
      controller,
      connection,
      timerApi,
      timers,
      readers,
      setTarget,
    } = createBodyHarness();
    connection.openSessionBody
      .mockResolvedValueOnce({ streamId: "active-A1", totalBytes: "10" })
      .mockResolvedValueOnce({ streamId: "active-B1", totalBytes: "10" })
      .mockResolvedValueOnce({ streamId: "active-B2", totalBytes: "10" });

    await controller.openQueueBody(
      {} as never,
      "A",
      { itemId: "item-1", revision: "1" } as never,
      { disabled: true } as HTMLButtonElement,
    );
    expect(controller.activeBodyRead?.streamId).toBe("active-A1");

    // Feed an in-flight chunk so the stream is actively reading before switching target
    const handled = controller.acceptViewFrame({} as never, 1, {
      type: "data",
      streamId: "active-A1",
      packetSequence: 1n,
      recordId: 1n,
      byteOffset: 0n,
      totalLength: 10n,
      kind: "original_raw",
      last: false,
      payload: new Uint8Array([65, 66]),
    });
    expect(handled).toBe(true);
    expect(readers[0]?.accept).toHaveBeenCalledOnce();

    setTarget("B");
    expect(readers[0]?.seal).toHaveBeenCalledOnce();
    expect(connection.detachSessionStream).toHaveBeenCalledWith("active-A1");
    expect(timerApi.clearTimeout).toHaveBeenCalledWith(timers[0]?.id);
    expect(controller.activeBodyRead).toBeNull();

    // Re-opening within target B while active-B1 is established seals and detaches active-B1
    await controller.openQueueBody(
      {} as never,
      "B",
      { itemId: "item-2", revision: "1" } as never,
      { disabled: true } as HTMLButtonElement,
    );
    expect(controller.activeBodyRead?.streamId).toBe("active-B1");

    await controller.openQueueBody(
      {} as never,
      "B",
      { itemId: "item-3", revision: "2" } as never,
      { disabled: true } as HTMLButtonElement,
    );
    expect(readers[1]?.seal).toHaveBeenCalledOnce();
    expect(connection.detachSessionStream).toHaveBeenCalledWith("active-B1");
    expect(timerApi.clearTimeout).toHaveBeenCalledWith(timers[1]?.id);
    expect(controller.activeBodyRead?.streamId).toBe("active-B2");
  });
});
