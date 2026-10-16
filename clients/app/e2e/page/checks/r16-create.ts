/**
 * R16 会话创建的浏览器级验证。
 *
 * ## 断言的是字节，不是模型对象
 *
 * `src/features/create-session-model.ts` 的 `assertCreateSessionRefs` 只校验**本模块内部**
 * 构造出来的对象；它拦不住「命令信封里多带了一个字段」。因此本文件抓的是
 * `FakeHost.sent` 里那条**帧原文**：点击真实的提交按钮 → 真实的 `ClientStore.submitCreateSession`
 * → 真实的 `buildCreateSessionCommand` → 真实的 socket `send`，再对那串 JSON 文本做断言。
 *
 * ## 负向为主
 *
 * - 载荷键集必须**恰好**是 `{agentId}` 加**一个**目录引用（`workspaceAlias` 或 `workspacePath`）；
 * - 帧原文里不得出现 MCP、凭据类字段名或值；
 * - **入口不再有权限维度**：入口不是禁用态、目录页与详情页都没有权限提示条/「不可用」说明、
 *   也没有「电脑端授予」类文案，且认证与否都不改变它——把门控加回来即红。
 */

import { createElement } from "react";
import { createRoot } from "react-dom/client";

import { CreateSessionSheet } from "../../../src/components";

