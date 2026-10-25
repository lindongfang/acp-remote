/**
 * 本机会话生命周期的**页面**证据（`core.local-session-lifecycle.v1`，WP5）。
 *
 * 这些用例跑真实 `createComposition` + 真实路由 + 真实 `SyncClient`，假主机只替换传输：
 *
 * - 进入会话时**先**声明 `session.view`、**后**派发 `session.resume`（wire 顺序逐帧断言）；
 * - 恢复中历史可读、发送禁用、草稿保留；成功不自动提交草稿；
 * - A/B 快切后，迟到的 A 恢复不改导航、不续租 A 的 viewer；
 * - 可见阅读每 10 秒续租；隐藏/离页立即声明 `null` 并停止续租；返回可见立即重新声明并激活；
 * - 未协商的旧 peer：不发任何查看/恢复帧，并明确显示「自动激活不可用」。
 *
 * 隐藏状态由 `store.applyLifecycle` 驱动——它正是组合根把平台 `visibilitychange` 桥接进
 * feature 层的入口（`src/composition.ts` 的 lifecycle 订阅）；用例不直接改 DOM 的
 * `visibilityState`（浏览器不允许）。
 */

import {
  AGENTS,
  DIRECTORY_ID_ALPHA,
  SESSION_ALPHA,
  SESSION_BETA,
  WORKSPACES,
  sessionRoute,
  sessionSummary,
} from "../fixtures";
import {
  CheckFailure,
  expectEqual,
  expectThat,
  navigate,
  query,
  textOf,
  waitFor,
} from "../support";
import type { Check, Mounted } from "../support";
import { commandBodies, sessionScenario, withSessionPage } from "./conversation-timeline";
import type { ScenarioOptions } from "../support";

/** 场景：两条本机会话（A 在目录 alpha，B 同目录），协商本机生命周期。 */
function lifecycleScenario(overrides: Partial<ScenarioOptions> = {}): ScenarioOptions {
  return sessionScenario({
    sessions: [
      sessionSummary({ id: SESSION_ALPHA, directoryId: DIRECTORY_ID_ALPHA, state: "idle", localActivity: "active" }),
      sessionSummary({ id: SESSION_BETA, directoryId: DIRECTORY_ID_ALPHA, state: "idle", localActivity: "active" }),
    ],
    workspaces: WORKSPACES,
    agents: AGENTS,
    selectLocalLifecycle: true,
    // A/B 快切需要 B 的路由可达（真实路由装配，而不是改 DOM）。
    extraRoutes: [sessionRoute(SESSION_BETA)],
    ...overrides,
  });
}

/** `session.resume` 的 completed 结果（摘要带本机活跃程度）。 */
function resumeResult(sessionId: string): Record<string, unknown> {
  return {
    sessionId,
    session: sessionSummary({ id: sessionId, directoryId: DIRECTORY_ID_ALPHA, state: "idle", localActivity: "active" }),
  };
}

/** 解析一帧的 `body`；`type` 不符或形状非法返回 `null`（不信任未校验的解析结果）。 */
function frameBodyOf(text: string, type: string): Record<string, unknown> | null {
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch {
    return null;
  }
  if (typeof parsed !== "object" || parsed === null) return null;
  if (!("type" in parsed) || parsed.type !== type) return null;
  if (!("body" in parsed) || typeof parsed.body !== "object" || parsed.body === null) return null;
  return parsed.body as Record<string, unknown>;
}

/** `session.view` 与 `session.resume` 在出站帧里的位置（先声明后恢复的唯一证据）。 */
function assertDeclareBeforeResume(mounted: Mounted): void {
  const sent = mounted.host?.sent ?? [];
  const viewAt = sent.findIndex((text) => frameBodyOf(text, "session.view") !== null);
  const resumeAt = sent.findIndex((text) => frameBodyOf(text, "command")?.["command"] === "session.resume");
  expectThat(viewAt >= 0, "进入会话必须先发出 session.view");
  expectThat(resumeAt >= 0, "进入会话必须派发 session.resume");
  expectThat(viewAt < resumeAt, `wire 顺序必须是先 session.view 后 session.resume（view=${viewAt}, resume=${resumeAt}）`);
  const resume = commandBodies(sent, "session.resume")[0] ?? null;
  expectThat(resume !== null, "必须真的发出 session.resume 命令");
  expectEqual(resume?.["payload"], {}, "session.resume 的 Sync payload 必须是空对象");
}

