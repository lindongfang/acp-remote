/**
 * R16：会话创建是受限入口（模型层 + 渲染层）。
 *
 * 逐条 MUST NOT 的负向用例：
 *
 * - **载荷只含两个引用** → 对越界载荷（`cwd`/`path`/`mcpServers`/`token`）构造违规输入，
 *   断言 `assertCreateSessionRefs` 抛错；并断言实际派发的命令 JSON 里只有那两个键、
 *   不含任何路径或凭据字面量。
 * - **候选只来自本机已配置的 Agent** → 选一个不在快照 `agents` 里的 Agent 提交，
 *   断言被拒绝且**一条命令都没发**。
 * - **无权限时入口不可用且说明原因** → 断言按钮 `disabled` **且**说明文字出现在 HTML 里。
 * - **创建中与终态分别呈现** → 提交中的记录渲染「创建中」并禁止重复提交；
 *   `completed` 渲染已创建；`uncertain` 渲染显眼区块。
 * - **结果不确定不被自动消解** → 之后再来无关事件，它仍然在列表里。
 */

import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { CreateSessionSheet } from "./CreateSessionSheet";
import {
  CREATE_DENIED_NOTE,
  assertCreateSessionRefs,
  buildCreateSessionCommand,
  createSessionPermission,
} from "../features/create-session-model";
import type { CreateSessionEntryModel } from "../features/create-session-model";
import { resources, storeWith } from "../features/testing/fixtures";

const ALIAS = "work-api";
const AGENTS = [
  { agentId: "claude", displayName: "Claude", default: true },
  { agentId: "codex", displayName: "codex", default: false },
];

/** 渲染创建弹层。 */
function renderSheet(model: CreateSessionEntryModel, selectedAgentId: string | null = null): string {
  return renderToStaticMarkup(
    createElement(CreateSessionSheet, {
      model,
      selectedAgentId,
      onSelectAgent: () => undefined,
      onSubmit: () => undefined,
      onDismissUncertain: () => undefined,
      onClose: () => undefined,
    }),
  );
}

/** 有权限 + 尚未提交时的弹层模型。 */
function grantedModel(): CreateSessionEntryModel {
  const { store } = storeWith({ resources: resources({ agents: AGENTS }), scopes: ["session.list", "session.create"] });
  return store.createEntry(ALIAS);
}

describe("R16 提交载荷只含两个引用", () => {
  it("正常载荷的键恰好是两个引用", () => {
    const command = buildCreateSessionCommand({
      refs: { workspaceAlias: ALIAS, agentId: "claude" },
      requestId: "4c4dafda-dd98-442e-8d55-252b75bac72d",
      messageId: "bbbbbbbb-1111-4111-8111-000000000009",
      connectionId: "2de54db7-74ae-4c21-88e3-05df0dc2e637",
    });

    const payload = command.body.payload as Record<string, unknown>;
    expect(Object.keys(payload).sort()).toEqual(["agentId", "workspaceAlias"]);
    // 反例：若命令里带了任何路径或凭据字面量，这里会出现它们，断言变红。
    const serialized = JSON.stringify(command);
    expect(serialized).not.toContain("D:\\");
    expect(serialized).not.toContain("/home/");
    expect(serialized).not.toMatch(/token|password|apiKey|mcp|cwd|exportId|templateParams/i);
  });

  it("越界载荷（路径/目录追加/MCP/凭据）一律被拒绝", () => {
    const violations: readonly Record<string, unknown>[] = [
      { workspaceAlias: ALIAS, agentId: "claude", cwd: "D:\\work\\api" },
      { workspaceAlias: ALIAS, agentId: "claude", path: "/home/dev/api" },
      { workspaceAlias: ALIAS, agentId: "claude", exportId: "node-1" },
      { workspaceAlias: ALIAS, agentId: "claude", mcpServers: { fs: {} } },
      { workspaceAlias: ALIAS, agentId: "claude", token: "sk-xxx" },
      { workspaceAlias: ALIAS, agentId: "claude", templateParams: { a: 1 } },
    ];

    for (const payload of violations) {
      // 反例：若断言放宽成「至少两键」，这里不会抛错，用例变红。
      expect(() => assertCreateSessionRefs(payload)).toThrow(/越界/);
    }
    // 对照：合法载荷仍然通过（证明断言不是「一律拒绝」）。
    expect(assertCreateSessionRefs({ workspaceAlias: ALIAS, agentId: "claude" })).toEqual({
      workspaceAlias: ALIAS,
      agentId: "claude",
    });
  });

  it("实际派发的命令：载荷键只有两个，且带上稳定 requestId", () => {
    const { store, gateway } = storeWith({
      resources: resources({ agents: AGENTS }),
      scopes: ["session.create"],
    });

    const outcome = store.submitCreateSession({ workspaceAlias: ALIAS, agentId: "claude" });

    expect(outcome.ok).toBe(true);
    expect(gateway.dispatches).toHaveLength(1);
    const body = gateway.dispatches[0]?.body;
    if (body === undefined || body.command !== "session.create") throw new Error("未派发 session.create");
    expect(Object.keys(body.payload).sort()).toEqual(["agentId", "workspaceAlias"]);
    expect(body.requestId).toBe(outcome.ok ? outcome.requestId : "");
  });
});

