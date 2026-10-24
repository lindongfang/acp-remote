/**
 * 对话页「输入与取消」「授权应答边界」「模型与模式」的浏览器级验证
 * （`specs/pwa-web-client/spec.md` 的三条 Requirement；`plan.md` 的 Main E2E 中段三条）。
 *
 * ## 断言的是页面字节与呈现，不是模型对象
 *
 * 提交与取消的载荷取自 `FakeHost.sent` 里的**帧原文**（`R16` 的口径）：`{content:[{type:"text",…}]}`
 * 的键集、`{turnId}`、`{configId, value}` 与 `expectedVersion` 都只能从真正发出的那一帧读到。
 * 呈现则一律取 `data-*` 锚点（属性型选择器，不依赖 RNW 把 `role` 映射成哪个标签）。
 *
 * ## 判别力（把实现反着写，哪一条会红）
 *
 * - **提交不等于完成**：假主机**扣住**了 `session.prompt` 的应答（`holdCommands`），因此
 *   「点下去立刻显示已完成/已发送」在这里有观测窗口：断言 `data-submission="pending_confirmation"`
 *   且整页文本里没有「已发送」「已排队」。随后先回 `accepted`（仍不是完成）再回 `completed`
 *   （才是完成）——把 `accepted` 直接当完成、或本地写死终态，两条都会红。
 * - **取消以终态为准**：同样扣住应答，断言 `data-cancel="requested"` 且 `available`/`confirmed`
 *   都不出现；本地乐观清空取消态会让第一条变红。
 * - **附件不受支持**：断言 `data-attachments-supported="false"` 与一句**显式**说明同时在；
 *   提交载荷的键集恰好是 `content`——「静默丢弃附件后提交剩余文本」在这里既缺说明、又会多出键。
 * - **进行中可编辑但发送禁用**：活动轮次下输入框可编辑（写好下一条草稿、不排队），发送按钮
 *   `aria-disabled="true"`，有 `data-action="cancel-turn"`；回车不发出 `session.prompt`。
 *   「提交 → 待确认 → 已接受 → 已完成」因此在**没有活动轮次**的场景里验证（两个场景分开），
 *   否则用例只能在禁用的发送按钮上假装提交，测的就不是真实行为。
 * - **只对亲历请求呈现可选项**：同一个页面上四条交互各自落在不同可用性上（亲历可答 / 仅历史读到 /
 *   已被其它设备处理 / 读回的待处理项），断言的是**可点按钮的总数**与每条的归因；把
 *   `witnessed` 判定从「实时来源」放宽成「有可选项」时，历史那条会冒出按钮 → 红。
 * - **切换中与未提供**：模型切换被扣住应答，断言 `data-settings-state="switching"` 期间
 *   **当前值不变**（本地预改会让它变红）、标注里有「下一轮生效」；另有一个没有任何配置项的会话
 *   断言 `unavailable` 且 `[data-model-choice]` 数为 0。
 */

import {
  DIRECTORY_ID_ALPHA,
  DIRECTORY_ID_BETA,
  DIRECTORY_PATH_ALPHA,
  SESSION_ALPHA,
  SESSION_BETA,
  WORKSPACES_WITH_PATHS,
  configOption,
  historyItem,
  pendingInteraction,
  readMessage,
  readPage,
  sessionSummary,
  wireEvent,
} from "../fixtures";
import {
  CheckFailure,
  click,
  domText,
  expectAbsent,
  expectEqual,
  expectNone,
  expectThat,
  query,
  queryAll,
  textOf,
  waitFor,
} from "../support";
import type { Check } from "../support";
import { typeIntoInput } from "./r15-directory";
import {
  MESSAGE_ID,
  TURN_ID,
  commandBodies,
  commandBody,
  enterRoute,
  sessionScenario,
  withSessionPage,
} from "./conversation-timeline";

/** 轮次开始事件（`canCancel` 与「进行中」判定都挂在它上面）。 */
const EVENT_TURN_STARTED = "e0000000-0000-4000-8000-000000000101";
/** 四条交互各自的标识。 */
const PERM_WITNESSED = "e0000000-0000-4000-8000-000000000201";
const PERM_HISTORY = "e0000000-0000-4000-8000-000000000202";
const PERM_ELSEWHERE = "e0000000-0000-4000-8000-000000000203";
const PERM_PENDING_READ = "e0000000-0000-4000-8000-000000000204";
/** 另一台设备（**不是**本机预置的那把身份）——「已被其它设备处理」的判定依据。 */
const OTHER_DEVICE = "e0000000-0000-4000-8000-0000000000ff";

/** 提交的文本（断言载荷时逐字比对）。 */
const PROMPT_TEXT = "请检查部署脚本";

/** 授权请求的可选项（可选项只来自请求自身的下发内容）。 */
const PERMISSION_OPTIONS = [
  { optionId: "allow_once", label: "允许一次", kind: "allow_once" },
  { optionId: "deny", label: "拒绝", kind: "deny" },
];

/** 一条 `permission.requested` 视图（`options` 是亲历请求才有的那份内容）。 */
function permissionRequestedView(interactionId: string, withOptions: boolean): Record<string, unknown> {
  const base: Record<string, unknown> = {
    interactionId,
    turnId: TURN_ID,
    title: "写入工作区",
    description: "Agent 想改动仓库里的文件",
  };
  return withOptions ? { ...base, options: PERMISSION_OPTIONS } : base;
}

/** 模型分节的配置项：`category: "model"` 的那一项即当前模型与可选项的来源。 */
function modelOptions(): readonly Record<string, unknown>[] {
  return [
    configOption({
      id: "model",
      name: "模型",
      currentValue: "gpt-5",
      choices: [
        { value: "gpt-5", name: "GPT-5" },
        { value: "claude", name: "Claude" },
      ],
    }),
  ];
}

/** 输入区里的多行输入框（RNW 的多行 `TextInput` 渲染成 `<textarea>`）。 */
function composerTextArea(container: HTMLElement): HTMLTextAreaElement | null {
  const found = query(container, "[data-composer-input]");
  if (found instanceof HTMLTextAreaElement) return found;
  const nested = found?.querySelector("textarea") ?? null;
  return nested instanceof HTMLTextAreaElement ? nested : null;
}

/**
 * 往输入区的受控多行输入框里「打字」。
 *
 * 与目录页搜索框（`<input>`）同一个机关、同一个理由：React 在节点上装了记着上一次值的 `value`
 * 存取器，直接赋值会让它认为「值没变」而**不**触发 `onChangeText`。这里取 `HTMLTextAreaElement`
 * 原型上的原生 setter 绕开它，事件路径与用户敲键一致（真实 `input` 事件 + 冒泡）。
 */
