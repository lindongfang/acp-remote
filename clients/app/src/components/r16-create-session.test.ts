/**
 * R16：会话创建是受限入口（模型层 + 渲染层）。
 *
 * 逐条 MUST NOT 的负向用例：
 *
 * - **载荷恰好两类键集** → 目录标识或浏览选定的路径：键集分别是
 *   `{agentId, directoryId}`、`{agentId, workspacePath}`；对越界载荷
 *   （`cwd`/`path`/`mcpServers`/`token`/`createIfMissing`）与「目录引用不唯一」构造违规输入，
 *   断言 `assertCreateSessionRefs` 抛错；并断言实际派发的命令 JSON 里只有那两类键、不含凭据。
 * - **候选只来自本机已配置的 Agent** → 选一个不在快照 `agents` 里的 Agent 提交，
 *   断言被拒绝且**一条命令都没发**。
 * - **目录不由用户手工输入** → 弹层里没有目录输入框；目录与路径逐字来自服务端（目录标识不出现文本里）。
 * - **服务端不支持浏览时明确说明而不是静默失败** → 顶部入口形态给出说明、控件改名为「选择已登记的
 *   目录」、回退列表列出已登记目录，且**一条浏览命令都不发**（发了必然被 feature 门控拒）。
 * - **不能浏览的原因分岔** → 「还没连上主机」与「已连上但未协商到该能力」是**两条**文案：前者 MUST NOT
 *   写成「服务端没有提供该能力」（未认证时 `selectedFeatures` 本来就是空的，本机无从知道服务端支持什么）。
 * - **创建中与终态分别呈现** → 提交中的记录渲染「创建中」并禁止重复提交；
 *   `completed` 渲染已创建；`uncertain` 渲染显眼区块。
 * - **结果不确定不被自动消解** → 之后再来无关事件，它仍然在列表里。
 */

import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { CommandResultMessage } from "../protocol";

import { CreateSessionSheet } from "./CreateSessionSheet";
import { DirectoryPicker } from "./DirectoryPicker";
import type { ClientStore } from "../features/client-store";
import type {
  BrowseUnavailableReason,
  CreateSessionEntryModel,
  DirectoryBrowseState,
  DirectorySelection,
  RegisteredDirectoryModel,
} from "../features/create-session-model";
import {
  BROWSE_EMPTY_FALLBACK_NOTICES,
  BROWSE_OFFLINE_NOTICE,
  BROWSE_TRUNCATED_NOTICE,
  BROWSE_UNSUPPORTED_NOTICE,
  CREATE_EFFECT_NOTE,
  CREATE_FOLDER_NAME_RULE,
  WORKSPACE_BROWSE_FEATURE,
  assertCreateSessionRefs,
  buildCreateSessionCommand,
  buildDirectoryBrowserModel,
  createDirectoryNoticeText,
  initialDirectoryBrowseState,
} from "../features/create-session-model";
import type { RecordingGateway } from "../features/testing/fixtures";
import { WORKSPACES_WITH_PATHS, onlineStore, resources, session, storeWith } from "../features/testing/fixtures";

const DIRECTORY_ID = "work-api";
const AGENTS = [
  { agentId: "claude", displayName: "Claude", default: true },
  { agentId: "codex", displayName: "codex", default: false },
] as const;

/** 只含协议字段的已登记目录（回退列表的输入）。 */
const REGISTERED: readonly RegisteredDirectoryModel[] = [
  { directoryId: "work-api", displayName: "api", path: "D:\\work\\api" },
  { directoryId: "personal-api", displayName: "api", path: "D:\\personal\\api" },
];

/** 渲染创建弹层。 */
function renderSheet(
  model: CreateSessionEntryModel,
  selectedAgentId: string | null = null,
  submitIssue: string | null = null,
): string {
  return renderToStaticMarkup(
    createElement(CreateSessionSheet, {
      model,
      selectedAgentId,
      submitIssue,
      onSelectAgent: () => undefined,
      onSubmit: () => undefined,
      onOpenDirectoryPicker: () => undefined,
      onClose: () => undefined,
    }),
  );
}

/** 渲染目录选择器（纯组件：直接喂一份浏览状态，不经过仓库）。 */
function renderPicker(input: {
  readonly supported: boolean;
  readonly browse: DirectoryBrowseState;
  readonly initialPath: string | null;
  readonly registeredDirectories?: readonly RegisteredDirectoryModel[];
  /** 「不能逐层浏览」的原因；**缺省不传**——与「调用方给不出更准的事实」那条路径逐字相同。 */
  readonly unavailable?: BrowseUnavailableReason;
}): string {
  return renderToStaticMarkup(
    createElement(DirectoryPicker, {
      initialPath: input.initialPath,
      registeredDirectories: input.registeredDirectories ?? REGISTERED,
      browser: buildDirectoryBrowserModel({ supported: input.supported, browse: input.browse }),
      // `exactOptionalPropertyTypes`：缺省时不能显式传 `undefined`。
      ...(input.unavailable === undefined ? {} : { unavailable: input.unavailable }),
      onBrowse: () => undefined,
      onCreateFolder: () => undefined,
      onConfirm: () => undefined,
      onCancel: () => undefined,
    }),
  );
}

/**
 * 渲染结果的**可见文本**（剥掉标签）。
 *
 * 「目录标识不出现在界面上」是文本口径的断言（目录标识仍作为机器标识留在 `data-*` 上，例如
 * `data-create-sheet`），因此它必须在文本上判，而不是在整段 HTML 上判。
 */
function textOfHtml(html: string): string {
  return html.replace(/<[^>]*>/g, "");
}

/** 取出带指定 `data-action` 的那个控件的**开标签**。
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

/**
 * 第 `index` 条 `session.create` 派发的载荷。
 *
 * 按命令名筛过一遍：目录浏览（`workspace.browse`）同样会进记账网关的派发列表，直接按下标取会把
 * 「第几条创建命令」读成「第几条命令」。形状逐层确证后按记录读键——断言要证明的正是这个对象的
 * 键集与取值。
 */
function createPayloadAt(gateway: RecordingGateway, index: number): Record<string, unknown> {
  const commands = gateway.dispatches.filter((command) => command.body.command === "session.create");
  const body = commands[index]?.body;
  if (body === undefined || body.command !== "session.create") {
    throw new Error(`派发记录里没有第 ${index} 条 session.create`);
  }
  const payload: unknown = body.payload;
  if (typeof payload !== "object" || payload === null || Array.isArray(payload)) {
    throw new Error("session.create 的载荷不是对象");
  }
  // 上面已确证「是对象、非数组」，这里只是补上索引签名。
  return payload as Record<string, unknown>;
}

/** 当前弹层模型（顶部入口形态：目录未知）。 */
function browseEntry(store: ClientStore): CreateSessionEntryModel {
  return store.createEntry(null);
}

