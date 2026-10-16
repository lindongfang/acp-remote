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
  CREATE_EFFECT_NOTE,
  assertCreateSessionRefs,
  buildCreateSessionCommand,
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

/**
 * 取出带指定 `data-action` 的那个控件的**开标签**。
 *
 * 为什么不写成 `/<tag data-action=…>…/`：迁移到 RNW 原语后标签名由渲染器决定
 * （`role="button"` → `<button>`），标签不是本仓库的契约（`design.md` D2：机读证据
 * 从「标签 + 属性」降为「属性」）。同时属性顺序也不该被断言绑住——RNW 先写
 * `aria-disabled`/`disabled`，再写 `data-*`。因此在**同一个元素的开标签内**断言。
 */
function openingTagWithAction(html: string, action: string): string {
  const tag = new RegExp(`<[^>]*\\sdata-action="${action}"[^>]*>`).exec(html);
  if (tag === null) throw new Error(`渲染结果里找不到 data-action="${action}" 的控件`);
  return tag[0];
}

/** 尚未提交时的弹层模型（入口没有权限维度，因此不需要任何授权输入）。 */
function entryModel(): CreateSessionEntryModel {
  const { store } = storeWith({ resources: resources({ agents: AGENTS }) });
  return store.createEntry(ALIAS);
}

describe("R16 提交载荷只含一个目录引用与 Agent 标识", () => {
  it("别名载荷：键恰好是 agentId 与 workspaceAlias", () => {
    const command = buildCreateSessionCommand({
      refs: { workspaceAlias: ALIAS, agentId: "claude" },
      requestId: "4c4dafda-dd98-442e-8d55-252b75bac72d",
      messageId: "bbbbbbbb-1111-4111-8111-000000000009",
      connectionId: "2de54db7-74ae-4c21-88e3-05df0dc2e637",
    });

    const payload = command.body.payload as Record<string, unknown>;
    expect(Object.keys(payload).sort()).toEqual(["agentId", "workspaceAlias"]);
    // 反例：若命令里带了任何凭据字面量，这里会出现它们，断言变红。
    const serialized = JSON.stringify(command);
    expect(serialized).not.toMatch(/token|password|apiKey|mcp|exportId|templateParams/i);
  });

  it("目录载荷：键恰好是 agentId 与 workspacePath", () => {
    const command = buildCreateSessionCommand({
      refs: { workspacePath: "D:\\work\\api", agentId: "claude" },
      requestId: "4c4dafda-dd98-442e-8d55-252b75bac72d",
      messageId: "bbbbbbbb-1111-4111-8111-00000000000a",
      connectionId: "2de54db7-74ae-4c21-88e3-05df0dc2e637",
    });

    const payload = command.body.payload as Record<string, unknown>;
    // 反例：把路径塞进 workspaceAlias（或反过来丢弃它）→ 键集与取值都会红。
    expect(Object.keys(payload).sort()).toEqual(["agentId", "workspacePath"]);
    expect(payload["workspacePath"]).toBe("D:\\work\\api");
  });

  it("越界载荷与「目录引用不唯一」一律被拒绝", () => {
    const violations: readonly Record<string, unknown>[] = [
      { workspaceAlias: ALIAS, agentId: "claude", cwd: "D:\\work\\api" },
      { workspaceAlias: ALIAS, agentId: "claude", path: "/home/dev/api" },
      { workspaceAlias: ALIAS, agentId: "claude", exportId: "node-1" },
      { workspaceAlias: ALIAS, agentId: "claude", mcpServers: { fs: {} } },
      { workspaceAlias: ALIAS, agentId: "claude", token: "sk-xxx" },
      { workspaceAlias: ALIAS, agentId: "claude", templateParams: { a: 1 } },
      // 目录引用必须**恰好一个**：两个都给，或都不给。
      { workspaceAlias: ALIAS, workspacePath: "D:\\work\\api", agentId: "claude" },
      { agentId: "claude" },
    ];

    for (const payload of violations) {
      // 反例：若断言放宽成「至少一个目录引用」，后两条不会抛错，用例变红。
      expect(() => assertCreateSessionRefs(payload)).toThrow();
    }
    // 对照：两种合法载荷仍然通过（证明断言不是「一律拒绝」）。
    expect(assertCreateSessionRefs({ workspaceAlias: ALIAS, agentId: "claude" })).toEqual({
      workspaceAlias: ALIAS,
      agentId: "claude",
    });
    expect(assertCreateSessionRefs({ workspacePath: "D:\\work\\api", agentId: "claude" })).toEqual({
      workspacePath: "D:\\work\\api",
      agentId: "claude",
    });
  });

  it("实际派发的命令：别名与目录两条路径的载荷键都只有两个", () => {
    const { store, gateway } = storeWith({ resources: resources({ agents: AGENTS }) });

    const aliased = store.submitCreateSession({ directoryAlias: ALIAS, agentId: "claude" });
    expect(aliased.ok).toBe(true);
    const aliasedBody = gateway.dispatches[0]?.body;
    if (aliasedBody === undefined || aliasedBody.command !== "session.create") throw new Error("未派发 session.create");
    expect(Object.keys(aliasedBody.payload).sort()).toEqual(["agentId", "workspaceAlias"]);
    expect(aliasedBody.requestId).toBe(aliased.ok ? aliased.requestId : "");

    // 用户输入目录时走 `workspacePath`：别名那份不再出现在载荷里。
    const pathBased = store.submitCreateSession({
      directoryAlias: ALIAS,
      agentId: "claude",
      directoryPath: "D:\\work\\api",
    });
    expect(pathBased.ok).toBe(true);
    const pathBody = gateway.dispatches[1]?.body;
    if (pathBody === undefined || pathBody.command !== "session.create") throw new Error("未派发 session.create");
    expect(Object.keys(pathBody.payload).sort()).toEqual(["agentId", "workspacePath"]);
    expect((pathBody.payload as Record<string, unknown>)["workspacePath"]).toBe("D:\\work\\api");
  });
});

