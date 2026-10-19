/**
 * 文件栏 feature 状态测试（tasks 4.2–4.4）。
 *
 * 覆盖三来源分离、会话改动归并、互斥内容态/请求代次、刷新合并与失败保留。所有断言都通过
 * `ClientStore` 的公开状态/方法与记账网关完成，不 mock 解码器或归并函数。
 */

import { describe, expect, it } from "vitest";

import type {
  CommandMessage,
  CommandResultMessage,
  EventMessage,
  PublicError,
  SessionSummary,
} from "../protocol";
import { ClientStore } from "./client-store";
import { SESSION_FILES_FEATURE } from "../protocol/session-files";
import type { SessionFilesCommandName } from "../protocol/session-files";
import { encodeBase64Url } from "../sync-client";
import {
  ONLINE_SESSION_ID,
  historyItem,
  onlineStore,
  readResult,
  resources,
  session,
  storeWith,
} from "./testing/fixtures";
import type { RecordingGateway } from "./testing/fixtures";

const OTHER_SESSION_ID = "aaaaaaaa-0000-4000-8000-000000000002";
const CONNECTION_ID = "2de54db7-74ae-4c21-88e3-05df0dc2e637";

/** 可手动推进的定时器（文件栏刷新合并/限流退避）。 */
function fakeScheduler(): {
  readonly schedule: (delayMs: number, task: () => void) => () => void;
  readonly delays: number[];
  runAll(): void;
} {
  const tasks: { delayMs: number; task: () => void; cancelled: boolean }[] = [];
  const delays: number[] = [];
  return {
    delays,
    schedule(delayMs, task) {
      delays.push(delayMs);
      const entry = { delayMs, task, cancelled: false };
      tasks.push(entry);
      return () => {
        entry.cancelled = true;
      };
    },
    runAll() {
      for (const entry of [...tasks]) {
        if (entry.cancelled) continue;
        entry.cancelled = true;
        entry.task();
      }
    },
  };
}

/** 一条 `file.changed` 事件（只带归并/呈现需要的结构化字段）。 */
function fileChanged(input: {
  readonly eventId: string;
  readonly sessionId?: string;
  readonly displayPath: string;
  readonly sequence: string;
  readonly outsideWorkspace?: boolean;
  readonly addedLines?: string;
  readonly deletedLines?: string;
  readonly diffLines?: readonly { readonly kind: "added" | "deleted" | "context"; readonly text: string }[];
  readonly originKind?: "agent" | "device" | "local_cli";
}): EventMessage {
  return {
    protocolVersion: 1,
    type: "event",
    messageId: "11111111-1111-4111-8111-000000000001",
    connectionId: CONNECTION_ID,
    connectionSequence: "1",
    body: {
      globalSequence: input.sequence,
      sessionSequence: input.sequence,
      eventId: input.eventId,
      sessionId: input.sessionId ?? ONLINE_SESSION_ID,
      eventType: "file.changed",
      causationRequestId: null,
      origin: { kind: input.originKind ?? "agent", deviceId: null },
      remoteOrigin: null,
      createdAt: `2026-10-01T00:00:${input.sequence.padStart(2, "0")}.000Z`,
      payload: {
        view: {
          changeId: input.eventId,
          kind: "modified",
          displayPath: input.displayPath,
          summary: `第 ${input.sequence} 次修改`,
          ...(input.addedLines === undefined ? {} : { addedLines: input.addedLines }),
          ...(input.deletedLines === undefined ? {} : { deletedLines: input.deletedLines }),
          ...(input.outsideWorkspace === undefined ? {} : { outsideWorkspace: input.outsideWorkspace }),
          ...(input.diffLines === undefined ? {} : { diffLines: input.diffLines }),
        },
      },
    },
  };
}

/** 构造一条会话文件查询的 completed/rejected 结果。 */
function commandResult(
  requestId: string,
  command: SessionFilesCommandName,
  result: Record<string, unknown> | null,
  error: PublicError | null = null,
): CommandResultMessage {
  return {
    protocolVersion: 1,
    type: "command.result",
    messageId: "22222222-2222-4222-8222-000000000001",
    connectionId: CONNECTION_ID,
    connectionSequence: "1",
    body:
      error === null
        ? {
            requestId,
            command,
            status: "completed",
            acceptedAt: "2026-10-01T00:00:01.000Z",
            terminalEventId: null,
            result,
            error: null,
          }
        : {
            requestId,
            command,
            status: "rejected",
            acceptedAt: null,
            terminalEventId: null,
            result: null,
            error,
          },
  };
}

/** 最近一条派发的命令。 */
function lastCommand(gateway: RecordingGateway): CommandMessage {
  const command = gateway.dispatches[gateway.dispatches.length - 1];
  if (command === undefined) throw new Error("没有任何派发记录");
  return command;
}

/** 一条 file list 结果。 */
function listResult(input: {
  readonly entries: readonly Record<string, unknown>[];
  readonly next?: string;
  readonly path?: string;
}): Record<string, unknown> {
  return {
    path: input.path ?? "",
    gitState: "available",
    entries: input.entries,
    ...(input.next === undefined ? {} : { next: input.next }),
  };
}