/** 尚未提交时的弹层模型（目录带服务端下发的路径；入口没有权限维度，因此不需要任何授权输入）。 */
function entryModel(): CreateSessionEntryModel {
  // 新入口只在协商了会话准备能力后可用（未协商时明确禁用，不退回旧 session.create）。
  const { store } = onlineStore({
    resources: resources({ workspaces: WORKSPACES_WITH_PATHS, agents: [...AGENTS] }),
  });
  return store.createEntry(DIRECTORY_ID);
}

/** 给一条 `workspace.browse` 请求回一条结果：用例扮演服务端的那一侧。 */
function answerBrowse(
  store: ClientStore,
  requestId: string,
  result: { readonly path: string | null; readonly entries: readonly Record<string, unknown>[]; readonly truncated: boolean },
): void {
  store.apply({
    kind: "command_result",
    result: {
      protocolVersion: 1,
      type: "command.result",
      messageId: "bbbbbbbb-1111-4111-8111-0000000000aa",
      connectionId: "2de54db7-74ae-4c21-88e3-05df0dc2e637",
      connectionSequence: "1",
      body: {
        requestId,
        command: "workspace.browse",
        status: "completed",
        acceptedAt: "2026-10-01T00:00:01.000Z",
        terminalEventId: null,
        result,
        error: null,
      },
    },
  });
}

function answerMkdir(store: ClientStore, requestId: string, status: "accepted" | "completed" | "failed" | "uncertain"): void {
  store.apply({ kind: "command_result", result: {
    protocolVersion: 1, type: "command.result", messageId: "bbbbbbbb-1111-4111-8111-0000000000aa",
    connectionId: "2de54db7-74ae-4c21-88e3-05df0dc2e637", connectionSequence: "1",
    body: { requestId, command: "workspace.mkdir", status, acceptedAt: "2026-10-01T00:00:01.000Z",
      terminalEventId: null, result: status === "completed" ? { created: true } : null,
      error: status === "failed" ? { code: "internal.unavailable", message: "新建失败", retryable: false, details: {} } : null },
  }});
}

/** 走一次真实的浏览往返（派发请求 → 注入服务端应答）。 */
function browseTo(
  store: ClientStore,
  gateway: RecordingGateway,
  path: string | null,
  entries: readonly Record<string, unknown>[],
  truncated = false,
): void {
  const outcome = store.browseDirectory(path);
  if (!outcome.ok) throw new Error(`浏览请求未派发：${outcome.reason}`);
  // 请求必须**真的经网关发出去**、带的还得是这一层：只看仓库状态的变更证明不了这条往返
  // （那一路没有任何东西在断言命令发到了线上）。这也是「浏览结果来自服务端」这个前提的一部分。
  const dispatched = gateway.dispatches[gateway.dispatches.length - 1]?.body;
  if (dispatched === undefined || dispatched.command !== "workspace.browse" || dispatched.payload.path !== path) {
    throw new Error(
      `第 ${String(gateway.dispatches.length)} 条派发记录不是 path=${String(path)} 的 workspace.browse`,
    );
  }
  answerBrowse(store, outcome.requestId, { path, entries, truncated });
}

describe("R16 提交载荷只含一个目录引用、Agent 标识与一个建目录开关", () => {
  it("目录标识载荷：键恰好是 agentId 与 directoryId", () => {
    const command = buildCreateSessionCommand({
      refs: { directoryId: DIRECTORY_ID, agentId: "claude" },
      requestId: "4c4dafda-dd98-442e-8d55-252b75bac72d",
      messageId: "bbbbbbbb-1111-4111-8111-000000000009",
      connectionId: "2de54db7-74ae-4c21-88e3-05df0dc2e637",
    });

    const payload = command.body.payload as Record<string, unknown>;
    expect(Object.keys(payload).sort()).toEqual(["agentId", "directoryId"]);
    // 反例：若命令里带了任何凭据字面量，这里会出现它们，断言变红。
    const serialized = JSON.stringify(command);
    expect(serialized).not.toMatch(/token|password|apiKey|mcp|exportId|templateParams/i);
  });

  it("目录载荷键恰好是 agentId 与 workspacePath；createIfMissing 是越界键", () => {
    const command = buildCreateSessionCommand({
      refs: { workspacePath: "D:\\work\\api", agentId: "claude" },
      requestId: "4c4dafda-dd98-442e-8d55-252b75bac72d",
      messageId: "bbbbbbbb-1111-4111-8111-00000000000a",
      connectionId: "2de54db7-74ae-4c21-88e3-05df0dc2e637",
    });

    const payload = command.body.payload as Record<string, unknown>;
    // 反例：把路径塞进 directoryId（或反过来丢弃它）→ 键集与取值都会红。
    expect(Object.keys(payload).sort()).toEqual(["agentId", "workspacePath"]);
    expect(payload["workspacePath"]).toBe("D:\\work\\api");
    expect(() =>
      assertCreateSessionRefs({ workspacePath: "D:\\work\\api", agentId: "claude", createIfMissing: false }),
    ).toThrow();
  });

  it("越界载荷与目录引用不唯一一律被拒绝", () => {
    const violations: readonly Record<string, unknown>[] = [
      { directoryId: DIRECTORY_ID, agentId: "claude", cwd: "D:\\work\\api" },
      { directoryId: DIRECTORY_ID, agentId: "claude", path: "/home/dev/api" },
      { directoryId: DIRECTORY_ID, agentId: "claude", exportId: "node-1" },
      { directoryId: DIRECTORY_ID, agentId: "claude", mcpServers: { fs: {} } },
      { directoryId: DIRECTORY_ID, agentId: "claude", token: "sk-xxx" },
      { directoryId: DIRECTORY_ID, agentId: "claude", templateParams: { a: 1 } },
      // 目录引用必须**恰好一个**：两个都给，或都不给。
      { directoryId: DIRECTORY_ID, workspacePath: "D:\\work\\api", agentId: "claude" },
      { agentId: "claude" },
      // 建目录开关不属于 Web 提交载荷，目录标识或路径旁出现都必须拒绝。
      { directoryId: DIRECTORY_ID, agentId: "claude", createIfMissing: true },
      { workspacePath: "D:\\work\\api", agentId: "claude", createIfMissing: false },
      { workspacePath: "D:\\work\\api", agentId: "claude", createIfMissing: true },
      { workspacePath: "D:\\work\\api", agentId: "claude", createIfMissing: "yes" },
    ];

    for (const payload of violations) {
      // 反例：若断言放宽成「至少一个目录引用」，后两条不会抛错，用例变红。
      expect(() => assertCreateSessionRefs(payload)).toThrow();
    }
    // 对照：两种合法载荷仍然通过（证明断言不是「一律拒绝」）。
    expect(assertCreateSessionRefs({ directoryId: DIRECTORY_ID, agentId: "claude" })).toEqual({
      directoryId: DIRECTORY_ID,
      agentId: "claude",
    });
    expect(assertCreateSessionRefs({ workspacePath: "D:\\work\\api", agentId: "claude" })).toEqual({
      workspacePath: "D:\\work\\api",
      agentId: "claude",
    });
  });

  it("实际派发的命令：载荷键集恰好是规范允许的两类", () => {
    const { store, gateway } = onlineStore({
      resources: resources({ workspaces: WORKSPACES_WITH_PATHS, agents: [...AGENTS] }),
      features: [WORKSPACE_BROWSE_FEATURE, "core.workspace-mkdir.v1"],
    });

    // 1) 目录已知（卡片/详情页入口）：目录标识引用。
    expect(store.submitCreateSession({ directoryId: DIRECTORY_ID, agentId: "claude" }).ok).toBe(true);
    expect(Object.keys(createPayloadAt(gateway, 0)).sort()).toEqual(["agentId", "directoryId"]);

    // 2) 顶部入口浏览到一个**已存在**的目录：路径引用，不带建目录开关。
    browseTo(store, gateway, "D:\\work", [{ name: "api", path: "D:\\work\\api", registered: true }]);
    const existing = store.directoryBrowser().entries[0];
    if (existing === undefined) throw new Error("浏览结果里应有条目");
    store.selectBrowsedDirectory({
      kind: "path",
      path: existing.path,
      name: existing.name,
      registered: existing.registered,
      willCreate: false,
    });
    expect(
      store.submitCreateSession({
        directoryId: null,
        agentId: "claude",
        directoryPath: existing.path,
      }).ok,
    ).toBe(true);
    expect(Object.keys(createPayloadAt(gateway, 1)).sort()).toEqual(["agentId", "workspacePath"]);

    // 独立新建不选择，不提交第二个会话。
    expect(store.createFolderInBrowse("new-api").ok).toBe(true);
    expect(store.state.directoryBrowse.selection?.kind).toBe("path");
    expect(gateway.dispatches.filter((frame) => frame.body.command === "session.create")).toHaveLength(2);

    // 反例：任何一条载荷里出现凭据类字段名，这里会出现它们，断言变红。
    for (const command of gateway.dispatches) {
      expect(JSON.stringify(command)).not.toMatch(/token|password|apiKey|mcpServers|exportId|templateParams/i);
    }
  });
});

