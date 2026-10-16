/**
 * R16 会话创建的浏览器级验证。
 *
 * ## 断言的是字节，不是模型对象
 *
 * `src/features/create-session-model.ts` 的 `assertCreateSessionRefs` 只校验**本模块内部**
 * 构造出来的对象；它拦不住「命令信封里多带了一个字段」。因此本文件抓的是
 * `FakeHost.sent` 里那条**帧原文**：点击真实的提交按钮 → 真实的
 * `ClientStore.submitCreateSession` → 真实的 `buildCreateSessionCommand` → 真实的 socket `send`，
 * 再对那串 JSON 文本做断言。
 *
 * ## 负向为主
 *
 * - 载荷键集必须**恰好**是规范允许的三类之一：`{agentId, workspaceAlias}`、
 *   `{agentId, workspacePath}`、`{agentId, workspacePath, createIfMissing}`；
 * - 帧原文里不得出现 MCP、凭据类字段名或值；
 * - **目录不由用户手工输入**：弹层里没有目录输入框，路径逐字来自服务端（快照或浏览结果）；
 * - **别名不出现在界面文本里**（只作 `data-*` 机器标识）；
 * - **服务端不支持浏览时明确说明**并保留「选已登记目录」这条路，且一条浏览命令都不发；
 * - **不能浏览的原因分岔**：「还没连上主机」与「已连上却没协商到该能力」是两条文案与两个锚点取值，
 *   未协商 MUST NOT 被写成「服务端没有提供该能力」（未认证起点没有快照，`selectedFeatures` 为空
 *   只是「还没协商」）；
 * - **入口不再有权限维度**：认证与否都不改变它——把门控加回来即红。
 *
 * ## 目录浏览的两条场景
 *
 * 假主机协商的 feature 里**没有** `core.workspace-browse.v1`，因此：
 *
 * - 「服务端不支持浏览」那一条走的是**完全真实**的链路（真实握手 → 真实快照 → 真实组件）；
 * - 「支持浏览」那一条由用例注入一次协商结果（`store.apply` 一条 `authenticated`），浏览结果则经
 *   `FakeHost.completeHeldCommand` 从**真实 socket** 投递（wire 解码、结果归并、渲染都走生产代码）。
 *   目的是把「逐层选定 / 已登记标注 / 新建一层目录 → createIfMissing」这三条规格场景在浏览器里钉住。
 */

import { createElement } from "react";
import { createRoot } from "react-dom/client";

import { CreateSessionSheet } from "../../../src/components";
import { WORKSPACE_BROWSE_FEATURE } from "../../../src/features/create-session-model";

import {
  AGENTS,
  DIRS_ROUTE,
  WORKSPACES,
  WORKSPACES_WITH_PATHS,
  dirRoute,
  sessionsForBothDirectories,
} from "../fixtures";
import {
  CANONICAL_ORIGIN,
  DEVICE_ID,
  PERMISSION_GATE_VOCABULARY,
  click,
  domText,
  expectAbsent,
  expectEqual,
  expectNone,
  expectThat,
  mountScenario,
  query,
  queryAll,
  textOf,
  waitFor,
} from "../support";
import type { Check, Mounted, ScenarioOptions } from "../support";
import { typeIntoInput } from "./r15-directory";

/** 创建弹层只出现在目录页与目录详情页；场景里没有任何授权设置可给（设备方向已无权限维度）。 */
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
    readResult: () => ({ sessionId: "", resources: { messages: [] }, hasEarlier: false }),
    route: dirRoute("alpha"),
    ...overrides,
  };
}

/**
 * 打开目录详情页的创建弹层并选好 Agent。
 *
 * 返回「选 Agent 之前」提交按钮的禁用状态——它是对照：若按钮在任何时候都禁用，
 * 后面的「确实发出了帧」就毫无意义。
 */