/** 一条 read 结果（单块）。 */
function readResultOf(input: {
  readonly path: string;
  readonly text: string;
  readonly version?: string;
  readonly offset?: string;
  readonly eof?: boolean;
  readonly previewTruncated?: boolean;
}): Record<string, unknown> {
  return {
    path: input.path,
    kind: "text",
    sizeBytes: String(input.text.length),
    version: input.version ?? "v1",
    offset: input.offset ?? "0",
    dataBase64: encodeBase64Url(new TextEncoder().encode(input.text)),
    eof: input.eof ?? true,
    previewTruncated: input.previewTruncated ?? false,
  };
}

/** 一条任意 kind 的 read 结果（用字节数组直接构造，图片预算用例需要真实大小）。 */
function readBytesOf(input: {
  readonly path: string;
  readonly kind: "text" | "image" | "binary";
  readonly bytes: Uint8Array;
  readonly offset?: string;
  readonly eof?: boolean;
  readonly totalBytes?: number;
}): Record<string, unknown> {
  return {
    path: input.path,
    kind: input.kind,
    sizeBytes: String(input.totalBytes ?? input.bytes.length),
    version: "v1",
    offset: input.offset ?? "0",
    dataBase64: encodeBase64Url(input.bytes),
    eof: input.eof ?? true,
    previewTruncated: false,
  };
}

/**
 * 按客户端可接受的分块上限逐块回放一条 `session.files.read`。
 *
 * 每块非 EOF 结果都会让客户端派发下一次读取，因此回放过程中要重新取最近一条请求；一旦
 * 客户端报错（例如超出预算）就停止，避免继续对着已终止的上下文回放。
 */
function feedReadChunks(
  store: ClientStore,
  gateway: RecordingGateway,
  input: {
    readonly path: string;
    readonly kind: "text" | "image" | "binary";
    readonly totalBytes: number;
    readonly chunkSize: number;
  },
): void {
  let request = lastCommand(gateway);
  for (let offset = 0; offset < input.totalBytes; offset += input.chunkSize) {
    const size = Math.min(input.chunkSize, input.totalBytes - offset);
    const eof = offset + size >= input.totalBytes;
    store.apply({
      kind: "command_result",
      result: commandResult(request.body.requestId, "session.files.read", readBytesOf({
        path: input.path,
        kind: input.kind,
        bytes: new Uint8Array(size),
        offset: String(offset),
        eof,
        totalBytes: input.totalBytes,
      })),
    });
    const content = store.sessionFilesPage(ONLINE_SESSION_ID).content;
    if (content.kind === "file" && content.status === "error") return;
    if (!eof) request = lastCommand(gateway);
  }
}

/** 带协商 feature 的在线 store。 */
function fileStore(input: {
  readonly features?: readonly string[];
  readonly sessions?: readonly SessionSummary[];
} = {}) {
  return onlineStore({
    resources: resources({
      sessions: input.sessions ?? [session({ id: ONLINE_SESSION_ID, state: "running" })],
    }),
    features: input.features ?? [SESSION_FILES_FEATURE],
  });
}

describe("文件栏：未协商与远程降级", () => {
  it("未协商 core.session-files.v1：进入全部文件/Git 不派发，只给出显式降级说明", () => {
    const { store, gateway } = fileStore({ features: [] });
    store.selectFileSource(ONLINE_SESSION_ID, "all");
    store.selectFileSource(ONLINE_SESSION_ID, "git");
    store.openFileContent(ONLINE_SESSION_ID, "all", "src/a.ts");

    expect(gateway.dispatches).toHaveLength(0);
    const page = store.sessionFilesPage(ONLINE_SESSION_ID);
    expect(page.allNotice).toContain("core.session-files.v1");
    expect(page.gitNotice).toContain("core.session-files.v1");
    expect(page.content.kind === "file" ? page.content.status : null).toBe("error");
  });

  it("imported 会话保留已收到的会话变更，另外两个来源暂不支持且不查本机目录", () => {
    const remoteId = "aaaaaaaa-0000-4000-8000-0000000000aa";
    const { store, gateway } = fileStore({
      sessions: [
        session({
          id: remoteId,
          origin: {
            kind: "remote",
            ownerNodeId: "bbbbbbbb-0000-4000-8000-000000000001",
            exportId: "export-1",
            originEpoch: "cccccccc-0000-4000-8000-000000000001",
            online: true,
          },
        }),
      ],
    });
    store.apply({ kind: "event", event: fileChanged({ eventId: "eeeeeeee-0000-4000-8000-000000000001", sessionId: remoteId, displayPath: "src/a.ts", sequence: "1" }) });
    store.selectFileSource(remoteId, "all");

    expect(gateway.dispatches).toHaveLength(0);
    const page = store.sessionFilesPage(remoteId);
    expect(page.allNotice).toContain("远程导入会话");
    expect(page.sessionChanges.fileCount).toBe(1);
  });
});

