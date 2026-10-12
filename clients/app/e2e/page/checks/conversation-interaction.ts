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
 * - **只对亲历请求呈现可选项**：同一个页面上四条交互各自落在不同可用性上（亲历可答 / 仅历史读到 /
 *   已被其它设备处理 / 读回的待处理项），断言的是**可点按钮的总数**与每条的归因；把
 *   `witnessed` 判定从「实时来源」放宽成「有可选项」时，历史那条会冒出按钮 → 红。
 * - **切换中与未提供**：模型切换被扣住应答，断言 `data-settings-state="switching"` 期间
 *   **当前值不变**（本地预改会让它变红）、标注里有「下一轮生效」；另有一个没有任何配置项的会话
 *   断言 `unavailable` 且 `[data-model-choice]` 数为 0。
 */

import {
  SESSION_ALPHA,
  configOption,
  historyItem,
  pendingInteraction,
  readPage,
  sessionSummary,
  wireEvent,
} from "../fixtures";
import {
  CheckFailure,
  click,
  domText,
  expectEqual,
  expectNone,
  expectThat,
  query,
  queryAll,
  textOf,
  waitFor,
} from "../support";
import type { Check } from "../support";
import { TURN_ID, commandBody, enterRoute, sessionScenario, withSessionPage } from "./conversation-timeline";

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

const checks: readonly Check[] = [
  {
    name: "CP-4 输入区只收文本、附件显式不受支持；提交与取消在终态之前不是已发送/已空闲",
    async run() {
      await withSessionPage(
        sessionScenario({
          // 扣住这两条命令的应答：没有观测窗口的话，「立刻当成已完成」根本无从证伪。
          holdCommands: ["session.prompt", "session.cancel"],
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
          const attachmentNotice = textOf(query(container, "[data-attachment-notice]"));
          expectThat(
            attachmentNotice.includes("附件") && attachmentNotice.includes("不受支持"),
            `输入区必须用一句说明交代附件不受支持；实际「${attachmentNotice}」`,
          );
          // 进行中的轮次决定输入区是「进行中」这一态（不是「可输入」也不是「已关闭」）。
          expectEqual(
            query(container, "[data-composer]")?.getAttribute("data-composer"),
            "in_progress",
            "有进行中的轮次时输入区必须是进行中",
          );
          expectThat(
            textOf(query(container, "[data-composer-state]")).includes("进行中"),
            "进行中这一态必须有对应的文案",
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

          // 服务端确认「已接受」仍不等于完成；只有终态才算。
          expectEqual(
            mounted.host?.completeHeldCommand("session.prompt", { status: "accepted" }) ?? false,
            true,
            "必须有被扣住的 session.prompt 可供应答",
          );
          await waitFor(() => query(container, '[data-submission="accepted"]') !== null, "接受后呈现已接受");
          expectNone(domText(container), ["已发送", "已排队"], "已接受仍不得写成已发送");
          mounted.host?.completeHeldCommand("session.prompt", { status: "completed" });
          await waitFor(() => query(container, '[data-submission="completed"]') !== null, "终态后呈现已完成");

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
          await waitFor(() => query(container, '[data-cancel="confirmed"]') !== null, "终态后呈现已确认");
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
          sessions: [sessionSummary({ id: SESSION_ALPHA, alias: "alpha", state: "waiting_permission" })],
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
];

export const CONVERSATION_INTERACTION_CHECKS = checks;