async function openSheetWithAgent(
  container: HTMLElement,
  agentId: string,
): Promise<{ readonly sheet: Element; readonly disabledBeforeSelect: boolean }> {
  await waitFor(
    () => query(container, '[data-action="open-create-sheet"]') !== null,
    "目录详情页渲染",
  );
  click(query(container, '[data-action="open-create-sheet"]'), "新建会话入口");
  await waitFor(() => query(container, "[data-create-sheet]") !== null, "创建弹层打开");
  const disabledBeforeSelect =
    query(container, '[data-action="submit-create"]')?.hasAttribute("disabled") ?? false;
  click(query(container, `button[data-agent-id="${agentId}"]`), `Agent 选项 ${agentId}`);
  await waitFor(
    () => query(container, `button[data-agent-id="${agentId}"]`)?.getAttribute("data-selected") === "true",
    `Agent ${agentId} 被选中`,
  );
  const sheet = query(container, "[data-create-sheet]");
  if (sheet === null) throw new Error("创建弹层消失");
  return { sheet, disabledBeforeSelect };
}

/** 一帧出站 JSON 的 `body`（先确证是对象、非数组，再按记录读键——不做内联类型断言）。 */
function frameBody(text: string): Record<string, unknown> | null {
  const parsed: unknown = JSON.parse(text);
  if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed) || !("body" in parsed)) return null;
  const body: unknown = parsed.body;
  if (typeof body !== "object" || body === null || Array.isArray(body)) return null;
  // 上面已确证形状，这里只是补上索引签名：按记录读键，键名由调用点的断言把关。
  return body as Record<string, unknown>;
}

/** 出站 `session.create` 帧原文；没发出去返回 `null`。 */
function createFrame(sent: readonly string[]): string | null {
  for (const text of sent) {
    const body = frameBody(text);
    if (body === null || body["command"] !== "session.create") continue;
    return text;
  }
  return null;
}

/**
 * 出站 `session.create` 帧里的 `payload`，逐层收窄后按记录返回；形状不符返回 `null`。
 *
 * 为什么返回原样的键值、而不是给载荷下一句 `{ workspaceAlias: string }` 之类的断言：本文件的
 * 断言要证明的正是**实际发出的那个对象**有哪些键、值是什么；先把形状断言一遍再读，读到的只会
 * 是那句断言的回声。这里只逐层确认「是对象、不是数组」（`null` 由调用点断言）。
 */
function createPayloadOf(sent: readonly string[]): Record<string, unknown> | null {
  const frame = createFrame(sent);
  if (frame === null) return null;
  const body = frameBody(frame);
  if (body === null) return null;
  const payload: unknown = body["payload"];
  if (typeof payload !== "object" || payload === null || Array.isArray(payload)) return null;
  // 到这里「是对象、非数组」已逐层查过，缺的只是索引签名：按记录读键值，键名由断言把关。
  return payload as Record<string, unknown>;
}

/** 出站 `workspace.browse` 帧的请求路径（按发送顺序）。 */
function browsePaths(sent: readonly string[]): readonly unknown[] {
  const paths: unknown[] = [];
  for (const text of sent) {
    const body = frameBody(text);
    if (body === null || body["command"] !== "workspace.browse") continue;
    const payload: unknown = body["payload"];
    if (typeof payload !== "object" || payload === null || Array.isArray(payload)) continue;
    paths.push((payload as Record<string, unknown>)["path"]);
  }
  return paths;
}

/** 载荷里**不得**出现的字段名与凭据/路径样式（出现即判红）。 */
const FORBIDDEN_IN_PAYLOAD = [
  "path",
  "cwd",
  "directory",
  "mcp",
  "MCP",
  "credential",
  "token",
  "secret",
  "password",
  "apiKey",
  "env",
  "ownerPath",
] as const;

/**
 * 一次「创建入口有没有被权限挡住」的观测：全部取自**真实组件**渲染出来的 DOM。
 *
 * 比较两个场景（认证 / 未认证）的这份观测，就是在比「入口的门控条件是否与认证有关」——
 * 设备方向已没有权限维度，两者必须逐项相同。
 *
 * 这份观测**刻意只收门控事实**，尤其**不**收「选中 Agent 之后提交按钮可点」：未认证起点没有快照，
 * 目录块退回「顶部入口」形态、目录还没定下来，那时提交按钮的 `disabled` 只反映「目录还没选」，
 * 与本用例要证的权限门控无关。两个起点因此不在这条上比对（认证起点单独的完整链路见 R16-1/R16-4/R16-5）。
 */