describe("文件栏：会话变更归并", () => {
  it("重复重放不增加记录，同名不同路径不合并，其他会话不混入", () => {
    const { store } = fileStore();
    const first = fileChanged({
      eventId: "eeeeeeee-0000-4000-8000-000000000001",
      displayPath: "src/a.ts",
      sequence: "1",
      addedLines: "12",
      deletedLines: "3",
      diffLines: [{ kind: "added", text: "+const x = 1;" }],
    });
    store.apply({ kind: "event", event: first });
    store.apply({ kind: "event", event: first });
    store.apply({
      kind: "event",
      event: fileChanged({
        eventId: "eeeeeeee-0000-4000-8000-000000000002",
        displayPath: "lib/a.ts",
        sequence: "2",
      }),
    });
    store.apply({
      kind: "event",
      event: fileChanged({
        eventId: "eeeeeeee-0000-4000-8000-000000000003",
        displayPath: "src/a.ts",
        sequence: "3",
        addedLines: "5",
        deletedLines: "1",
      }),
    });
    store.apply({
      kind: "event",
      event: fileChanged({
        eventId: "eeeeeeee-0000-4000-8000-000000000004",
        sessionId: OTHER_SESSION_ID,
        displayPath: "src/other.ts",
        sequence: "4",
      }),
    });
    // 手动修改（设备来源）属于其他来源，不并入会话变更。
    store.apply({
      kind: "event",
      event: fileChanged({
        eventId: "eeeeeeee-0000-4000-8000-000000000005",
        displayPath: "src/manual.ts",
        sequence: "5",
        originKind: "device",
      }),
    });

    const page = store.sessionFilesPage(ONLINE_SESSION_ID);
    expect(page.sessionChanges.fileCount).toBe(2);
    const byPath = new Map(page.sessionChanges.files.map((file) => [file.path, file]));
    expect(byPath.get("src/a.ts")?.records.map((record) => record.eventId)).toEqual([
      "eeeeeeee-0000-4000-8000-000000000001",
      "eeeeeeee-0000-4000-8000-000000000003",
    ]);
    expect(byPath.get("lib/a.ts")?.records).toHaveLength(1);
    expect(page.sessionChanges.files.some((file) => file.path === "src/other.ts")).toBe(false);
    expect(page.sessionChanges.files.some((file) => file.path === "src/manual.ts")).toBe(false);
    // 单次统计保留在详情，不提供累计净差异。
    expect(byPath.get("src/a.ts")?.records.map((record) => [record.addedLines, record.deletedLines])).toEqual([
      ["12", "3"],
      ["5", "1"],
    ]);
    expect(Object.keys(byPath.get("src/a.ts") ?? {})).not.toContain("totalAddedLines");
  });

  it("根外事件只给 basename 时按事件各自保留且不可读取", () => {
    const { store } = fileStore();
    store.apply({
      kind: "event",
      event: fileChanged({
        eventId: "eeeeeeee-0000-4000-8000-000000000011",
        displayPath: "outside.txt",
        sequence: "1",
        outsideWorkspace: true,
      }),
    });
    store.apply({
      kind: "event",
      event: fileChanged({
        eventId: "eeeeeeee-0000-4000-8000-000000000012",
        displayPath: "outside.txt",
        sequence: "2",
        outsideWorkspace: true,
      }),
    });
    const page = store.sessionFilesPage(ONLINE_SESSION_ID);
    expect(page.sessionChanges.files).toHaveLength(2);
    for (const file of page.sessionChanges.files) {
      expect(file.readable).toBe(false);
      expect(file.path).toBeNull();
      expect(file.unreadableReason).toContain("无法读取");
      expect(file.records).toHaveLength(1);
    }
    store.openFileContent(ONLINE_SESSION_ID, "session", "outside.txt");
    const content = store.sessionFilesPage(ONLINE_SESSION_ID).content;
    expect(content.kind === "file" ? content.status : null).toBe("error");
  });

  it("只展示已加载历史，并用既有历史标记说明范围", () => {
    const { store, gateway } = onlineStore({
      resources: resources({ sessions: [session({ id: ONLINE_SESSION_ID })] }),
      features: [SESSION_FILES_FEATURE, "core.session-history.v1"],
    });
    store.loadSession(ONLINE_SESSION_ID);
    const request = lastCommand(gateway);
    const requestId = request.body.requestId;
    store.apply({
      kind: "command_result",
      result: readResult({
        requestId,
        history: [
          historyItem({
            eventId: "eeeeeeee-0000-4000-8000-000000000021",
            eventType: "file.changed",
            sessionSequence: "10",
            view: {
              changeId: "eeeeeeee-0000-4000-8000-000000000021",
              kind: "modified",
              displayPath: "src/history.ts",
              summary: "历史里的改动",
            },
          }),
        ],
        historyHasEarlier: true,
      }),
    });
    const page = store.sessionFilesPage(ONLINE_SESSION_ID);
    expect(page.historyCoverage).toBe("partial");
    expect(page.historyNotice).toContain("历史");
    expect(page.sessionChanges.fileCount).toBe(1);
  });
});

