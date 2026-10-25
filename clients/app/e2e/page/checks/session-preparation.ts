/**
 * 会话准备（`/session/<id>` 的首条前阶段）的**浏览器级**验证（`web-session-preparation` WP7 / 7.5）。
 *
 * ## 为什么必须在页面层验
 *
 * 准备阶段的承诺是「页面始终停在同一 `/session/<id>`、首条前没有正式会话行、入口未协商时明确
 * 不可用」。这三条都只能从**真实组件树 + 真实 socket 帧**观察：feature 层的单测能证明状态机，
 * 但证明不了「点击提交的是准备而不是旧 `session.create`」、也证明不了「分配之后页面没有偷偷
 * 变成正式会话页」。因此本文件只做两件事：点真实按钮、查真实出站帧与真实 DOM。
 *
 * ## 与 R16 的分工
 *
 * `r16-create.ts` 的入口断言已改成「新建入口 → prepare → allocate」；本文件继续往下走完准备页
 * 本身：准备面板、刷新恢复、未知地址、未协商降级、首条提交/受理/核实与三条 D6 收口
 * （`store_rebuilt` / `already_committed` / `uncertain`）。
 *
 * ## 时间线与状态
 *
 * 假主机的 `preparationResult` 原样投递 `session.preparation.result`，wire 解码与仓库归并都是
 * 生产代码；命令（`session.start` / `command.status`）与既有用例一样可以扣住，用
 * `completeHeldCommand` 决定终态——「保存完成」与「服务端受理」是两件事，只有能扣住才谈得上
 * 分别断言。
 *
 * ## 附件上传不在本文件的观测面内
 *
 * 页面级假主机不协商 `core.attachment-transfer.v1`（第二条已认证连接没有替身），因此首条提交
 * 只能覆盖「无附件」这一支：断言的是**没有**任何附件帧、`session.start` 在点击后只发一次、
 * 且受理之前页面不显示已保存。附件顺序/单次上传由 WP6/WP5 的案件覆盖，不在页面层伪造。
 */

import type { PreparationReceipt } from "../../../src/protocol";
import { PREPARATION_UNSUPPORTED_NOTICE } from "../../../src/features/preparation-model";
import { openPlatform, openPreparationDraftStore } from "../../../src/platform";
import type { PlatformPorts } from "../../../src/platform";
import { createFakeHostIdentity } from "../fake-host";
import {
  AGENTS,
  DIRECTORY_ID_ALPHA,
  DIRECTORY_PATH_ALPHA,
  DIRS_ROUTE,
  PREPARED_SESSION_ID,
  PREPARATION_MODES,
  WORKSPACES_WITH_PATHS,
  preparationReceipt,
  readyPreparationResult,
  sessionRoute,
  sessionsForBothDirectories,
} from "../fixtures";
import {
  CANONICAL_ORIGIN,
  DEVICE_ID,
  HOST_ID,
  click,
  domText,
  expectAbsent,
  expectEqual,
  expectThat,
  mountScenario,
  query,
  queryAll,
  seedPairedHost,
  textOf,
  waitFor,
} from "../support";
import type { Check, Mounted, ScenarioOptions } from "../support";
import { currentPath, navigate } from "../router-stub";

const PREPARATION_TEXT = "帮我看一下这个仓库的首条消息";
/** 一个不存在、也没有任何本标签页准备的会话 ID。 */
const MISSING_SESSION_ID = "eeeeeeee-0000-4000-8000-00000000dead";
/**
 * 刷新恢复用例专用的准备 ID。
 *
 * 与 {@link PREPARED_SESSION_ID} 分开，是为了让「上一次页面保存的草稿」与别的用例互不干扰：
 * 浏览器准备库与 sessionStorage 定位在同一个 e2e 页面里跨用例存活，同一个 ID 会撞上前面用例留下的
 * 活标签页归属（按复制标签页拒绝），那样验到的就不是刷新而是「另一个标签页」。
 */
const RELOAD_SESSION_ID = "eeeeeeee-0000-4000-8000-0000000000c1";

/**
 * 本场景的凭据：绑定当前已配对节点/来源/设备。
 *
 * 浏览器只保存/回传它（验签与签发都在 Owner），因此这里只需要形状合法。
 */
const PREPARATION_RECEIPT: PreparationReceipt = preparationReceipt({
  ownerNodeId: HOST_ID,
  origin: CANONICAL_ORIGIN,
  deviceId: DEVICE_ID,
});

/** 刷新恢复用例的凭据：绑定它自己的准备 ID。 */
const RELOAD_RECEIPT: PreparationReceipt = preparationReceipt({
  ownerNodeId: HOST_ID,
  origin: CANONICAL_ORIGIN,
  deviceId: DEVICE_ID,
  sessionId: RELOAD_SESSION_ID,
});

/** 假主机对任何准备控制请求的 `ready` 应答：模式/配置是 Agent 真实值。 */
function answerPreparation(body: Record<string, unknown>): Record<string, unknown> {
  return readyPreparationResult(body, PREPARATION_RECEIPT);
}

/** 一帧出站 JSON 的解析结果：`type` 原样、`body` 仅在确实是对象时给出。 */
interface ParsedFrame {
  readonly type: unknown;
  readonly body: Record<string, unknown> | null;
}