describe("R16 候选只来自本机已配置的 Agent", () => {
  it("未手动选择时选中默认 Agent，手动选择后保留用户选择", () => {
    const automatic = renderSheet(entryModel(), null);
    const defaultTag = automatic.match(/<[^>]*data-agent-id="claude"[^>]*>/)?.[0];
    expect(defaultTag).toContain('data-selected="true"');
    expect(openingTagWithAction(automatic, "submit-create")).not.toContain('aria-disabled="true"');
    const manual = renderSheet(entryModel(), "codex");
    expect(manual.match(/<[^>]*data-agent-id="codex"[^>]*>/)?.[0]).toContain('data-selected="true"');
    expect(manual.match(/<[^>]*data-agent-id="claude"[^>]*>/)?.[0]).toContain('data-selected="false"');
  });
  it("弹层只列出快照里的 Agent，且目录不由用户手工输入", () => {
    const html = renderSheet(entryModel());
    expect(html).toContain('data-agent-id="claude"');
    expect(html).toContain('data-agent-id="codex"');
    // 反例：若候选来自任何其它来源（例如本地配置或用户输入），这里会出现它，断言变红。
    expect(html).not.toContain("gemini");
    // 目录**不是**自由文本：弹层里没有任何文本输入框（Agent 也不是），路径只可能是服务端下发的那一份。
    expect(html.match(/<input/g) ?? []).toHaveLength(0);
    expect(html).toContain('data-create-directory-block="known"');
  });

  it("选一个不在目录里的 Agent 提交：被拒绝且一条命令都不发", () => {
    const { store, gateway } = storeWith({ resources: resources({ agents: [...AGENTS] }) });

    const outcome = store.submitCreateSession({ directoryId: DIRECTORY_ID, agentId: "not-installed" });

    expect(outcome.ok).toBe(false);
    // 反例：若客户端不校验就把用户选的 Agent 发出去，这里会出现派发记录，断言变红。
    expect(gateway.dispatches).toEqual([]);
  });

  it("与授权无关：没有任何授权输入也照样能提交（创建入口不再有权限门控）", () => {
    const { store, gateway } = storeWith({ resources: resources({ agents: [...AGENTS] }) });

    const outcome = store.submitCreateSession({ directoryId: DIRECTORY_ID, agentId: "claude" });

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

  it("副作用说明照常呈现，且目录块给出目录/路径两行与说明", () => {
    const html = renderSheet(entryModel(), "claude");
    // spec「创建入口的副作用说明」：入口旁必须说明会在电脑上启动一个 Agent 进程。
    expect(html).toContain("将在你的电脑上启动一个 Agent 进程");
    expect(html).toContain(CREATE_EFFECT_NOTE);
    // 目录块：展示名与路径逐字来自服务端，目录标识不出现在**文本**里（它只作机器标识）。
    expect(html).toContain('data-create-directory-name="api"');
    expect(html).toContain("D:\\work\\api");
    expect(textOfHtml(html)).not.toContain(DIRECTORY_ID);
    expect(html).not.toContain("服务端会拦截");
  });

  it("目录已知时提交按钮可用（目录引用已经定下来了）", () => {
    const html = renderSheet(entryModel(), "claude");
    expect(openingTagWithAction(html, "submit-create")).not.toContain("disabled");
  });
});

describe("R16 不能逐层浏览时按原因说明，并保留已登记目录这条路", () => {
  /** 未协商 `core.workspace-browse.v1` 的在线仓库 + 顶部入口形态的弹层。 */
  function unsupportedSheet(): { readonly store: ClientStore; readonly gateway: RecordingGateway } {
    const fixture = onlineStore({
      resources: resources({ workspaces: WORKSPACES_WITH_PATHS, agents: [...AGENTS] }),
      // 未协商的只是**目录浏览**；会话准备能力单独协商（新入口不以浏览能力为前提）。
    });
    fixture.store.openCreateSheetToBrowse();
    return fixture;
  }

  it("弹层说明不可用、控件改名为「选择已登记的目录」，且不呈现手工输入", () => {
    const { store } = unsupportedSheet();
    const html = renderSheet(browseEntry(store), "claude");

    expect(html).toContain('data-create-directory-block="browse"');
    // 已连上（在线）却没协商到该能力：这才是「服务端没有提供」那一支。
    expect(html).toContain('data-create-browse-unavailable="unsupported"');
    expect(html).toContain(BROWSE_UNSUPPORTED_NOTICE);
    expect(openingTagWithAction(html, "open-directory-picker")).toContain('data-picker-supported="false"');
    // 反例：把「选择文件夹」画成一个点了必然失败的控件时，这里会看到 supported=true。
    expect(html.match(/<input/g) ?? []).toHaveLength(0);
    expect(html).not.toContain('data-create-directory-notice="unselected"');
    expect(openingTagWithAction(html, "submit-create")).not.toContain('aria-disabled="true"');
  });

  it("还没连上时说的是「还没连上」，不是「服务端没有提供该能力」", () => {
    // 未认证 / 还没拿到协商结果时 `selectedFeatures` 本来就是空的，把它当成「服务端不支持浏览」
    // 是在断言一件本机无从知道的事（R16-3 的未认证起点就是这一支）。
    const { store } = storeWith({
      resources: resources({ workspaces: WORKSPACES_WITH_PATHS, agents: [...AGENTS] }),
    });
    store.openCreateSheetToBrowse();
    const html = renderSheet(browseEntry(store), "claude");

    expect(html).toContain('data-create-browse-unavailable="offline"');
    expect(html).toContain(BROWSE_OFFLINE_NOTICE);
    // 反例：把「还没协商」写成「服务端没有提供该能力」时，这两条会红。
    expect(html).not.toContain(BROWSE_UNSUPPORTED_NOTICE);
    expect(html).not.toContain("服务端没有提供目录浏览能力");
    // 控件仍只是改名、不是不可点：已登记目录那条路照旧在。
    expect(openingTagWithAction(html, "open-directory-picker")).toContain('data-picker-supported="false"');

    // 对照：同一形态在**已连上**却没协商到该能力时才是「服务端没有提供」（上一条用例）。
    const online = unsupportedSheet();
    expect(renderSheet(browseEntry(online.store), "claude")).toContain(BROWSE_UNSUPPORTED_NOTICE);
  });

  it("选择器也说明不可用，并把已登记目录摆出来（不是空白页）", () => {
    const { store } = unsupportedSheet();
    const html = renderPicker({
      supported: false,
      browse: store.state.directoryBrowse,
      initialPath: null,
      unavailable: "unsupported",
    });

    expect(html).toContain('data-directory-picker="open"');
    expect(html).toContain('data-browse-unsupported="unsupported"');
    expect(html).toContain(BROWSE_UNSUPPORTED_NOTICE);
    // 已登记目录逐项可点：展示名是文本，目录标识与路径是机器属性。
    expect(html).toContain('data-action="choose-registered-directory"');
    expect(html).toContain('data-registered-directory-id="work-api"');
    expect(html).toContain('data-registered-directory-id="personal-api"');
    // 路径逐字来自快照条目（两条都始终在场）。
    expect(html).toContain("D:\\work\\api");
    // 不支持浏览时不呈现「选择此文件夹」：当前层无从浏览，画出来只会必然失败。
    expect(html).not.toContain('data-action="confirm-directory"');
  });

  it("选择器在未连上时同样分岔：不说「服务端没有提供」，也不建议升级服务端", () => {
    // 选择器自己只有一个「能不能」的布尔，分不出「还没连上」与「服务端未提供」：原因由页面从
    // 顶部入口的目录块带进来（`unavailable`）。
    const { store } = storeWith({ resources: resources({ workspaces: [], agents: [] }) });
    store.openCreateSheetToBrowse();
    store.openDirectoryPicker();
    const html = renderPicker({
      supported: false,
      browse: store.state.directoryBrowse,
      initialPath: null,
      registeredDirectories: [],
      unavailable: "offline",
    });

    expect(html).toContain('data-browse-unsupported="offline"');
    expect(html).toContain(BROWSE_OFFLINE_NOTICE);
    // 反例：把「还没连上」写成「服务端没有提供该能力」→ 这条红。
    expect(html).not.toContain(BROWSE_UNSUPPORTED_NOTICE);
    // 空列表那一行只说本机能做的事，且**不**建议升级服务端（真实原因还不是「服务端不支持」）。
    expect(html).toContain(BROWSE_EMPTY_FALLBACK_NOTICES.offline);
    expect(html).not.toContain("把服务端升级到支持目录浏览的版本");

    // 对照：已连上却没协商到该能力时，那条建议仍然逐字保留（这正是旧文案唯一成立的那一支）。
    const connected = renderPicker({
      supported: false,
      browse: store.state.directoryBrowse,
      initialPath: null,
      registeredDirectories: [],
      unavailable: "unsupported",
    });
    expect(connected).toContain(BROWSE_EMPTY_FALLBACK_NOTICES.unsupported);
    expect(connected).toContain("把服务端升级到支持目录浏览的版本");
  });

  it("一条浏览命令都不发，且从回退列表选了之后载荷仍只有两个引用", () => {
    const { store, gateway } = unsupportedSheet();
    store.openDirectoryPicker();

    // 反例：发了再看反应的话，这里会多一条必然被 feature 门控拒掉的 workspace.browse。
    expect(store.browseDirectory(null)).toEqual({
      ok: false,
      reason: "服务端未提供目录浏览能力（未协商 core.workspace-browse.v1）",
    });
    expect(gateway.dispatches).toEqual([]);

    // 回退列表给出的是**已登记目录**：选定它走目录标识引用，键集与目录层级入口逐字相同。
    const pick: DirectorySelection = {
      kind: "registered",
      directoryId: "work-api",
      displayName: "api",
      path: "D:\\work\\api",
    };
    store.selectBrowsedDirectory(pick);
    const html = renderSheet(browseEntry(store), "claude");
    expect(html).toContain('data-create-directory-notice="registered"');
    expect(html).toContain('data-create-selected-name="api"');

    expect(store.submitCreateSession({ directoryId: null, agentId: "claude", directoryPath: "" }).ok).toBe(
      true,
    );
    expect(Object.keys(createPayloadAt(gateway, 0)).sort()).toEqual(["agentId", "directoryId"]);
    expect(createPayloadAt(gateway, 0)["directoryId"]).toBe("work-api");
  });
});

describe("R16 浏览：已登记目录被标注、逐层选定与新建一层目录", () => {
  /** 已协商浏览能力的在线仓库（带快照与 Agent）。 */
  function browsingStore(): { readonly store: ClientStore; readonly gateway: RecordingGateway } {
    const fixture = onlineStore({
      resources: resources({ workspaces: WORKSPACES_WITH_PATHS, agents: [...AGENTS] }),
      features: [WORKSPACE_BROWSE_FEATURE, "core.workspace-mkdir.v1"],
    });
    fixture.store.openCreateSheetToBrowse();
    fixture.store.openDirectoryPicker();
    return fixture;
  }

  it("根列表从「此电脑」开始，进入一层后已登记的子目录被标注", () => {
    const { store, gateway } = browsingStore();
    browseTo(store, gateway, null, [
      { name: "D:", path: "D:\\", registered: false },
      { name: "C:", path: "C:\\", registered: false },
    ]);
    const root = store.directoryBrowser();
    expect(root.here).toBeNull();
    expect(root.current.name).toBe("此电脑");
    expect(root.canConfirm).toBe(false);
    expect(root.entries.map((entry) => entry.name)).toEqual(["C:", "D:"]);
    expect(renderPicker({ supported: true, browse: store.state.directoryBrowse, initialPath: null })).toContain(
      "此电脑（先选一个盘）",
    );

    browseTo(store, gateway, "D:\\", [
      { name: "work", path: "D:\\work", registered: false },
      { name: "kept", path: "D:\\kept", registered: true },
    ]);
    const level = store.directoryBrowser();
    expect(level.crumbs.map((crumb) => crumb.name)).toEqual(["此电脑", "D:"]);
    expect(level.canConfirm).toBe(true);
    const html = renderPicker({ supported: true, browse: store.state.directoryBrowse, initialPath: "D:\\" });
    // 反例：不标注登记状态时这一条会红（两处：文案与机器属性）。
    expect(html).toContain("已登记");
    expect(html).toContain('data-entry-registered="true"');
    expect(html).toContain('data-entry-path="D:\\kept"');
    // 截断说明与「还有更多」必须出现，不得静默丢弃。
    browseTo(store, gateway, "D:\\kept", [], true);
    const truncated = renderPicker({ supported: true, browse: store.state.directoryBrowse, initialPath: "D:\\kept" });
    expect(truncated).toContain('data-browse-truncated="true"');
    expect(truncated).toContain(BROWSE_TRUNCATED_NOTICE);
    // 空层与截断是两种可区分的呈现。
    expect(truncated).toContain('data-browse-state="empty"');
  });

  it("「上一级」回到面包屑里的上一段，路径取自服务端回显（不是客户端算出来的）", () => {
    const { store, gateway } = browsingStore();
    browseTo(store, gateway, "D:\\", [{ name: "work", path: "D:\\work", registered: false }]);
    browseTo(store, gateway, "D:\\work", [{ name: "api", path: "D:\\work\\api", registered: true }]);
    expect(store.directoryBrowser().here).toBe("D:\\work");

    const parent = store.directoryBrowser().parent;
    if (parent === null) throw new Error("这一层应有上一级");
    expect(parent.path).toBe("D:\\");

    browseTo(store, gateway, parent.path, [{ name: "work", path: "D:\\work", registered: false }]);
    expect(store.directoryBrowser().crumbs.map((crumb) => crumb.path)).toEqual([null, "D:\\"]);
  });

  it("选定当前层：路径取服务端回显、名称取面包屑那一段，载荷用 workspacePath", () => {
    const { store, gateway } = browsingStore();
    browseTo(store, gateway, "D:\\", [{ name: "work", path: "D:\\work", registered: false }]);
    browseTo(store, gateway, "D:\\work", []);

    const selection = store.directoryBrowser();
    expect(selection.canConfirm).toBe(true);
    expect(selection.current.name).toBe("work");
    store.selectBrowsedDirectory({
      kind: "path",
      path: selection.here ?? "",
      name: selection.current.name,
      registered: selection.current.registered,
      willCreate: false,
    });
    const html = renderSheet(browseEntry(store), "claude");
    expect(html).toContain('data-create-selected-name="work"');
    expect(html).toContain('data-create-directory-path="D:\\work"');
    expect(html).not.toContain('data-create-directory-notice="unregistered"');
    expect(html).not.toContain('data-create-directory-hint');
    expect(html).not.toContain("它不在已登记目录里");
  });

  it("独立新建等完成后刷新真实父层，保持选择器并支持三级创建", () => {
    const { store, gateway } = browsingStore();
    let parent = "/home/dev";
    browseTo(store, gateway, parent, []);
    for (const name of ["project", "src", "api"]) {
      const sentBefore = gateway.dispatches.length;
      const outcome = store.createFolderInBrowse(name);
      if (!outcome.ok) throw new Error(outcome.reason);
      expect(store.createFolderInBrowse(name).ok).toBe(false);
      expect(gateway.dispatches).toHaveLength(sentBefore + 1);
      expect(store.state.directoryBrowse.selection).toBeNull();
      expect(store.state.directoryBrowse.open).toBe(true);
      expect(store.directoryBrowser().entries).toEqual([]);
      const frame = gateway.dispatches.at(-1);
      expect(frame?.body).toEqual({ requestId: outcome.requestId, command: "workspace.mkdir", payload: { parentPath: parent, name } });
      for (const status of ["accepted", "completed"] as const) {
        store.apply({ kind: "command_result", result: {
          protocolVersion: 1, type: "command.result", messageId: "bbbbbbbb-1111-4111-8111-0000000000aa",
          connectionId: "2de54db7-74ae-4c21-88e3-05df0dc2e637", connectionSequence: "1",
          body: { requestId: outcome.requestId, command: "workspace.mkdir", status,
            acceptedAt: "2026-10-01T00:00:01.000Z", terminalEventId: null,
            result: status === "completed" ? { created: true } : null, error: null },
        }});
        expect(store.state.directoryBrowse.selection).toBeNull();
      }
      const refresh = store.state.directoryBrowse.pendingRequestId;
      if (refresh === null) throw new Error("完成后必须刷新父层");
      const child = parent + "/" + name;
      answerBrowse(store, refresh, { path: parent, entries: [{ name, path: child, registered: false }], truncated: false });
      const enter = store.state.directoryBrowse.pendingRequestId;
      if (enter === null) throw new Error("新建后必须自动进入目录");
      expect(gateway.dispatches.at(-1)?.body.payload).toEqual({ path: child });
      answerBrowse(store, enter, { path: child, entries: [], truncated: false });
      expect(store.directoryBrowser().here).toBe(child);
      expect(store.state.directoryBrowse.open).toBe(true);
      expect(store.state.directoryBrowse.selection).toBeNull();
      parent = child;
    }
    store.selectBrowsedDirectory({ kind: "path", path: parent, name: "api", registered: false, willCreate: false });
    expect(store.state.directoryBrowse.selection).toMatchObject({ path: "/home/dev/project/src/api", willCreate: false });
    expect(gateway.dispatches.filter((frame) => frame.body.command === "session.create")).toHaveLength(0);
  });

  it("旧主机只禁用新建，仍可浏览选择已有目录", () => {
    const { store, gateway } = onlineStore({ features: [WORKSPACE_BROWSE_FEATURE] });
    browseTo(store, gateway, "/home/dev", []);
    const count = gateway.dispatches.length;
    expect(store.createFolderInBrowse("api").ok).toBe(false);
    expect(gateway.dispatches).toHaveLength(count);
    expect(store.directoryBrowser().canConfirm).toBe(true);
    expect(store.directoryBrowser().mkdirSupported).toBe(false);
  });

  it("关闭整个弹层不丢失在途或不确定请求，显式核对后只刷新不重发", () => {
    const { store, gateway } = onlineStore({
      resources: resources({ workspaces: WORKSPACES_WITH_PATHS, agents: [...AGENTS] }),
      features: [WORKSPACE_BROWSE_FEATURE, "core.workspace-mkdir.v1"],
    });
    store.openCreateSheetToBrowse();
    store.openDirectoryPicker();
    browseTo(store, gateway, "/home/dev", []);
    const outcome = store.createFolderInBrowse("api");
    if (!outcome.ok) throw new Error(outcome.reason);
    store.closeCreateSheet(); store.openCreateSheetToBrowse(); store.openDirectoryPicker();
    expect(store.directoryBrowser().folder?.requestId).toBe(outcome.requestId);
    expect(store.createFolderInBrowse("api").ok).toBe(false);
    answerMkdir(store, outcome.requestId, "uncertain");
    store.closeDirectoryPicker(); store.openDirectoryPicker();
    expect(store.directoryBrowser().folder?.state).toBe("uncertain");
    expect(store.directoryBrowser().folder?.name).toBe("api");
    expect(store.createFolderInBrowse("api").ok).toBe(false);
    store.resolveFolderUncertain();
    expect(gateway.dispatches.filter((frame) => frame.body.command === "workspace.mkdir")).toHaveLength(1);
    expect(gateway.dispatches.at(-1)?.body.command).toBe("workspace.browse");
    expect(store.state.directoryBrowse.selection).toBeNull();
  });

  it("失败保留输入和父层；成功后刷新失败不会重新 mkdir", () => {
    const { store, gateway } = browsingStore();
    browseTo(store, gateway, "/home/dev", []);
    const failed = store.createFolderInBrowse("api");
    if (!failed.ok) throw new Error(failed.reason);
    answerMkdir(store, failed.requestId, "failed");
    expect(store.directoryBrowser().here).toBe("/home/dev");
    expect(store.directoryBrowser().folder).toMatchObject({ state: "failed", name: "api" });
    const retry = store.createFolderInBrowse("other");
    if (!retry.ok) throw new Error(retry.reason);
    answerMkdir(store, retry.requestId, "completed");
    expect(store.directoryBrowser().folder?.state).toBe("completed");
    expect(store.directoryBrowser().entries).toEqual([]);
    expect(gateway.dispatches.filter((frame) => frame.body.command === "workspace.mkdir")).toHaveLength(2);
    store.closeCreateSheet();
    expect(gateway.dispatches.filter((frame) => frame.body.command === "session.create")).toHaveLength(0);
  });

  it("名字非法：明确拒绝，既不构造候选路径也不改选定", () => {
    const { store, gateway } = browsingStore();
    browseTo(store, gateway, "D:\\", []);

    expect(store.createFolderInBrowse("   ")).toEqual({ ok: false, reason: CREATE_FOLDER_NAME_RULE });
    expect(store.createFolderInBrowse("a\\b")).toEqual({ ok: false, reason: CREATE_FOLDER_NAME_RULE });
    expect(store.createFolderInBrowse("D:")).toEqual({ ok: false, reason: CREATE_FOLDER_NAME_RULE });
    // 反例：静默失败时选定会变成一条拼出来的路径，这里会红。
    expect(store.state.directoryBrowse.selection).toBeNull();
    expect(store.directoryBrowser().folderError).toBe(CREATE_FOLDER_NAME_RULE);
    expect(renderPicker({ supported: true, browse: store.state.directoryBrowse, initialPath: "D:\\" })).toContain(
      'data-browse-folder-error="true"',
    );
  });

  it("浏览到已登记的目录：分支提示说明它已登记", () => {
    const { store } = browsingStore();
    store.selectBrowsedDirectory({
      kind: "path",
      path: "D:\\kept",
      name: "kept",
      registered: true,
      willCreate: false,
    });
    const html = renderSheet(browseEntry(store), "claude");
    expect(html).toContain('data-create-directory-notice="registered"');
    expect(html).toContain(createDirectoryNoticeText({ kind: "registered", displayName: "kept", path: "D:\\kept" }));
  });

  it("形状不符的浏览结果明确失败，不当成「这一层没有子目录」", () => {
    const { store, gateway } = browsingStore();
    const outcome = store.browseDirectory(null);
    if (!outcome.ok) throw new Error("浏览请求应已派发");
    // 服务端答非所问（例如回了别的命令的结果）：必须落到 failed，而不是空层。
    answerBrowseWithRaw(store, outcome.requestId, { sessionId: "dddddddd-0000-4000-8000-0000000000ff" });
    const browser = store.directoryBrowser();
    expect(browser.status).toBe("failed");
    expect(browser.entries).toEqual([]);
    expect(gateway.dispatches).toHaveLength(1);
    expect(
      renderPicker({ supported: true, browse: store.state.directoryBrowse, initialPath: null }),
    ).toContain('data-browse-state="failed"');
  });

  it("读取失败的那一层：不给「新建文件夹」，也不把它说成已进入的层", () => {
    const { store } = browsingStore();
    const outcome = store.browseDirectory("D:\\broken");
    if (!outcome.ok) throw new Error("浏览请求应已派发");
    // 服务端答非所问 → failed。失败层里 `here` **不是** null：它是乐观写入的**请求**路径。
    answerBrowseWithRaw(store, outcome.requestId, { sessionId: "dddddddd-0000-4000-8000-0000000000ff" });
    const browser = store.directoryBrowser();
    expect(browser.status).toBe("failed");
    expect(browser.here).toBe("D:\\broken");
    // 模型那边对同一状态的口径：「当前层可选」要求 `status === "ready"`（失败层不可选）。
    expect(browser.canConfirm).toBe(false);

    const html = renderPicker({ supported: true, browse: store.state.directoryBrowse, initialPath: "D:\\broken" });

    expect(html).toContain('data-browse-state="failed"');
    // 反例：只判 `here !== null` 时，失败层与「＋ 新建文件夹…」会同屏。
    expect(html).not.toContain('data-browse-new-folder-row="true"');
    expect(html).not.toContain('data-action="open-new-folder"');
    // 也不把它说成「当前位置：…」——那一层并没有读出来（`here` 只是当初请求的路径）。
    expect(html).toContain("这一层没读出来：D:\\broken");
    expect(html).not.toContain("当前位置：");
    // 对照：「选择此文件夹」的口径本来就没变（同一个 `status === "ready"`）。
    expect(openingTagWithAction(html, "confirm-directory")).toContain("disabled");
  });
});

/** 注入一条**形状不符**的浏览结果（服务端答非所问的那一路）。 */
function answerBrowseWithRaw(store: ClientStore, requestId: string, result: Record<string, unknown>): void {
  store.apply({
    kind: "command_result",
    result: {
      protocolVersion: 1,
      type: "command.result",
      messageId: "bbbbbbbb-1111-4111-8111-0000000000ab",
      connectionId: "2de54db7-74ae-4c21-88e3-05df0dc2e637",
      connectionSequence: "1",
      body: {
        requestId,
        command: "workspace.browse",
        status: "completed",
        acceptedAt: "2026-10-01T00:00:01.000Z",
        terminalEventId: null,
        result,
        error: null,
      },
    },
  });
}

describe("R16 创建中与终态分别呈现", () => {
  it("新目录创建完成后刷新权威快照，返回与重载后都有目录及会话，重投不重复刷新", () => {
    const { store, gateway } = storeWith({ resources: resources({ agents: [...AGENTS] }) });
    const summary = session({ id: "5d73cd10-a465-43cd-b3f1-704e2d49e99e", directoryId: "new-dir", title: "新目录会话" });
    const result: CommandResultMessage = {
      protocolVersion: 1, type: "command.result",
      messageId: "bbbbbbbb-1111-4111-8111-000000000001",
      connectionId: "2de54db7-74ae-4c21-88e3-05df0dc2e637", connectionSequence: "1",
      body: { requestId: "bbbbbbbb-1111-4111-8111-000000000002", command: "session.create",
        status: "completed", acceptedAt: "2026-10-01T00:00:01.000Z", terminalEventId: null,
        result: { sessionId: summary.sessionId, session: summary }, error: null },
    };
    store.apply({ kind: "command_result", result: { ...result, body: { ...result.body, status: "accepted", result: null } } });
    expect(gateway.snapshotRefreshes).toBe(0);
    const oldDirectories = store.state.resources?.workspaces;
    store.apply({ kind: "command_result", result });
    expect(gateway.snapshotRefreshes).toBe(1);
    expect(store.state.resources?.workspaces).toBe(oldDirectories);
    expect(store.conversationPage(summary.sessionId)?.header.title).toBe("新目录会话");
    store.apply({ kind: "command_result", result });
    expect(gateway.snapshotRefreshes).toBe(1);

    const refreshed = resources({ sessions: [summary], workspaces: [
      ...WORKSPACES_WITH_PATHS, { directoryId: "new-dir", path: "D:\\actual\\new-dir" },
    ] });
    store.apply({ kind: "snapshot_verified", snapshot: {
      snapshotId: "8194de43-e213-423d-acf4-2e3549304566",
      cursor: { serverEpoch: "00384a03-bc90-4095-b65d-82fb8cc47e13", globalSequence: "11" },
      resources: refreshed, chunkCount: 2, receivedChunks: 2, snapshotDigest: "verified",
    } });
    expect(store.directoryPage().kind).toBe("ready");
    const detail = store.directoryDetail("new-dir");
    expect(detail.kind === "ready" && detail.directory.path).toBe("D:\\actual\\new-dir");
    expect(store.directoryPageSessions().get("new-dir")?.[0]?.id).toBe(summary.sessionId);
    const reloaded = storeWith({ resources: refreshed }).store;
    expect(reloaded.directoryPageSessions().get("new-dir")?.[0]?.id).toBe(summary.sessionId);
    expect(gateway.dispatches).toHaveLength(0);
  });

  it.each(["failed", "uncertain"] as const)("%s 的新目录结果不触发目录刷新", (status) => {
    const { store, gateway } = storeWith({ resources: resources({ agents: [...AGENTS] }) });
    store.apply({ kind: "command_result", result: {
      protocolVersion: 1, type: "command.result", messageId: "bbbbbbbb-1111-4111-8111-000000000001",
      connectionId: "2de54db7-74ae-4c21-88e3-05df0dc2e637", connectionSequence: "1",
      body: { requestId: "bbbbbbbb-1111-4111-8111-000000000002", command: "session.create", status,
        acceptedAt: "2026-10-01T00:00:01.000Z", terminalEventId: null, result: null, error: null },
    } });
    expect(gateway.snapshotRefreshes).toBe(0);
    expect(store.state.resources?.workspaces).toEqual(resources({}).workspaces);
  });

  it("提交后呈现「创建中」且不接受重复提交", () => {
    const { store, gateway } = storeWith({ resources: resources({ agents: [...AGENTS] }) });
    store.submitCreateSession({ directoryId: DIRECTORY_ID, agentId: "claude" });

    const model = store.createEntry(DIRECTORY_ID);
    expect(model.submission.kind).toBe("creating");
    const html = renderSheet(model, "claude");

    expect(html).toContain('data-create-submission="creating"');
    expect(html).toContain("创建中");
    // 反例：若创建中仍可重复提交，按钮不会带禁用态，断言变红。
    // RNW 对 `role="button"` 且 `disabled` 的可点控件同时渲染原生 `disabled` 与 `aria-disabled`，
    // 两者都落在同一个开标签里（属性顺序由渲染器决定，因此断言不看顺序）。
    const submit = openingTagWithAction(html, "submit-create");
    expect(submit).toContain("aria-disabled=\"true\"");
    // e2e 判定的是**原生**属性（`hasAttribute("disabled")`），所以单独钉住它：
    // `\s` 前缀把 `aria-disabled` 排除在外。
    expect(submit).toMatch(/\sdisabled\b/);
    expect(gateway.dispatches).toHaveLength(1);
  });

  it("completed 终态进入已创建，返回目录后立即显示新会话，无需刷新或额外事件", () => {
    const { store, gateway } = storeWith({ resources: resources({ agents: [...AGENTS] }) });
    const outcome = store.submitCreateSession({ directoryId: DIRECTORY_ID, agentId: "claude" });
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
          result: {
            sessionId: "5d73cd10-a465-43cd-b3f1-704e2d49e99e",
            session: session({ id: "5d73cd10-a465-43cd-b3f1-704e2d49e99e", title: "刚创建的会话" }),
          },
          error: null,
        },
      },
    });

    const model = store.createEntry(DIRECTORY_ID);
    expect(gateway.snapshotRefreshes).toBe(0);
    expect(model.submission).toEqual({ kind: "created", sessionId: "5d73cd10-a465-43cd-b3f1-704e2d49e99e" });
    expect(renderSheet(model, "claude")).toContain('data-create-submission="created"');
    expect(store.conversationPage("5d73cd10-a465-43cd-b3f1-704e2d49e99e")?.header.title).toBe("刚创建的会话");
    expect(store.directoryPageSessions().get(DIRECTORY_ID)?.some(
      (row) => row.id === "5d73cd10-a465-43cd-b3f1-704e2d49e99e" && row.title === "刚创建的会话",
    )).toBe(true);
    const detail = store.directoryDetail(DIRECTORY_ID);
    expect(detail.kind === "ready" && detail.sessions.some(
      (row) => row.id === "5d73cd10-a465-43cd-b3f1-704e2d49e99e",
    )).toBe(true);
    store.openCreateSheet(DIRECTORY_ID);
    store.closeCreateSheet();
    store.openCreateSheet(DIRECTORY_ID);
    expect(store.createEntry(DIRECTORY_ID).submission.kind).toBe("idle");
    // 再次打开新建入口不能复用上次成功结果、重新跳回旧会话。
    expect(store.state.commands.get(outcome.requestId)?.state).toBe("completed");
  });

  it("失败终态按参数类与服务端不可用类分开说明", () => {
    const { store } = storeWith({ resources: resources({ agents: [...AGENTS] }) });
    const outcome = store.submitCreateSession({ directoryId: DIRECTORY_ID, agentId: "claude" });
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

    const html = renderSheet(store.createEntry(DIRECTORY_ID), "claude");
    expect(html).toContain('data-create-submission="failed"');
    expect(html).toContain('data-failure-code="capability.unsupported_by_agent"');
    expect(html).toContain('data-failure-retryable="false"');
    expect(html).toContain('data-failure-kind="other"');
    expect(html).toContain("Agent 缺少凭据");
  });

  it("本地闸门拒绝提交时呈现原因，而不是静默无反应", () => {
    const { store } = storeWith({ resources: resources({ agents: [...AGENTS] }) });
    const html = renderSheet(
      store.createEntry(DIRECTORY_ID),
      "claude",
      "尚未建立已认证连接，无法提交命令",
    );
    expect(html).toContain('data-create-submit-issue="true"');
    expect(html).toContain("尚未建立已认证连接，无法提交命令");
  });
});