describe("R16 候选只来自本机已配置的 Agent", () => {
  it("弹层只列出快照里的 Agent", () => {
    const html = renderSheet(grantedModel());
    expect(html).toContain('data-agent-id="claude"');
    expect(html).toContain('data-agent-id="codex"');
    // 反例：若候选来自任何其它来源（例如本地配置或用户输入），这里会出现它，断言变红。
    expect(html).not.toContain("gemini");
    expect(html).not.toContain("<input");
  });

  it("选一个不在目录里的 Agent 提交：被拒绝且一条命令都不发", () => {
    const { store, gateway } = storeWith({
      resources: resources({ agents: AGENTS }),
      scopes: ["session.create"],
    });

    const outcome = store.submitCreateSession({ workspaceAlias: ALIAS, agentId: "not-installed" });

    expect(outcome.ok).toBe(false);
    // 反例：若客户端不校验就把用户选的 Agent 发出去，这里会出现派发记录，断言变红。
    expect(gateway.dispatches).toEqual([]);
  });

  it("无权限时提交同样被拒绝且不发命令", () => {
    const { store, gateway } = storeWith({
      resources: resources({ agents: AGENTS }),
      scopes: ["session.list", "session.read"],
    });

    const outcome = store.submitCreateSession({ workspaceAlias: ALIAS, agentId: "claude" });

    expect(outcome.ok).toBe(false);
    expect(gateway.dispatches).toEqual([]);
  });
});

describe("R16 无权限态不可用且说明原因", () => {
  it("权限不含 session.create：入口 disabled，且说明「需要电脑端授予」", () => {
    const denied = createSessionPermission(["session.list", "session.read"]);
    const html = renderSheet({
      directoryAlias: ALIAS,
      permission: denied,
      agents: [],
      submission: { kind: "idle" },
      uncertainRequiresUserAction: false,
    });

    expect(html).toContain('data-create-permission="denied"');
    expect(html).toContain("disabled");
    // MUST NOT 仅隐藏入口而不作说明。
    expect(html).toContain(CREATE_DENIED_NOTE);
    expect(html).toContain("电脑端");
  });

  it("对照：有权限时入口可用（证明上一条不是因为按钮永远禁用）", () => {
    const html = renderSheet(grantedModel(), "claude");
    expect(html).toContain('data-create-permission="granted"');
    expect(html).not.toContain(CREATE_DENIED_NOTE);
    expect(html).toContain('data-action="submit-create"');
  });

  it("尚未认证时同样不可用，且理由与「没有权限」可区分", () => {
    const permission = createSessionPermission(null);
    expect(permission.kind).toBe("denied");
    if (permission.kind !== "denied") return;
    expect(permission.reason).toContain("尚未连接");
    expect(permission.note).toBe(CREATE_DENIED_NOTE);
  });
});