function typeIntoComposer(container: HTMLElement, value: string): void {
  const node = composerTextArea(container);
  if (node === null) throw new CheckFailure("输入区：找不到多行输入框");
  const own = Object.getOwnPropertyDescriptor(node, "value");
  const native = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value");
  if (native?.set !== undefined && native.set !== own?.set) native.set.call(node, value);
  else if (own?.set !== undefined) own.set.call(node, value);
  node.dispatchEvent(new Event("input", { bubbles: true }));
}

/**
 * 在输入区的多行输入框里按一次回车。
 *
 * 派发的是**真实宿主 `keydown`**：RNW 的 `TextInput` 把 `keydown` 原样交给 `onKeyPress`
 * （`node_modules/react-native-web/dist/exports/TextInput/index.js` 的 `handleKeyDown`），
 * 因此这条路与用户敲键一致（`key`/`shiftKey` 都在、`preventDefault()` 也拦得住换行）。
 * 合成事件不触发浏览器的默认行为（`isTrusted: false`），所以「Shift+回车插入换行」这一侧
 * 由「不拦截」的断言覆盖（草稿原样留着、不产生任何提交），而不是断言换行符真的被插进去。
 */
function pressEnter(container: HTMLElement, options: { readonly shiftKey?: boolean } = {}): void {
  const node = composerTextArea(container);
  if (node === null) throw new CheckFailure("输入区：找不到多行输入框");
  node.dispatchEvent(
    new KeyboardEvent("keydown", {
      key: "Enter",
      shiftKey: options.shiftKey === true,
      bubbles: true,
      cancelable: true,
    }),
  );
}

/** 时间线里消息条目的标识集合（按集合比较，不依赖 DOM 顺序）。 */
function messageIdsOf(container: HTMLElement): readonly string[] {
  return queryAll(container, "[data-message-id]")
    .map((node) => node.getAttribute("data-message-id") ?? "")
    .sort();
}