/** 解析一帧出站原文；形状不符返回 `null`。 */
function parseFrame(text: string): ParsedFrame | null {
  const parsed: unknown = JSON.parse(text);
  if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) return null;
  // 上面已逐层查过「对象、非数组」，这里只补索引签名：键名由调用点的断言把关。
  const record = parsed as Record<string, unknown>;
  const body: unknown = record["body"];
  const bodyIsObject = typeof body === "object" && body !== null && !Array.isArray(body);
  return {
    type: record["type"],
    body: bodyIsObject ? (body as Record<string, unknown>) : null,
  };
}

/** 全部出站命令的 `body`（`type === "command"`）。 */
function commandBodies(sent: readonly string[]): readonly Record<string, unknown>[] {
  const bodies: Record<string, unknown>[] = [];
  for (const text of sent) {
    const frame = parseFrame(text);
    if (frame?.type === "command" && frame.body !== null) bodies.push(frame.body);
  }
  return bodies;
}

/** 全部出站 `session.preparation` 的 body（按发送顺序）。 */
function preparationBodies(sent: readonly string[]): readonly Record<string, unknown>[] {
  const bodies: Record<string, unknown>[] = [];
  for (const text of sent) {
    const frame = parseFrame(text);
    if (frame?.type === "session.preparation" && frame.body !== null) bodies.push(frame.body);
  }
  return bodies;
}

/** 出站命令的名字（按发送顺序）。 */
function commandNames(sent: readonly string[]): readonly unknown[] {
  return commandBodies(sent).map((body) => body["command"]);
}

/** 出站 `session.start` 的 body（按发送顺序）。 */
function startBodies(sent: readonly string[]): readonly Record<string, unknown>[] {
  return commandBodies(sent).filter((body) => body["command"] === "session.start");
}

/** 出站 `command.status` 的 body（按发送顺序）。 */
function statusBodies(sent: readonly string[]): readonly Record<string, unknown>[] {
  return commandBodies(sent).filter((body) => body["command"] === "command.status");
}

/** 出站 `session.create` 的处数（新入口 MUST NOT 再发它，也不能偷偷退回）。 */
function createFrameCount(sent: readonly string[]): number {
  return commandNames(sent).filter((name) => name === "session.create").length;
}

/** 出站帧类型集合（用于断言「没有任何附件帧」这类负向事实）。 */
function frameTypes(sent: readonly string[]): readonly unknown[] {
  return sent.map((text) => parseFrame(text)?.type ?? null);
}

/** 首帧 `session.preparation` 的 `action` 等于给定值的那一份 body；没有则 `undefined`。 */
function preparationAction(
  sent: readonly string[],
  action: string,
): Record<string, unknown> | undefined {
  return preparationBodies(sent).find((body) => body["action"] === action);
}

/** 一条命令帧的 `payload`（对象、非数组；否则 `null`）。 */
function payloadOf(body: Record<string, unknown> | undefined): Record<string, unknown> | null {
  const payload: unknown = body?.["payload"];
  const isObject = typeof payload === "object" && payload !== null && !Array.isArray(payload);
  return isObject ? (payload as Record<string, unknown>) : null;
}

/** 所有用例共用的起点：已配对、真实握手、真实快照、准备能力已协商。 */
function baseScenario(overrides: Partial<ScenarioOptions> = {}): ScenarioOptions {
  return {
    canonicalOrigin: CANONICAL_ORIGIN,
    workspaces: WORKSPACES_WITH_PATHS,
    sessions: sessionsForBothDirectories(),
    agents: AGENTS,
    authenticate: true,
    tamperHostProof: false,
    deliverSnapshot: true,
    barrierGlobalSequence: "10",
    events: [],
    readResult: (sessionId) => ({ sessionId, resources: { messages: [] }, hasEarlier: false }),
    route: DIRS_ROUTE,
    extraRoutes: [sessionRoute(PREPARED_SESSION_ID)],
    selectPreparation: true,
    preparationResult: answerPreparation,
    ...overrides,
  };
}

/** 打开目录卡片的创建弹层并选中 Agent。 */
async function openDirectorySheet(container: HTMLElement): Promise<void> {
  await waitFor(
    () => query(container, `[data-directory-id="${DIRECTORY_ID_ALPHA}"]`) !== null,
    "目录卡片",
  );
  const card = query(container, `[data-directory-id="${DIRECTORY_ID_ALPHA}"]`);
  click(query(card, '[data-action="create-session"]'), "目录内新建（alpha）");
  await waitFor(() => query(container, "[data-create-sheet]") !== null, "创建弹层打开");
  if (query(container, 'button[data-agent-id="codex-a"]')?.getAttribute("data-selected") !== "true") {
    click(query(container, 'button[data-agent-id="codex-a"]'), "选择 Agent codex-a");
    await waitFor(
      () => query(container, 'button[data-agent-id="codex-a"]')?.getAttribute("data-selected") === "true",
      "Agent codex-a 被选中",
    );
  }
}

