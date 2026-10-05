/**
 * 用例共享的快照夹具与路由条目。
 *
 * ## 为什么夹具刻意携带违规字段
 *
 * `path` / `root` 是**协议里不存在**的字段（`WorkspaceRef` 只有 alias + displayName）。
 * 把它们塞进快照的 workspaces chunk，是为了让「不泄露本机路径」这条 MUST NOT 有一条
 * **能红**的用例：如果哪天有人在目录行里顺手渲染了 `directory.path`，
 * 负向断言会立刻抓住它，而不是靠「模型对象里没有这个键」蒙混过去。
 */

import type { ReactElement } from "react";

import type { SessionSummary, WorkspaceRef } from "../../src/protocol";
import DirectoriesRoute from "../../app/index";
import DirectoryDetailRoute from "../../app/dir/[alias]";
import SessionRoute from "../../app/session/[id]";
import type { RouteEntry } from "./router-stub";
import type { StagedResources } from "../../src/sync-client";


/** 违规路径片段：出现在任何一处渲染结果里都判红。 */
export const SECRET_FRAGMENTS = ["D:\\", "secret", "vault", "repo-x", "repo-y", "\\work"] as const;

/** 两条同名目录，各自带一个**不存在于协议**的路径字段。 */
export const WORKSPACES_WITH_PATHS = [
  { alias: "alpha", displayName: "api", path: "D:\\secret\\vault\\repo-x", root: "D:\\secret\\vault" },
  { alias: "beta", displayName: "api", path: "D:\\secret\\vault\\repo-y", root: "D:\\secret\\vault" },
] as unknown as readonly WorkspaceRef[];

/** 只含协议字段的同名目录（对照：不带违规字段时行为必须一致）。 */
export const WORKSPACES = [
  { alias: "alpha", displayName: "api" },
  { alias: "beta", displayName: "api" },
] as const satisfies readonly WorkspaceRef[];


/** 两条同名 Agent（`R19` 的同名区分与 `R16` 的候选来源）。 */
export const AGENTS = [
  { agentId: "codex-a", displayName: "codex", default: true },
  { agentId: "codex-b", displayName: "codex", default: false },
] as const satisfies StagedResources["agents"];

/** 目录 alpha 下的会话（运行中）。 */
export const SESSION_ALPHA = "aaaaaaaa-0000-4000-8000-00000000aa01";

/** 目录 beta 下的会话（等待输入 → 计入待处理）。 */
export const SESSION_BETA = "aaaaaaaa-0000-4000-8000-00000000bb01";

/** 构造一条会话摘要。 */
export function sessionSummary(input: {
  readonly id: string;
  readonly alias: string;
  readonly state: SessionSummary["state"];
}): SessionSummary {
  return {
    sessionId: input.id,
    title: `会话 ${input.id.slice(-2)}`,
    agent: { agentId: "codex-a", name: "codex" },
    state: input.state,
    origin: { kind: "local" },
    currentMode: null,
    version: "1",
    createdAt: "2026-10-01T00:00:00.000Z",
    updatedAt: "2026-10-01T00:00:00.000Z",
    workspace: { alias: input.alias, displayName: input.alias },
  };
}

/** 两条目录各一条会话：alpha 运行中（运行中 +1），beta 等待输入（待处理 +1）。 */
export function sessionsForBothDirectories(): readonly SessionSummary[] {
  return [
    sessionSummary({ id: SESSION_ALPHA, alias: "alpha", state: "running" }),
    sessionSummary({ id: SESSION_BETA, alias: "beta", state: "waiting_input" }),
  ];
}

/** 目录页路由。 */
export const DIRS_ROUTE: RouteEntry = { path: "/", component: DirectoriesRoute as RouteComponent, params: {} };

/** 目录详情路由。 */
export function dirRoute(alias: string): RouteEntry {
  return {
    path: `/dir/${alias}`,
    component: DirectoryDetailRoute as RouteComponent,
    params: { alias },
  };
}

/** 会话详情路由。 */
export function sessionRoute(sessionId: string): RouteEntry {
  return {
    path: `/session/${sessionId}`,
    component: SessionRoute as RouteComponent,
    params: { id: sessionId },
  };
}

/** 路由组件的最小形状（`RouteEntry.component` 就是它）。 */
type RouteComponent = () => ReactElement;

/**
 * 一条 `event` 消息原文。
 *
 * `globalSequence` 必须**严格递增且大于屏障**，否则会被去重账本判成回退/重复而丢弃——
 * 那会让降级卡片凭空消失，用例变成「断言了一个从未投递的事件」。
 */
export function wireEvent(input: {
  readonly globalSequence: number;
  readonly eventId: string;
  readonly sessionId: string;
  readonly eventType: string;
  readonly view?: Record<string, unknown>;
  readonly acp?: Record<string, unknown>;
}): Record<string, unknown> {
  const payload: { view: Record<string, unknown>; acp?: Record<string, unknown> } = {
    view: input.view ?? {},
  };
  if (input.acp !== undefined) payload.acp = input.acp;
  return {
    protocolVersion: 1,
    type: "event",
    messageId: `11111111-1111-4111-8111-${String(input.globalSequence).padStart(12, "0")}`,
    connectionSequence: "1",
    body: {
      globalSequence: String(input.globalSequence),
      sessionSequence: null,
      eventId: input.eventId,
      sessionId: input.sessionId,
      eventType: input.eventType,
      causationRequestId: null,
      origin: { kind: "agent", deviceId: null },
      remoteOrigin: null,
      createdAt: "2026-10-01T00:00:02.000Z",
      payload,
    },
  };
}

/** `session.read` 的空页应答（让会话页的线程有内容，避免把线程错误误当成降级问题）。 */
export function emptyReadResult(sessionId: string): Record<string, unknown> {
  return { sessionId, resources: { messages: [] }, hasEarlier: false };
}