describe("文件栏：全部文件分页与失败保留", () => {
  it("分页追加、失败保留成功条目，忽略开关会重新枚举", () => {
    const { store, gateway } = fileStore();
    store.selectFileSource(ONLINE_SESSION_ID, "all");
    expect(gateway.dispatches).toHaveLength(1);
    const first = lastCommand(gateway);
    expect(first.body.command).toBe("session.files.list");
    store.apply({
      kind: "command_result",
      result: commandResult(first.body.requestId, "session.files.list", listResult({
        entries: [{ path: "src", name: "src", kind: "directory", openable: true }],
        next: "page-2",
      })),
    });
    let page = store.sessionFilesPage(ONLINE_SESSION_ID);
    expect(page.all.directories.get("")?.entries.map((entry) => entry.path)).toEqual(["src"]);

    store.loadMoreAllFiles(ONLINE_SESSION_ID, "");
    const second = lastCommand(gateway);
    expect(second.body.command === "session.files.list" ? second.body.payload.after : null).toBe("page-2");
    store.apply({
      kind: "command_result",
      result: commandResult(second.body.requestId, "session.files.list", listResult({
        entries: [{ path: "README.md", name: "README.md", kind: "file", openable: true }],
      })),
    });
    page = store.sessionFilesPage(ONLINE_SESSION_ID);
    expect(page.all.directories.get("")?.entries.map((entry) => entry.path)).toEqual(["src", "README.md"]);

    store.refreshFiles(ONLINE_SESSION_ID);
    const refresh = lastCommand(gateway);
    store.apply({
      kind: "command_result",
      result: commandResult(refresh.body.requestId, "session.files.list", null, {
        code: "internal.unavailable",
        message: "主机暂时不可用",
        retryable: true,
        details: {},
      }),
    });
    page = store.sessionFilesPage(ONLINE_SESSION_ID);
    expect(page.all.directories.get("")?.entries.map((entry) => entry.path)).toEqual(["src", "README.md"]);
    expect(page.all.directories.get("")?.error?.message).toContain("主机暂时不可用");

    store.toggleHideIgnored(ONLINE_SESSION_ID);
    const toggled = lastCommand(gateway);
    expect(toggled.body.command === "session.files.list" ? toggled.body.payload.hideIgnored : null).toBe(true);
  });
});

describe("文件栏：Git 状态与差异", () => {
  it("Git 状态保留在来源、差异在内容态，二者互不冒充", () => {
    const { store, gateway } = fileStore();
    store.selectFileSource(ONLINE_SESSION_ID, "git");
    const status = lastCommand(gateway);
    expect(status.body.command).toBe("session.git.status");
    store.apply({
      kind: "command_result",
      result: commandResult(status.body.requestId, "session.git.status", {
        gitState: "available",
        entries: [
          { path: "src/a.ts", status: "modified", conflicted: false, binary: false },
          { path: "src/new.ts", status: "untracked", conflicted: false, binary: false },
        ],
        next: "git-page-2",
      }),
    });
    let page = store.sessionFilesPage(ONLINE_SESSION_ID);
    expect(page.git.gitState).toBe("available");
    expect(page.git.entries.map((entry) => entry.path)).toEqual(["src/a.ts", "src/new.ts"]);
    expect(page.git.next).toBe("git-page-2");
    expect(page.sessionChanges.fileCount).toBe(0);

    store.openFileContent(ONLINE_SESSION_ID, "git", "src/a.ts");
    const diff = lastCommand(gateway);
    expect(diff.body.command).toBe("session.git.diff");
    store.apply({
      kind: "command_result",
      result: commandResult(diff.body.requestId, "session.git.diff", {
        path: "src/a.ts",
        base: "head",
        kind: "text",
        diffLines: ["@@ -1 +1 @@", "-old", "+new"],
        truncated: false,
      }),
    });
    page = store.sessionFilesPage(ONLINE_SESSION_ID);
    expect(page.content.kind === "file" ? page.content.status : null).toBe("ready");
    const content = page.content;
    expect(content.kind === "file" && content.status === "ready" && content.content.kind === "git_diff"
      ? content.content.diff.diffLines
      : null).toEqual(["@@ -1 +1 @@", "-old", "+new"]);
  });
});