const checks: readonly Check[] = [
  {
    name: "CP-TURNENDED 本轮取消后撤掉缓存审批，四个选项显示中文",
    async run() {
      const interactionId = "e0000000-0000-4000-8000-000000000308";
      const options = [
        { optionId: "once", label: "Allow once", kind: "allow_once" },
        { optionId: "always", label: "Always allow", kind: "allow_always" },
        { optionId: "reject", label: "Reject", kind: "reject_once" },
        { optionId: "never", label: "Always reject", kind: "reject_always" },
      ];
      await withSessionPage(sessionScenario({ selectPendingInteractions: true,
        events: [wireEvent({ globalSequence: 11, eventId: "e0000000-0000-4000-8000-000000000309", sessionId: SESSION_ALPHA,
          eventType: "permission.requested", view: { interactionId, turnId: TURN_ID, requestVersion: "7", title: "运行命令", options } })],
        readResult: (sessionId) => readPage({ sessionId, pendingInteractions: [{ interactionId, sessionId, kind: "permission",
          requestVersion: "7", expiresAt: null, options, schema: null }] }),
      }), (container) => queryAll(container, '[data-answer-option]').length === 4, "四个审批选项出现", async (mounted) => {
        for (const [id, label] of [["once", "允许一次"], ["always", "始终允许"], ["reject", "拒绝一次"], ["never", "始终拒绝"]]) {
          expectEqual(textOf(query(mounted.container, `[data-answer-option="${id}"]`)), label, "审批选项中文展示");
        }
        mounted.host?.deliverEvent(wireEvent({ globalSequence: 12, eventId: "e0000000-0000-4000-8000-000000000310",
          sessionId: SESSION_ALPHA, eventType: "turn.cancelled", view: { turnId: TURN_ID, state: "cancelled" } }));
        await waitFor(() => queryAll(mounted.container, '[data-answer-option]').length === 0, "取消立即撤掉旧审批");
        expectThat(textOf(mounted.container).includes("本轮已结束"), "明确说明失效原因");
      });
    },
  },
  {
    name: "CP-STALE 服务端核实已失效后撤掉旧审批按钮",
    async run() {
      let effective = true;
      const interactionId = "e0000000-0000-4000-8000-000000000298";
      await withSessionPage(sessionScenario({ selectPendingInteractions: true,
        events: [wireEvent({ globalSequence: 11, eventId: "e0000000-0000-4000-8000-000000000299",
          sessionId: SESSION_ALPHA, eventType: "permission.requested", view: { interactionId, requestVersion: "7", title: "读取文件",
            options: [{ optionId: "allow-once", label: "允许一次", kind: "allow_once" }] } })],
        readResult: (sessionId) => readPage({ sessionId, pendingInteractions: effective ? [{ interactionId, sessionId,
          kind: "permission", requestVersion: "7", expiresAt: null, schema: null,
          options: [{ optionId: "allow-once", label: "允许一次", kind: "allow_once" }] }] : [] }),
      }), (container) => query(container, '[data-answer-option="allow-once"]') !== null, "审批按钮出现", async (mounted) => {
        effective = false;
        // 等首屏读取完成，专门核实当前请求，不能用旧请求历史继续授权。
        await waitFor(() => mounted.composition?.store.refreshInteractions(SESSION_ALPHA).ok === true, "有效请求核实已发出");
        await waitFor(() => query(mounted.container, '[data-answer-option="allow-once"]') === null, "旧审批按钮消失");
        expectThat(textOf(mounted.container).includes("授权当前无效"), "明确显示授权无效");
      });
    },
  },
  {
    name: "CP-APPROVAL 实时工具授权缺少版本时自动核实并显示审批项",
    async run() {
      let requested = false;
      const interactionId = "e0000000-0000-4000-8000-000000000288";
      await withSessionPage(sessionScenario({ selectPendingInteractions: true,
        readResult: (sessionId) => readPage({ sessionId, pendingInteractions: requested ? [{
          interactionId, sessionId, kind: "permission", requestVersion: "7", expiresAt: null,
          options: [{ optionId: "allow-once", label: "允许一次", kind: "allow_once" },
            { optionId: "reject", label: "拒绝", kind: "reject_once" }], schema: null,
        }] : [] }),
      }), (container) => query(container, '[data-composer]') !== null, "会话页面就绪", async (mounted) => {
        requested = true;
        mounted.host?.deliverEvent(wireEvent({ globalSequence: 11,
          eventId: "e0000000-0000-4000-8000-000000000289", sessionId: SESSION_ALPHA,
          eventType: "permission.requested", view: { interactionId, title: "读取文件",
            toolKind: "execute", toolDetail: { input: { command: "pwd", cwd: "/workspace" }, text: ["核对当前命令执行位置"] },
            options: [{ optionId: "allow-once", label: "允许一次", kind: "allow_once" },
              { optionId: "reject", label: "拒绝", kind: "reject_once" }] } }));
        await waitFor(() => query(mounted.container, '[data-answer-option="allow-once"]') !== null,
          "无需手动刷新即可出现审批项");
        expectThat(query(mounted.container, '[data-answer-option="reject"]') !== null, "同时提供拒绝选项");
        expectEqual(textOf(query(mounted.container, '[data-approval-context="true"]')), "pwd", "授权内容区域仅显示完整命令");
        click(query(mounted.container, '[data-answer-option="allow-once"]'), "允许一次");
        await waitFor(() => commandBody(mounted.host?.sent ?? [], "permission.resolve") !== null, "审批命令发出");
        expectEqual(commandBody(mounted.host?.sent ?? [], "permission.resolve")?.["payload"],
          { interactionId, optionId: "allow-once", requestVersion: "7" }, "审批携带已核实版本");
      });
    },
  },
  {
    name: "CP-TODO 输入框上方清单默认折叠、实时更新并在取消后移除",
    async run() {
      const plan = (sequence: number, statuses: readonly string[]) => wireEvent({
        globalSequence: sequence, sessionId: SESSION_ALPHA,
        eventId: `e0000000-0000-4000-8000-${String(sequence).padStart(12, "0")}`,
        eventType: "session.plan.changed",
        view: { entries: statuses.map((status, index) => ({ content: `清单任务 ${index + 1}`, status, priority: "medium" })) },
      });
      await withSessionPage(sessionScenario({ events: [
        wireEvent({ globalSequence: 11, eventId: EVENT_TURN_STARTED,
          sessionId: SESSION_ALPHA, eventType: "turn.started", view: { turnId: TURN_ID, state: "running" } }),
        plan(12, ["in_progress", "pending"]),
      ] }), (container) => query(container, '[data-composer-todo="true"]') !== null, "任务清单出现", async (mounted) => {
        const container = mounted.container;
        const dock = () => query(container, '[data-composer-todo="true"]');
        expectEqual(query(dock()!, '[data-action="toggle-composer-todo"]')?.getAttribute("aria-expanded"), "false", "默认折叠");
        expectEqual(queryAll(dock()!, '[data-todo-status]').length, 0, "折叠不挤占输入区");
        click(query(dock()!, '[data-action="toggle-composer-todo"]'), "展开任务清单");
        await waitFor(() => queryAll(dock()!, '[data-todo-status]').length === 2, "展示两项任务");
        mounted.host?.deliverEvent(plan(13, ["completed", "in_progress"]));
        await waitFor(() => textOf(dock()).includes("1/2"), "完成比例实时更新");
        expectEqual(queryAll(dock()!, '[data-todo-status]').length, 2, "更新保留展开选择");
        mounted.host?.deliverEvent(wireEvent({ globalSequence: 14,
          eventId: "e0000000-0000-4000-8000-000000000014", sessionId: SESSION_ALPHA,
          eventType: "turn.cancelled", view: { turnId: TURN_ID, state: "cancelled" } }));
        await waitFor(() => dock() === null, "取消后移除输入区清单");
        expectThat(query(container, '[data-timeline-card="plan"]') !== null, "历史计划仍保留");
      });
    },
  },
  {
    name: "CP-4 输入区只收文本、附件显式不受支持；进行中可编辑但发送禁用，提交在无可提交轮次时以终态为准",
    async run() {
      // ① 有进行中的轮次：可编辑但发送禁用、只有停止；回车不得发出任何命令。
      await withSessionPage(
        sessionScenario({
          // 扣住取消应答：没有观测窗口的话，「立刻当成已取消」根本无从证伪。
          holdCommands: ["session.cancel"],
          events: [
            wireEvent({
              globalSequence: 11,
              eventId: EVENT_TURN_STARTED,
              sessionId: SESSION_ALPHA,
              eventType: "turn.started",
              view: { turnId: TURN_ID, state: "running" },
            }),
          ],
        }),
        (container) => query(container, 'button[data-action="cancel-turn"]') !== null,
        "进行中的轮次可取消",
        async (mounted) => {
          const container = mounted.container;

          // 附件不受支持是**显式**说明，不是「没提」。
          expectEqual(
            query(container, "[data-attachments-supported]")?.getAttribute("data-attachments-supported"),
            "false",
            "输入区必须显式标明附件不受支持",
          );
          const attachmentButton = query(container, '[data-action="pick-attachment"]');
          expectThat(
            (attachmentButton?.getAttribute("aria-label") ?? "").includes("不可用") &&
              (attachmentButton?.getAttribute("aria-label") ?? "").includes("附件"),
            `输入区必须显式说明附件不可用；实际「${attachmentButton?.getAttribute("aria-label") ?? ""}」`,
          );
          // 进行中的轮次决定输入区是「进行中」这一态（不是「可输入」也不是「已关闭」）。
          expectEqual(
            query(container, "[data-composer]")?.getAttribute("data-composer"),
            "in_progress",
            "有进行中的轮次时输入区必须是进行中",
          );
          expectThat(
            textOf(query(container, "[data-submit-unavailable]")).includes("进行中"),
            "进行中这一态必须有对应的不可提交说明",
          );

          // 进行中：发送入口仍在，但被闸门禁用；回车不派发、不排队（spec「不可提交时回车无副作用」）。
          expectThat(
            query(container, '[data-action="submit-prompt"]') !== null,
            "进行中仍应呈现发送入口（禁用而不是消失）",
          );
          expectEqual(
            query(container, "[data-submit-enabled]")?.getAttribute("data-submit-enabled"),
            "false",
            "进行中发送必须标为不可提交",
          );
          expectEqual(
            query(container, '[data-action="submit-prompt"]')?.getAttribute("aria-disabled"),
            "true",
            "进行中的发送按钮必须真的禁用",
          );
          expectThat(
            query(container, 'button[data-action="cancel-turn"]') !== null,
            "进行中必须呈现停止入口",
          );

          // 执行中仍可编辑下一条草稿（不建立队列）：通过界面真的把字打进去。
          const before = composerTextArea(container);
          expectThat(before !== null, "进行中仍必须有一段多行输入框");
          expectEqual(before.readOnly, false, "进行中的输入框必须可编辑");
          expectEqual(
            query(container, '[data-composer-input="in_progress"]')?.hasAttribute("readonly") ?? false,
            false,
            "进行中的输入框不得带 readonly",
          );
          typeIntoComposer(container, PROMPT_TEXT);
          await waitFor(
            () => mounted.composition?.store.draftOf(SESSION_ALPHA) === PROMPT_TEXT,
            "执行中的输入必须写进草稿",
          );
          pressEnter(container);
          expectEqual(
            commandBodies(mounted.host?.sent ?? [], "session.prompt").length,
            0,
            "进行中按回车 MUST NOT 发出 session.prompt",
          );
          expectThat(
            query(container, "[data-submission]") === null,
            "进行中按回车不得产生提交呈现",
          );
          expectEqual(
            mounted.composition?.store.draftOf(SESSION_ALPHA),
            PROMPT_TEXT,
            "进行中按回车不得改变草稿",
          );

          // 取消：未确认前不是空闲。
          click(query(container, 'button[data-action="cancel-turn"]'), "取消当前轮次");
          await waitFor(() => query(container, '[data-cancel="requested"]') !== null, "取消后等待服务端终态");
          expectEqual(query(container, '[data-cancel="available"]'), null, "未确认前不得回到「当前轮次可取消」");
          expectEqual(query(container, '[data-cancel="confirmed"]'), null, "未确认前不得呈现为已确认");
          expectThat(
            textOf(query(container, '[data-cancel="requested"]')).includes("等待服务端终态"),
            "取消必须说明在等终态",
          );
          expectEqual(
            commandBody(mounted.host?.sent ?? [], "session.cancel")?.["payload"],
            { turnId: TURN_ID },
            "取消必须指向当前进行中的轮次",
          );
          mounted.host?.completeHeldCommand("session.cancel", { status: "completed" });
          await waitFor(
            () => query(container, '[data-cancel="requested"]') === null,
            "终态后不再呈现取消等待",
          );
        },
      );

      // ② 没有进行中的轮次：可以输入、可以提交；未收到终态前不是「已发送」。
      await withSessionPage(
        sessionScenario({
          // 扣住提交应答：没有观测窗口的话，「立刻当成已完成」根本无从证伪。
          holdCommands: ["session.prompt"],
        }),
        (container) =>
          query(container, '[data-composer="editable"]') !== null &&
          composerTextArea(container) !== null,
        "可提交的输入区",
        async (mounted) => {
          const container = mounted.container;
          expectThat(
            query(container, 'button[data-action="submit-prompt"]') !== null,
            "没有进行中的轮次时必须呈现发送入口",
          );

          // 打字：受控输入框 → `onChangeText` → 仓库草稿（文本是唯一通路）。
          typeIntoComposer(container, PROMPT_TEXT);
          await waitFor(
            () => mounted.composition?.store.draftOf(SESSION_ALPHA) === PROMPT_TEXT,
            "草稿进入仓库",
          );
          expectEqual(composerTextArea(container)?.value, PROMPT_TEXT, "受控输入框必须回显草稿");

          // 提交：未收到终态前**不是**「已发送」。
          click(query(container, 'button[data-action="submit-prompt"]'), "发送");
          await waitFor(
            () => query(container, '[data-submission="pending_confirmation"]') !== null,
            "提交后呈现待确认",
          );
          expectNone(domText(container), ["已发送", "已排队"], "未收到终态前不得呈现为已发送/已排队");
          expectThat(
            textOf(query(container, '[data-submission="pending_confirmation"]')).includes("等待终态确认"),
            "待确认的呈现必须说清在等服务端终态",
          );

          // 实际发出的载荷恰好是一段文本：附件在这里没有表达方式，也不可能被静默丢弃后提交。
          const promptBody = commandBody(mounted.host?.sent ?? [], "session.prompt");
          expectThat(promptBody !== null, "点击发送必须真的发出 session.prompt");
          const promptPayload = promptBody?.["payload"];
          const payloadKeys =
            typeof promptPayload === "object" && promptPayload !== null ? Object.keys(promptPayload) : null;
          expectEqual(payloadKeys, ["content"], "session.prompt 载荷的键集必须只有 content");
          expectEqual(
            promptPayload,
            { content: [{ type: "text", text: PROMPT_TEXT }] },
            "session.prompt 载荷必须恰好是那一段文本",
          );

          // 服务端确认「已接受」仍不等于完成：待确认行消失，但页面上不得写成已发送/已排队。
          expectEqual(
            mounted.host?.completeHeldCommand("session.prompt", { status: "accepted" }) ?? false,
            true,
            "必须有被扣住的 session.prompt 可供应答",
          );
          await waitFor(
            () => query(container, '[data-submission="pending_confirmation"]') === null,
            "接受后不再呈现待确认",
          );
          expectNone(domText(container), ["已发送", "已排队"], "已接受仍不得写成已发送");
          mounted.host?.completeHeldCommand("session.prompt", { status: "completed" });
          await waitFor(
            () => query(container, '[data-submission="pending_confirmation"]') === null,
            "终态后不残留待确认",
          );
          expectNone(domText(container), ["已发送", "已排队"], "终态不得写成已发送");
        },
      );
    },
  },
  {
    name: "CP-5 授权应答边界：只有亲历请求可点，历史待处理项不可操作，已由其它设备处理的呈现为已处理",
    async run() {
      await withSessionPage(
        sessionScenario({
          holdCommands: ["permission.resolve"],
          sessions: [sessionSummary({ id: SESSION_ALPHA, directoryId: DIRECTORY_ID_ALPHA, state: "pending" })],
          events: [
            wireEvent({
              globalSequence: 11,
              eventId: "e0000000-0000-4000-8000-000000000301",
              sessionId: SESSION_ALPHA,
              eventType: "permission.requested",
              view: permissionRequestedView(PERM_WITNESSED, true),
            }),
            wireEvent({
              globalSequence: 12,
              eventId: "e0000000-0000-4000-8000-000000000302",
              sessionId: SESSION_ALPHA,
              eventType: "permission.requested",
              view: permissionRequestedView(PERM_ELSEWHERE, true),
            }),
            wireEvent({
              globalSequence: 13,
              eventId: "e0000000-0000-4000-8000-000000000303",
              sessionId: SESSION_ALPHA,
              eventType: "permission.resolved",
              view: { interactionId: PERM_ELSEWHERE, resolution: "拒绝", resolvedByDeviceId: OTHER_DEVICE },
            }),
          ],
          readResult: (sessionId) =>
            readPage({
              sessionId,
              // 只从历史读到的待处理项：**带**可选项也一样不可应答（亲历与否只看来源）。
              history: [
                historyItem({
                  eventId: "e0000000-0000-4000-8000-000000000304",
                  eventType: "permission.requested",
                  sessionSequence: 6,
                  createdAt: "2026-10-01T00:14:00.000Z",
                  view: permissionRequestedView(PERM_HISTORY, true),
                }),
              ],
              pendingInteractions: [
                pendingInteraction({
                  interactionId: PERM_PENDING_READ,
                  sessionId,
                  kind: "permission",
                  createdAt: "2026-10-01T00:15:00.000Z",
                }),
              ],
            }),
        }),
        (container) => query(container, `[data-pending-interaction="${PERM_PENDING_READ}"]`) !== null,
        "读回的待处理项呈现",
        async (mounted) => {
          const container = mounted.container;
          const witnessed = query(container, `[data-interaction-id="${PERM_WITNESSED}"]`);
          expectThat(witnessed !== null, "亲历的授权请求必须呈现卡片");
          expectEqual(
            witnessed?.getAttribute("data-interaction-availability"),
            "answerable",
            "亲历且持有可选项的请求可应答",
          );
          // 可选项只来自这次请求自身的下发内容：按钮的 `optionId` 与文案都与请求逐字一致。
          const options = queryAll(witnessed, "[data-answer-option]");
          expectEqual(
            options.map((option) => option.getAttribute("data-answer-option")).sort(),
            ["allow_once", "deny"],
            "可选项必须来自请求自身的下发内容",
          );
          expectEqual(
            options.map((option) => textOf(option)).sort(),
            ["允许一次", "拒绝"],
            "可选项的文案必须来自请求自身的下发内容",
          );
          // 整页只应有这两个可点按钮：其余三条交互一个都不能有。
          expectEqual(queryAll(container, "[data-answer-option]").length, 2, "整页可点应答按钮的总数");

          // 仅从历史读到的待处理项（时间线上的卡片）：不可操作且说明原因。
          const fromHistory = query(container, `[data-interaction-id="${PERM_HISTORY}"]`);
          expectThat(fromHistory !== null, "历史里的授权请求必须呈现卡片");
          expectEqual(
            fromHistory?.getAttribute("data-interaction-availability"),
            "not_answerable",
            "仅从历史读到的请求不可应答",
          );
          expectThat(
            query(fromHistory, '[data-interaction-unavailable="read_from_history"]') !== null,
            "仅从历史读到的请求必须说明原因",
          );
          expectEqual(
            queryAll(fromHistory, "[data-answer-option]").length,
            0,
            "仅从历史读到的请求不得呈现任何应答按钮",
          );

          // 已被其它设备处理：呈现为已处理并说明不是本设备所为，且不可重复提交。
          const elsewhere = query(container, `[data-interaction-id="${PERM_ELSEWHERE}"]`);
          expectThat(elsewhere !== null, "被处理的授权请求必须呈现卡片");
          expectEqual(
            elsewhere?.getAttribute("data-interaction-availability"),
            "handled_elsewhere",
            "已被其它设备处理的请求必须与「可应答」区分开",
          );
          expectEqual(
            query(elsewhere, '[data-interaction-handled="other-device"]')?.getAttribute("data-resolved-by-device-id"),
            OTHER_DEVICE,
            "必须给出处理设备标识",
          );
          expectThat(
            textOf(query(elsewhere, '[data-interaction-handled="other-device"]')).includes("其它设备"),
            "必须说明不是本设备所为",
          );
          expectEqual(
            queryAll(elsewhere, "[data-answer-option]").length,
            0,
            "已被其它设备处理的请求不得呈现应答按钮",
          );

          // 本设备没有收到过请求内容的待处理项：单独一面呈现，同样不可操作。
          const pending = query(container, `[data-pending-interaction="${PERM_PENDING_READ}"]`);
          expectEqual(
            pending?.getAttribute("data-pending-interaction-availability"),
            "not_answerable",
            "读回的待处理项不可应答",
          );
          expectThat(
            query(pending, '[data-interaction-unavailable="read_from_history"]') !== null,
            "读回的待处理项必须说明原因",
          );
          expectEqual(
            queryAll(pending, "[data-answer-option]").length,
            0,
            "读回的待处理项不得呈现应答按钮",
          );

          // 应答亲历的那一条：载荷来自请求自身的可选项，且呈现以终态为准。
          click(query(witnessed, '[data-answer-option="allow_once"]'), "允许一次");
          await waitFor(
            () => query(container, '[data-interaction-availability="submitting"]') !== null,
            "应答提交后等待终态",
          );
          expectEqual(
            commandBody(mounted.host?.sent ?? [], "permission.resolve")?.["payload"],
            { interactionId: PERM_WITNESSED, optionId: "allow_once" },
            "permission.resolve 的载荷必须取自这次请求自身的可选项",
          );
          expectEqual(
            queryAll(query(container, `[data-interaction-id="${PERM_WITNESSED}"]`), "[data-answer-option]").length,
            0,
            "等待终态期间不得再呈现可点按钮",
          );
          expectEqual(
            mounted.host?.completeHeldCommand("permission.resolve", { status: "completed" }) ?? false,
            true,
            "必须有被扣住的 permission.resolve 可供应答",
          );
          await waitFor(
            () => query(container, '[data-interaction-handled="this-device"]') !== null,
            "终态后呈现已由本设备处理",
          );
        },
      );
    },
  },
  {
    name: "CP-6 模型与模式：切换中、下一轮生效标注、未提供时不展示虚假可选项",
    async run() {
      await withSessionPage(
        sessionScenario({
          holdCommands: ["session.config.set"],
          readResult: (sessionId) =>
            readPage({ sessionId, configOptions: modelOptions(), configVersion: "7" }),
        }),
        (container) => query(container, '[data-model-choice="claude"]') !== null,
        "模型可选项渲染",
        async (mounted) => {
          const container = mounted.container;
          const section = query(container, '[data-settings-section="model"]');
          expectEqual(section?.getAttribute("data-settings-config-id"), "model", "模型分节的配置项 id");
          expectEqual(
            query(section, "[data-settings-current-name]")?.getAttribute("data-settings-current-name"),
            "GPT-5",
            "当前模型名",
          );
          expectEqual(section?.getAttribute("data-settings-state"), "available", "可切换");
          expectEqual(
            query(container, '[data-model-choice="gpt-5"]')?.getAttribute("data-choice-current"),
            "true",
            "当前取值必须在可选项上标注出来",
          );

          // 模式清单在本页从未被请求过（页面不发 `session.mode.list`）：必须如实呈现「尚未加载」，
          // 而不是画出一排点了必然失败的选项。
          const mode = query(container, '[data-settings-section="mode"]');
          expectEqual(mode?.getAttribute("data-settings-state"), "not_loaded", "未取到模式清单时的分节状态");
          expectEqual(
            textOf(query(mode, "[data-settings-state-label]")),
            "尚未加载",
            "未取到模式清单时的呈现文案",
          );
          expectEqual(queryAll(container, "[data-mode-choice]").length, 0, "不得展示没有来源的可选项");

          // 切换：未收到服务端确认前是 switching，且当前值**不**被本地预改。
          click(query(container, '[data-model-choice="claude"]'), "切换到 Claude");
          await waitFor(
            () => query(container, '[data-settings-state="switching"]') !== null,
            "切换中状态",
          );
          expectEqual(
            query(container, '[data-settings-current-name]')?.getAttribute("data-settings-current-name"),
            "GPT-5",
            "未确认前不得把当前值预先改成新值",
          );
          expectThat(
            textOf(query(container, '[data-settings-notice="switching"]')).includes("下一轮生效"),
            "切换中必须标注变更在下一轮生效",
          );
          const switchBody = commandBody(mounted.host?.sent ?? [], "session.config.set");
          expectEqual(
            switchBody?.["payload"],
            { configId: "model", value: "claude" },
            "切换必须写回配置项 id 与选中的取值（两者不可混用）",
          );
          expectEqual(switchBody?.["expectedVersion"], "7", "切换必须带上配置版本，不得编造并发基线");

          // 服务端确认后才把「下一轮生效」写成已切换的结论。
          expectEqual(
            mounted.host?.completeHeldCommand("session.config.set", { status: "completed" }) ?? false,
            true,
            "必须有被扣住的 session.config.set 可供应答",
          );
          await waitFor(
            () => query(container, '[data-settings-notice="available"]') !== null,
            "确认后的标注",
          );
          expectThat(
            textOf(query(container, '[data-settings-notice="available"]')).includes("下一轮生效"),
            "确认后必须标注变更在下一轮生效",
          );
          expectEqual(
            query(container, '[data-settings-state]')?.getAttribute("data-settings-state"),
            "available",
            "确认后回到可切换态",
          );
        },
      );

      // 未提供：该会话的 Agent 一个模型配置项都没有时，模型分节是 unavailable 且没有任何可选项。
      const absent = await enterRoute(sessionScenario({ readResult: (sessionId) => readPage({ sessionId }) }));
      try {
        await waitFor(
          () => query(absent.container, '[data-settings-section="model"]') !== null,
          "设置面板渲染",
        );
        const section = query(absent.container, '[data-settings-section="model"]');
        expectEqual(section?.getAttribute("data-settings-state"), "unavailable", "未提供模型时的分节状态");
        expectEqual(
          queryAll(absent.container, "[data-model-choice]").length,
          0,
          "未提供时不得展示任何可点选的模型",
        );
        expectThat(
          textOf(query(absent.container, '[data-settings-notice="unavailable"]')).includes("未提供"),
          "未提供时必须说明原因",
        );
      } finally {
        await absent.unmount();
      }
    },
  },
  {
    name: "CP-7 左栏：目录上下文与同目录会话，目录内搜索只影响左栏，吸底新建会话打开同一目录的弹层",
    async run() {
      const SIBLING_A = "aaaaaaaa-0000-4000-8000-00000000aa02";
      const SIBLING_B = "aaaaaaaa-0000-4000-8000-00000000aa03";
      await withSessionPage(
        sessionScenario({
          // 两条同尾段目录都带路径（`alpha` 的那条就是左栏目录上下文的来源）。
          workspaces: WORKSPACES_WITH_PATHS,
          sessions: [
            sessionSummary({ id: SESSION_ALPHA, directoryId: DIRECTORY_ID_ALPHA, title: "部署脚本", state: "running" }),
            sessionSummary({ id: SIBLING_A, directoryId: DIRECTORY_ID_ALPHA, title: "登录流程", state: "idle" }),
            sessionSummary({ id: SIBLING_B, directoryId: DIRECTORY_ID_ALPHA, title: "缓存清理", state: "idle" }),
            // 别的目录的那条：左栏**只**列同目录会话，它一次都不该出现。
            sessionSummary({ id: SESSION_BETA, directoryId: DIRECTORY_ID_BETA, title: "别处的会话", state: "idle" }),
          ],
          readResult: (sessionId) =>
            readPage({
              sessionId,
              messages: [readMessage({ messageId: MESSAGE_ID, sessionId, role: "user", text: "看一下部署脚本", createdAt: "2026-10-01T00:09:00.000Z" })],
            }),
        }),
        (container) => query(container, "[data-conversation-sidebar]") !== null,
        "左栏渲染",
        async (mounted) => {
          const container = mounted.container;

          // 目录上下文：展示名 + **逐字**路径 + 进详情页的入口。
          expectEqual(
            query(container, "[data-sidebar-directory]")?.getAttribute("data-sidebar-directory"),
            DIRECTORY_ID_ALPHA,
            "左栏的目录引用",
          );
          expectEqual(textOf(query(container, "[data-sidebar-directory-name]")), "api", "目录展示名取自快照的目录条目");
          expectEqual(
            query(container, "[data-sidebar-directory-path]")?.getAttribute("data-sidebar-directory-path"),
            DIRECTORY_PATH_ALPHA,
            "路径逐字来自服务端下发的引用",
          );
          expectThat(query(container, '[data-action="open-directory-list"]') !== null, "左栏必须给出返回目录列表的入口");
          expectEqual(query(container, '[data-action="open-directory"]'), null, "对话页左栏不重复目录详情入口");
          expectAbsent(domText(container), DIRECTORY_ID_ALPHA, "左栏可见文本里出现了目录标识");
          expectThat(query(container, "[data-sidebar-search]") !== null, "必须有目录内搜索框");

          // 只列同目录会话，且当前会话被标记为选中。
          const rowsBefore = queryAll(container, "[data-sidebar-session]")
            .map((node) => node.getAttribute("data-sidebar-session") ?? "")
            .sort();
          expectEqual(rowsBefore, [SIBLING_A, SIBLING_B, SESSION_ALPHA].sort(), "左栏只列同目录会话");
          expectEqual(
            query(container, `[data-sidebar-session="${SESSION_ALPHA}"]`)?.getAttribute("data-sidebar-session-selected"),
            "true",
            "当前会话在列表里被标记为选中",
          );
          expectEqual(query(container, `[data-sidebar-session="${SESSION_BETA}"]`), null, "别的目录的会话不得出现");
          expectEqual(
            query(container, "[data-sidebar-total]")?.getAttribute("data-sidebar-total"),
            "3",
            "「已呈现 N / M」的分母",
          );
          expectEqual(query(container, '[data-action="load-more-sidebar"]'), null, "三条约一页放得下，不呈现继续加载");

          // 中栏的读取结果先落地再取「搜索前的事实」：`session.read` 的应答是异步的，读早了
          // 会拿到 0 条条目，后面就会把「读取到达」误判成「搜索改了中栏」。
          await waitFor(
            () => query(container, "[data-timeline-entry-count]")?.getAttribute("data-timeline-entry-count") === "1",
            "中栏读取落地",
          );

          // 搜索前的**中栏事实**：搜索只允许改左栏，因此这些值一个都不能变。
          const beforeFacts = {
            entryCount: query(container, "[data-timeline-entry-count]")?.getAttribute("data-timeline-entry-count"),
            messageIds: messageIdsOf(container),
            title: textOf(query(container, "[data-session-title]")),
          };

          typeIntoInput(container, "[data-sidebar-search]", "登录", "目录内搜索");
          await waitFor(
            () => query(container, "[data-sidebar-total]")?.getAttribute("data-sidebar-total") === "1",
            "搜索把左栏收窄到一条",
          );
          expectEqual(
            queryAll(container, "[data-sidebar-session]").map((node) => node.getAttribute("data-sidebar-session")),
            [SIBLING_A],
            "命中的那一条留下",
          );
          // 反例：把搜索词也带进中栏（按关键词过滤时间线）→ 下面三条会红。
          expectEqual(
            query(container, "[data-timeline-entry-count]")?.getAttribute("data-timeline-entry-count"),
            beforeFacts.entryCount,
            "搜索不得改变中栏的时间线条目数",
          );
          expectEqual(messageIdsOf(container), beforeFacts.messageIds, "搜索不得改变中栏的消息集合");
          expectEqual(textOf(query(container, "[data-session-title]")), beforeFacts.title, "搜索不得改变当前会话的标题");

          // 吸底新建会话：入口打开的是**同一目录**的创建弹层（弹层开关是仓库的单份状态）。
          expectThat(
            textOf(query(container, '[data-action="create-session-in-directory"]')).includes("新建会话"),
            "左栏底部必须有新建会话入口",
          );
          click(query(container, '[data-action="create-session-in-directory"]'), "在目录内新建会话");
          await waitFor(
            () => query(container, `[data-create-sheet="${DIRECTORY_ID_ALPHA}"]`) !== null,
            "创建弹层打开",
          );
        },
      );
    },
  },
  {
    name: "CP-8 会话头：Agent 胶囊沿用连接覆盖层（缺覆盖层即「状态未知」）；路由层不再自建连接表达与导航",
    async run() {
      await withSessionPage(
        sessionScenario(),
        (container) => query(container, '[data-agent-chip="agents"]') !== null,
        "会话头渲染",
        async (mounted) => {
          const container = mounted.container;

          // 会话头三项：标题、状态徽标、Agent 名。
          expectThat(textOf(query(container, "[data-session-title]")).length > 0, "会话头必须有标题");
          expectThat(query(container, "[data-session-state]") !== null, "会话头必须有状态徽标");
          expectEqual(textOf(query(container, "[data-session-agent]")), "codex", "会话头必须给出 Agent 名");

          // 胶囊的弹层内容**始终在 DOM**（收起时只是不显示），因此静置状态就能断言。
          expectEqual(
            query(container, "[data-agent-popover]")?.getAttribute("data-agent-popover"),
            "closed",
            "弹层缺省收起",
          );
          const rows = queryAll(container, "[data-agent-row]");
          expectEqual(
            rows.map((row) => row.getAttribute("data-agent-row")).sort(),
            ["codex-a", "codex-b"],
            "名单来自快照的 Agent 目录（同名靠标识区分，不合并）",
          );
          expectEqual(
            rows.map((row) => row.getAttribute("data-agent-connection")),
            ["unknown", "unknown"],
            "覆盖层缺席时状态是未知",
          );
          // 反例：把「缺席」写成「已断开」→ 上面这条与下面这条都会红。
          expectEqual(query(container, '[data-agent-connection="disconnected"]'), null, "状态未知不等于已断开");
          expectThat(
            textOf(query(container, "[data-agent-chip-source]")).includes("连接事件"),
            "必须说明名单来源是连接事件而不是静态配置",
          );

          // 点开：弹层展开（内容本来就在 DOM 里，展开只是把样式换掉）。
          click(query(container, '[data-action="toggle-agent-chip"]'), "展开 Agent 胶囊");
          await waitFor(() => query(container, '[data-agent-popover="open"]') !== null, "弹层展开");
          expectEqual(
            query(container, "[data-agent-chip]")?.getAttribute("data-agent-chip-open"),
            "true",
            "展开状态可从锚点读出",
          );

          // 路由自己那一层：不再自建导航，也不再有第二个连接状态徽标
          // （连接状态只由共享外壳的设备胶囊表达；外壳在路由层之外，这里只需证明路由层没有）。
          const pageLayer = query(container, '[data-route="session"]')?.parentElement ?? null;
          expectThat(pageLayer !== null, '对话页应渲染出带 `data-route="session"` 的容器');
          expectEqual(query(pageLayer, "nav"), null, "路由层不得再自建导航");
          expectEqual(
            query(pageLayer, "[data-connection-state], [data-connection-label]"),
            null,
            "路由层不得再拼第二个连接状态徽标",
          );
        },
      );
    },
  },
  {
    name: "CP-9 回车提交、Shift+回车不提交；不可提交时回车不发任何命令",
    async run() {
      // ① 没有进行中的轮次：回车提交、Shift+回车不提交。
      await withSessionPage(
        sessionScenario(),
        (container) => query(container, '[data-composer="editable"]') !== null,
        "可提交的输入区渲染",
        async (mounted) => {
          const container = mounted.container;
          typeIntoComposer(container, PROMPT_TEXT);
          await waitFor(() => mounted.composition?.store.draftOf(SESSION_ALPHA) === PROMPT_TEXT, "草稿进入仓库");

          // Shift+回车：换行那一支，**不**提交——草稿原样留着，页面上不出现任何提交呈现。
          pressEnter(container, { shiftKey: true });
          expectEqual(composerTextArea(container)?.value, PROMPT_TEXT, "Shift+回车不得清空草稿");
          expectEqual(mounted.composition?.store.draftOf(SESSION_ALPHA), PROMPT_TEXT, "Shift+回车不得提交草稿");
          expectEqual(query(container, "[data-submission]"), null, "Shift+回车不得产生任何提交呈现");

          // 回车：提交当前草稿（载荷与点击发送同一形状）。
          pressEnter(container);
          await waitFor(
            () => commandBodies(mounted.host?.sent ?? [], "session.prompt").length === 1,
            "回车后发出一次 session.prompt",
          );
          expectEqual(
            commandBodies(mounted.host?.sent ?? [], "session.prompt").length,
            1,
            "回车应恰好发出一次 session.prompt",
          );
          expectEqual(
            commandBody(mounted.host?.sent ?? [], "session.prompt")?.["payload"],
            { content: [{ type: "text", text: PROMPT_TEXT }] },
            "回车提交的载荷取自当前草稿",
          );
        },
      );

      // ② 有进行中的轮次：发送禁用但输入仍可编辑；回车不派发、不排队。
      await withSessionPage(
        sessionScenario({
          events: [
            wireEvent({
              globalSequence: 11,
              eventId: EVENT_TURN_STARTED,
              sessionId: SESSION_ALPHA,
              eventType: "turn.started",
              view: { turnId: TURN_ID, state: "running" },
            }),
          ],
        }),
        (container) => query(container, '[data-composer="in_progress"]') !== null,
        "进行中的输入区渲染",
        async (mounted) => {
          const container = mounted.container;
          expectEqual(
            query(container, '[data-action="submit-prompt"]')?.getAttribute("aria-disabled"),
            "true",
            "进行中的发送必须禁用（不是消失）",
          );
          expectThat(
            query(container, 'button[data-action="cancel-turn"]') !== null,
            "进行中必须呈现停止入口",
          );
          expectEqual(composerTextArea(container)?.readOnly, false, "进行中的输入框必须可编辑");

          typeIntoComposer(container, PROMPT_TEXT);
          await waitFor(
            () => mounted.composition?.store.draftOf(SESSION_ALPHA) === PROMPT_TEXT,
            "执行中的输入必须写进草稿",
          );
          pressEnter(container);
          expectEqual(
            commandBodies(mounted.host?.sent ?? [], "session.prompt").length,
            0,
            "进行中回车 MUST NOT 发出 session.prompt",
          );
          expectThat(
            query(container, "[data-submission]") === null,
            "进行中回车不得产生提交呈现",
          );
          expectEqual(
            mounted.composition?.store.draftOf(SESSION_ALPHA),
            PROMPT_TEXT,
            "进行中回车不得改变草稿",
          );
        },
      );

      // 来源离线的会话：回车 MUST NOT 发出任何命令，也不改变输入区的状态说明。
      // 三态下不存在「已关闭」主状态；不可提交只由当前事实（这里来源离线）决定。
      const unavailable = await enterRoute(
        sessionScenario({
          sessions: [
            sessionSummary({
              id: SESSION_ALPHA,
              directoryId: DIRECTORY_ID_ALPHA,
              state: "idle",
              origin: {
                kind: "remote",
                ownerNodeId: "f0000000-0000-4000-8000-0000000000aa",
                exportId: "export-1",
                originEpoch: "f0000000-0000-4000-8000-0000000000bb",
                online: false,
              },
            }),
          ],
        }),
      );
      try {
        await waitFor(
          () => query(unavailable.container, '[data-composer="unavailable"]') !== null,
          "来源离线的输入区",
        );
        const reason = textOf(query(unavailable.container, "[data-submit-unavailable]"));
        pressEnter(unavailable.container);
        expectEqual(
          commandBodies(unavailable.host?.sent ?? [], "session.prompt").length,
          0,
          "不可提交时回车 MUST NOT 发出 session.prompt",
        );
        expectEqual(query(unavailable.container, "[data-submission]"), null, "回车不得产生提交呈现");
        expectEqual(
          textOf(query(unavailable.container, "[data-submit-unavailable]")),
          reason,
          "回车不得改变输入区的状态说明",
        );
      } finally {
        await unavailable.unmount();
      }
    },
  },
  {
    name: "CP-10 跳转胶囊：没有用户输入时不呈现；输入滚出中栏可视带时呈现，跳转后短暂高亮目标",
    async run() {
      // ① 一条用户输入都没有：MUST NOT 呈现跳转入口。
      const withoutInput = await enterRoute(
        sessionScenario({
          readResult: (sessionId) =>
            readPage({
              sessionId,
              messages: [
                readMessage({
                  messageId: MESSAGE_ID,
                  sessionId,
                  role: "agent",
                  text: "先看下这个项目怎么跑起来",
                  createdAt: "2026-10-01T00:09:00.000Z",
                }),
              ],
            }),
        }),
      );
      try {
        await waitFor(
          () => query(withoutInput.container, `[data-message-id="${MESSAGE_ID}"]`) !== null,
          "Agent 消息渲染",
        );
        expectEqual(
          query(withoutInput.container, "[data-jump-pill]"),
          null,
          "没有用户输入时 MUST NOT 呈现跳转入口",
        );
      } finally {
        await withoutInput.unmount();
      }

      // ② 一条用户输入 + 后面一长串 Agent 消息：滚到底后用户输入在视口外 → 呈现向上跳转入口。
      const longText = Array.from({ length: 24 }, (_, line) => `第 ${String(line)} 行：这一段用来把时间线撑长。`).join("\n");
      const messages = [
        readMessage({
          messageId: MESSAGE_ID,
          sessionId: SESSION_ALPHA,
          role: "user",
          text: "先看下这个项目怎么跑起来",
          createdAt: "2026-10-01T00:09:00.000Z",
        }),
        ...Array.from({ length: 12 }, (_, index) =>
          readMessage({
            messageId: `f0000000-0000-4000-8000-00000000${String(index + 10).padStart(4, "0")}`,
            sessionId: SESSION_ALPHA,
            role: "agent",
            text: longText,
            createdAt: `2026-10-01T00:${String(10 + index).padStart(2, "0")}:00.000Z`,
          }),
        ),
      ];
      const scrolled = await enterRoute(sessionScenario({ readResult: (sessionId) => readPage({ sessionId, messages }) }));
      try {
        const lastMessageId = `f0000000-0000-4000-8000-00000000${String(21).padStart(4, "0")}`;
        await waitFor(
          () => query(scrolled.container, `[data-message-id="${lastMessageId}"]`) !== null,
          "最后一条消息渲染（读取已完成）",
        );

        // 对话页的滚动者是中栏自己的容器（三栏独立滚动），不是 `window`：把中栏滚到底，
        // 用户输入才离开它的可视带。
        const scroller = query(scrolled.container, '[data-conversation-scroll="timeline"]');
        expectThat(scroller instanceof HTMLElement, "中栏必须有独立滚动容器");
        await waitFor(
          () => scroller.scrollHeight > scroller.clientHeight,
          `中栏时间线必须超过可视高度（scrollHeight=${String(scroller.scrollHeight)}, clientHeight=${String(scroller.clientHeight)}）`,
        );
        scroller.scrollTop = scroller.scrollHeight;
        scroller.dispatchEvent(new Event("scroll"));
        await waitFor(
          () => query(scrolled.container, '[data-jump-pill="up"]') !== null,
          "用户输入滚出可视带后呈现向上跳转入口",
        );
        expectEqual(
          query(scrolled.container, '[data-jump-pill="up"]')?.getAttribute("data-jump-remaining"),
          "1",
          "向上还有一条输入",
        );

        click(query(scrolled.container, '[data-jump-pill="up"]'), "跳到你最近的输入");
        await waitFor(
          () => query(scrolled.container, `[data-jump-flash="${MESSAGE_ID}"]`) !== null,
          "跳转后短暂高亮目标",
        );
      } finally {
        // 滚动位置属于该场景自己的中栏容器，随卸载一起消失，不会留给后面的用例。
        await scrolled.unmount();
      }
    },
  },
];

export const CONVERSATION_INTERACTION_CHECKS = checks;