/** 当前查看声明（最后一条）；没有即判红。 */
function lastView(mounted: Mounted): { readonly sessionId: string | null; readonly viewVersion: number } {
  const frames = mounted.host?.viewFrames() ?? [];
  const last = frames.at(-1);
  if (last === undefined) throw new CheckFailure("没有任何 session.view 声明");
  return last;
}

/** 等一条 `session.view` 声明指向某个会话（或 `null`）。 */
function waitForView(mounted: Mounted, sessionId: string | null, label: string): Promise<void> {
  return waitFor(() => lastView(mounted).sessionId === sessionId, label);
}

const checks: readonly Check[] = [
  {
    name: "LC-1 进入会话先声明查看再恢复：恢复中历史可读、发送禁用、草稿保留，成功不自动提交",
    async run() {
      await withSessionPage(
        lifecycleScenario({ holdCommands: ["session.resume"] }),
        (container) => query(container, '[data-composer="unavailable"]') !== null,
        "恢复中不可提交",
        async (mounted) => {
          assertDeclareBeforeResume(mounted);
          const container = mounted.container;
          // 历史正文仍可读（消息列表已在页面上），发送不可用。
          expectThat(query(container, "[data-conversation-column='timeline']") !== null, "对话页时间线必须渲染");
          expectThat(
            query(container, '[data-composer="unavailable"]') !== null,
            "恢复中发送必须禁用（不能把 active 标签当成可发送）",
          );
          expectThat(
            textOf(query(container, "[data-submit-unavailable]")).includes("正在恢复"),
            "恢复中必须在输入区给出明确原因",
          );
          expectEqual(query(container, "[data-session-activation]"), null, "会话头不再渲染激活提示");

          const store = mounted.composition?.store;
          if (store === null || store === undefined) throw new CheckFailure("用例必须拿到仓库");
          store.setDraft(SESSION_ALPHA, "恢复期间写的草稿");
          expectEqual(store.draftOf(SESSION_ALPHA), "恢复期间写的草稿", "恢复中必须保留草稿");

          expectThat(
            mounted.host?.completeHeldCommand("session.resume", { result: resumeResult(SESSION_ALPHA) }) === true,
            "假主机必须扣住了 session.resume",
          );
          await waitFor(
            () => query(container, '[data-composer="editable"]') !== null,
            "恢复完成且输入可用",
          );
          expectEqual(store.draftOf(SESSION_ALPHA), "恢复期间写的草稿", "恢复成功 MUST NOT 自动提交草稿");
          expectEqual(
            commandBodies(mounted.host?.sent ?? [], "session.prompt").length,
            0,
            "恢复成功不得派发 prompt",
          );
        },
      );
    },
  },
  {
    name: "LC-2 A/B 快切：迟到的 A 恢复不切回 A，也不续租旧 viewer",
    async run() {
      await withSessionPage(
        lifecycleScenario({ holdCommands: ["session.resume"], route: sessionRoute(SESSION_ALPHA) }),
        (container) => query(container, `[data-session-header="${SESSION_ALPHA}"]`) !== null,
        "A 页渲染",
        async (mounted) => {
          const container = mounted.container;
          navigate(sessionRoute(SESSION_BETA).path);
          await waitFor(() => query(container, `[data-session-header="${SESSION_BETA}"]`) !== null, "切到 B 页");
          await waitForView(mounted, SESSION_BETA, "B 的查看声明");

          // A 的恢复此时才回来：不得改变当前页，也不得把 viewer 续租回 A。
          expectThat(
            mounted.host?.completeHeldCommand("session.resume", { result: resumeResult(SESSION_ALPHA) }) === true,
            "假主机必须扣住了 A 的 session.resume",
          );
          await waitFor(
            () => mounted.composition?.store.state.lifecycle.activations.get(SESSION_ALPHA)?.phase === "ready",
            "A 的结果被归并",
          );
          expectThat(
            query(container, `[data-session-header="${SESSION_BETA}"]`) !== null,
            "迟到的 A 恢复不得把页面切回 A",
          );
          expectEqual(lastView(mounted).sessionId, SESSION_BETA, "B 之后不得出现 A 的查看声明");
          expectEqual(
            mounted.composition?.store.state.lifecycle.selectedSessionId,
            SESSION_BETA,
            "仓库的选择必须仍是 B",
          );
        },
      );
    },
  },
  {
    name: "LC-3 只读保活：可见阅读每 10 秒续租；隐藏停租并声明离开，返回可见立即重新声明与激活",
    async run() {
      await withSessionPage(
        lifecycleScenario(),
        (container) => query(container, '[data-session-title]') !== null,
        "首次激活完成",
        async (mounted) => {
          await waitFor(() => mounted.composition?.store.state.lifecycle.activations.get(SESSION_ALPHA)?.phase === "ready", "activation ready");
          const before = mounted.host?.viewFrames().length ?? 0;
          await waitFor(
            () => (mounted.host?.viewFrames().length ?? 0) > before,
            "可见阅读 10 秒后续租",
            15_000,
          );
          const renewed = lastView(mounted);
          expectEqual(renewed.sessionId, SESSION_ALPHA, "续租必须指向当前会话");

          const store = mounted.composition?.store;
          if (store === null || store === undefined) throw new CheckFailure("用例必须拿到仓库");
          const resumeBefore = commandBodies(mounted.host?.sent ?? [], "session.resume").length;
          store.applyLifecycle({ kind: "visibility", state: "hidden" });
          await waitForView(mounted, null, "隐藏后立即声明离开");
          expectThat(
            lastView(mounted).viewVersion > renewed.viewVersion,
            "离页声明必须使用更高版本",
          );
          const hiddenCount = mounted.host?.viewFrames().length ?? 0;
          const { promise: quiet, resolve: quietDone } = Promise.withResolvers<void>();
          setTimeout(quietDone, 1_200);
          await quiet;
          expectEqual(
            mounted.host?.viewFrames().length ?? 0,
            hiddenCount,
            "隐藏后不得继续续租",
          );
          expectEqual(
            commandBodies(mounted.host?.sent ?? [], "session.resume").length,
            resumeBefore,
            "隐藏期间不得重新派发恢复",
          );

          store.applyLifecycle({ kind: "visibility", state: "visible" });
          await waitForView(mounted, SESSION_ALPHA, "返回可见立即重新声明");
          await waitFor(
            () => commandBodies(mounted.host?.sent ?? [], "session.resume").length > resumeBefore,
            "返回可见立即重新激活",
          );
        },
      );
    },
  },
  {
    name: "LC-4 旧 peer：不发未协商的新帧，会话头不显示静态恢复说明",
    async run() {
      await withSessionPage(
        lifecycleScenario({ selectLocalLifecycle: false }),
        (container) => query(container, '[data-composer]') !== null,
        "会话输入区出现",
        async (mounted) => {
          expectEqual(mounted.host?.viewFrames().length ?? 0, 0, "未协商时 MUST NOT 发送 session.view");
          expectEqual(
            commandBodies(mounted.host?.sent ?? [], "session.resume").length,
            0,
            "未协商时 MUST NOT 派发 session.resume",
          );
          expectEqual(query(mounted.container, "[data-session-activation]"), null, "static activation explanation removed");
          // 输入区继续呈现，实际发送限制仍由输入模型表达。
          expectThat(
            query(mounted.container, '[data-composer]') !== null,
            "旧 peer 保留输入区",
          );
        },
      );
    },
  },
];

