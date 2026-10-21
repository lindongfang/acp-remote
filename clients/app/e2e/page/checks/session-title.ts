/**
 * 顶部标题双击编辑的页面级验证（PWA spec「对话页顶部标题支持编辑」）。
 *
 * ## 判别力
 *
 * - **只双击进入**：单次点击后仍然是普通文本；第二次点击（时间窗内）才出现输入框，
 *   页面里没有任何编辑/恢复 Agent 标题的按钮；
 * - **一次提交一个 requestId**：Enter 后恰好一条 `session.title.set`，`payload` 是输入值的
 *   trim 结果、body 上没有 `expectedVersion`；
 * - **终态不等于完成**：命令 `completed` 之后、权威 `session.updated` 落地之前，输入框仍在
 *   （提交中）；摘要更新后才退出编辑，且会话头与左栏列表显示同一个值；
 * - 双击判定用点击时间窗（不是浏览器 `dblclick`），因此这里也是 PWA 的真实路径。
 */

import { DIRECTORY_ID_ALPHA, SESSION_ALPHA, sessionSummary } from "../fixtures";
import { click, expectEqual, expectThat, query, queryAll, textOf, waitFor } from "../support";
import type { Check } from "../support";
import { commandBody, sessionScenario, withSessionPage } from "./conversation-timeline";

const ORIGINAL_TITLE = "Agent 原标题";
const SAVED_TITLE = "自定义标题";

/** 标题输入框：RNW 可能把 `dataSet` 放在外层节点上，真实 `<input>` 在其内部。 */
function titleInput(container: HTMLElement): HTMLInputElement | null {
  const found = query(container, "[data-session-title-input]");
  if (found instanceof HTMLInputElement) return found;
  const nested = found?.querySelector("input") ?? null;
  return nested instanceof HTMLInputElement ? nested : null;
}

/**
 * 往受控标题输入框里打字。
 *
 * React 在节点上装了记着上一次值的 `value` 存取器，直接赋值不会触发 `onChangeText`；这里用
 * 原型上的原生 setter 并派发真实 `input` 事件（与目录页/输入区的既有机关同一理由）。
 */
function typeIntoTitledInput(node: HTMLInputElement, value: string): void {
  const own = Object.getOwnPropertyDescriptor(node, "value");
  const native = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value");
  if (native?.set !== undefined && native.set !== own?.set) native.set.call(node, value);
  else if (own?.set !== undefined) own.set.call(node, value);
  node.dispatchEvent(new Event("input", { bubbles: true }));
}

/** 一条 `session.updated`：标题与版本都前进一格（版本比读取摘要新）。 */
function sessionUpdatedEvent(): Record<string, unknown> {
  const summary = {
    ...sessionSummary({
      id: SESSION_ALPHA,
      directoryId: DIRECTORY_ID_ALPHA,
      state: "running",
      title: SAVED_TITLE,
      updatedAt: "2026-10-01T00:00:20.000Z",
    }),
    version: "2",
  };
  return {
    protocolVersion: 1,
    type: "event",
    messageId: "aaaaaaaa-1111-4111-8111-0000000000f1",
    connectionId: "3c9f1b6e-6a41-4a2e-9a1c-2f0a5b7d9e13",
    connectionSequence: "1",
    body: {
      globalSequence: "21",
      sessionSequence: null,
      eventId: "dddddddd-0000-4000-8000-0000000000f1",
      sessionId: SESSION_ALPHA,
      eventType: "session.updated",
      causationRequestId: null,
      origin: { kind: "agent", deviceId: null },
      remoteOrigin: null,
      createdAt: "2026-10-01T00:00:20.000Z",
      payload: { view: { session: summary } },
    },
  };
}