interface CreateGateObservation {
  /** 弹层里出现的权限门控类文案（空数组 = 一处都没有）。 */
  readonly gatingText: readonly string[];
  /** `data-create-denied-reason` 节点数（「被拒绝」的呈现）。 */
  readonly deniedReasons: number;
  /** `data-create-permission` 标记数（权限标识）。 */
  readonly permissionMarkers: number;
  /** 未选 Agent 时提交按钮是否禁用（禁用只该由「还没选 Agent」造成）。 */
  readonly disabledWithoutAgent: boolean;
  /** 入口旁的副作用说明文本。 */
  readonly effectNote: string;
  /** 弹层里的文本输入框数量（目录不由用户手工输入，因此恒为 0）。 */
  readonly inputs: number;
}

/**
 * 目录选择器与弹层之间的那条边界：等一条出站浏览请求。
 *
 * `sent` 每次重新取（假主机的 `sent` 是快照数组），因此传一个取值函数而不是数组。
 */
async function waitForBrowseRequest(sent: () => readonly string[], count: number): Promise<void> {
  await waitFor(() => browsePaths(sent()).length >= count, `第 ${count} 条 workspace.browse 请求`);
}

/**
 * 用例注入一次协商结果：假主机只协商两条必需 feature。
 *
 * 这是**浏览器级**用例唯一一处非真实输入（真实链路上这来自 `auth.server_challenge`），
 * 浏览结果本身仍经真实 socket 投递。
 */
function negotiateBrowse(mounted: Mounted): void {
  const store = mounted.composition?.store;
  expectThat(store !== undefined, "组合根未装配");
  store.apply({
    kind: "authenticated",
    info: {
      deviceId: DEVICE_ID,
      serverEpoch: "00384a03-bc90-4095-b65d-82fb8cc47e13",
      headGlobalSequence: "10",
      heartbeatIntervalMs: 30000,
      limits: { maxMessageBytes: 1048576, maxPromptBytes: 262144, maxReplayEventsPerBatch: 500 },
    },
    selectedFeatures: ["core.command-status.v1", "core.event-ack.v1", WORKSPACE_BROWSE_FEATURE],
  });
  expectEqual(
    store.state.selectedFeatures.includes(WORKSPACE_BROWSE_FEATURE),
    true,
    "协商结果应已写入仓库",
  );
}