describe("R16 创建中与终态分别呈现", () => {
  it("提交后呈现「创建中」且不接受重复提交", () => {
    const { store, gateway } = storeWith({
      resources: resources({ agents: AGENTS }),
      scopes: ["session.create"],
    });
    store.submitCreateSession({ workspaceAlias: ALIAS, agentId: "claude" });

    const model = store.createEntry(ALIAS);
    expect(model.submission.kind).toBe("creating");
    const html = renderSheet(model, "claude");

    expect(html).toContain('data-create-submission="creating"');
    expect(html).toContain("创建中");
    // 反例：若创建中仍可重复提交，按钮不会带 disabled，断言变红。
    expect(html).toMatch(/data-action="submit-create"[^>]*disabled/);
    expect(gateway.dispatches).toHaveLength(1);
  });

  it("completed 终态进入已创建并带新会话标识", () => {
    const { store } = storeWith({ resources: resources({ agents: AGENTS }), scopes: ["session.create"] });
    const outcome = store.submitCreateSession({ workspaceAlias: ALIAS, agentId: "claude" });
    if (!outcome.ok) throw new Error("提交应当成功");

    store.apply({
      kind: "command_result",
      result: {
        protocolVersion: 1,
        type: "command.result",
        messageId: "bbbbbbbb-1111-4111-8111-000000000001",
        connectionId: "2de54db7-74ae-4c21-88e3-05df0dc2e637",
        connectionSequence: "1",
        body: {
          requestId: outcome.requestId,
          command: "session.create",
          status: "completed",
          acceptedAt: "2026-10-01T00:00:01.000Z",
          terminalEventId: null,
          result: { sessionId: "5d73cd10-a465-43cd-b3f1-704e2d49e99e" },
          error: null,
        },
      },
    });

    const model = store.createEntry(ALIAS);
    expect(model.submission).toEqual({ kind: "created", sessionId: "5d73cd10-a465-43cd-b3f1-704e2d49e99e" });
    expect(renderSheet(model, "claude")).toContain('data-create-submission="created"');
  });

  it("失败终态呈现结构化失败原因（码与可重试分别可见）", () => {
    const { store } = storeWith({ resources: resources({ agents: AGENTS }), scopes: ["session.create"] });
    const outcome = store.submitCreateSession({ workspaceAlias: ALIAS, agentId: "claude" });
    if (!outcome.ok) throw new Error("提交应当成功");

    store.apply({
      kind: "command_result",
      result: {
        protocolVersion: 1,
        type: "command.result",
        messageId: "bbbbbbbb-1111-4111-8111-000000000002",
        connectionId: "2de54db7-74ae-4c21-88e3-05df0dc2e637",
        connectionSequence: "1",
        body: {
          requestId: outcome.requestId,
          command: "session.create",
          status: "failed",
          acceptedAt: "2026-10-01T00:00:01.000Z",
          terminalEventId: null,
          result: null,
          error: { code: "capability.unsupported_by_agent", message: "Agent 缺少凭据", retryable: false, details: {} },
        },
      },
    });

    const html = renderSheet(store.createEntry(ALIAS), "claude");
    expect(html).toContain('data-create-submission="failed"');
    expect(html).toContain('data-failure-code="capability.unsupported_by_agent"');
    expect(html).toContain('data-failure-retryable="false"');
    expect(html).toContain("Agent 缺少凭据");
  });
});

describe("R16 结果不确定显眼且不被自动消解", () => {
  it("服务端确认 uncertain 后呈现显眼区块，并在后续事件后仍然保留", () => {
    const { store } = storeWith({ resources: resources({ agents: AGENTS }), scopes: ["session.create"] });
    const outcome = store.submitCreateSession({ workspaceAlias: ALIAS, agentId: "claude" });
    if (!outcome.ok) throw new Error("提交应当成功");

    store.apply({
      kind: "command_result",
      result: {
        protocolVersion: 1,
        type: "command.result",
        messageId: "bbbbbbbb-1111-4111-8111-000000000003",
        connectionId: "2de54db7-74ae-4c21-88e3-05df0dc2e637",
        connectionSequence: "1",
        body: {
          requestId: outcome.requestId,
          command: "session.create",
          status: "uncertain",
          acceptedAt: "2026-10-01T00:00:01.000Z",
          terminalEventId: null,
          result: null,
          error: null,
        },
      },
    });

    expect(store.state.uncertainRequestIds).toEqual([outcome.requestId]);
    expect(store.createEntry(ALIAS).uncertainRequiresUserAction).toBe(true);
    const html = renderSheet(store.createEntry(ALIAS), "claude");
    expect(html).toContain('data-create-submission="uncertain"');
    expect(html).toContain('role="alert"');

    // 再来一条无关事件与一次「时间推进」（now 变了）：它必须仍在。
    store.apply({
      kind: "event",
      event: {
        protocolVersion: 1,
        type: "event",
        messageId: "11111111-1111-4111-8111-000000000009",
        connectionId: "2de54db7-74ae-4c21-88e3-05df0dc2e637",
        connectionSequence: "1",
        body: {
          globalSequence: "11",
          sessionSequence: null,
          eventId: "dddddddd-0000-4000-8000-000000000009",
          sessionId: null,
          eventType: "session.updated",
          causationRequestId: null,
          origin: { kind: "daemon", deviceId: null },
          remoteOrigin: null,
          createdAt: "2026-10-01T00:10:00.000Z",
          payload: { view: {} },
        },
      },
    });

    expect(store.state.uncertainRequestIds).toEqual([outcome.requestId]);
    // 用户主动处理之后才消失。
    store.dismissUncertain(outcome.requestId);
    expect(store.state.uncertainRequestIds).toEqual([]);
  });
});