/** 点击创建弹层的提交按钮（先等它真的可点，否则「没有帧」证明不了任何事）。 */
async function submitSheet(container: HTMLElement): Promise<void> {
  // Pressable 只在禁用时写 `aria-disabled="true"`；可用时该属性缺席（不是 "false"）。
  await waitFor(
    () => query(container, '[data-action="submit-create"]')?.getAttribute("aria-disabled") !== "true",
    "创建入口可提交",
  );
  click(query(container, '[data-action="submit-create"]'), "提交创建");
}

/** 从目录页走完 allocate，并等真实路由切到准备页。 */
async function enterPreparation(container: HTMLElement, mounted: Mounted): Promise<void> {
  await openDirectorySheet(container);
  await submitSheet(container);
  await waitFor(
    () => currentPath() === `/session/${PREPARED_SESSION_ID}`,
    "路由切到后端分配的同一 ID",
  );
  await waitFor(
    () => mounted.composition?.store.preparationPage(PREPARED_SESSION_ID) !== null,
    "仓库里出现同一 ID 的准备",
  );
  await waitFor(
    () => query(container, "[data-session-title]") !== null,
    "准备页渲染",
  );
}

/** 准备页的多行输入框（RNW 的 `TextInput` 渲染成 `<textarea>`）。 */
function preparationInputOf(container: HTMLElement): HTMLTextAreaElement | null {
  const node = query(container, "[data-composer-input]");
  if (node instanceof HTMLTextAreaElement) return node;
  const nested = node?.querySelector("textarea") ?? null;
  return nested instanceof HTMLTextAreaElement ? nested : null;
}

/** 往准备页的受控多行输入框里「打字」（原生 setter + 冒泡 `input`，与用户敲键同一事件路径）。 */
function typeIntoPreparation(container: HTMLElement, value: string): void {
  const node = preparationInputOf(container);
  expectThat(node !== null, "准备页：找不到多行输入框");
  const own = Object.getOwnPropertyDescriptor(node, "value");
  const native = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value");
  if (native?.set !== undefined && native.set !== own?.set) native.set.call(node, value);
  else if (own?.set !== undefined) own.set.call(node, value);
  node.dispatchEvent(new Event("input", { bubbles: true }));
}

/** 「什么也没发生」的观测窗口：事件/定时器都在这个窗口之后才算落地。 */
async function settle(ms = 250): Promise<void> {
  const { promise, resolve } = Promise.withResolvers<void>();
  setTimeout(resolve, ms);
  await promise;
}

/** 预置一份「上一次页面已保存」的真实浏览器草稿（走生产草稿端口，不是内存假件）。 */
async function seedDraft(sessionId: string, receipt: PreparationReceipt): Promise<void> {
  const identity = await createFakeHostIdentity(HOST_ID);
  await seedPairedHost(identity);
  const seeding = openPreparationDraftStore();
  const begun = await seeding.beginTab({
    scope: {
      canonicalOrigin: CANONICAL_ORIGIN,
      ownerNodeId: HOST_ID,
      deviceId: DEVICE_ID,
      tabSlotId: crypto.randomUUID(),
      sessionId,
    },
    navigationKind: "navigate",
    nowMs: Date.now(),
  });
  expectThat(begun.status === "owner", "预置草稿：标签页归属必须成功");
  await seeding.saveDraft({
    access: begun.access,
    expectedRevision: null,
    content: {
      text: PREPARATION_TEXT,
      mode: "build",
      configOptions: { model: "gpt-5-mini" },
      agentId: "codex-a",
      workspace: {
        directoryId: DIRECTORY_ID_ALPHA,
        path: DIRECTORY_PATH_ALPHA,
        source: "directoryId",
      },
      receipt,
      pendingRequestId: null,
    },
    attachments: [],
    nowMs: Date.now(),
  });
}