export const SESSION_LIFECYCLE_CHECKS: readonly Check[] = checks;

/** 本机活跃程度的页面证据：主标签用本机词表，任务态另存机器证据。 */
export const SESSION_ACTIVITY_PAGE_CHECKS: readonly Check[] = [
  {
    name: "LC-5 主状态用本机活跃程度：active 且无任务时允许发送，任务态仍留在 DOM",
    async run() {
      await withSessionPage(
        lifecycleScenario({
          sessions: [
            sessionSummary({
              id: SESSION_ALPHA,
              directoryId: DIRECTORY_ID_ALPHA,
              state: "idle",
              localActivity: "active",
            }),
          ],
        }),
        (container) => query(container, '[data-session-header] [data-session-activity="active"]') !== null,
        "本机活跃标签出现",
        async (mounted) => {
          const pill = query(mounted.container, '[data-session-header] [data-session-activity="active"]');
          expectThat(pill !== null, "活跃会话必须有本机活跃标签");
          expectThat(textOf(pill).includes("活跃"), "主标签必须是「活跃」而不是「运行中」");
          expectEqual(pill?.getAttribute("data-session-state"), "idle", "任务事实必须仍是 idle");
          expectThat(
            query(mounted.container, '[data-composer="editable"]') !== null,
            "活跃且无任务时允许发送",
          );
        },
      );
    },
  },
];