describe("文件栏：刷新合并、限流与不持续监听", () => {
  it("没有定时端口时不自行刷新，手动刷新仍发一条查询", () => {
    const { store, gateway } = onlineStore({
      resources: resources({ sessions: [session({ id: ONLINE_SESSION_ID }), session({ id: OTHER_SESSION_ID })] }),
      features: [SESSION_FILES_FEATURE],
    });
    // `onlineStore` 不接受 schedule，因此这里用同一条 store 的构造参数无法注入；
    // 通过直接调用手动刷新验证限流退避仍然成立（见下方断言）。
    store.selectFileSource(ONLINE_SESSION_ID, "git");
    const first = lastCommand(gateway);
    store.apply({
      kind: "command_result",
      result: commandResult(first.body.requestId, "session.git.status", { gitState: "available", entries: [] }),
    });
    // 其他会话的事件不触发当前来源刷新。
    store.apply({
      kind: "event",
      event: fileChanged({
        eventId: "eeeeeeee-0000-4000-8000-000000000030",
        sessionId: OTHER_SESSION_ID,
        displayPath: "src/other.ts",
        sequence: "9",
      }),
    });
    expect(gateway.dispatches).toHaveLength(1);
    // 没有定时端口时当前会话事件也不会自行刷新（不持续监听）；手动刷新仍发一条查询。
    store.apply({
      kind: "event",
      event: fileChanged({
        eventId: "eeeeeeee-0000-4000-8000-000000000031",
        displayPath: "src/a.ts",
        sequence: "10",
      }),
    });
    expect(gateway.dispatches).toHaveLength(1);

    store.refreshFiles(ONLINE_SESSION_ID);
    const refresh = lastCommand(gateway);
    expect(refresh.body.command).toBe("session.git.status");
  });

  it("注入定时端口后，突发事件合并为一次刷新，限流错误按 retryAfterMs 再排期", () => {
    const scheduler = fakeScheduler();
    const snapshot = storeWith({
      resources: resources({ sessions: [session({ id: ONLINE_SESSION_ID })] }),
      schedule: scheduler.schedule,
    });
    let counter = 0;
    const store = new ClientStore({
      sync: snapshot.gateway,
      ids: {
        uuid(): string {
          counter += 1;
          return `00000000-0000-4000-8000-${counter.toString().padStart(12, "0")}`;
        },
      },
      now: () => 1_700_000_000_000,
      identity: { connectionId: CONNECTION_ID },
      schedule: scheduler.schedule,
    });
    store.apply({
      kind: "authenticated",
      info: {
        deviceId: "2ae1c07c-9242-46e9-a9d2-4ec58c130f49",
        serverEpoch: "00384a03-bc90-4095-b65d-82fb8cc47e13",
        headGlobalSequence: "10",
        heartbeatIntervalMs: 30000,
        limits: { maxMessageBytes: 1048576, maxPromptBytes: 262144, maxReplayEventsPerBatch: 500 },
      },
      selectedFeatures: [SESSION_FILES_FEATURE],
    });
    store.apply({ kind: "snapshot_verified", snapshot: {
      snapshotId: "8194de43-e213-423d-acf4-2e3549304566",
      cursor: { serverEpoch: "00384a03-bc90-4095-b65d-82fb8cc47e13", globalSequence: "10" },
      resources: resources({ sessions: [session({ id: ONLINE_SESSION_ID })] }),
      chunkCount: 3,
      receivedChunks: 3,
      snapshotDigest: "kQ8x2mB7yV1cJ0nP5",
    } });
    store.pairSucceeded();
    store.apply({ kind: "authenticating" });
    store.apply({ kind: "replaying", cursor: null });
    store.apply({ kind: "online", cursor: { serverEpoch: "00384a03-bc90-4095-b65d-82fb8cc47e13", globalSequence: "10" } });
    const gateway = snapshot.gateway;

    store.selectFileSource(ONLINE_SESSION_ID, "all");
    const first = gateway.dispatches[0];
    if (first === undefined) throw new Error("没有首次列表查询");
    store.apply({
      kind: "command_result",
      result: commandResult(first.body.requestId, "session.files.list", listResult({ entries: [] })),
    });

    for (const sequence of ["11", "12", "13"]) {
      store.apply({
        kind: "event",
        event: fileChanged({
          eventId: `eeeeeeee-0000-4000-8000-00000000004${sequence.slice(-1)}`,
          displayPath: "src/a.ts",
          sequence,
        }),
      });
    }
    expect(scheduler.delays).toEqual([150]);
    scheduler.runAll();
    expect(gateway.dispatches).toHaveLength(2);

    const refresh = gateway.dispatches[1];
    if (refresh === undefined) throw new Error("没有刷新查询");
    store.apply({
      kind: "command_result",
      result: commandResult(refresh.body.requestId, "session.files.list", null, {
        code: "resource.rate_limited",
        message: "太频繁",
        retryable: true,
        details: { retryAfterMs: 500 },
      }),
    });
    expect(scheduler.delays).toEqual([150, 500]);
  });
});