const checks: readonly Check[] = [
  {
    name: "R16-1 目录已知的入口：目录/路径两行逐字来自服务端，载荷恰好是 agentId 与 workspaceAlias",
    async run() {
      const mounted = await mountScenario(baseScenario());
      try {
        const { sheet, disabledBeforeSelect } = await openSheetWithAgent(mounted.container, "codex-a");
        // 对照：未选 Agent 时按钮禁用、选中后可点——否则下面的「确实发出了帧」毫无意义。
        expectEqual(disabledBeforeSelect, true, "未选 Agent 时提交按钮应禁用");
        expectEqual(
          query(mounted.container, '[data-action="submit-create"]')?.hasAttribute("disabled") ?? true,
          false,
          "选中 Agent 后提交按钮应可点",
        );

        // 目录块是「已知目录」那一形态：展示名与路径各一行，路径逐字是快照下发的那一份。
        expectEqual(
          query(sheet, '[data-create-directory-block="known"]') !== null,
          true,
          "目录已知时应呈现目录/路径两行",
        );
        expectEqual(textOf(query(sheet, "[data-create-directory-name]")), "api", "目录行是展示名");
        expectEqual(
          textOf(query(sheet, "[data-create-directory-path]")),
          "D:\\secret\\vault\\repo-x",
          "路径行逐字呈现服务端下发的路径",
        );
        // 反例：把别名当展示名渲染（或干脆拿别名顶路径）时，文本里会出现 alpha。
        expectEqual(domText(sheet).includes("alpha"), false, "弹层文本里不得出现目录别名");
        // 目录不由用户手工输入：弹层里没有任何文本输入框。
        expectEqual(queryAll(sheet, "input").length, 0, "弹层不得呈现目录输入框");

        click(query(mounted.container, '[data-action="submit-create"]'), "提交创建");
        await waitFor(() => createFrame(mounted.host?.sent ?? []) !== null, "发出 session.create 帧");
        const frame = createFrame(mounted.host?.sent ?? []);
        expectThat(frame !== null, "点击提交后必须真的发出一帧 session.create");

        const payload = createPayloadOf(mounted.host?.sent ?? []);
        expectThat(payload !== null, "session.create 帧里必须带一个对象载荷");
        expectEqual(
          Object.keys(payload).sort(),
          ["agentId", "workspaceAlias"],
          "目录已知时载荷的键集必须恰好是两个引用",
        );
        expectEqual(payload["workspaceAlias"], "alpha", "载荷里的目录引用");
        expectEqual(payload["agentId"], "codex-a", "载荷里的 Agent 引用");

        // MUST NOT：帧**原文**里不得携带 Provider/MCP 凭据，也不得凭空多带一条宿主路径。
        expectNone(frame ?? "", FORBIDDEN_IN_PAYLOAD, "session.create 帧里出现了越界字段名");
        expectAbsent(frame ?? "", "D:", "session.create 帧里出现了本机盘符路径");
        expectAbsent(frame ?? "", "secret", "session.create 帧里出现了敏感片段");
      } finally {
        await mounted.unmount();
      }
    },
  },
  {
    name: "R16-2 快照没下发路径时：路径行呈现「—」，MUST NOT 用别名或展示名拼一段像路径的文本",
    async run() {
      // 同一页的第二种场景：workspaces 只带协议字段（`path` 缺席）。
      const mounted = await mountScenario(baseScenario({ workspaces: WORKSPACES }));
      try {
        const { sheet } = await openSheetWithAgent(mounted.container, "codex-a");
        expectEqual(textOf(query(sheet, "[data-create-directory-name]")), "api", "目录行仍是展示名");
        const pathText = textOf(query(sheet, "[data-create-directory-path]"));
        expectEqual(pathText, "—", "未下发路径时呈现「—」");
        // 反例：用别名或展示名冒充路径时，这一段会变成 alpha/api 之类的文本。
        expectEqual(pathText.includes("alpha"), false, "不得用别名冒充路径");
        expectEqual(pathText.includes("api"), false, "不得用展示名冒充路径");
      } finally {
        await mounted.unmount();
      }
    },
  },
  {
    name: "R16-3 认证与否都不构成创建门控：两种起点都不出现「被拒绝」的呈现，认证起点选中 Agent 即可提交",
    async run() {
      /**
       * 观测一个起点下的创建入口：模型由**真实** ClientStore 在浏览器里算出（`store.createEntry`），
       * 再交给真实组件渲染，断的是页面看到的东西。
       *
       * 两个起点都要看：设备方向已没有权限维度，认证与否都不该改变入口的门控事实——把门控重新
       * 挂到认证（或别处）上，其中一个起点就会与另一个不一致。
       */
      const observe = async (authenticate: boolean, label: string): Promise<CreateGateObservation> => {
        const mounted = await mountScenario(baseScenario({ authenticate }));
        try {
          // 取模型前必须先等**该场景的握手真的走到那一步**，否则读到的可能只是「什么都还没发生」。
          if (authenticate) {
            await waitFor(
              () => mounted.composition?.store.state.deviceId !== null,
              "认证事实（deviceId）已写入 store",
            );
          } else {
            await waitFor(
              () => (mounted.host?.deviceProofs.length ?? 0) === 1,
              "客户端签出设备证明（服务端不再下发认证）",
            );
          }

          const entry = mounted.composition?.store.createEntry("alpha");
          if (entry === undefined) throw new Error("组合根未装配");
          const host = document.createElement("div");
          document.body.append(host);
          const root = createRoot(host);
          const render = (selectedAgentId: string | null): void => {
            root.render(
              createElement(CreateSessionSheet, {
                model: entry,
                selectedAgentId,
                onSelectAgent: () => undefined,
                onSubmit: () => undefined,
                onDismissUncertain: () => undefined,
                onClose: () => undefined,
              }),
            );
          };
          const submitButton = (): Element | null => query(host, '[data-action="submit-create"]');

          render(null);
          await waitFor(
            () => query(host, "[data-create-sheet]") !== null,
            "创建弹层渲染（直接渲染真实组件）",
          );
          const buttonWithoutAgent = submitButton();
          expectThat(buttonWithoutAgent !== null, `${label}：创建弹层必须有提交按钮`);
          const gatingText = PERMISSION_GATE_VOCABULARY.filter((needle) => domText(host).includes(needle));
          const deniedReasons = queryAll(host, "[data-create-denied-reason]").length;
          const permissionMarkers = queryAll(host, "[data-create-permission]").length;
          const effectNote = textOf(query(host, "[data-create-note]"));
          const inputs = queryAll(host, "input").length;
          const disabledWithoutAgent = buttonWithoutAgent.hasAttribute("disabled");

          // 认证起点有快照 → 目录块是「已知目录」那一形态、目录已经定下来，因此选中 Agent 之后必须
          // 可点：这就是「入口不因权限/认证不可用」的可观察形式。
          //
          // 未认证起点**不**在这里断言这件事：它没有快照，目录块退回「顶部入口」形态、目录还没定，
          // 提交按钮的禁用只反映「目录还没选」，与权限门控无关（把它算进观测会让两个起点在这条上
          // 必然不等，而那条不等并不说明门控回来了）。
          if (authenticate) {
            render("codex-a");
            await waitFor(
              () => (submitButton()?.hasAttribute("disabled") ?? true) === false,
              `${label}：选中 Agent 后提交按钮必须可点`,
            );
          } else {
            // 未认证 / 没有快照时 `selectedFeatures` 是空的，但那只是「还没协商」：说明文字必须说
            // 「还没连上主机」，而不是断言服务端不支持浏览（那句只在已连上却没协商到该能力时成立）。
            expectThat(
              query(host, '[data-create-browse-unavailable="offline"]') !== null,
              `${label}：不能逐层浏览的原因应说明「还没连上主机」`,
            );
            expectEqual(
              domText(host).includes("服务端没有提供目录浏览能力"),
              false,
              `${label}：未协商不得写成「服务端没有提供该能力」`,
            );
          }

          root.unmount();
          host.remove();
          return {
            gatingText,
            deniedReasons,
            permissionMarkers,
            disabledWithoutAgent,
            effectNote,
            inputs,
          };
        } finally {
          await mounted.unmount();
        }
      };

      /** 无论哪个起点，这些事实都必须成立（不成立即说明又出现了权限门控）。 */
      const assertNoGate = (observation: CreateGateObservation, label: string): void => {
        expectEqual(observation.gatingText, [], `${label}：弹层出现了权限门控类文案`);
        expectEqual(observation.deniedReasons, 0, `${label}：不得出现「被拒绝」的入口呈现`);
        expectEqual(observation.permissionMarkers, 0, `${label}：不得出现权限标识`);
        expectEqual(observation.inputs, 0, `${label}：弹层不得呈现目录输入框`);
        expectThat(
          observation.effectNote.includes("Agent 进程"),
          `${label}：副作用说明必须呈现（${JSON.stringify(observation.effectNote)}）`,
        );
        // 禁用只由「还没选 Agent」造成（提交按钮「呈现出来而不是被藏起来」由 `observe` 里的
        // 非空断言把关，两个起点各跑一次）。
        expectEqual(observation.disabledWithoutAgent, true, `${label}：未选 Agent 时提交按钮应禁用`);
      };

      const authenticated = await observe(true, "认证");
      const unauthenticated = await observe(false, "未认证");
      assertNoGate(authenticated, "认证");
      assertNoGate(unauthenticated, "未认证");
      // 两种起点逐项一致：认证不是创建入口的可用性条件。
      expectEqual(unauthenticated, authenticated, "认证与未认证两种起点下创建入口的呈现必须一致");
    },
  },
  {
    name: "R16-4 服务端不支持浏览：顶部入口明确说明并保留已登记目录，载荷仍恰好两个引用",
    async run() {
      const mounted = await mountScenario(baseScenario({ route: DIRS_ROUTE }));
      try {
        // 这条用例的前提是「**已连上**（认证与追平都完成）却没协商到该能力」：先等连接真的走到在线，
        // 否则说明文字会落到「还没连上」那一支，与用例名里的口径不符（那种起点由 R16-3 覆盖）。
        await waitFor(
          () => query(mounted.container, '[data-connection-state="online"]') !== null,
          "连接已到在线",
        );
        await waitFor(() => queryAll(mounted.container, "[data-directory-alias]").length === 2, "目录页渲染出两条目录");
        // 顶部入口落在**目录列表工具条右端**（原型的位置），不是列表上方单独一行：这条按 DOM 包含
        // 关系量（工具条的机器锚点 `data-directory-toolbar` 由 `DirectoryList` 发出）。
        expectThat(
          query(mounted.container, '[data-directory-toolbar] [data-action="create-session-browse"]') !== null,
          "顶部创建入口必须画在目录列表的工具条里",
        );
        // 顶部入口与卡片入口是**两个**不同的控件：卡片入口属于被点的目录，顶部入口的目录还没定。
        expectEqual(
          queryAll(mounted.container, '[data-action="create-session"]').length,
          2,
          "每个目录项各有一个「⊕ 新建」入口",
        );
        click(query(mounted.container, '[data-action="create-session-browse"]'), "顶部新建入口");
        await waitFor(() => query(mounted.container, "[data-create-sheet]") !== null, "创建弹层打开");

        const sheet = query(mounted.container, "[data-create-sheet]");
        expectThat(sheet !== null, "创建弹层必须渲染出来");
        expectEqual(
          query(mounted.container, '[data-create-directory-block="browse"]') !== null,
          true,
          "顶部入口呈现「选择文件夹 + 已选路径 + 说明」那一形态",
        );
        // 假主机没协商 `core.workspace-browse.v1`：入口必须**明确说明**该能力不可用。
        // 锚点带**原因**：这个场景是「已连上（在线）却没协商到该能力」——正是「服务端没有提供」那一支。
        expectThat(
          query(mounted.container, '[data-create-browse-unavailable="unsupported"]') !== null,
          "不支持浏览时入口必须说明该能力不可用",
        );
        expectEqual(
          query(mounted.container, '[data-action="open-directory-picker"]')?.getAttribute("data-picker-supported") ?? null,
          "false",
          "控件应标出浏览能力不可用",
        );
        expectEqual(queryAll(sheet, "input").length, 0, "不得退化成手工输入目录路径");

        // 保留下来的那条路：从已登记目录里选一个。
        click(query(mounted.container, '[data-action="open-directory-picker"]'), "选择已登记的目录");
        await waitFor(() => query(mounted.container, '[data-directory-picker="open"]') !== null, "选择器打开");
        expectThat(
          query(mounted.container, '[data-browse-unsupported="unsupported"]') !== null,
          "选择器里同样要说明浏览能力不可用",
        );
        // 反例：静默失败时这里既没有说明、也没有可选项（列表为空）。
        expectEqual(
          queryAll(mounted.container, '[data-action="choose-registered-directory"]').length,
          2,
          "已登记目录集合必须作为回退选项摆出来",
        );
        // 一条浏览命令都不许发：发了必然被 feature 门控拒（服务端在触碰文件系统之前就回错误）。
        expectEqual(browsePaths(mounted.host?.sent ?? []).length, 0, "不支持浏览时不得派发 workspace.browse");

        click(
          query(mounted.container, '[data-action="choose-registered-directory"][data-registered-alias="beta"]'),
          "回退列表里的 beta",
        );
        await waitFor(() => query(mounted.container, "[data-directory-picker]") === null, "选择器关闭");
        await waitFor(
          () =>
            query(mounted.container, '[data-create-directory-notice="registered"]') !== null,
          "分支提示说明该目录已登记",
        );
        expectEqual(
          textOf(query(mounted.container, "[data-create-selected-name]")),
          "api",
          "已选目录呈现展示名",
        );

        click(query(mounted.container, 'button[data-agent-id="codex-a"]'), "Agent codex-a");
        await waitFor(
          () => query(mounted.container, 'button[data-agent-id="codex-a"]')?.getAttribute("data-selected") === "true",
          "Agent 被选中",
        );
        click(query(mounted.container, '[data-action="submit-create"]'), "提交创建");
        await waitFor(() => createFrame(mounted.host?.sent ?? []) !== null, "发出 session.create 帧");
        const payload = createPayloadOf(mounted.host?.sent ?? []);
        expectThat(payload !== null, "session.create 帧里必须带一个对象载荷");
        expectEqual(
          Object.keys(payload).sort(),
          ["agentId", "workspaceAlias"],
          "从已登记目录选出来的目录引用走别名，键集恰好两个",
        );
        expectEqual(payload["workspaceAlias"], "beta", "载荷里的目录引用是选中的那一项");
        expectNone(createFrame(mounted.host?.sent ?? []) ?? "", FORBIDDEN_IN_PAYLOAD, "帧里出现了越界字段名");
      } finally {
        await mounted.unmount();
      }
    },
  },
  {
    name: "R16-5 协商到浏览能力：逐层选定、已登记标注、新建一层目录 → 载荷恰好三个键",
    async run() {
      const mounted = await mountScenario(
        baseScenario({ route: DIRS_ROUTE, holdCommands: ["workspace.browse"] }),
      );
      const sent = (): readonly string[] => mounted.host?.sent ?? [];
      try {
        await waitFor(() => query(mounted.container, '[data-action="create-session-browse"]') !== null, "目录页渲染");
        // 目录浏览要在浏览器里被验，就得先把它协商出来（假主机只协商两条必需 feature）。
        negotiateBrowse(mounted);

        click(query(mounted.container, '[data-action="create-session-browse"]'), "顶部新建入口");
        await waitFor(() => query(mounted.container, '[data-action="open-directory-picker"]') !== null, "弹层打开");
        click(query(mounted.container, '[data-action="open-directory-picker"]'), "选择文件夹");
        await waitFor(() => query(mounted.container, '[data-directory-picker="open"]') !== null, "选择器打开");

        // 打开即请求**根列表**（`path: null`）。
        await waitForBrowseRequest(sent, 1);
        expectEqual(browsePaths(sent()), [null], "第一层请求是根列表");
        // 根列表来自服务端：这一层由假主机经真实 socket 投递。
        mounted.host?.completeHeldCommand("workspace.browse", {
          result: {
            path: null,
            entries: [{ name: "D:", path: "D:\\", registered: false }],
            truncated: false,
          },
        });
        await waitFor(
          () => queryAll(mounted.container, '[data-action="enter-directory"]').length === 1,
          "根列表渲染出盘符条目",
        );
        expectEqual(
          textOf(query(mounted.container, '[data-action="confirm-directory"]')),
          "选择此文件夹",
          "根层不可选，但仍呈现该控件",
        );
        expectEqual(
          query(mounted.container, '[data-action="confirm-directory"]')?.hasAttribute("disabled") ?? false,
          true,
          "根列表本身不是目录，「选择此文件夹」应禁用",
        );

        // 逐层进入：第二层的请求路径就是条目的服务端路径（客户端不拼路径）。
        click(query(mounted.container, '[data-action="enter-directory"]'), "进入 D:");
        await waitForBrowseRequest(sent, 2);
        expectEqual(browsePaths(sent()), [null, "D:\\"], "第二层请求的父路径逐字来自服务端条目");
        mounted.host?.completeHeldCommand("workspace.browse", {
          result: {
            path: "D:\\",
            entries: [
              { name: "work", path: "D:\\work", registered: false },
              { name: "kept", path: "D:\\kept", registered: true },
            ],
            truncated: false,
          },
        });
        await waitFor(
          () => queryAll(mounted.container, '[data-action="enter-directory"]').length === 2,
          "这一层渲染出两个子目录",
        );
        // 已登记的目录被就地标注（spec 场景「已登记目录被标注」）。
        const entries = queryAll(mounted.container, '[data-action="enter-directory"]');
        const kept = entries.find((node) => node.getAttribute("data-entry-path") === "D:\\kept");
        expectThat(kept !== undefined, "这一层必须包含 kept 条目");
        expectEqual(kept.getAttribute("data-entry-registered"), "true", "已登记目录必须被标注");
        expectThat(domText(kept).includes("已登记"), "已登记目录的标注必须是可见文本");
        expectEqual(
          entries.find((node) => node.getAttribute("data-entry-path") === "D:\\work")?.getAttribute("data-entry-registered"),
          "false",
          "未登记的目录不得被标成已登记",
        );

        // 在当前层新建一层目录：候选路径 = 服务端回显的当前层 + 用户输入的名字。
        const work = queryAll(mounted.container, '[data-action="enter-directory"]').find(
          (node) => node.getAttribute("data-entry-path") === "D:\\work",
        );
        click(work ?? null, "进入 work");
        await waitForBrowseRequest(sent, 3);
        mounted.host?.completeHeldCommand("workspace.browse", {
          result: { path: "D:\\work", entries: [], truncated: false },
        });
        await waitFor(() => query(mounted.container, '[data-action="open-new-folder"]') !== null, "这一层可新建文件夹");
        click(query(mounted.container, '[data-action="open-new-folder"]'), "新建文件夹");
        // RNW 的 `onPress` 到 DOM 更新要一个 tick：点完必须等下一条锚点真的出现，再往里输入
        // （全仓 e2e 的既有写法，`typeIntoInput` 找不到节点时只会抛一句读不出原因的错）。
        await waitFor(
          () => query(mounted.container, "[data-browse-folder-name]") !== null,
          "新建文件夹输入框",
        );
        typeIntoInput(mounted.container, "[data-browse-folder-name]", "api", "新文件夹名字");
        await waitFor(
          () => query(mounted.container, "[data-browse-folder-name]")?.getAttribute("data-browse-folder-name-value") === "api",
          "输入框吃下了这份名字",
        );
        click(query(mounted.container, '[data-action="create-folder"]'), "新建并选择");
        await waitFor(() => query(mounted.container, '[data-directory-picker]') === null, "选择器关闭");
        await waitFor(
          () => query(mounted.container, '[data-create-directory-notice="will_create"]') !== null,
          "弹层说明会先在主机上新建这个目录",
        );
        expectEqual(
          textOf(query(mounted.container, "[data-create-directory-path]")),
          "D:\\work\\api",
          "已选路径是父层回显与名字拼出的候选路径",
        );
        expectEqual(textOf(query(mounted.container, "[data-create-selected-name]")), "api", "已选目录的名字");

        click(query(mounted.container, 'button[data-agent-id="codex-a"]'), "Agent codex-a");
        await waitFor(
          () => query(mounted.container, 'button[data-agent-id="codex-a"]')?.getAttribute("data-selected") === "true",
          "Agent 被选中",
        );
        click(query(mounted.container, '[data-action="submit-create"]'), "提交创建");
        await waitFor(() => createFrame(sent()) !== null, "发出 session.create 帧");
        const payload = createPayloadOf(sent());
        expectThat(payload !== null, "session.create 帧里必须带一个对象载荷");
        // 三键：目录引用走路径那一支，并显式带上「不存在则先创建」。
        expectEqual(
          Object.keys(payload).sort(),
          ["agentId", "createIfMissing", "workspacePath"],
          "新建目录的载荷恰好是 agentId + workspacePath + createIfMissing",
        );
        expectEqual(payload["workspacePath"], "D:\\work\\api", "载荷里的路径是选定的那一层");
        expectEqual(payload["createIfMissing"], true, "新建的目录必须带上建目录开关");
        expectEqual(payload["agentId"], "codex-a", "载荷里的 Agent 引用");
        expectNone(createFrame(sent()) ?? "", FORBIDDEN_IN_PAYLOAD, "帧里出现了越界字段名");
      } finally {
        await mounted.unmount();
      }
    },
  },
];

export const R16_CHECKS = checks;
