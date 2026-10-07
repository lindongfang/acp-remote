/**
 * R16「会话创建是受限入口」的浏览器级验证。
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
 * - 载荷键集必须**恰好**是 `{workspaceAlias, agentId}`；
 * - 帧原文里不得出现路径、MCP、凭据类字段名或值；
 * - 无权限与未认证两种拒绝理由必须**不同**，且都不得把按钮悄悄藏起来。
 */

import { createElement } from "react";
import { createRoot } from "react-dom/client";

import { CreateSessionSheet } from "../../../src/components";

import { AGENTS, WORKSPACES_WITH_PATHS, dirRoute, sessionsForBothDirectories } from "../fixtures";
import {
  CANONICAL_ORIGIN,
  click,
  domHtml,
  expectAbsent,
  expectEqual,
  expectNone,
  expectThat,
  mountScenario,
  query,
  textOf,
  waitFor,
} from "../support";
import type { Check, ScenarioOptions } from "../support";

/** 创建弹层只出现在目录详情页；默认给足创建权限。 */
function baseScenario(overrides: Partial<ScenarioOptions> = {}): ScenarioOptions {
  return {
    canonicalOrigin: CANONICAL_ORIGIN,
    workspaces: WORKSPACES_WITH_PATHS,
    sessions: sessionsForBothDirectories(),
    agents: AGENTS,
    scopes: ["session.list", "session.read", "session.create"],
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

        const parsed: unknown = JSON.parse(frame ?? "{}");
        const payload = (parsed as { body: { payload: unknown } }).body.payload;
        expectEqual(
          Object.keys(payload as Record<string, unknown>).sort(),
          ["agentId", "workspaceAlias"],
          "载荷的键集必须恰好是两个引用",
        );
        expectEqual(
          (payload as { workspaceAlias: string }).workspaceAlias,
          "alpha",
          "载荷里的目录引用",
        );
        expectEqual((payload as { agentId: string }).agentId, "codex-a", "载荷里的 Agent 引用");

        // MUST NOT：帧**原文**里不得携带 Owner 路径或 Provider/MCP 凭据。
        expectNone(frame ?? "", FORBIDDEN_IN_PAYLOAD, "session.create 帧里出现了越界字段名");
        expectAbsent(frame ?? "", "D:", "session.create 帧里出现了本机盘符路径");
        expectAbsent(frame ?? "", "secret", "session.create 帧里出现了敏感片段");
        // 对照：弹层本身的渲染也不泄露路径（弹层只有目录别名，没有任何路径来源）。
        expectAbsent(domHtml(mounted.container), "D:\\", "创建弹层渲染泄露了本机路径");
        expectEqual(
          textOf(sheet.querySelector("h2")).includes("alpha"),
          true,
          "创建弹层标题应给出目录别名",
        );
      } finally {
        await mounted.unmount();
      }
    },
  },
  {
    name: "R16-2 无 scope 时创建入口不可用，且页面必须说明原因",
    async run() {
      const denied = await mountScenario(baseScenario({ scopes: ["session.list", "session.read"] }));
      try {
        await waitFor(
          () => query(denied.container, '[data-action="open-create-sheet"]') !== null,
          "目录详情页渲染",
        );
        // 无权限时入口按钮是 `disabled` 的（`DirectoryDetail` 的 `canCreate` 闸门），
        // 因此这里断言的是「禁用 + 说明」，而不是「点得开」。
        expectEqual(
          query(denied.container, '[data-action="open-create-sheet"]')?.hasAttribute("disabled") ?? false,
          true,
          "无 scope 时创建入口必须禁用",
        );
        const note = textOf(query(denied.container, '[data-create-entry-unavailable="true"]'));
        expectThat(note.length > 0, "无权限时必须给出说明（不可用不等于不解释）");
        // 点击禁用按钮不会有任何反应：不得发出命令。
        click(query(denied.container, '[data-action="open-create-sheet"]'), "禁用的创建入口");
        expectThat(createFrame(denied.host?.sent ?? []) === null, "无权限时不得发出 session.create");
      } finally {
        await denied.unmount();
      }
    },
  },
  {
    name: "R16-3 未认证与无 scope 是两种可区分的拒绝，理由不同且都不发命令",
    async run() {
      // 两个模型都由**真实** ClientStore 在浏览器里算出（`store.createEntry`），
      // 再交给真实组件渲染：断的是「从页面点开弹层」这一步，不是判定本身。
      const readEntry = async (scopes: readonly string[] | null): Promise<string> => {
        const mounted = await mountScenario(baseScenario({ scopes }));
        try {
          // 取模型前必须先等**该场景的认证结果真的落地**，否则读到的可能只是「什么都还没发生」：
          // 有 scope 的场景要等 `auth.authenticated` 把 scopes 写进 store；未认证的场景
          // 服务端不会下发认证，因此以「客户端确实签出了设备证明」为握手走完的标志。
          // 旧写法等的是 `[data-route="dir-detail"]`——它同时被占位 `RuntimeUnavailable` 命中，
          // 于是这条等待在首帧占位时就成立，两个场景都可能读到「未认证」，断言随机变红。
          if (scopes === null) {
            await waitFor(
              () => (mounted.host?.deviceProofs.length ?? 0) === 1,
              "客户端签出设备证明（服务端不再下发认证）",
            );
          } else {
            await waitFor(
              () => mounted.composition?.store.state.scopes !== null,
              "认证授予的 scopes 已落地",
            );
          }
          const entry = mounted.composition?.store.createEntry("alpha");
          if (entry === undefined) throw new Error("组合根未装配");
          const host = document.createElement("div");
          document.body.append(host);
          const root = createRoot(host);
          root.render(
            createElement(CreateSessionSheet, {
              model: entry,
              selectedAgentId: null,
              onSelectAgent: () => undefined,
              onSubmit: () => undefined,
              onDismissUncertain: () => undefined,
              onClose: () => undefined,
            }),
          );
          await waitFor(
            () => query(host, "[data-create-sheet]") !== null,
            "创建弹层渲染（直接渲染真实组件）",
          );
          const reason = textOf(query(host, "[data-create-denied-reason]"));
          expectEqual(
            query(host, "[data-create-sheet]")?.getAttribute("data-create-permission") ?? null,
            "denied",
            "权限标识",
          );
          expectEqual(
            query(host, '[data-action="submit-create"]')?.hasAttribute("disabled") ?? false,
            true,
            "拒绝态下提交按钮必须禁用",
          );
          root.unmount();
          host.remove();
          return reason;
        } finally {
          await mounted.unmount();
        }
      };

      const deniedReason = await readEntry(["session.list", "session.read"]);
      const unauthenticatedReason = await readEntry(null);
      expectThat(deniedReason.length > 0, "无 scope 时必须给出拒绝理由");
      expectThat(unauthenticatedReason.length > 0, "未认证时必须给出拒绝理由");
      expectThat(
        deniedReason !== unauthenticatedReason,
        `未认证与无 scope 的拒绝理由必须不同（都是 ${deniedReason}）`,
      );
    },
  },
];

export const R16_CHECKS = checks;