describe("文件栏：互斥内容态与请求代次", () => {
  it("A 的迟到响应不覆盖 B；关闭后旧响应不能重新打开", () => {
    const { store, gateway } = fileStore();
    store.openFileContent(ONLINE_SESSION_ID, "all", "a.txt");
    const a = lastCommand(gateway);
    store.openFileContent(ONLINE_SESSION_ID, "all", "b.txt");
    const b = lastCommand(gateway);
    expect(a.body.requestId).not.toBe(b.body.requestId);

    // A 迟到：内容仍是 B 的 loading。
    store.apply({ kind: "command_result", result: commandResult(a.body.requestId, "session.files.read", readResultOf({ path: "a.txt", text: "A" })) });
    let content = store.sessionFilesPage(ONLINE_SESSION_ID).content;
    expect(content.kind === "file" ? content.context.path : null).toBe("b.txt");
    expect(content.kind === "file" ? content.status : null).toBe("loading");

    store.apply({ kind: "command_result", result: commandResult(b.body.requestId, "session.files.read", readResultOf({ path: "b.txt", text: "B" })) });
    content = store.sessionFilesPage(ONLINE_SESSION_ID).content;
    expect(content.kind === "file" && content.status === "ready" ? content.content.kind : null).toBe("preview");
    expect(content.kind === "file" ? content.context.path : null).toBe("b.txt");

    store.closeFileContent();
    store.apply({ kind: "command_result", result: commandResult("00000000-0000-4000-8000-000000000999", "session.files.read", readResultOf({ path: "c.txt", text: "C" })) });
    expect(store.sessionFilesPage(ONLINE_SESSION_ID).content.kind).toBe("conversation");
  });

  it("切换会话后旧会话的迟到响应不会在该会话重新打开文件", () => {
    const { store, gateway } = fileStore({
      sessions: [session({ id: ONLINE_SESSION_ID }), session({ id: OTHER_SESSION_ID })],
    });
    store.openFileContent(ONLINE_SESSION_ID, "all", "a.txt");
    const request = lastCommand(gateway);
    // 只切会话、不显式关闭：旧读取仍必须作废。
    store.setActiveConversationSession(OTHER_SESSION_ID);
    expect(store.sessionFilesPage(ONLINE_SESSION_ID).content.kind).toBe("conversation");
    store.apply({ kind: "command_result", result: commandResult(request.body.requestId, "session.files.read", readResultOf({ path: "a.txt", text: "A" })) });
    expect(store.sessionFilesPage(ONLINE_SESSION_ID).content.kind).toBe("conversation");
  });

  it("文件视图开关只替换 sessionFiles 切片：草稿、线程与次级视图保持同一引用", () => {
    const { store } = fileStore();
    store.setDraft(ONLINE_SESSION_ID, "还没发的草稿");
    store.setConversationSecondaryView("timeline");
    const before = store.state;
    store.openFileContent(ONLINE_SESSION_ID, "session", "src/a.ts");
    store.closeFileContent();
    const after = store.state;
    expect(after.drafts).toBe(before.drafts);
    expect(after.threads).toBe(before.threads);
    expect(after.structured).toBe(before.structured);
    expect(after.conversationSecondaryView).toBe("timeline");
    expect(store.draftOf(ONLINE_SESSION_ID)).toBe("还没发的草稿");
    // 文件切片自身不拥有草稿/锚点/展开状态（它们属于上层）。
    expect(Object.keys(after.sessionFiles).sort()).toEqual(["content", "generation", "selectedSources", "sources"]);
  });

  it("查看期间同一文件改变只标记过时，不覆盖正在阅读的内容", () => {
    const { store, gateway } = fileStore();
    store.openFileContent(ONLINE_SESSION_ID, "all", "src/a.ts");
    const request = lastCommand(gateway);
    store.apply({ kind: "command_result", result: commandResult(request.body.requestId, "session.files.read", readResultOf({ path: "src/a.ts", text: "old" })) });
    store.apply({
      kind: "event",
      event: fileChanged({
        eventId: "eeeeeeee-0000-4000-8000-000000000061",
        displayPath: "src/a.ts",
        sequence: "20",
      }),
    });
    const content = store.sessionFilesPage(ONLINE_SESSION_ID).content;
    expect(content.kind === "file" ? content.stale : null).toBe(true);
    expect(content.kind === "file" && content.status === "ready" && content.content.kind === "preview"
      ? new TextDecoder().decode(content.content.bytes)
      : null).toBe("old");
  });
});

describe("文件栏：查询结果不进入命令记录", () => {
  it("完成一次 read 后 commands 不含该 requestId，多块读取也不增长", () => {
    const { store, gateway } = fileStore();
    const initialCommands = store.state.commands.size;
    store.openFileContent(ONLINE_SESSION_ID, "all", "big.bin");
    let request = lastCommand(gateway);
    expect(store.state.commands.has(request.body.requestId)).toBe(false);

    const seen: string[] = [];
    let offset = 0;
    for (let index = 0; index < 3; index += 1) {
      seen.push(request.body.requestId);
      const text = String.fromCharCode(65 + index);
      store.apply({
        kind: "command_result",
        result: commandResult(request.body.requestId, "session.files.read", readResultOf({
          path: "big.bin",
          text,
          offset: String(offset),
          eof: index === 2,
        })),
      });
      offset += text.length;
      if (index < 2) request = lastCommand(gateway);
    }

    for (const requestId of seen) expect(store.state.commands.has(requestId)).toBe(false);
    // 三块结果都只更新内容态/在途表，绝不把正文（dataBase64 的解码源）留在 commands 里。
    expect(store.state.commands.size).toBe(initialCommands);
    const content = store.sessionFilesPage(ONLINE_SESSION_ID).content;
    expect(content.kind === "file" && content.status === "ready" ? content.content.kind : null).toBe("preview");
  });
});

