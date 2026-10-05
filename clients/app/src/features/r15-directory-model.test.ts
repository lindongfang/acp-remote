/**
 * R15：目录页以目录为主角且可离线渲染（模型层与仓库层）。
 *
 * 渲染层（`DirectoryList`）的负向断言在 `src/components/r15-directory-list.test.ts`；
 * 本文件负责两件只能在这里证伪的事：
 *
 * 1. **MUST NOT 依赖额外的在线查询命令**：注入一个「任何 `dispatch` 都记账」的替身，
 *    走完目录页的完整读取路径后断言**一次都没有派发**。
 * 2. **同名目录不合并 / 两种空态可区分 / 断连仍可渲染**：模型层的判定。
 *
 * 判别力来源：反例（把目录项渲染成含路径、或让目录页发 `session.list`）会让
 * 「模型里没有路径字段」与「`dispatches` 为空」这两条断言变红；
 * 对照用例（用户意图确实会派发）证明派发通道本身是通的，不是被误伤。
 */

import { describe, expect, it } from "vitest";

import type { CommandMessage, SessionSummary, WorkspaceRef } from "../protocol";
import type { CompletedSnapshot, StagedResources } from "../sync-client";
import { ClientStore } from "./client-store";
import type { SyncGateway } from "./ports";
import { buildDirectoryDetailModel, buildDirectoryPageModel } from "./directory-model";

/**
 * 带路径字段的目录项。
 *
 * wire 上的 `WorkspaceRef` 只有 `alias`/`displayName`；这里额外挂上路径类字段，
 * 模拟「宿主多带了字段」或「有人把路径塞进展示数据」的违规输入。
 * 模型必须**只**取展示名与别名。
 */
const WORKSPACES_WITH_PATHS = [
  { alias: "work-api", displayName: "api", path: "D:\\work\\api", root: "D:\\work", cwd: "D:\\work\\api" },
  { alias: "personal-api", displayName: "api", path: "D:\\personal\\api", root: "D:\\personal" },
  { alias: "notes", displayName: "notes", path: "/home/dev/notes" },
] as unknown as readonly WorkspaceRef[];

const WORKSPACES = [
  { alias: "work-api", displayName: "api" },
  { alias: "personal-api", displayName: "api" },
  { alias: "notes", displayName: "notes" },
] as const satisfies readonly WorkspaceRef[];

/** 一条会话摘要（`currentMode`/`version` 等字段必须齐备，与 wire 一致）。 */
function session(input: {
  readonly id: string;
  readonly state: SessionSummary["state"];
  readonly alias?: string | null;
}): SessionSummary {
  const alias = input.alias === undefined ? "work-api" : input.alias;
  return {
    sessionId: input.id,
    title: `会话 ${input.id.slice(-4)}`,
    agent: { agentId: "claude", name: "Claude" },
    state: input.state,
    origin: { kind: "local" },
    currentMode: null,
    version: "1",
    createdAt: "2026-10-01T00:00:00.000Z",
    updatedAt: "2026-10-01T00:00:00.000Z",
    workspace: alias === null ? null : { alias, displayName: alias },
  };
}

const SESSIONS = [
  session({ id: "aaaaaaaa-0000-4000-8000-000000000001", state: "running" }),
  session({ id: "aaaaaaaa-0000-4000-8000-000000000002", state: "waiting_permission" }),
  session({ id: "aaaaaaaa-0000-4000-8000-000000000003", state: "idle", alias: "personal-api" }),
];

const RESOURCES: StagedResources = {
  sessions: SESSIONS,
  workspaces: WORKSPACES,
  agents: [{ agentId: "claude", displayName: "Claude", default: true }],
};

/** 已完成快照（仓库面）：目录页读的是它的 `resources`。 */
function snapshotOf(resources: StagedResources): CompletedSnapshot {
  return {
    snapshotId: "8194de43-e213-423d-acf4-2e3549304566",
    cursor: { serverEpoch: "00384a03-bc90-4095-b65d-82fb8cc47e13", globalSequence: "10" },
    resources,
    verifiedAtMs: 1_700_000_000_000,
  };
}

/** 记账式同步网关：任何命令派发都会被记录（用来证伪「目录页发了查询命令」）。 */
interface RecordingGateway extends SyncGateway {
  readonly dispatches: CommandMessage[];
}

function recordingGateway(resources: StagedResources): RecordingGateway {
  const dispatches: CommandMessage[] = [];
  return {
    dispatches,
    current: snapshotOf(resources),
    authenticatedInfo: null,
    dispatch(command: CommandMessage) {
      dispatches.push(command);
    },
  };
}

/** 确定性标识源。 */
function sequentialIds(): { uuid(): string } {
  let counter = 0;
  return {
    uuid(): string {
      counter += 1;
      return `00000000-0000-4000-8000-${counter.toString().padStart(12, "0")}`;
    },
  };
}

/** 建一个仓库并喂入一次「快照已验证通过」事件——资源只从这条路进来。 */
function storeWith(resources: StagedResources): { store: ClientStore; gateway: RecordingGateway } {
  const gateway = recordingGateway(resources);
  const store = new ClientStore({
    sync: gateway,
    ids: sequentialIds(),
    now: () => 1_700_000_000_000,
    identity: { connectionId: "2de54db7-74ae-4c21-88e3-05df0dc2e637" },
  });
  const snapshot = snapshotOf(resources);
  store.apply({
    kind: "snapshot_verified",
    snapshot: { ...snapshot, chunkCount: 3, receivedChunks: 3, snapshotDigest: "kQ8x2mB7yV1cJ0nP5" },
  });
  return { store, gateway };
}