const checks: readonly Check[] = [
  {
    name: "ST-1 双击标题进入原地编辑；Enter 只发一条 session.title.set，终态与权威摘要后才退出",
    async run() {
      await withSessionPage(
        sessionScenario({
          sessions: [
            sessionSummary({
              id: SESSION_ALPHA,
              directoryId: DIRECTORY_ID_ALPHA,
              state: "running",
              title: ORIGINAL_TITLE,
            }),
          ],
          // 扣住标题命令：先观察「已派发但未完成」，再由用例给终态与权威摘要。
          holdCommands: ["session.title.set"],
        }),
        (container) => query(container, `[data-session-header="${SESSION_ALPHA}"]`) !== null,
        "会话头渲染",
        async (mounted) => {
          const container = mounted.container;
          const host = mounted.host;
          expectThat(host !== null, "标题用例需要假主机");

          // 没有额外编辑入口；标题是唯一可双击的动作。
          expectThat(query(container, '[data-action="edit-title"]') === null, "不得新增编辑按钮");
          expectThat(query(container, '[data-action="restore-title"]') === null, "不得新增恢复 Agent 标题入口");

          const entry = query(container, '[data-session-title-entry="double-click"]');
          click(entry, "标题双击入口（第一次）");
          // 单次点击不是编辑：仍没有输入框。
          expectThat(query(container, "[data-session-title-input]") === null, "单次点击不得进入编辑");
          click(query(container, '[data-session-title-entry="double-click"]'), "标题双击入口（第二次）");
          await waitFor(() => titleInput(container) !== null, "标题输入框出现");

          const input = titleInput(container);
          if (input === null) throw new Error("标题输入框不是 HTMLInputElement");
          expectEqual(input.value, "", "编辑框不预填真实输入");
          expectEqual(input.placeholder, ORIGINAL_TITLE, "旧标题作为灰色占位");

          // 不输入直接 Enter：使用旧标题，不派发命令并退出编辑。
          input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }));
          await waitFor(() => titleInput(container) === null, "空输入确认后退出编辑");
          expectThat(
            commandBody(host.sent, "session.title.set") === null,
            "空输入不得派发 session.title.set",
          );

          // 重新双击进入，输入新标题。
          click(query(container, '[data-session-title-entry="double-click"]'), "标题双击入口（第一次）");
          click(query(container, '[data-session-title-entry="double-click"]'), "标题双击入口（第二次）");
          await waitFor(() => titleInput(container) !== null, "再次进入标题输入框");
          const inputForEdit = titleInput(container);
          if (inputForEdit === null) throw new Error("标题输入框不是 HTMLInputElement");
          typeIntoTitledInput(inputForEdit, `  ${SAVED_TITLE}  `);
          const submitInput = titleInput(container);
          if (submitInput === null) throw new Error("标题输入框在输入后消失");
          submitInput.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }));

          await waitFor(
            () => commandBody(host.sent, "session.title.set") !== null,
            "发出 session.title.set",
          );
          const body = commandBody(host.sent, "session.title.set");
          expectThat(body !== null, "session.title.set 必须已发出");
          expectEqual(body["sessionId"], SESSION_ALPHA, "命令绑定原会话");
          expectEqual(body["payload"], { title: SAVED_TITLE }, "payload 是 trim 后的 title");
          expectThat(!("expectedVersion" in body), "标题命令的 body 上不得出现 expectedVersion");
          expectEqual(
            (host.sent.join("").match(/"command":"session\.title\.set"/gu) ?? []).length,
            1,
            "一次提交只发一条标题命令",
          );

          // 命令已派发、终态未到：输入框仍在，不能显示完成。
          await waitFor(() => query(container, "[data-session-title-input]") !== null, "提交中保留输入框");
          expectThat(host.completeHeldCommand("session.title.set", { status: "completed" }), "假主机应有一条被扣住的标题命令");
          // 假主机经微任务投递：等它被归并后再断言「仍未退出」。
          const settled = Promise.withResolvers<void>();
          setTimeout(settled.resolve, 50);
          await settled.promise;

          // completed 但没有权威摘要：仍停留在提交中（这是「终态 + 摘要」两段确认的第一段）。
          expectThat(
            query(container, "[data-session-title-input]") !== null,
            "只有 completed、没有权威摘要时不得退出编辑",
          );

          host.deliverEvent(sessionUpdatedEvent());
          await waitFor(() => query(container, "[data-session-title-input]") === null, "摘要落地后退出编辑");

          const header = query(container, `[data-session-header="${SESSION_ALPHA}"] [data-session-title="true"]`);
          expectEqual(textOf(header), SAVED_TITLE, "会话头显示有效标题");
          const sidebarRow = query(
            container,
            `[data-sidebar-session="${SESSION_ALPHA}"] [data-session-row-title="true"]`,
          );
          expectEqual(textOf(sidebarRow), SAVED_TITLE, "左栏会话行显示同一个有效标题");

          // 没有编辑/恢复入口（第二次断言，防止实现用「完成态」把它加回来）。
          expectEqual(queryAll(container, '[data-action="edit-title"], [data-action="restore-title"]').length, 0, "无额外标题入口");
        },
      );
    },
  },
];

export const SESSION_TITLE_CHECKS = checks;