describe("文件栏：层级浏览首屏入口", () => {
  it("未加载的子目录调用入口会派发该目录 path 的第一页", () => {
    const { store, gateway } = fileStore();
    store.selectFileSource(ONLINE_SESSION_ID, "all");
    const root = lastCommand(gateway);
    store.apply({
      kind: "command_result",
      result: commandResult(root.body.requestId, "session.files.list", listResult({
        entries: [{ path: "src", name: "src", kind: "directory", openable: true }],
      })),
    });

    store.loadMoreAllFiles(ONLINE_SESSION_ID, "src");
    expect(gateway.dispatches).toHaveLength(2);
    const firstPage = lastCommand(gateway);
    if (firstPage.body.command !== "session.files.list") throw new Error("不是 session.files.list");
    expect(firstPage.body.payload.path).toBe("src");
    expect(firstPage.body.payload.hideIgnored).toBe(false);
    expect(firstPage.body.payload.after).toBeUndefined();
    expect(store.sessionFilesPage(ONLINE_SESSION_ID).all.directories.get("src")?.loading).toBe(true);

    // 加载中不重复派发。
    store.loadMoreAllFiles(ONLINE_SESSION_ID, "src");
    expect(gateway.dispatches).toHaveLength(2);

    // 该层没有下一页时也不派发。
    store.apply({
      kind: "command_result",
      result: commandResult(firstPage.body.requestId, "session.files.list", listResult({
        path: "src",
        entries: [],
      })),
    });
    store.loadMoreAllFiles(ONLINE_SESSION_ID, "src");
    expect(gateway.dispatches).toHaveLength(2);
  });
});

describe("文件栏：按内容 kind 选择预览预算", () => {
  it("1 MiB 图片可以预览，超过 4 MiB 才按 result_too_large 失败", () => {
    const { store, gateway } = fileStore();
    const imageBytes = 1024 * 1024;
    store.openFileContent(ONLINE_SESSION_ID, "all", "photo.png");
    // 服务端按 43691 字符分块；这里用 32767 字节的块，避免累计值恰好落在预算上而先完成。
    feedReadChunks(store, gateway, { path: "photo.png", kind: "image", totalBytes: imageBytes, chunkSize: 32767 });
    let content = store.sessionFilesPage(ONLINE_SESSION_ID).content;
    expect(content.kind === "file" ? content.status : null).toBe("ready");
    expect(content.kind === "file" && content.status === "ready" && content.content.kind === "preview"
      ? content.content.bytes.length
      : null).toBe(imageBytes);

    store.openFileContent(ONLINE_SESSION_ID, "all", "huge.png");
    feedReadChunks(store, gateway, {
      path: "huge.png",
      kind: "image",
      totalBytes: 4 * 1024 * 1024 + 1,
      chunkSize: 32767,
    });
    content = store.sessionFilesPage(ONLINE_SESSION_ID).content;
    expect(content.kind === "file" ? content.status : null).toBe("error");
    expect(content.kind === "file" && content.status === "error" ? content.error.code : null)
      .toBe("resource.result_too_large");
  });

  it("文本仍沿用 256 KiB 预算", () => {
    const { store, gateway } = fileStore();
    store.openFileContent(ONLINE_SESSION_ID, "all", "big.txt");
    feedReadChunks(store, gateway, {
      path: "big.txt",
      kind: "text",
      totalBytes: 256 * 1024 + 1,
      chunkSize: 32767,
    });
    const content = store.sessionFilesPage(ONLINE_SESSION_ID).content;
    expect(content.kind === "file" ? content.status : null).toBe("error");
    expect(content.kind === "file" && content.status === "error" ? content.error.code : null)
      .toBe("resource.result_too_large");
  });
});

describe("会话变更手动刷新", () => {
  it("重读最新历史并去重合并，不重置消息和历史分页；在途点击不重复派发", () => {
    const { store, gateway } = fileStore({ features: [SESSION_FILES_FEATURE, "core.session-history.v1"] });
    const old = historyItem({ eventId: "eeeeeeee-0000-4000-8000-000000000081", eventType: "file.changed",
      view: { kind: "modified", changeId: "old", displayPath: "src/old.ts", summary: "old" } });
    const latest = historyItem({ eventId: "eeeeeeee-0000-4000-8000-000000000082", eventType: "file.changed", sessionSequence: "2",
      view: { kind: "modified", changeId: "new", displayPath: "src/new.ts", summary: "new" } });
    store.loadSession(ONLINE_SESSION_ID);
    store.apply({ kind: "command_result", result: readResult({ requestId: lastCommand(gateway).body.requestId,
      messages: [], history: [old], historyHasEarlier: true }) });
    const thread = store.state.threads.get(ONLINE_SESSION_ID);
    expect(store.sessionFilesPage(ONLINE_SESSION_ID).refreshState.kind).toBe("available");
    store.refreshFiles(ONLINE_SESSION_ID);
    const refresh = lastCommand(gateway);
    expect(refresh.body.command).toBe("session.read");
    expect(refresh.body.payload).toEqual({ include: ["history"] });
    expect(store.sessionFilesPage(ONLINE_SESSION_ID).refreshState.kind).toBe("loading");
    store.refreshFiles(ONLINE_SESSION_ID);
    expect(gateway.dispatches).toHaveLength(2);
    store.apply({ kind: "command_result", result: readResult({ requestId: refresh.body.requestId,
      history: [old, latest], historyHasEarlier: false }) });
    expect(store.sessionFilesPage(ONLINE_SESSION_ID).sessionChanges.files.map(file => file.path)).toEqual(["src/new.ts", "src/old.ts"]);
    const after = store.state.threads.get(ONLINE_SESSION_ID);
    expect(after?.page).toBe(thread?.page);
    expect(after?.items).toBe(thread?.items);
    expect(after?.historyHasEarlier).toBe(true);
    expect(after?.nextBeforeHistory).toBe(thread?.nextBeforeHistory);
    expect(store.sessionFilesPage(ONLINE_SESSION_ID).refreshState.kind).toBe("available");
  });

  it("旧主机明确说明不可刷新，不发送不支持的历史查询", () => {
    const { store, gateway } = fileStore();
    store.refreshFiles(ONLINE_SESSION_ID);
    expect(gateway.dispatches).toHaveLength(0);
    const state = store.sessionFilesPage(ONLINE_SESSION_ID).refreshState;
    expect(state.kind).toBe("unavailable");
    expect(state.kind === "unavailable" ? state.reason : "").toContain("不支持读取会话历史");
  });
});

