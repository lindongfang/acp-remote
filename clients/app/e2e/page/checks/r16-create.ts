/**
 * R16：Web 新建会话直接调用 `session.create`。
 *
 * 浏览器用例只替换传输：真实组件调用 `ClientStore.submitCreateSession`，真实命令编码器
 * 发出帧，真实同步客户端解码完成结果，布局消费一次性导航目标。这里逐层核对实际 wire
 * 载荷、重复点击抑制、accepted 不导航，以及 completed 到达后的 active 首帧与查看顺序。
 */

import {
  AGENTS,
  DIRECTORY_ID_ALPHA,
  WORKSPACES_WITH_PATHS,
  dirRoute,
  readPage,
  sessionRoute,
  sessionSummary,
} from "../fixtures";
import {
  CANONICAL_ORIGIN,
  CheckFailure,
  click,
  expectEqual,
  expectThat,
  mountScenario,
  query,
  waitFor,
} from "../support";
import type { Check, ScenarioOptions } from "../support";
import { currentPath } from "../router-stub";
import { commandBodies } from "./conversation-timeline";

const CREATED_SESSION_ID = "eeeeeeee-0000-4000-8000-0000000000a1";

function bodyOfType(sent: readonly string[], type: string): Record<string, unknown> | null {
  for (const text of sent) {
    const parsed: unknown = JSON.parse(text);
    if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) continue;
    const frame = parsed as Record<string, unknown>;
    if (frame["type"] !== type) continue;
    const body = frame["body"];
    if (typeof body === "object" && body !== null && !Array.isArray(body)) {
      return body as Record<string, unknown>;
    }
  }
  return null;
}

function createResult(): Record<string, unknown> {
  return {
    sessionId: CREATED_SESSION_ID,
    session: sessionSummary({
      id: CREATED_SESSION_ID,
      directoryId: DIRECTORY_ID_ALPHA,
      state: "idle",
      localActivity: "active",
      title: "新会话",
    }),
  };
}

function scenario(): ScenarioOptions {
  return {
    canonicalOrigin: CANONICAL_ORIGIN,
    workspaces: WORKSPACES_WITH_PATHS,
    sessions: [],
    agents: AGENTS,
    authenticate: true,
    tamperHostProof: false,
    deliverSnapshot: true,
    barrierGlobalSequence: "10",
    events: [],
    selectLocalLifecycle: true,
    holdCommands: ["session.create"],
    createResult,
    readResult: (sessionId) => readPage({ sessionId }),
    route: dirRoute(DIRECTORY_ID_ALPHA),
    extraRoutes: [sessionRoute(CREATED_SESSION_ID)],
  };
}

const checks: readonly Check[] = [
  {
    name: "R16 直接创建：稳定 requestId、accepted 不导航、completed 后才进入 active 会话",
    async run() {
      const mounted = await mountScenario(scenario());
      const container = mounted.container;
      const sent = (): readonly string[] => mounted.host?.sent ?? [];
      try {
        await waitFor(
          () => query(container, '[data-action="open-create-sheet"]') !== null,
          "目录详情页渲染",
        );
        click(query(container, '[data-action="open-create-sheet"]'), "新建会话入口");
        await waitFor(() => query(container, "[data-create-sheet]") !== null, "创建弹层打开");
        click(query(container, 'button[data-agent-id="codex-a"]'), "选择 Agent");
        await waitFor(
          () => query(container, '[data-action="submit-create"]')?.getAttribute("aria-disabled") !== "true",
          "允许提交",
        );

        const submit = query(container, '[data-action="submit-create"]');
        click(submit, "创建会话");
        click(submit, "重复点击创建会话");
        await waitFor(() => commandBodies(sent(), "session.create").length === 1, "仅一条 session.create");

        const create = commandBodies(sent(), "session.create")[0];
        if (create === undefined) throw new CheckFailure("没有捕获 session.create 命令帧");
        const payloadValue = create["payload"];
        if (typeof payloadValue !== "object" || payloadValue === null || Array.isArray(payloadValue)) {
          throw new CheckFailure("session.create payload 不是对象");
        }
        const payload = payloadValue as Record<string, unknown>;
        expectEqual(
          Object.keys(payload).sort(),
          ["agentId", "directoryId"],
          "直接创建载荷只含 Agent 与已登记目录引用",
        );
        expectEqual(payload["agentId"], "codex-a", "Agent 引用逐字来自已配置目录");
        expectEqual(payload["directoryId"], DIRECTORY_ID_ALPHA, "目录引用逐字来自路由");

        const requestId = create["requestId"];
        expectThat(typeof requestId === "string", "创建请求必须带 requestId");
        expectThat(
          mounted.host?.completeHeldCommand("session.create", { status: "accepted", result: {} }) === true,
          "假主机应扣住创建命令",
        );
        await waitFor(
          () => query(container, '[data-create-submission="accepted"]') !== null,
          "accepted 如实呈现",
        );
        expectEqual(currentPath(), dirRoute(DIRECTORY_ID_ALPHA).path, "accepted 不得提前导航");
        expectEqual(
          mounted.composition?.store.state.createRequestIds.get(DIRECTORY_ID_ALPHA),
          requestId,
          "accepted 后仍保留原 requestId",
        );
        expectEqual(commandBodies(sent(), "session.create").length, 1, "accepted 后不得重发或换号");

        expectThat(
          mounted.host?.completeHeldCommand("session.create", {
            status: "completed",
            result: createResult(),
          }) === true,
          "同一 requestId 应能收到 completed",
        );
        await waitFor(
          () => currentPath() === sessionRoute(CREATED_SESSION_ID).path,
          "只有匹配的 completed 才导航",
        );
        await waitFor(
          () => query(container, `[data-session-header="${CREATED_SESSION_ID}"] [data-session-activity="active"]`) !== null,
          "新会话首帧直接呈现 active",
        );
        await waitFor(() => bodyOfType(sent(), "session.view") !== null, "声明新会话可见");
        await waitFor(() => commandBodies(sent(), "session.resume").length === 1, "派发 session.resume");

        const viewIndex = sent().findIndex((text) => {
          const parsed: unknown = JSON.parse(text);
          return typeof parsed === "object" && parsed !== null &&
            (parsed as Record<string, unknown>)["type"] === "session.view";
        });
        const resumeIndex = sent().findIndex((text) => commandBodies([text], "session.resume").length === 1);
        expectThat(viewIndex >= 0 && resumeIndex >= 0 && viewIndex < resumeIndex, "先声明 view 再 resume");
        expectEqual(
          bodyOfType(sent(), "session.view")?.["sessionId"],
          CREATED_SESSION_ID,
          "view 指向完成结果里的新会话",
        );
      } finally {
        await mounted.unmount();
      }
    },
  },
];

export const R16_CHECKS: readonly Check[] = checks;