describe("R15 目录项只含展示名与别名", () => {
  it("带路径字段的快照资源也只投影出展示名、别名与三项汇总", () => {
    const model = buildDirectoryPageModel({
      resources: { sessions: SESSIONS, workspaces: WORKSPACES_WITH_PATHS, agents: [] },
      connection: "online",
      lastSyncedAtMs: 1_700_000_000_000,
    });

    expect(model.kind).toBe("ready");
    if (model.kind !== "ready") return;
    // 反例：若模型把路径字段透传下来，这里会出现 `path`/`root` 键，断言变红。
    expect(Object.keys(model.directories[0] ?? {}).sort()).toEqual(["alias", "counts", "displayName"]);
    expect(JSON.stringify(model)).not.toContain("D:\\\\work");
    expect(JSON.stringify(model)).not.toContain("/home/dev");
  });

  it("同名目录不被合并：两个别名各自成项", () => {
    const model = buildDirectoryPageModel({
      resources: { sessions: SESSIONS, workspaces: WORKSPACES, agents: [] },
      connection: "online",
      lastSyncedAtMs: null,
    });

    expect(model.kind).toBe("ready");
    if (model.kind !== "ready") return;
    const sameName = model.directories.filter((item) => item.displayName === "api");
    // 反例：若按展示名去重，这里会只剩一项，断言变红。
    expect(sameName.map((item) => item.alias)).toEqual(["work-api", "personal-api"]);
  });

  it("三项汇总由会话摘要得出：断连时仍然给出", () => {
    const offline = buildDirectoryPageModel({
      resources: { sessions: SESSIONS, workspaces: WORKSPACES, agents: [] },
      connection: "disconnected",
      lastSyncedAtMs: 1_700_000_000_000,
    });

    expect(offline.kind).toBe("ready");
    if (offline.kind !== "ready") return;
    const workApi = offline.directories.find((item) => item.alias === "work-api");
    // 两条会话属于 work-api：一条 running（运行中 1）、一条 waiting_permission（待处理 1）。
    expect(workApi?.counts).toEqual({ total: 2, active: 1, pending: 1 });
    // 反例：断连时不给出汇总（而不是给 0），这里会变 null，断言变红。
    expect(offline.contentOrigin).toBe("last_sync");
  });
});

describe("R15 两种空态可区分", () => {
  it("一个目录都没有 → no_directories", () => {
    const model = buildDirectoryPageModel({
      resources: { sessions: [], workspaces: [], agents: [] },
      connection: "online",
      lastSyncedAtMs: null,
    });
    expect(model).toMatchObject({ kind: "empty", empty: "no_directories" });
  });

  it("目录存在但都没有会话 → directories_without_sessions（与上一种不是同一态）", () => {
    const model = buildDirectoryPageModel({
      resources: { sessions: [], workspaces: WORKSPACES, agents: [] },
      connection: "online",
      lastSyncedAtMs: null,
    });
    expect(model).toMatchObject({ kind: "empty", empty: "directories_without_sessions" });
  });

  it("从未同步过 → loading，不冒充任何一种空态", () => {
    // 反例：若把「无快照」当成「没有目录」，这里会变成 empty，断言变红。
    expect(buildDirectoryPageModel({ resources: null, connection: "online", lastSyncedAtMs: null })).toEqual({ kind: "loading" });
  });
});

describe("R15 目录数据只来自快照", () => {
  it("读取目录页、目录详情与主机面板都不派发任何命令", () => {
    const { store, gateway } = storeWith(RESOURCES);

    store.directoryPage();
    store.directoryDetail("work-api");
    store.hostPanel();

    // 反例：若目录页改走 `session.list` 或任何在线查询，这里会出现派发记录，断言变红。
    expect(gateway.dispatches).toEqual([]);
  });

  it("对照：用户意图（创建会话）确实会派发命令（证明上一条不是因为派发通道坏了）", () => {
    const { store, gateway } = storeWith(RESOURCES);
    store.apply({
      kind: "authenticated",
      info: {
        deviceId: "2ae1c07c-9242-46e9-a9d2-4ec58c130f49",
        scopes: ["session.list", "session.read", "session.create"],
        serverEpoch: "00384a03-bc90-4095-b65d-82fb8cc47e13",
        headGlobalSequence: "10",
        heartbeatIntervalMs: 30000,
        limits: {
          maxMessageBytes: 1048576,
          maxPromptBytes: 262144,
          maxReplayEventsPerBatch: 500,
        },
      },
    });

    const outcome = store.submitCreateSession({ workspaceAlias: "work-api", agentId: "claude" });

    expect(outcome.ok).toBe(true);
    expect(gateway.dispatches).toHaveLength(1);
  });
});

describe("R15 目录详情", () => {
  it("只列出该别名下的会话；别名不在快照里是第三种呈现", () => {
    const detail = buildDirectoryDetailModel({
      resources: { sessions: SESSIONS, workspaces: WORKSPACES, agents: [] },
      connection: "online",
      alias: "work-api",
      ownerLabelOf: () => "工作站",
    });
    expect(detail.kind).toBe("ready");
    if (detail.kind !== "ready") return;
    expect(detail.sessions.map((item) => item.id)).toEqual([
      "aaaaaaaa-0000-4000-8000-000000000001",
      "aaaaaaaa-0000-4000-8000-000000000002",
    ]);

    // 反例：别名失效若被当成「目录里没有会话」，这里会是 ready，断言变红。
    expect(
      buildDirectoryDetailModel({
        resources: { sessions: SESSIONS, workspaces: WORKSPACES, agents: [] },
        connection: "online",
        alias: "not-there",
        ownerLabelOf: () => "工作站",
      }),
    ).toMatchObject({ kind: "unknown_directory", alias: "not-there" });
  });
});
