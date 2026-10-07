/**
 * 组件层测试的共享夹具。
 *
 * 这里的每一份数据都**刻意**带上违规输入可能携带的字段（目录项上的 `path`、
 * 事件上的未知 `eventType`、原文的 `rawUnavailable`），这样负向断言才有意义：
 * 组件必须把它们原样丢掉或明确说明，而不是渲染成「看起来完整」的内容。
 *
 * 夹具只提供数据与构造器，不复制任何被测判定逻辑。
 */

import type {
  CommandMessage,
  EventMessage,
  RawAcp,
  SessionSummary,
  SnapshotItemMessages,
  WorkspaceRef,
} from "../../protocol";
import type { CompletedSnapshot, StagedResources } from "../../sync-client";
import { ClientStore } from "../client-store";
import type { SyncGateway } from "../ports";

/** 目录项带路径字段：违规输入的样子。 */
export const WORKSPACES_WITH_PATHS = [
  { alias: "work-api", displayName: "api", path: "D:\\work\\api", root: "D:\\work" },
  { alias: "personal-api", displayName: "api", path: "D:\\personal\\api", root: "D:\\personal" },
] as unknown as readonly WorkspaceRef[];

/** 两条同名目录（展示名相同，别名不同）。 */
export const WORKSPACES = [
  { alias: "work-api", displayName: "api" },
  { alias: "personal-api", displayName: "api" },
] as const satisfies readonly WorkspaceRef[];

/** 会话摘要构造器。 */
export function session(input: {
  readonly id: string;
  readonly state?: SessionSummary["state"];
  readonly alias?: string | null;
  readonly title?: string | null;
  readonly origin?: SessionSummary["origin"];
}): SessionSummary {
  const alias = input.alias === undefined ? "work-api" : input.alias;
  return {
    sessionId: input.id,
    title: input.title === undefined ? `会话 ${input.id.slice(-4)}` : input.title,
    agent: { agentId: "claude", name: "Claude" },
    state: input.state ?? "idle",
    origin: input.origin ?? { kind: "local" },
    currentMode: null,
    version: "1",
    createdAt: "2026-10-01T00:00:00.000Z",
    updatedAt: "2026-10-01T00:00:00.000Z",
    workspace: alias === null ? null : { alias, displayName: alias },
  };
}

/** 默认快照资源：两个同名目录、两条会话、两个同名 Agent。 */
export function resources(input: {
  readonly sessions?: readonly SessionSummary[];
  readonly workspaces?: readonly WorkspaceRef[];
  readonly agents?: StagedResources["agents"];
}): StagedResources {
  return {
    sessions: input.sessions ?? [session({ id: "aaaaaaaa-0000-4000-8000-000000000001", state: "running" })],
    workspaces: input.workspaces ?? WORKSPACES,
    agents: input.agents ?? [
      { agentId: "codex-a", displayName: "codex", default: true },
      { agentId: "codex-b", displayName: "codex", default: false },
    ],
  };
}

/** 记账式同步网关。 */
export interface RecordingGateway extends SyncGateway {
  readonly dispatches: CommandMessage[];
}

export function recordingGateway(input: {
  readonly resources: StagedResources | null;
  readonly scopes?: readonly string[] | null;
  readonly connectionId?: string | null;
}): RecordingGateway {
  const dispatches: CommandMessage[] = [];
  const current: CompletedSnapshot | null =
    input.resources === null
      ? null
      : {
          snapshotId: "8194de43-e213-423d-acf4-2e3549304566",
          cursor: { serverEpoch: "00384a03-bc90-4095-b65d-82fb8cc47e13", globalSequence: "10" },
          resources: input.resources,
          verifiedAtMs: 1_700_000_000_000,
        };
  return {
    dispatches,
    current,
    authenticatedInfo:
      input.scopes === undefined || input.scopes === null
        ? null
        : {
            deviceId: "2ae1c07c-9242-46e9-a9d2-4ec58c130f49",
            scopes: input.scopes,
            serverEpoch: "00384a03-bc90-4095-b65d-82fb8cc47e13",
            headGlobalSequence: "10",
            heartbeatIntervalMs: 30000,
            limits: {
              maxMessageBytes: 1048576,
              maxPromptBytes: 262144,
              maxReplayEventsPerBatch: 500,
            },
          },
    dispatch(command: CommandMessage) {
      dispatches.push(command);
    },
  };
}

/** 建一个已喂入快照（必要时还有认证结果）的仓库。 */
export function storeWith(input: {
  readonly resources: StagedResources | null;
  readonly scopes?: readonly string[] | null;
  readonly connectionId?: string | null;
}): { store: ClientStore; gateway: RecordingGateway } {
  const gateway = recordingGateway(input);
  let counter = 0;
  const store = new ClientStore({
    sync: gateway,
    ids: {
      uuid(): string {
        counter += 1;
        return `00000000-0000-4000-8000-${counter.toString().padStart(12, "0")}`;
      },
    },
    now: () => 1_700_000_000_000,
    // `??` 会把「显式传 null（未连接）」当成缺省，因此这里区分 undefined 与 null。
    identity: { connectionId: input.connectionId === undefined ? "2de54db7-74ae-4c21-88e3-05df0dc2e637" : input.connectionId },
  });

  if (input.resources !== null) {
    store.apply({
      kind: "snapshot_verified",
      snapshot: {
        ...(gateway.current as CompletedSnapshot),
        chunkCount: 3,
        receivedChunks: 3,
        snapshotDigest: "kQ8x2mB7yV1cJ0nP5",
      },
    });
  }
  if (input.scopes !== undefined && input.scopes !== null) {
    store.apply({
      kind: "authenticated",
      info: gateway.authenticatedInfo as NonNullable<SyncGateway["authenticatedInfo"]>,
    });
  }
  return { store, gateway };
}

/** 一条事件消息（降级用例的原料）。 */
export function eventMessage(input: {
  readonly eventType: string;
  readonly eventId?: string;
  readonly sessionId?: string | null;
  readonly view?: Record<string, unknown>;
  readonly acp?: RawAcp;
  readonly globalSequence?: string;
}): EventMessage {
  const payload: { view: Record<string, unknown>; acp?: RawAcp } = { view: input.view ?? {} };
  if (input.acp !== undefined) payload.acp = input.acp;
  return {
    protocolVersion: 1,
    type: "event",
    messageId: `11111111-1111-4111-8111-${(input.globalSequence ?? "1").padStart(12, "0")}`,
    connectionId: "2de54db7-74ae-4c21-88e3-05df0dc2e637",
    connectionSequence: "1",
    body: {
      globalSequence: input.globalSequence ?? "1",
      sessionSequence: null,
      eventId: input.eventId ?? "dddddddd-0000-4000-8000-000000000001",
      sessionId: input.sessionId ?? null,
      eventType: input.eventType,
      causationRequestId: null,
      origin: { kind: "agent", deviceId: null },
      remoteOrigin: null,
      createdAt: "2026-10-01T00:00:02.000Z",
      payload,
    },
  };
}

/** `session.read` 的一条消息项。 */
export function messageItem(input: {
  readonly id: string;
  readonly createdAt: string;
  readonly role?: "user" | "agent";
  readonly text?: string;
  readonly blocks?: SnapshotItemMessages["content"];
}): SnapshotItemMessages {
  return {
    messageId: input.id,
    sessionId: "aaaaaaaa-0000-4000-8000-000000000001",
    role: input.role ?? "user",
    content: input.blocks ?? [{ type: "text", text: input.text ?? "你好" }],
    status: "done",
    createdAt: input.createdAt,
    turnId: null,
  };
}