describe("文件栏刷新错误与会话读取错误分离", () => {
  it("首次加载只请求后端可读资源，失败只在对话读取处呈现", () => {
    const { store, gateway } = onlineStore({ resources: resources({ sessions: [session({ id: ONLINE_SESSION_ID })] }), features: ["core.session-history.v1"] });
    store.loadSession(ONLINE_SESSION_ID);
    const command = lastCommand(gateway);
    expect(command.body.command).toBe("session.read");
    expect(command.body.payload).toEqual({ include: ["messages", "config_options", "history"] });
    const result = readResult({ requestId: command.body.requestId });
    store.apply({ kind: "command_result", result: { ...result, body: { ...result.body,
      status: "failed", result: null,
      error: { code: "capability.unsupported_by_broker", message: "the broker cannot express this resource", retryable: false, details: {} },
    } } });
    expect(store.conversationPage(ONLINE_SESSION_ID)?.paging.error).toContain("capability.unsupported_by_broker");
    expect(store.sessionFilesPage(ONLINE_SESSION_ID).refreshError).toBeNull();
    store.refreshFiles(ONLINE_SESSION_ID);
    expect(store.conversationPage(ONLINE_SESSION_ID)?.paging.error).toContain("capability.unsupported_by_broker");
    store.apply({ kind: "command_result", result: readResult({ requestId: lastCommand(gateway).body.requestId }) });
    expect(store.conversationPage(ONLINE_SESSION_ID)?.paging.error).toContain("capability.unsupported_by_broker");
    expect(store.sessionFilesPage(ONLINE_SESSION_ID).refreshError).toBeNull();
  });

  it("文件栏手动刷新失败仍显示真实错误，重试时清除", () => {
    const { store, gateway } = onlineStore({ resources: resources({ sessions: [session({ id: ONLINE_SESSION_ID })] }), features: ["core.session-history.v1"] });
    store.refreshFiles(ONLINE_SESSION_ID);
    const command = lastCommand(gateway);
    expect(command.body.payload).toEqual({ include: ["history"] });
    const result = readResult({ requestId: command.body.requestId });
    store.apply({ kind: "command_result", result: { ...result, body: { ...result.body,
      status: "failed", result: null,
      error: { code: "internal.unavailable", message: "history unavailable", retryable: true, details: {} },
    } } });
    expect(store.sessionFilesPage(ONLINE_SESSION_ID).refreshError).toContain("history unavailable");
    expect(store.conversationPage(ONLINE_SESSION_ID)?.paging.error).toBeNull();
    store.loadSession(ONLINE_SESSION_ID);
    expect(store.sessionFilesPage(ONLINE_SESSION_ID).refreshError).toContain("history unavailable");
    store.apply({ kind: "command_result", result: readResult({ requestId: lastCommand(gateway).body.requestId }) });
    expect(store.sessionFilesPage(ONLINE_SESSION_ID).refreshError).toContain("history unavailable");
    store.refreshFiles(ONLINE_SESSION_ID);
    expect(store.sessionFilesPage(ONLINE_SESSION_ID).refreshError).toBeNull();
    expect(gateway.dispatches).toHaveLength(3);
  });
});

describe("文件栏：连接重置清理在途查询", () => {
  it("list 查询在途时连接 closed，手动刷新会重新派发", () => {
    const { store, gateway } = fileStore();
    store.selectFileSource(ONLINE_SESSION_ID, "all");
    expect(store.sessionFilesPage(ONLINE_SESSION_ID).all.directories.get("")?.loading).toBe(true);

    store.apply({ kind: "closed", code: 4500, retryable: true });
    expect(store.sessionFilesPage(ONLINE_SESSION_ID).all.directories.get("")?.loading).toBe(false);

    store.refreshFiles(ONLINE_SESSION_ID);
    expect(gateway.dispatches).toHaveLength(2);
    expect(lastCommand(gateway).body.command).toBe("session.files.list");
  });

  it("Git 状态在途时连接 blocked，来源 loading 复位并可再次刷新", () => {
    const { store, gateway } = fileStore();
    store.selectFileSource(ONLINE_SESSION_ID, "git");
    expect(store.sessionFilesPage(ONLINE_SESSION_ID).git.loading).toBe(true);

    store.apply({ kind: "blocked", cause: "device_revoked", code: 4401 });
    expect(store.sessionFilesPage(ONLINE_SESSION_ID).git.loading).toBe(false);

    store.refreshFiles(ONLINE_SESSION_ID);
    expect(gateway.dispatches).toHaveLength(2);
    expect(lastCommand(gateway).body.command).toBe("session.git.status");
  });
});