describe("R16 候选只来自本机已配置的 Agent", () => {
  it("弹层只列出快照里的 Agent", () => {
    const html = renderSheet(entryModel());
    expect(html).toContain('data-agent-id="claude"');
    expect(html).toContain('data-agent-id="codex"');
    // 反例：若候选来自任何其它来源（例如本地配置或用户输入），这里会出现它，断言变红。
    expect(html).not.toContain("gemini");
    // 弹层里唯一的文本输入是**目录**输入（Agent 不是自由文本）；多出别的输入框会变红。
    expect(html.match(/<input/g) ?? []).toHaveLength(1);
    expect(html).toContain('data-create-directory="true"');
  });

  it("选一个不在目录里的 Agent 提交：被拒绝且一条命令都不发", () => {
    const { store, gateway } = storeWith({ resources: resources({ agents: AGENTS }) });

    const outcome = store.submitCreateSession({ directoryAlias: ALIAS, agentId: "not-installed" });

    expect(outcome.ok).toBe(false);
    // 反例：若客户端不校验就把用户选的 Agent 发出去，这里会出现派发记录，断言变红。
    expect(gateway.dispatches).toEqual([]);
  });

  it("与授权无关：没有任何授权输入也照样能提交（创建入口不再有权限门控）", () => {
    const { store, gateway } = storeWith({ resources: resources({ agents: AGENTS }) });

    const outcome = store.submitCreateSession({ directoryAlias: ALIAS, agentId: "claude" });

    // 反例：若门控还在，`ok` 为 false（理由「没有创建会话的权限」）且 `dispatches` 为空 → 两条断言都红。
    expect(outcome.ok).toBe(true);
    expect(gateway.dispatches).toHaveLength(1);
  });
});

describe("R16 入口不因权限不可用", () => {
  it("入口恒可用：没有授权输入也渲染可点的提交按钮，且没有任何权限文案", () => {
    // 与旧实现成对：那时这里会渲染 `data-create-permission="denied"`、按钮带 `disabled`，
    // 并出现「这台设备没有创建会话的权限，需要在电脑端授予后才能使用。」。
    const html = renderSheet(entryModel(), "claude");

    expect(html).not.toContain("data-create-permission");
    expect(html).not.toContain("电脑端");
    expect(html).not.toContain("没有创建会话的权限");
    expect(html).toContain('data-action="submit-create"');
    // 反例：入口若仍被门控（或藏起来），按钮会带禁用态 / 不出现 → 断言变红。
    expect(openingTagWithAction(html, "submit-create")).not.toContain("disabled");
  });

  it("副作用说明照常呈现，且目录输入有可理解的失败提示", () => {
    const html = renderSheet(entryModel(), "claude");
    // spec「创建入口的副作用说明」：入口旁必须说明会在电脑上启动一个 Agent 进程。
    expect(html).toContain("将在你的电脑上启动一个 Agent 进程");
    expect(html).toContain(CREATE_EFFECT_NOTE);
    // 目录输入的提示：说清留空用本目录、填写要绝对路径，且不声称服务端会替用户拦截。
    expect(html).toContain('data-create-directory-hint="true"');
    expect(html).toContain("绝对目录");
    expect(html).not.toContain("服务端会拦截");
  });

  it("目录输入为空时提交按钮可用（不因缺少目录路径而禁用）", () => {
    const html = renderSheet(entryModel(), "claude");
    // 目录留空是有意义的默认（用本目录别名），因此它不该让入口不可点。
    expect(openingTagWithAction(html, "submit-create")).not.toContain("disabled");
  });
});

describe("R16 创建中与终态分别呈现", () => {
  it("提交后呈现「创建中」且不接受重复提交", () => {
    const { store, gateway } = storeWith({ resources: resources({ agents: AGENTS }) });
    store.submitCreateSession({ directoryAlias: ALIAS, agentId: "claude" });

    const model = store.createEntry(ALIAS);
    expect(model.submission.kind).toBe("creating");
    const html = renderSheet(model, "claude");

    expect(html).toContain('data-create-submission="creating"');
    expect(html).toContain("创建中");
    // 反例：若创建中仍可重复提交，按钮不会带禁用态，断言变红。
    // RNW 对 `role="button"` 且 `disabled` 的可点控件同时渲染原生 `disabled` 与 `aria-disabled`，
    // 两者都落在同一个开标签里（属性顺序由渲染器决定，因此断言不看顺序）。
    const submit = openingTagWithAction(html, "submit-create");
    expect(submit).toContain("aria-disabled=\"true\"");
    // e2e 判定的是**原生**属性（`hasAttribute("disabled")`，见 `e2e/page/checks/r16-create.ts:74`），
    // 所以单独钉住它：`\s` 前缀把 `aria-disabled` 排除在外。
    expect(submit).toMatch(/\sdisabled\b/);
    expect(gateway.dispatches).toHaveLength(1);
  });

  it("completed 终态进入已创建并带新会话标识", () => {
    const { store } = storeWith({ resources: resources({ agents: AGENTS }) });
    const outcome = store.submitCreateSession({ directoryAlias: ALIAS, agentId: "claude" });
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
    const { store } = storeWith({ resources: resources({ agents: AGENTS }) });
    const outcome = store.submitCreateSession({ directoryAlias: ALIAS, agentId: "claude" });
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
    const { store } = storeWith({ resources: resources({ agents: AGENTS }) });
    const outcome = store.submitCreateSession({ directoryAlias: ALIAS, agentId: "claude" });
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