import {
  AGENTS,
  DIRECTORY_ROUTES,
  DIRS_ROUTE,
  HOST_PATH_FRAGMENTS,
  WORKSPACES_WITH_PATHS,
  dirRoute,
  sessionsForBothDirectories,
} from "../fixtures";
import {
  CANONICAL_ORIGIN,
  PERMISSION_GATE_VOCABULARY,
  click,
  domHtml,
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
import type { Check, ScenarioOptions } from "../support";
import { typeIntoInput } from "./r15-directory";

/** 创建弹层只出现在目录详情页；场景里没有任何授权设置可给（设备方向已无权限维度）。 */
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
 * 打开创建弹层并选好 Agent。
 *
 * 返回弹层根节点与「选 Agent 之前」提交按钮的禁用状态——它是 R16-1 的对照：
 * 若按钮在任何时候都禁用，后面的「确实发出了帧」就毫无意义。
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

/** 出站 `session.create` 帧原文；没发出去返回 `null`。 */
function createFrame(sent: readonly string[]): string | null {
  for (const text of sent) {
    const parsed: unknown = JSON.parse(text);
    if (typeof parsed !== "object" || parsed === null) continue;
    const body = (parsed as { body?: unknown }).body;
    if (typeof body !== "object" || body === null) continue;
    if ((body as { command?: unknown }).command !== "session.create") continue;
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
  const parsed: unknown = JSON.parse(frame);
  if (typeof parsed !== "object" || parsed === null || !("body" in parsed)) return null;
  const body: unknown = parsed["body"];
  if (typeof body !== "object" || body === null || !("payload" in body)) return null;
  const payload: unknown = body["payload"];
  if (typeof payload !== "object" || payload === null || Array.isArray(payload)) return null;
  // 到这里「是对象、非数组」已逐层查过，缺的只是索引签名：按记录读键值，键名由断言把关。
  const record = payload as Record<string, unknown>;
  return record;
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
 * 用户在创建弹层里**手工输入**的目录（R16-2：目录引用走 `workspacePath` 那一支）。
 *
 * 浏览器拿不到宿主路径，因此这是 `workspacePath` 的唯一来源；用固定的字面量才能在帧原文里
 * 逐字断言「载荷带的就是用户输入的这一份」。取值刻意避开 `fixtures.ts` 的
 * `SECRET_FRAGMENTS`（那是「快照里的路径不得被渲染出来」那批断言用的），免得两件事看起来像一件。
 */
const USER_DIRECTORY_PATH = "C:\\projects\\api";

/**
 * 一次「创建入口有没有被权限挡住」的观测：全部取自**真实组件**渲染出来的 DOM。
 *
 * 比较两个场景（认证 / 未认证）的这份观测，就是在比「入口的门控条件是否与认证有关」——
 * 设备方向已没有权限维度，两者必须逐项相同。
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
  /** 选中 Agent 后提交按钮是否禁用（入口不该因权限/认证而不可点）。 */
  readonly disabledWithAgent: boolean;
  /** 入口旁的副作用说明文本。 */
  readonly effectNote: string;
}

const checks: readonly Check[] = [
  {
    name: "R16-1 创建载荷恰好是两个引用：断言的是实际发出的那一帧字节",
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

        click(query(mounted.container, '[data-action="submit-create"]'), "提交创建");
        await waitFor(() => createFrame(mounted.host?.sent ?? []) !== null, "发出 session.create 帧");
        const frame = createFrame(mounted.host?.sent ?? []);
        expectThat(frame !== null, "点击提交后必须真的发出一帧 session.create");

        const payload = createPayloadOf(mounted.host?.sent ?? []);
        expectThat(payload !== null, "session.create 帧里必须带一个对象载荷");
        expectEqual(
          Object.keys(payload).sort(),
          ["agentId", "workspaceAlias"],
          "载荷的键集必须恰好是两个引用",
        );
        expectEqual(payload["workspaceAlias"], "alpha", "载荷里的目录引用");
        expectEqual(payload["agentId"], "codex-a", "载荷里的 Agent 引用");

        // MUST NOT：帧**原文**里不得携带 Owner 路径或 Provider/MCP 凭据。
        expectNone(frame ?? "", FORBIDDEN_IN_PAYLOAD, "session.create 帧里出现了越界字段名");
        expectAbsent(frame ?? "", "D:", "session.create 帧里出现了本机盘符路径");
        expectAbsent(frame ?? "", "secret", "session.create 帧里出现了敏感片段");
        // 对照：弹层渲染**只**含提示语里的示例目录（`D:\work\api`），不得出现本机已登记目录的真实路径。
        // 目录输入由用户自己填写，服务端不回显任何宿主路径——因此这里按「真实路径片段」判，而不是按盘符。
        expectNone(domHtml(mounted.container), HOST_PATH_FRAGMENTS, "创建弹层渲染泄露了本机路径");
        expectEqual(
          textOf(query(sheet, "[data-create-sheet-title]")).includes("alpha"),
          true,
          "创建弹层标题应给出目录别名",
        );
      } finally {
        await mounted.unmount();
      }
    },
  },
  {
    name: "R16-2 没有任何授权设置时入口可用且无权限痕迹：填了目录就发 workspacePath，载荷恰好一个目录引用",
    async run() {
      const mounted = await mountScenario(baseScenario());
      try {
        await waitFor(
          () => query(mounted.container, '[data-action="open-create-sheet"]') !== null,
          "目录详情页渲染",
        );
        // 入口**不是禁用态**：设备不再有创建会话的权限维度（spec「入口不因权限不可用」）。
        // 场景里也没有任何授权设置可给——`auth.authenticated` 的 body 早已不含 scope 集合。
        const entryButton = query(mounted.container, '[data-action="open-create-sheet"]');
        expectThat(entryButton !== null, "目录详情页必须有创建会话入口");
        expectEqual(entryButton.hasAttribute("disabled"), false, "创建入口不得是禁用态");

        // 权限痕迹一处都不许有：提示条、「不可用并给出说明」的呈现、以及「电脑端授予」类文案。
        // 把门控加回来（哪怕只是把提示条藏进别的容器）→ 这三条中的一条变红。
        expectThat(
          query(mounted.container, "[data-create-permission-note]") === null,
          "目录详情页不得再呈现创建权限提示条",
        );
        expectThat(
          query(mounted.container, '[data-create-entry-unavailable="true"]') === null,
          "创建入口不得再呈现成「不可用并给出说明」",
        );
        expectNone(domText(mounted.container), PERMISSION_GATE_VOCABULARY, "目录详情页出现了权限门控类文案");
        // 副作用说明照常呈现（spec「创建入口的副作用说明」）：它曾与权限态成对出现，现在恒在。
        expectThat(
          textOf(query(mounted.container, "[data-create-side-effect]")).includes("Agent 进程"),
          "入口旁必须说明会在电脑上启动一个 Agent 进程",
        );

        // 入口接的是弹层（不是画了一个按钮）：点开，弹层里同样不许有权限痕迹。
        click(entryButton, "创建入口");
        await waitFor(() => query(mounted.container, "[data-create-sheet]") !== null, "创建弹层打开");
        const sheet = query(mounted.container, "[data-create-sheet]");
        expectThat(sheet !== null, "创建弹层必须渲染出来");
        expectThat(
          query(mounted.container, "[data-create-permission]") === null,
          "创建弹层不得再带权限标识",
        );
        expectNone(domText(sheet), PERMISSION_GATE_VOCABULARY, "创建弹层出现了权限门控类文案");

        // 在目录输入框里填一个电脑上的绝对目录：目录引用必须走 `workspacePath`。
        click(query(mounted.container, 'button[data-agent-id="codex-a"]'), "Agent codex-a");
        await waitFor(
          () =>
            query(mounted.container, 'button[data-agent-id="codex-a"]')?.getAttribute("data-selected") ===
            "true",
          "Agent 被选中",
        );
        typeIntoInput(
          mounted.container,
          "[data-create-directory]",
          USER_DIRECTORY_PATH,
          "创建弹层的目录输入框",
        );
        // 受控输入真的吃下了这份值再提交：否则「载荷里是用户输入的那一份」可能只是它渲染前的占位。
        await waitFor(
          () =>
            query(mounted.container, "[data-create-directory]")?.getAttribute("data-create-directory-value") ===
            USER_DIRECTORY_PATH,
          "目录输入框接受了用户输入的路径",
        );
        click(query(mounted.container, '[data-action="submit-create"]'), "提交创建");
        await waitFor(() => createFrame(mounted.host?.sent ?? []) !== null, "发出 session.create 帧");
        const payload = createPayloadOf(mounted.host?.sent ?? []);
        expectThat(payload !== null, "点击提交后必须真的发出一帧带对象载荷的 session.create");
        // 目录引用**恰好一个**：别名与路径同时给出（或都不给）都会被服务端按结构错误拒，
        // 键集相等在这里当场判红。
        expectEqual(
          Object.keys(payload).sort(),
          ["agentId", "workspacePath"],
          "填了目录时载荷恰好是 agentId + workspacePath（不是两者都给）",
        );
        expectEqual(payload["workspacePath"], USER_DIRECTORY_PATH, "载荷里的目录必须是用户输入的那一个");
        expectEqual(payload["agentId"], "codex-a", "载荷里的 Agent 引用");
      } finally {
        await mounted.unmount();
      }
    },
  },
  {
    name: "R16-3 认证与否都不构成创建门控：两种起点都不出现「被拒绝」的呈现，选中 Agent 即可提交",
    async run() {
      /**
       * 观测一个起点下的创建入口：模型由**真实** ClientStore 在浏览器里算出（`store.createEntry`），
       * 再交给真实组件渲染，断的是页面看到的东西。
       *
       * 两个起点都要看：设备方向已没有权限维度，认证与否都不该改变入口的可提交性——把门控重新
       * 挂到认证（或别处）上，其中一个起点就会与另一个不一致。
       */
      const observe = async (authenticate: boolean, label: string): Promise<CreateGateObservation> => {
        const mounted = await mountScenario(baseScenario({ authenticate }));
        try {
          // 取模型前必须先等**该场景的握手真的走到那一步**，否则读到的可能只是「什么都还没发生」。
          // 认证的场景等 `state.deviceId`——它是 `auth.authenticated` 的直接后果；设备方向已没有
          // scope 集合可等（那正是本次变更删掉的东西）。未认证的场景服务端不下发认证，因此以
          // 「客户端确实签出了设备证明」为握手走完的标志。
          // 旧写法等的是 `[data-route="dir-detail"]`——它同时被占位 `RuntimeUnavailable` 命中，
          // 于是这条等待在首帧占位时就成立，两个场景都可能读到「什么都还没发生」，断言随机变红。
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
          const disabledWithoutAgent = buttonWithoutAgent.hasAttribute("disabled");

          // 选中 Agent 之后必须可点：这就是「入口不因权限/认证不可用」的可观察形式。
          render("codex-a");
          await waitFor(
            () => (submitButton()?.hasAttribute("disabled") ?? true) === false,
            `${label}：选中 Agent 后提交按钮必须可点`,
          );
          const disabledWithAgent = submitButton()?.hasAttribute("disabled") ?? true;

          root.unmount();
          host.remove();
          return { gatingText, deniedReasons, permissionMarkers, disabledWithoutAgent, disabledWithAgent, effectNote };
        } finally {
          await mounted.unmount();
        }
      };

      /** 无论哪个起点，这些事实都必须成立（不成立即说明又出现了权限门控）。 */
      const assertNoGate = (observation: CreateGateObservation, label: string): void => {
        expectEqual(observation.gatingText, [], `${label}：弹层出现了权限门控类文案`);
        expectEqual(observation.deniedReasons, 0, `${label}：不得出现「被拒绝」的入口呈现`);
        expectEqual(observation.permissionMarkers, 0, `${label}：不得出现权限标识`);
        expectThat(
          observation.effectNote.includes("Agent 进程"),
          `${label}：副作用说明必须呈现（${JSON.stringify(observation.effectNote)}）`,
        );
        // 禁用只由「还没选 Agent」造成：未选时禁用、选中后可用。
        expectEqual(observation.disabledWithoutAgent, true, `${label}：未选 Agent 时提交按钮应禁用`);
        expectEqual(observation.disabledWithAgent, false, `${label}：选中 Agent 后提交按钮必须可点`);
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
    name: "R16-4 目录卡片的创建入口：弹层属于被点的那一项，同一时刻只有一个，提交载荷仍是两个引用",
    async run() {
      const mounted = await mountScenario(
        baseScenario({ route: DIRS_ROUTE, extraRoutes: DIRECTORY_ROUTES }),
      );
      try {
        await waitFor(
          () => queryAll(mounted.container, "[data-directory-alias]").length === 2,
          "目录页渲染出两条目录",
        );
        // **每个**目录项各有一个创建入口：一个都没有 = 入口没接线；只有一个 = 入口被挂成了
        // 页面级按钮而不是卡片上的那一个。入口不再有任何权限维度，因此它恒在、恒可用。
        expectEqual(
          queryAll(mounted.container, '[data-action="create-session"]').length,
          2,
          "每个目录项各有一个「⊕ 新建」入口",
        );

        click(
          query(mounted.container, '[data-directory-alias="alpha"] [data-action="create-session"]'),
          "alpha 的卡片创建入口",
        );
        await waitFor(() => query(mounted.container, "[data-create-sheet]") !== null, "创建弹层打开");
        expectEqual(
          queryAll(mounted.container, "[data-create-sheet]").length,
          1,
          "同一时刻只有一个创建弹层",
        );
        expectEqual(
          query(mounted.container, "[data-create-sheet]")?.getAttribute("data-create-sheet") ?? null,
          "alpha",
          "弹层绑定的是被点的那一项",
        );
        expectThat(
          textOf(query(mounted.container, "[data-create-sheet-title]")).includes("alpha"),
          "弹层标题必须给出被点目录的别名",
        );

        // 弹层开着时点**另一个**目录项的入口：换的是同一个弹层，不是再叠一个。
        click(
          query(mounted.container, '[data-directory-alias="beta"] [data-action="create-session"]'),
          "beta 的卡片创建入口",
        );
        await waitFor(
          () => query(mounted.container, "[data-create-sheet]")?.getAttribute("data-create-sheet") === "beta",
          "弹层切换到 beta",
        );
        expectEqual(
          queryAll(mounted.container, "[data-create-sheet]").length,
          1,
          "切换后仍然只有一个创建弹层",
        );

        // 提交：断言的是实际发出的那一帧字节（与 R16-1 同一手法）。
        expectEqual(
          query(mounted.container, '[data-action="submit-create"]')?.hasAttribute("disabled") ?? false,
          true,
          "未选 Agent 时提交按钮应禁用",
        );
        click(query(mounted.container, 'button[data-agent-id="codex-a"]'), "Agent codex-a");
        await waitFor(
          () => query(mounted.container, 'button[data-agent-id="codex-a"]')?.getAttribute("data-selected") === "true",
          "Agent 被选中",
        );
        click(query(mounted.container, '[data-action="submit-create"]'), "提交创建");
        await waitFor(() => createFrame(mounted.host?.sent ?? []) !== null, "发出 session.create 帧");
        const frame = createFrame(mounted.host?.sent ?? []);
        expectThat(frame !== null, "点击提交后必须真的发出一帧 session.create");
        const payload = createPayloadOf(mounted.host?.sent ?? []);
        expectThat(payload !== null, "session.create 帧里必须带一个对象载荷");
        expectEqual(
          Object.keys(payload).sort(),
          ["agentId", "workspaceAlias"],
          "目录卡片发起的创建，载荷仍恰好是两个引用",
        );
        // 目录引用是**被点的那一项**，不是页面第一次渲染时的那一个。
        expectEqual(payload["workspaceAlias"], "beta", "载荷里的目录引用");
        expectEqual(payload["agentId"], "codex-a", "载荷里的 Agent 引用");
        expectNone(frame ?? "", FORBIDDEN_IN_PAYLOAD, "session.create 帧里出现了越界字段名");
        expectAbsent(frame ?? "", "D:", "session.create 帧里出现了本机盘符路径");

        // 进入详情页时弹层复位：上一条弹层不会被带到下一屏（同一时刻因此永远只有一个）。
        click(
          query(mounted.container, '[data-directory-alias="beta"] [data-action="open-directory"]'),
          "beta 的卡片标题入口",
        );
        await waitFor(
          () => query(mounted.container, '[data-route="dir-detail"][data-directory-alias="beta"]') !== null,
          "进入 beta 的详情页",
        );
        await waitFor(
          () => query(mounted.container, "[data-create-sheet]") === null,
          "进入详情页后弹层已复位",
        );
      } finally {
        await mounted.unmount();
      }
    },
  },
];

export const R16_CHECKS = checks;