const checks: readonly Check[] = [
  {
    name: "R16P-10 新会话与旧会话可来回切换，草稿保留且新建默认复用",
    async run() {
      const oldId = sessionsForBothDirectories()[0]?.sessionId;
      expectThat(oldId !== undefined, "已有会话存在");
      if (oldId === undefined) return;
      const mounted = await mountScenario(baseScenario({ extraRoutes: [sessionRoute(PREPARED_SESSION_ID), sessionRoute(oldId)] }));
      try {
        await enterPreparation(mounted.container, mounted);
        typeIntoPreparation(mounted.container, "保留未发送文字");
        await waitFor(() => preparationInputOf(mounted.container)?.value === "保留未发送文字", "新会话草稿");
        expectEqual(textOf(query(mounted.container, `[data-sidebar-session="${PREPARED_SESSION_ID}"] [data-session-row-title]`)), "新会话", "新会话有左栏标题");
        click(query(mounted.container, `[data-sidebar-session="${oldId}"] [data-action="open-session"]`), "切换旧会话");
        await waitFor(() => currentPath() === `/session/${oldId}`, "进入旧会话");
        await waitFor(() => query(mounted.container, `[data-sidebar-session="${PREPARED_SESSION_ID}"]`) !== null, "旧会话左栏保留新会话");
        click(query(mounted.container, `[data-sidebar-session="${PREPARED_SESSION_ID}"] [data-action="open-session"]`), "切回新会话");
        await waitFor(() => preparationInputOf(mounted.container)?.value === "保留未发送文字", "切回后草稿不丢失");
        const before = preparationBodies(mounted.host?.sent ?? []).filter((body) => body["action"] === "allocate").length;
        click(query(mounted.container, '[data-action="create-session-in-directory"]'), "复用未发送会话");
        await settle();
        expectEqual(currentPath(), `/session/${PREPARED_SESSION_ID}`, "默认复用当前新会话");
        expectEqual(preparationBodies(mounted.host?.sent ?? []).filter((body) => body["action"] === "allocate").length, before, "不重复分配");
        expectEqual(query(mounted.container, '[data-create-sheet]'), null, "复用不打开弹层");
        expectThat(!domText(mounted.container).includes("首条消息已保存"), "没有常驻保存提示");
      } finally { await mounted.unmount(); }
    },
  },
  {
    name: "R16P-9 新目录会话立即显示目录文件和 Git 变更，查询不依赖会话保存",
    async run() {
      const path = "\\\\?\\D:\\fresh\\project";
      const receipt = { ...PREPARATION_RECEIPT, claims: { ...PREPARATION_RECEIPT.claims,
        normalizedWorkspacePath: path, workspaceSource: "workspacePath" as const } };
      const mounted = await mountScenario(baseScenario({ selectWorkspaceFiles: true, preparationResult: (body) => readyPreparationResult(body, receipt), filesResult: (command, _session, payload) => command === "session.files.list" ? { path: payload["path"], gitState: "available", entries: [{ path: "hello.txt", name: "hello.txt", kind: "file", openable: true }] } : { gitState: "available", entries: [{ path: "hello.txt", status: "modified", conflicted: false, binary: false }] } }));
      try {
        await waitFor(() => mounted.composition?.store.state.connection.state === "online", "连接在线");
        const store = mounted.composition?.store;
        expectThat(store !== undefined, "组合根已装配");
        const outcome = await store.beginPreparation({ directoryId: null, agentId: "codex-a", directoryPath: path });
        expectThat(outcome.ok, "新目录分配成功");
        if (!outcome.ok) return;
        navigate(`/session/${outcome.sessionId}`);
        await waitFor(() => query(mounted.container, '[data-session-title]') !== null, "会话界面出现");
        expectThat(textOf(query(mounted.container, '[data-conversation-sidebar]')).includes("project"), "左栏显示新目录");
        expectEqual(textOf(query(mounted.container, '[data-sidebar-directory-path]')), "D:\\fresh\\project", "界面不显示 Windows 扩展前缀");
        expectEqual(store.conversationPage(outcome.sessionId)?.sidebar.rows.length, 1, "只显示当前新会话，不混入旧目录会话");
        await waitFor(() => query(mounted.container, '[data-file-row="all:hello.txt"]') !== null, "directory file visible");
        click(query(mounted.container, '[data-file-source-option="git"]'), "Git");
        await waitFor(() => query(mounted.container, '[data-file-row="git:hello.txt"]') !== null, "directory change visible");
        const fileQueries = (mounted.host?.sent ?? []).map((frame) => JSON.parse(frame) as { body?: Record<string, unknown> }).map((frame) => frame.body ?? {}).filter((body) => body["command"] === "session.files.list" || body["command"] === "session.git.status");
        expectThat(fileQueries.length >= 2, "directory queries sent immediately");
        for (const body of fileQueries) {
          expectEqual(body["sessionId"], undefined, "file queries do not require a session");
          expectEqual(payloadOf(body)?.["workspace"], { workspacePath: path }, "selected directory target");
        }
        expectThat(!domText(mounted.container).includes("这一层加载失败"), "不显示无效查询错误");
        click(query(mounted.container, '[data-action="create-session-in-directory"]'), "在新目录继续新建会话");
        await settle();
        expectEqual(query(mounted.container, '[data-create-sheet]'), null, "reuse does not open allocation sheet");
        expectEqual(currentPath(), `/session/${outcome.sessionId}`, "reuse keeps the same conversation");
        expectEqual(commandNames(mounted.host?.sent ?? []).filter((name) => name === "session.start").length, 0, "no automatic send");
      } finally { await mounted.unmount(); }
    },
  },
  {
    name: "R16P-1 新建后直接进入正常会话界面，在会话输入区调整模式和模型；首条前不写会话行",
    async run() {
      const mounted = await mountScenario(baseScenario());
      const sent = (): readonly string[] => mounted.host?.sent ?? [];
      try {
        await enterPreparation(mounted.container, mounted);
        const store = mounted.composition?.store;
        expectThat(store !== undefined, "组合根未装配");

        // 分配帧只带 Agent/目录/标签页归属：正式 ID 由 Owner 分配，客户端不得指定。
        const allocate = preparationAction(sent(), "allocate") ?? null;
        expectThat(allocate !== null, "点击提交后必须发出 session.preparation 的 allocate");
        expectEqual(
          Object.keys(allocate).sort(),
          ["action", "agentId", "operationId", "tabNonce", "workspace"],
          "allocate 的键集合必须闭合",
        );
        expectEqual(allocate["action"], "allocate", "allocate 的动作");
        expectEqual(allocate["agentId"], "codex-a", "allocate 的 Agent 引用");
        expectEqual(allocate["workspace"], { directoryId: DIRECTORY_ID_ALPHA }, "目录引用用已登记标识，不拼路径");
        expectEqual(typeof allocate["tabNonce"], "string", "allocate 必须带标签页归属");
        expectAbsent(JSON.stringify(allocate), "sessionId", "allocate 不得由客户端指定正式 sessionId");

        // 直接呈现正常会话布局，设置位于会话输入区；不出现独立准备表单。
        const panel = mounted.container;
        expectThat(query(panel, '[data-session-title]') !== null, "显示正常会话头");
        expectThat(query(panel, '[data-preparation-title]') === null, "没有独立准备页");
        expectEqual(textOf(query(panel, '[data-session-title]')), "新会话", "使用正常新会话标题");
        expectEqual(store.preparationPage(PREPARED_SESSION_ID)?.phase, "ready", "首条发送前内部资源已就绪");
        expectEqual(queryAll(panel, '[data-mode-choice]').length, PREPARATION_MODES.length, "模式来自 Agent");
        expectThat(query(panel, '[data-mode-choice="plan"]') !== null, "会话设置包含 plan 模式");
        expectThat(query(panel, '[data-mode-choice="build"]') !== null, "会话设置包含 build 模式");
        expectThat(query(panel, '[data-settings-section="model"]') !== null, "会话输入区包含模型选择器");
        expectThat(query(panel, '[data-model-choice="gpt-5"]') !== null, "模型来自 Agent");
        expectEqual(store.conversationPage(PREPARED_SESSION_ID)?.sidebar.directory?.path, DIRECTORY_PATH_ALPHA, "目录上下文来自服务端");
        // 首条之前：已呈现正常对话界面，但没有正式会话行，也没有正式读取。
        expectEqual(store.preparationPage(PREPARED_SESSION_ID)?.phase, "ready", "仓库里是准备态");
        expectThat(store.conversationPage(PREPARED_SESSION_ID) !== null, "首条前已呈现正常对话页");
        expectEqual(
          store.state.resources?.sessions.some((item) => item.sessionId === PREPARED_SESSION_ID) ?? false,
          false,
          "准备不进入会话清单",
        );
        expectThat(query(mounted.container, '[data-session-state="unknown"]') === null, "不得呈现「未知会话」");
        expectEqual(createFrameCount(sent()), 0, "新入口不得再发 session.create");
        expectEqual(
          commandNames(sent()).filter((name) => name === "session.read").length,
          0,
          "准备阶段不派发正式读取",
        );
        expectEqual(startBodies(sent()).length, 0, "准备阶段不派发首条");
        expectEqual(frameTypes(sent()).filter((type) => String(type).startsWith("attachment.")).length, 0, "准备阶段没有附件帧");

        // 切换设置：以 configure 交给临时端点，页面之后再以 Agent 的确认结果为准。
        click(query(panel, '[data-mode-choice="build"]'), "选择 build 模式");
        await waitFor(
          () => preparationBodies(sent()).some((body) => body["action"] === "configure" && body["mode"] === "build"),
          "configure 帧（mode）",
        );
        const configure = preparationAction(sent(), "configure");
        expectThat(configure !== undefined, "切换模式必须发出 configure");
        expectEqual(configure["sessionId"], PREPARED_SESSION_ID, "configure 指向同一准备");
        expectThat(typeof configure["receipt"] === "object", "configure 必须回传凭据");
        click(query(panel, '[data-settings-pill="model"]'), "打开会话模型选择器");
        click(query(panel, '[data-model-choice="gpt-5-mini"]'), "在会话中选择模型");
        await waitFor(
          () => preparationBodies(sent()).some((body) => body["action"] === "configure" &&
            (body["configOptions"] as Record<string, unknown> | undefined)?.["model"] === "gpt-5-mini"),
          "会话模型选择交给服务端确认",
        );
        expectEqual(createFrameCount(sent()), 0, "切换设置也不得发 session.create");
      } finally {
        await mounted.unmount();
      }
    },
  },
  {
    name: "R16P-2 刷新后同 ID 恢复：面板可见、草稿文字来自浏览器存储、只发 restore",
    async run() {
      await seedDraft(RELOAD_SESSION_ID, RELOAD_RECEIPT);
      const identity = await createFakeHostIdentity(HOST_ID);
      await seedPairedHost(identity);
      // 真实 Web 平台取不到「这是一次刷新」的 Navigation Timing：只替换这一条环境事实，
      // 其余端口（IndexedDB/sessionStorage/设备身份）仍是生产实现。
      const platform: PlatformPorts = {
        ...openPlatform(),
        preparationDrafts: openPreparationDraftStore({ navigationKind: () => "reload" }),
      };
      const mounted: Mounted = await mountScenario(
        baseScenario({
          route: sessionRoute(RELOAD_SESSION_ID),
          identity,
          platform,
          preparationResult: (body) => readyPreparationResult(body, RELOAD_RECEIPT),
        }),
      );
      const sent = (): readonly string[] => mounted.host?.sent ?? [];
      try {
        await waitFor(
          () => mounted.composition?.store.preparationPage(RELOAD_SESSION_ID)?.phase === "ready",
          "恢复后回到 ready（同 ID 重建临时资源）",
        );
        expectEqual(currentPath(), `/session/${RELOAD_SESSION_ID}`, "刷新不换地址");
        expectEqual(
          preparationInputOf(mounted.container)?.value,
          PREPARATION_TEXT,
          "输入文字必须从浏览器草稿恢复",
        );
        // restore 带同一 ID 与凭据；不重新分配、不发首条。
        const restore = preparationAction(sent(), "restore");
        expectThat(restore !== undefined, "刷新后必须用原凭据 restore");
        expectEqual(restore["sessionId"], RELOAD_SESSION_ID, "restore 指向同一准备");
        expectThat(typeof restore["receipt"] === "object", "restore 必须回传凭据");
        expectEqual(preparationAction(sent(), "allocate"), undefined, "刷新不得重新分配新 ID");
        expectEqual(startBodies(sent()).length, 0, "刷新不得自动发送首条");
        expectEqual(createFrameCount(sent()), 0, "恢复不得退回 session.create");
      } finally {
        await mounted.unmount();
      }
    },
  },
  {
    name: "R16P-3 陌生地址仅在权威不存在后回目录；快照等待/未认证不误跳",
    async run() {
      // 分支一：已在线、快照完整、权威读取明确失败 → replace("/")，且不留失效历史条目。
      const redirecting = await mountScenario(
        baseScenario({
          route: sessionRoute(MISSING_SESSION_ID),
          extraRoutes: [DIRS_ROUTE, sessionRoute(MISSING_SESSION_ID)],
          commandFailure: (command, sessionId) =>
            command === "session.read" && sessionId === MISSING_SESSION_ID
              ? { code: "session.not_found", message: "会话不存在", retryable: false, details: {} }
              : null,
        }),
      );
      try {
        await waitFor(() => currentPath() === "/", "权威不存在后回目录");
        expectEqual(
          redirecting.composition?.store.preparationPage(MISSING_SESSION_ID),
          null,
          "陌生 ID 不得凭空生成准备",
        );
        expectEqual(
          preparationAction(redirecting.host?.sent ?? [], "allocate"),
          undefined,
          "陌生 ID 不得触发分配",
        );
      } finally {
        await redirecting.unmount();
      }

      // 分支二：认证完成但快照尚未就绪 → 不能把「还没到」解释成「不存在」。
      const waiting = await mountScenario(
        baseScenario({
          route: sessionRoute(MISSING_SESSION_ID),
          extraRoutes: [DIRS_ROUTE, sessionRoute(MISSING_SESSION_ID)],
          deliverSnapshot: false,
          commandFailure: () => ({
            code: "session.not_found",
            message: "会话不存在",
            retryable: false,
            details: {},
          }),
        }),
      );
      try {
        await waitFor(
          () => waiting.composition?.store.state.connection.state === "replaying",
          "连接已认证、快照尚未就绪",
        );
        await settle();
        expectEqual(currentPath(), `/session/${MISSING_SESSION_ID}`, "快照等待期间不得跳目录");
      } finally {
        await waiting.unmount();
      }

      // 分支三：连认证都没完成（离线/未授权）时同样没有权威结论，不得跳转。
      const offline = await mountScenario(
        baseScenario({
          route: sessionRoute(MISSING_SESSION_ID),
          extraRoutes: [DIRS_ROUTE, sessionRoute(MISSING_SESSION_ID)],
          authenticate: false,
          commandFailure: () => ({
            code: "session.not_found",
            message: "会话不存在",
            retryable: false,
            details: {},
          }),
        }),
      );
      try {
        await waitFor(() => (offline.host?.deviceProofs.length ?? 0) === 1, "客户端已签出设备证明");
        expectEqual(
          offline.composition?.store.state.connection.state === "online",
          false,
          "前提：连接不在在线态",
        );
        await settle();
        expectEqual(currentPath(), `/session/${MISSING_SESSION_ID}`, "未认证/离线不得按「不存在」跳转");
      } finally {
        await offline.unmount();
      }
    },
  },
  {
    name: "R16P-4 未协商准备能力：新建入口明确禁用，不发准备帧也不退回 session.create",
    async run() {
      const mounted = await mountScenario(baseScenario({ selectPreparation: false }));
      const sent = (): readonly string[] => mounted.host?.sent ?? [];
      try {
        await waitFor(
          () => mounted.composition?.store.state.connection.state === "online",
          "连接已到在线（否则只是「还没连上」）",
        );
        await waitFor(() => query(mounted.container, '[data-action="create-session"]') !== null, "目录卡片");
        click(query(mounted.container, '[data-action="create-session"]'), "目录内新建");
        await waitFor(() => query(mounted.container, "[data-create-sheet]") !== null, "创建弹层打开");

        // 「已连上却没协商到该能力」必须明确说明，而不是静默禁用或退回旧路径。
        const unavailable = query(mounted.container, "[data-create-unavailable]");
        expectThat(unavailable !== null, "未协商时必须显式说明入口不可用");
        expectEqual(
          textOf(unavailable),
          PREPARATION_UNSUPPORTED_NOTICE,
          "说明必须是「服务端未协商会话准备能力」",
        );
        expectEqual(
          query(mounted.container, '[data-action="submit-create"]')?.getAttribute("aria-disabled"),
          "true",
          "未协商时提交必须禁用",
        );
        click(query(mounted.container, '[data-action="submit-create"]'), "点击被禁用的提交");
        await settle();
        expectEqual(preparationBodies(sent()).length, 0, "未协商不得发送任何准备帧");
        expectEqual(createFrameCount(sent()), 0, "未协商不得偷偷退回 session.create");
        expectEqual(currentPath(), "/", "未协商不离开目录页");
      } finally {
        await mounted.unmount();
      }
    },
  },
  {
    name: "R16P-5 首条提交：点击后只发一次 session.start，受理前不显示已保存，URL 不变",
    async run() {
      const mounted = await mountScenario(baseScenario({ holdCommands: ["session.start"] }));
      const sent = (): readonly string[] => mounted.host?.sent ?? [];
      try {
        await enterPreparation(mounted.container, mounted);
        typeIntoPreparation(mounted.container, PREPARATION_TEXT);
        await waitFor(
          () => preparationInputOf(mounted.container)?.value === PREPARATION_TEXT,
          "输入框吃下首条文字",
        );
        expectEqual(startBodies(sent()).length, 0, "点击之前不得派发首条");
        const composerBeforeSend = query(mounted.container, "[data-composer-input]");

        // 双击：只允许一次执行（闸门在仓库，页面连点不应产生第二条）。
        click(query(mounted.container, '[data-action="submit-prompt"]'), "发送首条（第一次）");
        click(query(mounted.container, '[data-action="submit-prompt"]'), "发送首条（第二次）");
        await waitFor(() => startBodies(sent()).length >= 1, "发出 session.start");
        await settle(100);
        expectEqual(startBodies(sent()).length, 1, "双击只产生一次首条提交");

        const start = startBodies(sent())[0];
        expectThat(start !== undefined, "必须有 session.start 帧");
        expectEqual(start["command"], "session.start", "命令名");
        expectEqual(start["sessionId"], PREPARED_SESSION_ID, "首条指向同一准备 ID");
        const payload = payloadOf(start);
        expectThat(payload !== null, "session.start 必须带 payload");
        expectEqual(payload["kind"], "initial", "首条是 initial，不是 continue");
        expectThat(
          JSON.stringify(payload["content"]).includes(PREPARATION_TEXT),
          "首条内容带上用户输入的文字",
        );
        expectEqual(createFrameCount(sent()), 0, "首条流程不得发 session.create");
        expectEqual(
          frameTypes(sent()).filter((type) => String(type).startsWith("attachment.")).length,
          0,
          "没有附件时不得出现附件帧（上传只发生在点击之后、且此处无附件）",
        );

        // 已保存（请求已发出）≠ 已受理：扣住终态期间页面仍是准备页的 submitting，不显示已保存。
        expectThat(
          query(mounted.container, "[data-composer-input]") !== null && mounted.composition?.store.preparationPage(PREPARED_SESSION_ID)?.phase === "submitting",
          "提交中保留同一会话输入框",
        );
        expectEqual(domText(mounted.container).includes("首条消息已保存"), false, "服务端确认前不得显示已保存");
        expectEqual(currentPath(), `/session/${PREPARED_SESSION_ID}`, "提交期间地址不变");

        // 服务端受理之后才推进：同一 URL 改读正式会话。
        expectThat(
          mounted.host?.completeHeldCommand("session.start", { status: "accepted", result: {} }) === true,
          "假主机必须有一条被扣住的 session.start",
        );
        await waitFor(
          () => mounted.composition?.store.preparationPage(PREPARED_SESSION_ID)?.phase === "accepted",
          "服务端受理后进入 accepted",
        );
        expectEqual(currentPath(), `/session/${PREPARED_SESSION_ID}`, "受理后 URL 仍不变");
        await waitFor(
          () => commandNames(sent()).filter((name) => name === "session.read").length >= 1,
          "同页读取正式会话",
        );
        expectEqual(startBodies(sent()).length, 1, "受理后不重复提交");
        expectThat(query(mounted.container, "[data-composer-input]") === composerBeforeSend,
          "首条受理前后复用同一个会话输入框，不切换页面实例");
      } finally {
        await mounted.unmount();
      }
    },
  },
  {
    name: "R16P-6 preparation.store_rebuilt：丢弃旧材料并回目录，用原材料重试不发第二次",
    async run() {
      const mounted = await mountScenario(
        baseScenario({
          commandFailure: (command) =>
            command === "session.start"
              ? {
                  code: "preparation.store_rebuilt",
                  message: "服务端存储已重建",
                  retryable: false,
                  details: {},
                }
              : null,
        }),
      );
      const sent = (): readonly string[] => mounted.host?.sent ?? [];
      try {
        await enterPreparation(mounted.container, mounted);
        typeIntoPreparation(mounted.container, PREPARATION_TEXT);
        await waitFor(
          () => preparationInputOf(mounted.container)?.value === PREPARATION_TEXT,
          "输入框吃下首条文字",
        );
        click(query(mounted.container, '[data-action="submit-prompt"]'), "发送首条");
        await waitFor(() => startBodies(sent()).length === 1, "发出 session.start");
        await waitFor(() => currentPath() === "/", "旧存储代失效后回目录重新开始");
        expectEqual(
          mounted.composition?.store.preparationPage(PREPARED_SESSION_ID),
          null,
          "旧准备必须被丢弃（不再可继续）",
        );
        expectEqual(startBodies(sent()).length, 1, "不得用旧材料重发首条");
        expectEqual(query(mounted.container, "[data-preparation-title]"), null, "准备页必须离开");
      } finally {
        await mounted.unmount();
      }
    },
  },
  {
    name: "R16P-7 preparation.already_committed：不再上传/再提交，restore 取关联后读正式会话",
    async run() {
      const firstSubmissionId = "eeeeeeee-0000-4000-8000-0000000000b1";
      const firstRequestId = "eeeeeeee-0000-4000-8000-0000000000b2";
      const currentRequestId = "eeeeeeee-0000-4000-8000-0000000000b3";
      const firstMessageId = "eeeeeeee-0000-4000-8000-0000000000b4";
      const mounted = await mountScenario(
        baseScenario({
          commandFailure: (command) =>
            command === "session.start"
              ? {
                  code: "preparation.already_committed",
                  message: "首条消息已受理",
                  retryable: false,
                  details: {},
                }
              : null,
          preparationResult: (body) =>
            body["action"] === "restore"
              ? {
                  operationId: body["operationId"],
                  action: "restore",
                  sessionId: PREPARED_SESSION_ID,
                  status: "committed",
                  firstSubmissionId,
                  firstRequestId,
                  currentRequestId,
                  firstMessageId,
                }
              : answerPreparation(body),
        }),
      );
      const sent = (): readonly string[] => mounted.host?.sent ?? [];
      try {
        await enterPreparation(mounted.container, mounted);
        typeIntoPreparation(mounted.container, PREPARATION_TEXT);
        await waitFor(
          () => preparationInputOf(mounted.container)?.value === PREPARATION_TEXT,
          "输入框吃下首条文字",
        );
        click(query(mounted.container, '[data-action="submit-prompt"]'), "发送首条");
        await waitFor(() => startBodies(sent()).length === 1, "发出 session.start");
        await waitFor(
          () => preparationAction(sent(), "restore") !== undefined,
          "已受理时用 restore 取关联",
        );
        const restore = preparationAction(sent(), "restore");
        expectThat(restore !== undefined, "必须有 restore 帧");
        expectEqual(restore["sessionId"], PREPARED_SESSION_ID, "restore 指向同一准备");

        // 不再次上传、不再次提交：首条只有一份。
        expectEqual(startBodies(sent()).length, 1, "已受理不得重发 session.start");
        expectEqual(
          frameTypes(sent()).filter((type) => String(type).startsWith("attachment.")).length,
          0,
          "已受理不得重新上传附件",
        );
        expectEqual(createFrameCount(sent()), 0, "已受理不得退回 session.create");

        // committed 之后同页读取正式会话（页面改读正式状态）。
        await waitFor(
          () => mounted.composition?.store.preparationPage(PREPARED_SESSION_ID)?.phase === "committed",
          "restore 回 committed",
        );
        await waitFor(
          () => commandNames(sent()).filter((name) => name === "session.read").length >= 1,
          "同页读取正式会话",
        );
        expectEqual(query(mounted.container, "[data-preparation-title]"), null, "准备页让位于正式会话");
      } finally {
        await mounted.unmount();
      }
    },
  },
  {
    name: "R16P-8 uncertain：不自动重发；页面不再有顶部状态条",
    async run() {
      const mounted = await mountScenario(baseScenario({ holdCommands: ["session.start"] }));
      const sent = (): readonly string[] => mounted.host?.sent ?? [];
      try {
        await enterPreparation(mounted.container, mounted);
        typeIntoPreparation(mounted.container, PREPARATION_TEXT);
        await waitFor(
          () => preparationInputOf(mounted.container)?.value === PREPARATION_TEXT,
          "输入框吃下首条文字",
        );
        click(query(mounted.container, '[data-action="submit-prompt"]'), "发送首条");
        await waitFor(() => startBodies(sent()).length === 1, "发出 session.start");

        expectThat(
          mounted.host?.completeHeldCommand("session.start", { status: "uncertain" }) === true,
          "假主机必须有一条被扣住的 session.start",
        );
        await waitFor(
          () => mounted.composition?.store.preparationPage(PREPARED_SESSION_ID)?.phase === "uncertain",
          "仓库进入 uncertain",
        );
        // 顶部不再呈现状态条与「核实原请求」；不确定说明由输入区保留。
        expectEqual(query(mounted.container, "[data-preparation-status]"), null, "顶部不得呈现状态条");
        expectEqual(query(mounted.container, '[data-action="preparation-verify"]'), null, "不再提供核实入口");
        await waitFor(
          () => domText(mounted.container).includes("所属节点无法确认首条消息是否已派发"),
          "输入区保留不确定说明",
        );
        await settle();
        expectEqual(startBodies(sent()).length, 1, "不确定不得重发首条");
        expectEqual(createFrameCount(sent()), 0, "不确定不得新建会话");
        expectEqual(statusBodies(sent()).length, 0, "不再自动查询原请求");
        expectEqual(currentPath(), `/session/${PREPARED_SESSION_ID}`, "不确定时地址不变");
      } finally {
        await mounted.unmount();
      }
    },
  },
];

export const SESSION_PREPARATION_CHECKS = checks;