describe("R16 结果不确定显眼且不被自动消解", () => {
  it("服务端确认 uncertain 后呈现显眼区块，并在后续事件后仍然保留", () => {
    const { store } = storeWith({ resources: resources({ agents: [...AGENTS] }) });
    const outcome = store.submitCreateSession({ directoryId: DIRECTORY_ID, agentId: "claude" });
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
    expect(store.createEntry(DIRECTORY_ID).uncertainRequiresUserAction).toBe(true);
    const html = renderSheet(store.createEntry(DIRECTORY_ID), "claude");
    expect(html).toContain('data-create-submission="uncertain"');
    expect(html).toContain('role="alert"');
    expect(html).not.toContain("我知道了");
    expect(html).not.toContain("dismiss");

    expect(store.submitCreateSession({ directoryId: DIRECTORY_ID, agentId: "claude" })).toEqual({
      ok: false,
      reason: "创建结果不确定，必须保留原请求；系统不会自动重发或更换请求标识。",
    });

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
    expect(store.state.createRequestIds.get(DIRECTORY_ID)).toBe(outcome.requestId);
    expect(renderSheet(store.createEntry(DIRECTORY_ID), "claude")).not.toContain("我知道了");
  });
});

describe("R16 浏览状态本身没有路径拼接", () => {
  it("目录名升序忽略大小写，数字自然排序，且不修改原始条目与路径", () => {
    const entries = ["zeta", "Project10", "beta", "project2", "Alpha", "application"]
      .map((name) => ({ name, path: `D:\\${name}`, registered: false }));
    const original = entries.map((entry) => entry.name);
    const model = buildDirectoryBrowserModel({
      supported: true,
      browse: { ...initialDirectoryBrowseState(), loaded: true, entries },
    });
    expect(model.entries.map((entry) => entry.name)).toEqual([
      "Alpha", "application", "beta", "project2", "Project10", "zeta",
    ]);
    expect(entries.map((entry) => entry.name)).toEqual(original);
    expect(model.entries.every((entry) => entries.includes(entry))).toBe(true);
  });

  it("忽略大小写后同名时按原始路径稳定定序", () => {
    const entries = [
      { name: "api", path: "D:\\z\\api", registered: false },
      { name: "API", path: "D:\\a\\API", registered: true },
    ];
    const result = (items: typeof entries) => buildDirectoryBrowserModel({
      supported: true,
      browse: { ...initialDirectoryBrowseState(), loaded: true, entries: items },
    }).entries;
    expect(result(entries)).toEqual(result([...entries].reverse()));
    expect(result(entries)[0]?.path).toBe("D:\\a\\API");
  });

  it("初始状态只有「此电脑」那一段，且没有任何条目", () => {
    const state = initialDirectoryBrowseState();
    expect(state.crumbs).toEqual([{ name: "此电脑", path: null, registered: false }]);
    expect(state.entries).toEqual([]);
    expect(state.selection).toBeNull();
    expect(state.open).toBe(false);
    expect(state.loaded).toBe(false);
  });
});
  it("不同设备相同 requestId 不会误终结本机目录创建", () => {
    const { store, gateway } = onlineStore({
      resources: resources({ workspaces: WORKSPACES_WITH_PATHS, agents: [...AGENTS] }),
      features: [WORKSPACE_BROWSE_FEATURE, "core.workspace-mkdir.v1"],
    });
    store.openCreateSheetToBrowse();
    store.openDirectoryPicker();
    browseTo(store, gateway, "/home/dev", []);
    const outcome = store.createFolderInBrowse("api");
    if (!outcome.ok) throw new Error(outcome.reason);
    const deviceId = store.state.deviceId;
    if (deviceId === null) throw new Error("测试设备身份缺失");
    const count = gateway.dispatches.length;
    const emit = (origin: { kind: "device" | "daemon"; deviceId: string | null }, index: number, command?: string): void => {
      store.apply({ kind: "event", event: {
        protocolVersion: 1, type: "event", messageId: "11111111-1111-4111-8111-000000000009",
        connectionId: "2de54db7-74ae-4c21-88e3-05df0dc2e637", connectionSequence: String(index),
        body: { globalSequence: String(20 + index), sessionSequence: null,
          eventId: "dddddddd-0000-4000-8000-00000000000" + String(index), sessionId: null,
          eventType: "command.completed", causationRequestId: outcome.requestId, origin, remoteOrigin: null,
          createdAt: "2026-10-01T00:10:00.000Z",
          payload: { view: { requestId: outcome.requestId, result: { created: true }, ...(command === undefined ? {} : { command }) } },
        },
      }});
    };
    emit({ kind: "device", deviceId: "aaaaaaaa-1111-4111-8111-000000000099" }, 1);
    emit({ kind: "daemon", deviceId: null }, 2);
    emit({ kind: "device", deviceId }, 3, "session.create");
    expect(store.directoryBrowser().folder?.state).toBe("submitting");
    expect(gateway.dispatches).toHaveLength(count);
    emit({ kind: "device", deviceId }, 4);
    expect(store.directoryBrowser().folder?.state).toBe("completed");
    expect(gateway.dispatches.at(-1)?.body.command).toBe("workspace.browse");
  });
