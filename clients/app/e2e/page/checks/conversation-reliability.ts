/**
 * 对话可靠性（`conversation-reliability` 任务 5.1）的**浏览器级**回归断言。
 *
 * ## 为什么必须在页面层验
 *
 * 这一组承诺的用户行为只能从「真实组件树 + 真实 socket 帧 + 真实仓库归并」观察：
 *
 * - 旧历史里的“继续”不能抵消本次发送的“继续”（同文计数合并）；
 * - 正式正文必须按 `turnId` **原位**接替本地条目，而不是追加一条；
 * - 附件上传中与上传失败必须如实说明，且失败时**不派发** `session.prompt`；
 * - 受理后无输出 → 工具 → 回复的互斥过程行；
 * - 复制标签页不接管原准备；新会话不显示临时状态或放弃入口。
 *
 * 假主机（`../fake-host`）只替换传输与 Agent 边界；wire 解码、仓库归并、路由与组件都是生产代码。
 * 附件检查里假主机按最小协议应答 `attachment.*`（第二条已认证连接），因此「上传中/失败」两条
 * 断言不需要伪造 feature。
 *
 * ## 不在本文件的观测面
 *
 * 真实 Daemon + 真实 Chromium + 真实 Codex 的验收属于任务 5.2；本文件只跑假主机。
 * 后端的持久化/重启/并发证据见 `crates/app/tests/conversation_reliability_e2e.rs`。
 */

import { openPlatform, openPreparationDraftStore } from "../../../src/platform";
import type { PlatformPorts, PreparationDraftPort } from "../../../src/platform";
import type { PreparationReceipt } from "../../../src/protocol";
import { createFakeHostIdentity } from "../fake-host";
import {
  AGENTS,
  DIRECTORY_ID_ALPHA,
  DIRS_ROUTE,
  PREPARED_SESSION_ID,
  PREPARATION_CONFIG,
  PREPARATION_MODES,
  PREPARATION_LEASE_EXPIRES_AT,
  PREPARATION_RUNTIME_EPOCH,
  SESSION_ALPHA,
  WORKSPACES,
  preparationReceipt,
  readMessage,
  readPage,
  sessionRoute,
  sessionSummary,
  wireEvent,
} from "../fixtures";
import {
  CANONICAL_ORIGIN,
  CheckFailure,
  DEVICE_ID,
  HOST_ID,
  click,
  domText,
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
import { commandBody, commandBodies } from "./conversation-timeline";
import { currentPath, navigate } from "../router-stub";

/** 历史里那条“继续”的消息身份（它与本次发送**没有**任何身份关联）。 */
const HISTORY_MESSAGE_ID = "aaaaaaaa-0000-4000-8000-0000000000c1";
/** 本次接受的轮次（`session.prompt` 的 `accepted { turnId }`）。 */
const LOCAL_TURN_ID = "aaaaaaaa-0000-4000-8000-0000000000c2";
/** 正式用户消息身份（服务端在 `user.message.delta` 里给出）。 */
const LOCAL_MESSAGE_ID = "aaaaaaaa-0000-4000-8000-0000000000c3";
/** 工具调用身份（过程行「工具」阶段的真实内容）。 */
const TOOL_CALL_ID = "aaaaaaaa-0000-4000-8000-0000000000c4";
/** 正式回复消息身份。 */
const AGENT_MESSAGE_ID = "aaaaaaaa-0000-4000-8000-0000000000c5";
/** 本轮正式思考消息身份（权威序号 10）。 */
const THOUGHT_MESSAGE_ID = "aaaaaaaa-0000-4000-8000-0000000000c6";
/** 复制标签页用例专用的原准备（避免与其它用例的浏览器草稿/定位互相干扰）。 */
const COPIED_SESSION_ID = "eeeeeeee-0000-4000-8000-00000000c0de";

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
 * 与 `conversation-interaction.ts` 同一个机关：绕开 React 记着旧值的 `value` 存取器，
 * 再派发真实 `input` 事件（事件路径与用户敲键一致）。
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

async function settle(ms = 120): Promise<void> {
  const { promise, resolve } = Promise.withResolvers<void>();
  setTimeout(resolve, ms);
  await promise;
}

/**
 * 等一个绘制帧。
 *
 * 用来断言「片段到达后的**首帧**已经呈现」：只等一帧、不投递后续片段、不安排定时器。若实现改成
 * 定时积攒或下一帧批量提交正文，首帧里就还没有这段正文。
 */
function nextFrame(): Promise<void> {
  return new Promise((resolve) => {
    if (typeof requestAnimationFrame !== "function") {
      resolve();
      return;
    }
    requestAnimationFrame(() => {
      resolve();
    });
  });
}

/** 出站帧的解析结果（`type` 与 `body`）。 */
function parseFrame(text: string): { readonly type: unknown; readonly body: Record<string, unknown> | null } | null {
  const parsed: unknown = JSON.parse(text);
  if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) return null;
  const record = parsed as Record<string, unknown>;
  const body = record["body"];
  return {
    type: record["type"],
    body: typeof body === "object" && body !== null && !Array.isArray(body)
      ? (body as Record<string, unknown>)
      : null,
  };
}

/** 出站 `session.preparation` 的 body（按发送顺序）。 */
function preparationBodies(sent: readonly string[]): readonly Record<string, unknown>[] {
  const bodies: Record<string, unknown>[] = [];
  for (const text of sent) {
    const frame = parseFrame(text);
    if (frame?.type === "session.preparation" && frame.body !== null) bodies.push(frame.body);
  }
  return bodies;
}

/** 用户消息条目（`data-message-channel="user"`），按 DOM 顺序。 */
function userBubbles(container: HTMLElement): readonly Element[] {
  return queryAll(container, '[data-timeline-entry="message"][data-message-channel="user"]');
}

/** 一个 `ready` 应答（准备控制帧）。 */
function readyPreparationResult(
  body: Record<string, unknown>,
  receipt: PreparationReceipt,
): Record<string, unknown> {
  return {
    operationId: body["operationId"],
    action: body["action"],
    sessionId: typeof body["sessionId"] === "string" ? body["sessionId"] : PREPARED_SESSION_ID,
    status: "ready",
    receipt,
    runtimeEpoch: PREPARATION_RUNTIME_EPOCH,
    generation: "1",
    modes: PREPARATION_MODES,
    configOptions: PREPARATION_CONFIG,
    leaseExpiresAt: PREPARATION_LEASE_EXPIRES_AT,
  };
}

function receiptFor(sessionId: string): PreparationReceipt {
  return preparationReceipt({
    ownerNodeId: HOST_ID,
    origin: CANONICAL_ORIGIN,
    deviceId: DEVICE_ID,
    sessionId,
  });
}

/** `session.read` 的应答：能力声明里三种附件都支持（附件上传用例的前置）。 */
function attachmentReadResult(sessionId: string): Record<string, unknown> {
  const page = readPage({ sessionId });
  const resources = (page["resources"] as Record<string, unknown> | undefined) ?? {};
  return {
    ...page,
    resources: {
      ...resources,
      capabilities: [
        {
          sessionId,
          agentCapabilities: {
            promptCapabilities: { image: true, audio: true, embeddedContext: true },
          },
        },
      ],
    },
  };
}

/** 会话场景的公共起点（真实快照 + 认证 + 会话页路由）。 */
function reliabilityScenario(overrides: Partial<ScenarioOptions> = {}): ScenarioOptions {
  return {
    canonicalOrigin: CANONICAL_ORIGIN,
    workspaces: WORKSPACES,
    sessions: [sessionSummary({ id: SESSION_ALPHA, directoryId: DIRECTORY_ID_ALPHA, state: "running" })],
    agents: AGENTS,
    authenticate: true,
    tamperHostProof: false,
    deliverSnapshot: true,
    barrierGlobalSequence: "10",
    events: [],
    readResult: (sessionId) => readPage({ sessionId }),
    route: sessionRoute(SESSION_ALPHA),
    ...overrides,
  };
}

/** 进入会话页（先经过目录页，等认证与快照真正落地）。 */
async function enterSession(options: ScenarioOptions, ready: (container: HTMLElement) => boolean, label: string): Promise<Mounted> {
  const mounted = await mountScenario({
    ...options,
    route: DIRS_ROUTE,
    extraRoutes: [options.route, ...(options.extraRoutes ?? [])],
  });
  await waitFor(
    () => query(mounted.container, '[data-directory-list="directories"]') !== null,
    "目录页等到快照（等价于认证完成）",
  );
  navigate(options.route.path);
  await waitFor(() => ready(mounted.container), label);
  return mounted;
}

/** 在输入区选一个文件（真实文件选择器被替换成固定夹具）。 */
function pickFile(container: HTMLElement, fileName: string, contents: string): void {
  const originalClick = HTMLInputElement.prototype.click;
  try {
    HTMLInputElement.prototype.click = function () {
      const transfer = new DataTransfer();
      transfer.items.add(new File([contents], fileName, { type: "text/plain" }));
      this.files = transfer.files;
      this.dispatchEvent(new Event("change"));
    };
    click(
      query(container, '[data-action="pick-attachment"]'),
      "选择附件",
    );
    HTMLInputElement.prototype.click = originalClick;
  } finally {
    HTMLInputElement.prototype.click = originalClick;
  }
}

const checks: readonly Check[] = [
  {
    name: "CR-1 旧“继续”不抵消新“继续”：本地条目先独立可见，正式正文按 turnId 原位接替",
    async run() {
      const mounted = await enterSession(
        reliabilityScenario({
          // 扣住提交应答：「已提交但未确认」与「正式正文到达」是两个必须分开的观察窗口。
          holdCommands: ["session.prompt"],
          readResult: (sessionId) =>
            readPage({
              sessionId,
              messages: [
                readMessage({
                  messageId: HISTORY_MESSAGE_ID,
                  sessionId,
                  role: "user",
                  text: "继续",
                  createdAt: "2026-10-01T00:00:00.000Z",
                }),
              ],
            }),
        }),
        (container) => query(container, `[data-message-id="${HISTORY_MESSAGE_ID}"]`) !== null,
        "历史里的“继续”渲染",
      );
      const container = mounted.container;
      const sent = (): readonly string[] => mounted.host?.sent ?? [];
      try {
        expectEqual(userBubbles(container).length, 1, "历史里只有一条用户消息");

        typeIntoComposer(container, "继续");
        click(query(container, 'button[data-action="submit-prompt"]'), "发送");
        await waitFor(
          () => query(container, '[data-message-local="submitted"]') !== null,
          "本次输入立即显示（待服务端确认）",
        );

        // 旧实现按相同文字抵消：历史那条会把本地条目吞掉，这里只剩一条。
        const bubbles = userBubbles(container);
        expectEqual(bubbles.length, 2, "旧“继续”不得抵消本次“继续”");
        expectThat(
          textOf(bubbles[1]).includes("继续"),
          "第二条用户气泡必须是本次输入",
        );
        expectEqual(
          bubbles[1]?.getAttribute("data-message-local"),
          "submitted",
          "本地条目必须说明「已提交，等待服务端确认」",
        );
        expectEqual(
          commandBody(sent(), "session.prompt")?.["payload"],
          { content: [{ type: "text", text: "继续" }] },
          "发出的就是本次文字（不是与历史合并的推断）",
        );

        // 受理：拿到稳定 turnId，本地条目进入「已受理」。
        mounted.host?.completeHeldCommand("session.prompt", {
          status: "accepted",
          result: { sessionId: SESSION_ALPHA, turnId: LOCAL_TURN_ID },
        });
        await waitFor(
          () => query(container, '[data-message-local="accepted"]') !== null,
          "受理后本地条目进入已受理",
        );

        // 正式用户正文到达：只按 turnId 接替，原位、不追加。
        mounted.host?.deliverEvent(
          wireEvent({
            globalSequence: 11,
            eventId: "aaaaaaaa-0000-4000-8000-0000000000d1",
            sessionId: SESSION_ALPHA,
            eventType: "user.message.delta",
            createdAt: "2026-10-01T00:00:02.000Z",
            view: {
              messageId: LOCAL_MESSAGE_ID,
              turnId: LOCAL_TURN_ID,
              deltaIndex: "0",
              text: "继续",
              source: "owned_prompt",
            },
          }),
        );
        await waitFor(
          () => query(container, `[data-message-id="${LOCAL_MESSAGE_ID}"]`) !== null,
          "正式用户消息按 turnId 接替本地条目",
        );
        const after = userBubbles(container);
        expectEqual(after.length, 2, "接替不得追加第三条用户消息");
        expectEqual(
          after[0]?.getAttribute("data-message-id"),
          HISTORY_MESSAGE_ID,
          "历史消息仍在前",
        );
        expectEqual(
          after[1]?.getAttribute("data-message-id"),
          LOCAL_MESSAGE_ID,
          "本次输入仍在原来的位置（原位接替）",
        );
        expectEqual(
          after[1]?.getAttribute("data-message-volatile"),
          "true",
          "正式正文来自实时事件（尚未被持久读取覆盖）",
        );
        expectThat(
          query(container, '[data-message-local]:not([data-message-local="false"])') === null,
          "被同一 turnId 的正式正文接替后，不得再留本地说明",
        );
      } finally {
        await mounted.unmount();
      }
    },
  },
  {
    name: "CR-2 等待 → 工具 → 回复：过程行只陈述已知事实，有实际内容即让位",
    async run() {
      const mounted = await enterSession(
        reliabilityScenario({ holdCommands: ["session.prompt"] }),
        (container) => query(container, '[data-composer="editable"]') !== null,
        "可提交的输入区",
      );
      const container = mounted.container;
      try {
        typeIntoComposer(container, "帮我检查部署脚本");
        click(query(container, 'button[data-action="submit-prompt"]'), "发送");
        await waitFor(
          () => query(container, '[data-message-local="submitted"]') !== null,
          "本次输入可见",
        );
        mounted.host?.completeHeldCommand("session.prompt", {
          status: "accepted",
          result: { sessionId: SESSION_ALPHA, turnId: LOCAL_TURN_ID },
        });

        // 受理后没有任何输出：唯一的依据是「本轮未终结且没有实际内容」。
        await waitFor(() => query(container, '[data-message-local="submitted"]') !== null, "受理后保留用户输入");
        const awaiting = textOf(query(container, '[data-process-row="awaiting"]'));
        expectThat(query(container, '[data-process-row="awaiting"]') === null, "等待不插入临时高度");
        expectThat(query(container, '[data-response-cursor="true"]') === null, "等待不插入临时光标");
        expectThat(!awaiting.includes("等待响应"), "等待效果不显示文字提示");
        expectThat(!awaiting.includes("思考"), `不得把等待写成 Agent 正在思考：${awaiting}`);

        // 工具开始：实际内容占屏，过程行让位（不重复反馈）。
        mounted.host?.deliverEvent(
          wireEvent({
            globalSequence: 12,
            eventId: "aaaaaaaa-0000-4000-8000-0000000000d2",
            sessionId: SESSION_ALPHA,
            eventType: "tool.call.started",
            createdAt: "2026-10-01T00:00:03.000Z",
            view: {
              toolCallId: TOOL_CALL_ID,
              turnId: LOCAL_TURN_ID,
              title: "读取部署脚本",
              state: "running",
            },
          }),
        );
        await waitFor(
          () => query(container, `[data-timeline-card="tool-call"][data-tool-call-id="${TOOL_CALL_ID}"]`) !== null,
          "工具卡渲染",
        );
        expectEqual(
          query(container, "[data-process-row]"),
          null,
          "工具内容出现后不得再占一条过程行",
        );

        // 回复开始：仍是实际内容阶段，过程行保持让位。
        mounted.host?.deliverEvent(
          wireEvent({
            globalSequence: 13,
            eventId: "aaaaaaaa-0000-4000-8000-0000000000d3",
            sessionId: SESSION_ALPHA,
            eventType: "agent.message.delta",
            createdAt: "2026-10-01T00:00:04.000Z",
            view: {
              messageId: AGENT_MESSAGE_ID,
              turnId: LOCAL_TURN_ID,
              deltaIndex: "0",
              text: "部署脚本检查完成",
            },
          }),
        );
        await waitFor(
          () => query(container, `[data-message-id="${AGENT_MESSAGE_ID}"]`) !== null,
          "回复条目渲染",
        );
        expectThat(
          textOf(query(container, `[data-message-id="${AGENT_MESSAGE_ID}"]`)).includes("部署脚本检查完成"),
          "回复正文必须可见",
        );
        expectEqual(query(container, "[data-process-row]"), null, "回复内容出现后过程行仍不占行");
      } finally {
        await mounted.unmount();
      }
    },
  },
  {
    name: "CR-3 附件上传中：输入与附件都立刻可见，且尚未派发 session.prompt",
    async run() {
      const mounted = await enterSession(
        reliabilityScenario({
          selectAttachments: true,
          // 扣住第一块的确认：上传阶段成为可观察窗口。
          attachmentPauseAt: "attachment.chunk",
          readResult: attachmentReadResult,
        }),
        (container) => query(container, '[data-composer="editable"]') !== null,
        "可提交的输入区（已协商附件）",
      );
      const container = mounted.container;
      const sent = (): readonly string[] => mounted.host?.sent ?? [];
      try {
        pickFile(container, "notes.txt", "hello attachment");
        await waitFor(
          () => query(container, '[data-attachment-status="selected"]') !== null,
          "附件被选中",
        );
        typeIntoComposer(container, "带附件的输入");
        click(query(container, 'button[data-action="submit-prompt"]'), "发送");

        await waitFor(
          () => query(container, '[data-message-local="uploading"]') !== null,
          "上传中必须立刻有本地反馈",
        );
        const local = query(container, '[data-message-local="uploading"]');
        expectThat(textOf(local).includes("附件上传中"), "本地条目必须说明附件上传中");
        expectThat(
          textOf(local).includes("Agent 尚未收到任何内容"),
          "上传阶段不得假装 Agent 已收到",
        );
        await waitFor(
          () => query(container, '[data-process-row="submitting"]') !== null,
          "过程行进入提交中",
        );
        expectThat(
          textOf(query(container, '[data-process-row="submitting"]')).includes("附件上传中"),
          "过程行必须说明提交中的真实阶段",
        );
        expectEqual(
          query(container, '[data-attachment-status="uploading"]') !== null,
          true,
          "附件本身必须显示上传中",
        );
        expectEqual(
          commandBody(sent(), "session.prompt"),
          null,
          "上传尚未完成时不得派发 prompt",
        );

        // 放行被扣住的块（用例结束前让上传继续，避免留下悬挂的等待槽）。
        mounted.host?.releaseAttachmentPause();
      } finally {
        await mounted.unmount();
      }
    },
  },
  {
    name: "CR-4 附件上传失败：明确说明原因、保留输入，且一条 session.prompt 都不发",
    async run() {
      const mounted = await enterSession(
        reliabilityScenario({
          selectAttachments: true,
          attachmentFailure: (frameType) =>
            frameType === "attachment.chunk"
              ? {
                  code: "attachment.budget_exhausted",
                  message: "附件上传预算已用完（假主机）",
                  retryable: true,
                }
              : null,
          readResult: attachmentReadResult,
        }),
        (container) => query(container, '[data-composer="editable"]') !== null,
        "可提交的输入区（已协商附件）",
      );
      const container = mounted.container;
      const sent = (): readonly string[] => mounted.host?.sent ?? [];
      try {
        pickFile(container, "notes.txt", "upload will fail");
        await waitFor(
          () => query(container, '[data-attachment-status="selected"]') !== null,
          "附件被选中",
        );
        typeIntoComposer(container, "上传失败也要保留输入");
        click(query(container, 'button[data-action="submit-prompt"]'), "发送");

        await waitFor(
          () => query(container, '[data-message-local="failed"]') !== null,
          "上传失败后本地条目进入未发送",
        );
        const local = query(container, '[data-message-local="failed"]');
        expectThat(textOf(local).includes("未发送"), "失败必须明确标为未发送");
        expectThat(
          textOf(local).includes("附件上传预算已用完（假主机）"),
          "失败原因必须来自服务端错误，而不是泛化文案",
        );
        await waitFor(
          () => query(container, '[data-attachment-status="failed"]') !== null,
          "附件条目显示失败",
        );
        expectEqual(
          commandBody(sent(), "session.prompt"),
          null,
          "任一附件失败都不得派发 prompt（Agent 从未收到内容）",
        );
        // 完整草稿保留：用户可以直接修正后重试。
        expectEqual(
          composerTextArea(container)?.value,
          "上传失败也要保留输入",
          "失败后完整草稿必须保留",
        );
      } finally {
        await mounted.unmount();
      }
    },
  },
  {
    name: "CR-5 复制标签页不接管：不发 restore/allocate，普通新建仍从空槽位开始",
    async run() {
      const identity = await createFakeHostIdentity(HOST_ID);
      await seedPairedHost(identity);
      // 真实草稿端口 + 一条只对「原准备」返回 duplicate 的归属判定：
      // 这正是复制/opener 标签页在平台层得到的结果（判定本身由 preparation-draft 单测覆盖）。
      const real = openPreparationDraftStore();
      const drafts: PreparationDraftPort = new Proxy(real, {
        get(target, property, receiver) {
          if (property === "readTabLocator") {
            return async () => ({
              canonicalOrigin: CANONICAL_ORIGIN,
              ownerNodeId: HOST_ID,
              deviceId: DEVICE_ID,
              tabSlotId: "11111111-1111-4111-8111-111111111111",
              instanceId: "22222222-2222-4222-8222-222222222222",
              sessionId: COPIED_SESSION_ID,
              savedAtMs: Date.now(),
            });
          }
          if (property === "beginTab") {
            return async (input: { readonly scope: { readonly sessionId: string } }) =>
              input.scope.sessionId === COPIED_SESSION_ID
                ? { status: "duplicate", reason: "live_tab_owner" }
                : target.beginTab(input as never);
          }
          const value = Reflect.get(target, property, receiver) as unknown;
          return typeof value === "function" ? (value as (...args: unknown[]) => unknown).bind(target) : value;
        },
      });
      const platform: PlatformPorts = { ...openPlatform(), preparationDrafts: drafts };
      const mounted = await mountScenario({
        ...reliabilityScenario({
          selectPreparation: true,
          preparationResult: (body) => readyPreparationResult(body, receiptFor(COPIED_SESSION_ID)),
          route: sessionRoute(COPIED_SESSION_ID),
        }),
        identity,
        platform,
        route: DIRS_ROUTE,
        extraRoutes: [sessionRoute(COPIED_SESSION_ID), DIRS_ROUTE],
      });
      const sent = (): readonly string[] => mounted.host?.sent ?? [];
      try {
        await waitFor(
          () => query(mounted.container, '[data-directory-list="directories"]') !== null,
          "目录页等到快照",
        );
        navigate(sessionRoute(COPIED_SESSION_ID).path);
        await waitFor(
          () => mounted.composition?.store.preparationPage(COPIED_SESSION_ID)?.notice?.includes("另一个标签页") === true,
          "复制标签页得到明确占用说明",
        );
        // 不接管：既不恢复原准备、也不分配新的。
        expectEqual(
          preparationBodies(sent()).filter((body) => body["action"] === "restore").length,
          0,
          "复制标签页不得恢复原准备",
        );
        expectEqual(
          preparationBodies(sent()).filter((body) => body["action"] === "allocate").length,
          0,
          "复制标签页不得凭陌生地址分配准备",
        );
        expectEqual(
          mounted.composition?.store.activePreparationSessionId() ?? null,
          null,
          "复制标签页不得把原会话当成自己的槽位",
        );
        expectEqual(
          JSON.stringify(mounted.composition?.store.preparationSlotRoute() ?? null),
          JSON.stringify({ kind: "none" }),
          "复制标签页的普通新建入口仍按「没有槽位」放行",
        );
        // 占用说明条目不提供「放弃」（本标签页从未持有它的资源）。
        expectEqual(
          mounted.composition?.store.canAbandonPreparation(COPIED_SESSION_ID) ?? true,
          false,
          "复制标签页不得执行放弃（会误清自己的定位）",
        );
      } finally {
        await mounted.unmount();
      }
    },
  },
  {
    name: "CR-6 新会话不显示临时状态行或放弃入口",
    async run() {
      const identity = await createFakeHostIdentity(HOST_ID);
      await seedPairedHost(identity);
      const mounted = await mountScenario({
        ...reliabilityScenario({
          selectPreparation: true,
          preparationResult: (body) => readyPreparationResult(body, receiptFor(PREPARED_SESSION_ID)),
        }),
        identity,
        route: DIRS_ROUTE,
        extraRoutes: [sessionRoute(PREPARED_SESSION_ID), DIRS_ROUTE],
      });
      const container = mounted.container;
      try {
        await waitFor(
          () => query(container, '[data-directory-list="directories"]') !== null,
          "目录页等到快照",
        );

        // 第一次普通新建：目录项入口 → 选 Agent → 提交。
        click(
          query(query(container, `[data-directory-id="${DIRECTORY_ID_ALPHA}"]`), '[data-action="create-session"]'),
          "目录内新建",
        );
        await waitFor(() => query(container, "[data-create-sheet]") !== null, "创建弹层");
        click(query(container, 'button[data-agent-id="codex-a"]'), "选择 Agent");
        await waitFor(
          () => query(container, '[data-action="submit-create"]')?.getAttribute("aria-disabled") !== "true",
          "允许提交",
        );
        click(query(container, '[data-action="submit-create"]'), "创建会话");
        await waitFor(
          () => currentPath() === `/session/${PREPARED_SESSION_ID}`,
          "进入后端分配的准备页",
        );
        await waitFor(
          () => query(container, "[data-composer-input]") !== null,
          "准备页输入区就绪",
        );
        typeIntoComposer(container, "普通草稿");
        await settle();

        // 用户明确要求：新会话不显示任何临时状态提示、放弃按钮或确认弹层。
        for (const forbidden of ["放弃新会话", "尚未转正"]) {
          expectEqual(domText(container).includes(forbidden), false, `新会话不得显示「${forbidden}」`);
        }
        for (const selector of [
          '[data-action="abandon-preparation"]',
          '[data-action="confirm-abandon-preparation"]',
          '[data-action="cancel-abandon-preparation"]',
          '[data-route="preparation-abandon"]',
          '[data-preparation-discard-dialog]',
        ]) {
          expectEqual(query(container, selector), null, `新会话不得存在 ${selector}`);
        }
      } finally {
        await mounted.unmount();
      }
    },
  },
  {
    name: "CR-7 单个文字片段到达即呈现：不等后续片段、定时器或下一次绘制帧批量提交",
    async run() {
      const mounted = await enterSession(
        reliabilityScenario(),
        (container) => query(container, '[data-composer="editable"]') !== null,
        "可提交的输入区",
      );
      const container = mounted.container;
      try {
        mounted.host?.deliverEvent(
          wireEvent({
            globalSequence: 11,
            eventId: "aaaaaaaa-0000-4000-8000-0000000000d7",
            sessionId: SESSION_ALPHA,
            eventType: "agent.message.delta",
            createdAt: "2026-10-01T00:00:02.000Z",
            view: { messageId: AGENT_MESSAGE_ID, turnId: LOCAL_TURN_ID, deltaIndex: "0", text: "第一段" },
          }),
        );
        // 只等**一个**绘制帧：后续片段从未投递，也没有任何定时器被安排。
        await nextFrame();
        const node = query(container, `[data-message-id="${AGENT_MESSAGE_ID}"]`);
        expectThat(node !== null, "单个片段到达后的首帧就必须有这条正文");
        expectThat(textOf(node).includes("第一段"), `首帧正文必须是这个片段本身：${textOf(node)}`);
        // 再采一帧：没有内容被「攒到下一帧」才补进来。
        const firstFrame = textOf(node);
        await nextFrame();
        expectEqual(
          textOf(query(container, `[data-message-id="${AGENT_MESSAGE_ID}"]`)),
          firstFrame,
          "后续帧不得再补入别的内容（不存在批量提交）",
        );
      } finally {
        await mounted.unmount();
      }
    },
  },
  {
    name: "CR-8 收尾接替不换节点：轮次归属补齐与权威读取都复用同一片正文的 DOM 元素",
    async run() {
      let reads = 0;
      const mounted = await enterSession(
        reliabilityScenario({
          holdCommands: ["session.prompt"],
          readResult: (sessionId) => {
            reads += 1;
            if (reads === 1) return readPage({ sessionId });
            return readPage({
              sessionId,
              messages: [
                readMessage({
                  messageId: LOCAL_MESSAGE_ID, sessionId, role: "user", text: "继续",
                  createdAt: "2026-10-01T00:00:02.000Z", turnId: LOCAL_TURN_ID,
                }),
                readMessage({
                  messageId: AGENT_MESSAGE_ID, sessionId, role: "agent", text: "部署脚本检查完成",
                  createdAt: "2026-10-01T00:00:04.000Z", turnId: LOCAL_TURN_ID,
                }),
              ],
            });
          },
        }),
        (container) => query(container, '[data-composer="editable"]') !== null,
        "可提交的输入区",
      );
      const container = mounted.container;
      try {
        typeIntoComposer(container, "继续");
        click(query(container, 'button[data-action="submit-prompt"]'), "发送");
        await waitFor(
          () => query(container, '[data-message-local="submitted"]') !== null,
          "本地回显可见",
        );
        // 事件流与命令应答没有相互顺序：本轮事件可能先到，受理结果随后才把 turnId 补上。
        mounted.host?.deliverEvent(
          wireEvent({
            globalSequence: 11,
            eventId: "aaaaaaaa-0000-4000-8000-0000000000d8",
            sessionId: SESSION_ALPHA,
            eventType: "turn.started",
            createdAt: "2026-10-01T00:00:01.500Z",
            view: { turnId: LOCAL_TURN_ID, state: "running" },
          }),
        );
        const echoBefore = query(container, '[data-message-local="submitted"]');
        expectThat(echoBefore !== null, "受理前本地回显可见");
        const timelineList = query(container, '[data-timeline="entries"]');
        expectThat(timelineList !== null, "时间线列表存在");
        const echoEntry = echoBefore?.closest("[data-timeline-key]") ?? null;
        const echoKey = echoEntry?.getAttribute("data-timeline-key") ?? null;
        expectThat(echoEntry !== null && echoKey !== null, "本地回显在带稳定键的条目里");
        // 条目在列表里**平级**渲染：父级就是这个列表，而不是某个轮次分组容器。
        expectThat(echoEntry?.parentElement === timelineList, "条目直接挂在时间线列表下");

        mounted.host?.completeHeldCommand("session.prompt", {
          status: "accepted",
          result: { sessionId: SESSION_ALPHA, turnId: LOCAL_TURN_ID },
        });
        await waitFor(
          () => query(container, '[data-message-local="accepted"]') !== null,
          "受理后本地回显进入已受理",
        );
        // 旧实现：回显的父分组键从首条条目键拼成，轮次归属补齐会换名，这个节点被卸载再挂载。
        expectThat(
          query(container, '[data-message-local="accepted"]') === echoBefore,
          "轮次归属补齐不得重挂载本地回显节点",
        );
        expectEqual(
          echoBefore?.closest("[data-timeline-key]")?.getAttribute("data-timeline-key") ?? null,
          echoKey,
          "回显的条目键不得因轮次归属补齐漂移",
        );

        mounted.host?.deliverEvent(
          wireEvent({
            globalSequence: 12,
            eventId: "aaaaaaaa-0000-4000-8000-0000000000d9",
            sessionId: SESSION_ALPHA,
            eventType: "agent.message.delta",
            createdAt: "2026-10-01T00:00:04.000Z",
            view: { messageId: AGENT_MESSAGE_ID, turnId: LOCAL_TURN_ID, deltaIndex: "0", text: "部署脚本检查完成" },
          }),
        );
        await waitFor(
          () => query(container, `[data-message-id="${AGENT_MESSAGE_ID}"][data-message-volatile="true"]`) !== null,
          "流式回复可见",
        );
        const streaming = query(container, `[data-message-id="${AGENT_MESSAGE_ID}"]`);
        expectThat(streaming !== null, "流式回复条目存在");
        const streamingEntry = streaming?.closest("[data-timeline-key]") ?? null;
        expectThat(streamingEntry !== null, "流式回复在带稳定键的条目里");

        // 权威读取接替：同一 messageId 的正文必须仍是同一个 DOM 元素。
        mounted.composition?.store.loadSession(SESSION_ALPHA);
        await waitFor(
          () => query(container, `[data-message-id="${AGENT_MESSAGE_ID}"][data-message-volatile="false"]`) !== null,
          "权威读取接替流式正文",
        );
        expectThat(
          query(container, `[data-message-id="${AGENT_MESSAGE_ID}"]`) === streaming,
          "读取接替不得更换同一条正文的 DOM 节点",
        );
        // 正式用户正文（已确认本地关联）按权威位置落进这一轮、插在 Agent 正文之前：这是键控
        // 重排，已有 Agent 正文的条目节点、键与父级都必须照旧。
        expectThat(
          query(container, `[data-message-id="${AGENT_MESSAGE_ID}"]`)?.closest("[data-timeline-key]") === streamingEntry,
          "正式用户正文落位不得重挂载已有 Agent 正文条目",
        );
        // 接替后节点属性已是正式身份（`data-message-local="false"`、`data-message-id` 为服务端
        // 标识），不能再按本地阶段查它；用原节点引用与正式消息标识一起确认「还是同一条目」。
        const formalEcho = query(container, `[data-message-id="${LOCAL_MESSAGE_ID}"]`);
        expectThat(formalEcho !== null, "正式用户正文已接替本地回显");
        expectThat(formalEcho === echoBefore, "接替后本地回显仍是同一个 DOM 节点");
        expectThat(
          formalEcho?.closest("[data-timeline-key]") === echoEntry,
          "接替后本地回显仍用同一个条目节点",
        );
        expectEqual(
          echoEntry?.getAttribute("data-timeline-key"),
          echoKey,
          "接替后条目键仍必须是本地稳定键",
        );
        expectEqual(
          echoEntry?.parentElement === timelineList,
          true,
          "接替后条目仍直接挂在时间线列表下",
        );
        // 本地回显也被正式用户消息原位接替：节点还在，只是属性换成了正式身份。
        expectThat(echoBefore?.isConnected === true, "接替后本地回显节点不得被移除");
        expectEqual(
          echoBefore?.getAttribute("data-message-id"),
          LOCAL_MESSAGE_ID,
          "同一个节点必须更新为正式消息标识",
        );
        expectEqual(
          echoBefore?.getAttribute("data-message-local"),
          "false",
          "接替后不得再留本地阶段标注",
        );
        // 权威顺序：用户正文在本轮 Agent 正文之前（读取结果的创建时刻更早）。
        const order = [...queryAll(container, "[data-timeline-key]")]
          .map((node) => node.getAttribute("data-timeline-key"));
        expectThat(
          echoKey !== null && order.indexOf(echoKey) >= 0 &&
            order.indexOf(echoKey) < order.indexOf(`message:${AGENT_MESSAGE_ID}`),
          `用户正文必须排在 Agent 正文之前：${order.join(" > ")}`,
        );
        // 轮次分组只是呈现资料：这一轮的条目共享同一段标识与计数（turn 卡 + 用户 + 回复）。
        expectEqual(echoEntry?.getAttribute("data-turn-group-id"), LOCAL_TURN_ID, "用户正文归属本轮");
        expectEqual(echoEntry?.getAttribute("data-turn-group-entry-count"), "3", "本轮共有三条条目");
        expectEqual(streamingEntry?.getAttribute("data-turn-group-id"), LOCAL_TURN_ID, "Agent 正文归属本轮");
      } finally {
        await mounted.unmount();
      }
    },
  },
  {
    name: "CR-9 接替后本地回显不重现：压缩后正文保持到权威消息页原位接管",
    async run() {
      const mounted = await enterSession(
        reliabilityScenario({
          holdCommands: ["session.prompt"],
          readResult: (sessionId) => readPage({ sessionId }),
        }),
        (container) => query(container, '[data-composer="editable"]') !== null,
        "可提交的输入区",
      );
      const container = mounted.container;
      try {
        typeIntoComposer(container, "只应出现一次");
        click(query(container, 'button[data-action="submit-prompt"]'), "发送");
        await waitFor(
          () => query(container, '[data-message-local="submitted"]') !== null,
          "本地回显可见",
        );
        mounted.host?.completeHeldCommand("session.prompt", {
          status: "accepted",
          result: { sessionId: SESSION_ALPHA, turnId: LOCAL_TURN_ID },
        });
        mounted.host?.deliverEvent(
          wireEvent({
            globalSequence: 11,
            eventId: "aaaaaaaa-0000-4000-8000-0000000000da",
            sessionId: SESSION_ALPHA,
            eventType: "user.message.delta",
            createdAt: "2026-10-01T00:00:02.000Z",
            view: {
              messageId: LOCAL_MESSAGE_ID, turnId: LOCAL_TURN_ID, deltaIndex: "0",
              text: "只应出现一次", source: "owned_prompt",
            },
          }),
        );
        await waitFor(
          () => query(container, `[data-message-id="${LOCAL_MESSAGE_ID}"]`) !== null,
          "正式用户消息出现",
        );
        expectThat(
          query(container, '[data-message-local]:not([data-message-local="false"])') === null,
          "接替后本地回显必须已退出",
        );

        // 增量被压缩：先保留可见正文，等仓库重读返回权威消息页后再替换。
        mounted.host?.holdCommand("session.read");
        const beforeCompaction = query(container, `[data-message-id="${LOCAL_MESSAGE_ID}"]`);
        expectThat(beforeCompaction !== null, "压缩前正式用户正文可见");
        mounted.host?.deliverEvent(
          wireEvent({
            globalSequence: 12,
            eventId: "aaaaaaaa-0000-4000-8000-0000000000db",
            sessionId: SESSION_ALPHA,
            eventType: "turn.delta_compacted",
            createdAt: "2026-10-01T00:00:03.000Z",
            view: { turnId: LOCAL_TURN_ID, deltaCount: "1" },
          }),
        );
        await waitFor(() => commandBodies(mounted.host!.sent, "session.read").length >= 2, "压缩触发权威重读");
        await new Promise(resolve => setTimeout(resolve, 150));
        expectThat(beforeCompaction?.isConnected === true, "权威重读返回前正文仍挂载");
        expectThat(
          query(container, '[data-message-local]:not([data-message-local="false"])') === null,
          "保留期间不得让本地回显重新出现",
        );

        // 权威消息页为空：正式正文原位退出，本地回显仍不得复活。
        expectThat(
          mounted.host?.completeHeldCommand("session.read", {
            result: readPage({ sessionId: SESSION_ALPHA }),
          }) === true,
          "假主机应有一条被扣住的权威读取",
        );
        await waitFor(
          () => query(container, `[data-message-id="${LOCAL_MESSAGE_ID}"]`) === null,
          "权威消息页接管后正文移除",
        );
        expectThat(
          query(container, '[data-message-local]:not([data-message-local="false"])') === null,
          "页替换/重读不得让已接替的本地回显重新出现",
        );
        expectEqual(userBubbles(container).length, 0, "权威页为空时也不得留本地条目冒充已发送");
      } finally {
        await mounted.unmount();
      }
    },
  },
  {
    name: "CR-10 新回复逐片段增长时，已完成的历史条目节点与 DOM 都不被重写",
    async run() {
      const mounted = await enterSession(
        reliabilityScenario({
          readResult: (sessionId) =>
            readPage({
              sessionId,
              messages: [
                readMessage({
                  messageId: HISTORY_MESSAGE_ID, sessionId, role: "user", text: "历史正文",
                  createdAt: "2026-10-01T00:00:00.000Z",
                }),
              ],
            }),
        }),
        (container) => query(container, `[data-message-id="${HISTORY_MESSAGE_ID}"]`) !== null,
        "历史消息渲染",
      );
      const container = mounted.container;
      let counting: MutationObserver | null = null;
      let mutations = 0;
      try {
        const history = query(container, `[data-message-id="${HISTORY_MESSAGE_ID}"]`);
        expectThat(history !== null, "历史消息存在");
        counting = new MutationObserver((records) => {
          mutations += records.length;
        });
        counting.observe(history as Node, {
          attributes: true, childList: true, characterData: true, subtree: true,
        });

        for (let index = 0; index < 6; index += 1) {
          mounted.host?.deliverEvent(
            wireEvent({
              globalSequence: 20 + index,
              eventId: `aaaaaaaa-0000-4000-8000-0000000001${String(index).padStart(2, "0")}`,
              sessionId: SESSION_ALPHA,
              eventType: "agent.message.delta",
              createdAt: "2026-10-01T00:00:05.000Z",
              view: {
                messageId: AGENT_MESSAGE_ID, turnId: LOCAL_TURN_ID, deltaIndex: String(index),
                text: `片段${String(index)}`,
              },
            }),
          );
        }
        await waitFor(
          () => textOf(query(container, `[data-message-id="${AGENT_MESSAGE_ID}"]`)).includes("片段5"),
          "新回复逐片段增长到末尾",
        );
        expectThat(
          query(container, `[data-message-id="${HISTORY_MESSAGE_ID}"]`) === history,
          "已完成条目的节点不得随新回复片段重建",
        );
        // 旧实现的 React 差分也不会重写未变化条目的 DOM；这条守的是「条目被重建/整表重挂载」的回归：
        // 一旦条目身份不稳定，这里会立刻出现 childList/attributes 变更。
        mutations += counting?.takeRecords().length ?? 0;
        expectEqual(mutations, 0, "已完成条目的 DOM 不应产生任何变异");
      } finally {
        counting?.disconnect();
        await mounted.unmount();
      }
    },
  },
  {
    name: "CR-11 真实变化仍被渲染：工具状态、授权卡、媒体块、未信任 Markdown 与降级条目",
    async run() {
      const mounted = await enterSession(
        reliabilityScenario(),
        (container) => query(container, '[data-composer="editable"]') !== null,
        "可提交的输入区",
      );
      const container = mounted.container;
      try {
        // 工具调用：ACP 的非终态是 `in_progress`（`pending`/`in_progress` 默认展开），完成后
        // 状态与默认展开值都真实更新；显式展开/收起只影响这一条（默认策略不改）。
        const toolFold = (): Element | null =>
          query(container, `[data-tool-call-id="${TOOL_CALL_ID}"] [data-fold-entry]`);
        const toolExpanded = (): string | null => toolFold()?.getAttribute("data-entry-expanded") ?? null;
        mounted.host?.deliverEvent(
          wireEvent({
            globalSequence: 11, eventId: "aaaaaaaa-0000-4000-8000-0000000000e1", sessionId: SESSION_ALPHA,
            eventType: "tool.call.started", createdAt: "2026-10-01T00:00:02.000Z",
            view: { toolCallId: TOOL_CALL_ID, turnId: LOCAL_TURN_ID, title: "读取部署脚本", state: "in_progress" },
          }),
        );
        await waitFor(
          () => query(container, `[data-tool-call-id="${TOOL_CALL_ID}"][data-tool-state="in_progress"]`) !== null,
          "工具卡进行中",
        );
        expectEqual(toolExpanded(), "true", "进行中的工具默认展开");
        mounted.host?.deliverEvent(
          wireEvent({
            globalSequence: 12, eventId: "aaaaaaaa-0000-4000-8000-0000000000e2", sessionId: SESSION_ALPHA,
            eventType: "tool.call.completed", createdAt: "2026-10-01T00:00:03.000Z",
            view: { toolCallId: TOOL_CALL_ID, turnId: LOCAL_TURN_ID, title: "读取部署脚本", state: "completed" },
          }),
        );
        await waitFor(
          () => query(container, `[data-tool-call-id="${TOOL_CALL_ID}"][data-tool-state="completed"]`) !== null,
          "工具状态真实更新为已完成",
        );
        expectEqual(toolExpanded(), "false", "完成的工具默认收起");
        // 显式操作仍然生效：展开 → 正文可见，再点一次收起；覆盖值只属于这一条。
        click(
          query(container, `[data-tool-call-id="${TOOL_CALL_ID}"] [data-action="toggle-timeline-entry"]`),
          "展开已完成的工具调用",
        );
        await waitFor(() => toolExpanded() === "true", "工具卡展开");
        expectThat(
          query(toolFold(), "[data-fold-body]")?.getAttribute("data-fold-body") === "expanded",
          "展开后折叠正文必须可见",
        );
        click(
          query(container, `[data-tool-call-id="${TOOL_CALL_ID}"] [data-action="toggle-timeline-entry"]`),
          "收起已完成的工具调用",
        );
        await waitFor(() => toolExpanded() === "false", "工具卡收起");
        expectEqual(
          query(container, `[data-tool-call-id="${TOOL_CALL_ID}"]`)?.getAttribute("data-tool-state"),
          "completed",
          "展开/收起不改变工具的真实状态",
        );

        // 授权请求：结构化卡片与可应答选项都在（不是被静默文本化）。
        mounted.host?.deliverEvent(
          wireEvent({
            globalSequence: 13, eventId: "aaaaaaaa-0000-4000-8000-0000000000e3", sessionId: SESSION_ALPHA,
            eventType: "permission.requested", createdAt: "2026-10-01T00:00:04.000Z",
            view: {
              interactionId: "aaaaaaaa-0000-4000-8000-00000000e3aa",
              turnId: LOCAL_TURN_ID,
              title: "允许写入 src/ 吗？",
              description: null,
              options: [{ optionId: "allow-once", label: "允许一次", kind: "allow_once" }],
            },
          }),
        );
        await waitFor(
          () => query(container, '[data-timeline-card="permission-requested"]') !== null,
          "授权卡渲染",
        );
        // 卡片本身与请求标题必须真实呈现（授权是否可应答由既有可应答性判定决定，不在这里改判）。
        expectThat(
          textOf(query(container, '[data-timeline-card="permission-requested"]')).includes("允许写入 src/ 吗？"),
          "授权卡必须呈现请求标题",
        );
        expectThat(
          query(container, '[data-interaction-id="aaaaaaaa-0000-4000-8000-00000000e3aa"]') !== null,
          "授权卡必须保留请求标识",
        );

        // 媒体块与未信任 Markdown：同一条正文里两种降级/媒体呈现都要出来。
        mounted.host?.deliverEvent(
          wireEvent({
            globalSequence: 14, eventId: "aaaaaaaa-0000-4000-8000-0000000000e4", sessionId: SESSION_ALPHA,
            eventType: "agent.message.delta", createdAt: "2026-10-01T00:00:05.000Z",
            view: {
              messageId: AGENT_MESSAGE_ID, turnId: LOCAL_TURN_ID, deltaIndex: "0",
              text: "截图如下 <script>window.x=1</script>",
              block: {
                type: "image_ref", mimeType: "image/png",
                attachmentId: "aaaaaaaa-0000-4000-8000-00000000e4bb", byteLength: "4",
                displayState: "available",
              },
            },
          }),
        );
        await waitFor(
          () => query(container, `[data-message-id="${AGENT_MESSAGE_ID}"] [data-media-block="image"]`) !== null,
          "增量里的媒体块渲染",
        );
        expectThat(
          query(container, `[data-message-id="${AGENT_MESSAGE_ID}"] [data-markdown-omitted="html"]`) !== null,
          "原始 HTML 仍按不可信文本降级呈现",
        );

        // 降级条目（覆盖未命中的事件）：不被静默丢弃。
        mounted.host?.deliverEvent(
          wireEvent({
            globalSequence: 15, eventId: "aaaaaaaa-0000-4000-8000-0000000000e5", sessionId: SESSION_ALPHA,
            eventType: "acp.agent.vendor_status", createdAt: "2026-10-01T00:00:06.000Z",
            view: { phase: "review" },
          }),
        );
        await waitFor(
          () => query(container, '[data-degraded-entry="aaaaaaaa-0000-4000-8000-0000000000e5"]') !== null,
          "未知事件走降级卡片",
        );
      } finally {
        await mounted.unmount();
      }
    },
  },
  {
    name: "CR-12 输出先于受理应答到达：临时回显取得 turnId 后前插，正式正文节点与父级不换",
    async run() {
      const mounted = await enterSession(
        reliabilityScenario({
          // 扣住受理应答：让「本轮输出已到」与「回显补上轮次归属」成为两个可分开观察的时刻。
          holdCommands: ["session.prompt"],
        }),
        (container) => query(container, '[data-composer="editable"]') !== null,
        "可提交的输入区",
      );
      const container = mounted.container;
      try {
        typeIntoComposer(container, "继续");
        click(query(container, 'button[data-action="submit-prompt"]'), "发送");
        await waitFor(
          () => query(container, '[data-message-local="submitted"]') !== null,
          "本地回显可见",
        );
        const timelineList = query(container, '[data-timeline="entries"]');
        const echoBefore = query(container, '[data-message-local="submitted"]');
        expectThat(echoBefore !== null, "本地回显节点存在");
        const echoEntry = echoBefore.closest("[data-timeline-key]");
        const echoKey = echoEntry?.getAttribute("data-timeline-key") ?? null;
        expectThat(echoEntry !== null && echoKey !== null, "本地回显在带稳定键的条目里");

        // 本轮输出**先于**受理应答到达：此时回显还没有可靠 `turnId`，按「未取得关联」留在末尾。
        mounted.host?.deliverEvent(
          wireEvent({
            globalSequence: 11,
            eventId: "aaaaaaaa-0000-4000-8000-0000000000f1",
            sessionId: SESSION_ALPHA,
            eventType: "agent.message.delta",
            createdAt: "2026-10-01T00:00:04.000Z",
            view: {
              messageId: AGENT_MESSAGE_ID, turnId: LOCAL_TURN_ID, deltaIndex: "0",
              text: "部署脚本检查完成",
            },
          }),
        );
        await waitFor(
          () => query(container, `[data-message-id="${AGENT_MESSAGE_ID}"][data-message-volatile="true"]`) !== null,
          "流式回复可见",
        );
        const agent = query(container, `[data-message-id="${AGENT_MESSAGE_ID}"]`);
        expectThat(agent !== null, "Agent 正文节点存在");
        const agentEntry = agent.closest("[data-timeline-key]");
        const agentKey = agentEntry?.getAttribute("data-timeline-key") ?? null;
        expectThat(agentEntry !== null && agentKey !== null, "Agent 正文在带稳定键的条目里");
        // 两条条目都平级挂在同一个列表下。
        expectThat(echoEntry?.parentElement === timelineList, "回显条目直接挂在时间线列表下");
        expectThat(agentEntry?.parentElement === timelineList, "Agent 正文条目直接挂在时间线列表下");

        mounted.host?.completeHeldCommand("session.prompt", {
          status: "accepted",
          result: { sessionId: SESSION_ALPHA, turnId: LOCAL_TURN_ID },
        });
        await waitFor(
          () => query(container, '[data-message-local="accepted"]') !== null,
          "受理后本地回显进入已受理",
        );

        // 回显仍是**没有权威位置**的临时条目：拿到可靠关联后前插到本轮输出之前。这只是键控
        // 重排 —— 两个正文节点、条目键与父级都必须照旧。
        expectThat(
          query(container, '[data-message-local="accepted"]') === echoBefore,
          "轮次归属补齐不得重挂载本地回显节点",
        );
        expectThat(
          query(container, `[data-message-id="${AGENT_MESSAGE_ID}"]`) === agent,
          "同轮输出先到也不得被前插的回显顶掉（Agent 正文节点必须连续）",
        );
        expectEqual(
          echoBefore?.closest("[data-timeline-key]")?.getAttribute("data-timeline-key") ?? null,
          echoKey,
          "回显的条目键不得因轮次归属补齐漂移",
        );
        expectEqual(
          agent?.closest("[data-timeline-key]")?.getAttribute("data-timeline-key") ?? null,
          agentKey,
          "Agent 正文的条目键不得因回显前插漂移",
        );
        // 顺序仍然正确：临时回显排在本轮输出之前，正式正文之间没有任何重排。
        const order = [...queryAll(container, "[data-timeline-key]")]
          .map((node) => node.getAttribute("data-timeline-key"));
        expectThat(
          echoKey !== null && order.indexOf(echoKey) >= 0 &&
            order.indexOf(echoKey) < order.indexOf(`message:${AGENT_MESSAGE_ID}`),
          `取得可靠轮次关联后回显必须排在本轮输出之前：${order.join(" > ")}`,
        );
        // 分组只是资料：临时回显与同轮输出连续，因此共享同一段。
        expectEqual(echoEntry?.getAttribute("data-turn-group-id"), LOCAL_TURN_ID, "回显归属本轮");
        expectEqual(echoEntry?.getAttribute("data-turn-group-entry-count"), "2", "本轮共两条条目");
        expectEqual(agentEntry?.getAttribute("data-turn-group-id"), LOCAL_TURN_ID, "Agent 正文归属本轮");
      } finally {
        await mounted.unmount();
      }
    },
  },
  {
    name: "CR-12b 用户实时消息先于受理结果到达：补齐 turnId 后原位接替，不重挂节点",
    async run() {
      const mounted = await enterSession(
        reliabilityScenario({ holdCommands: ["session.prompt"] }),
        (container) => query(container, '[data-composer="editable"]') !== null,
        "可提交的输入区",
      );
      const container = mounted.container;
      try {
        typeIntoComposer(container, "先到的消息");
        click(query(container, 'button[data-action="submit-prompt"]'), "发送");
        await waitFor(
          () => query(container, '[data-message-local="submitted"]') !== null,
          "本地回显可见",
        );
        const echo = query(container, '[data-message-local="submitted"]');
        const echoEntry = echo?.closest("[data-timeline-key]") ?? null;
        const echoKey = echoEntry?.getAttribute("data-timeline-key") ?? null;
        expectThat(echo !== null && echoEntry !== null && echoKey !== null, "本地回显与稳定条目存在");

        // 真实服务可能先推用户事件、后回命令受理结果。事件已带 turnId，但提交时回显还没有。
        mounted.host?.deliverEvent(
          wireEvent({
            globalSequence: 11,
            eventId: "aaaaaaaa-0000-4000-8000-0000000000f2",
            sessionId: SESSION_ALPHA,
            causationRequestId:
              (commandBody(mounted.host?.sent ?? [], "session.prompt")?.["requestId"] as string | undefined) ?? null,
            eventType: "user.message.delta",
            createdAt: "2026-10-01T00:00:02.000Z",
            view: {
              messageId: LOCAL_MESSAGE_ID,
              turnId: LOCAL_TURN_ID,
              deltaIndex: "0",
              text: "先到的消息",
              source: "owned_prompt",
            },
          }),
        );
        await waitFor(
          () =>
            query(container, `[data-message-id="${LOCAL_MESSAGE_ID}"][data-message-local="false"]`) === echo &&
            userBubbles(container).length === 1,
          "用户实时消息可见",
        );
        expectThat(echo?.isConnected === true, "等待受理时本地回显保持挂载");

        mounted.host?.completeHeldCommand("session.prompt", {
          status: "accepted",
          result: { sessionId: SESSION_ALPHA, turnId: LOCAL_TURN_ID },
        });
        await waitFor(
          () =>
            query(container, `[data-message-id="${LOCAL_MESSAGE_ID}"][data-message-local="false"]`) === echo &&
            userBubbles(container).length === 1,
          "用户事件与本地回显完成身份接替",
        );

        expectThat(
          query(container, `[data-message-id="${LOCAL_MESSAGE_ID}"]`) === echo,
          "补齐 turnId 不得更换用户消息正文的 DOM 节点",
        );
        expectThat(
          echo?.closest("[data-timeline-key]") === echoEntry,
          "补齐 turnId 不得更换用户消息的稳定条目节点",
        );
        expectEqual(
          echoEntry?.getAttribute("data-timeline-key"),
          echoKey,
          "补齐 turnId 后稳定键保持不变",
        );
        expectEqual(userBubbles(container).length, 1, "同一次提交仍只显示一个用户消息");
      } finally {
        await mounted.unmount();
      }
    },
  },
  {
    name: "CR-13 权威序号 10/11/12：带本地接替关联的用户正文不得被前置到思考之前，已有正文不重挂",
    async run() {
      // 第三次读取把用户正文放在思考 10 与回复 12 之间（序号 11）；它的 `turnId` 在第四次读取里
      // 缺省，但已经确认的本地接替关联（`messageId → requestId`）仍让它沿用本地稳定键。
      type ReadStage = "empty" | "thought_and_reply" | "with_user" | "user_without_turn";
      let stage: ReadStage = "empty";
      const mounted = await enterSession(
        reliabilityScenario({
          holdCommands: ["session.prompt"],
          readResult: (sessionId) => {
            const thought = {
              ...readMessage({
                messageId: THOUGHT_MESSAGE_ID, sessionId, role: "agent", text: "先核对部署脚本",
                createdAt: "2026-10-01T00:00:01.000Z", turnId: LOCAL_TURN_ID,
              }),
              channel: "thought",
              sequence: "10",
            };
            const user = (turnId: string | null) => ({
              ...readMessage({
                messageId: LOCAL_MESSAGE_ID, sessionId, role: "user", text: "继续",
                createdAt: "2026-10-01T00:00:02.000Z", turnId,
              }),
              sequence: "11",
            });
            const reply = {
              ...readMessage({
                messageId: AGENT_MESSAGE_ID, sessionId, role: "agent", text: "部署脚本检查完成",
                createdAt: "2026-10-01T00:00:04.000Z", turnId: LOCAL_TURN_ID,
              }),
              sequence: "12",
            };
            if (stage === "thought_and_reply") {
              // 只有思考 10 与回复 12：先让两个正式正文节点落地。
              return readPage({ sessionId, messages: [thought, reply] });
            }
            if (stage === "with_user") {
              // 用户正文（序号 11，`turnId` 存在）插到两者之间。
              return readPage({ sessionId, messages: [thought, user(LOCAL_TURN_ID), reply] });
            }
            if (stage === "user_without_turn") {
              // 同一条正式用户正文缺少 `turnId`：顺序仍必须是 10/11/12。
              return readPage({ sessionId, messages: [thought, user(null), reply] });
            }
            return readPage({ sessionId });
          },
        }),
        (container) => query(container, '[data-composer="editable"]') !== null,
        "可提交的输入区",
      );
      const container = mounted.container;
      try {
        typeIntoComposer(container, "继续");
        click(query(container, 'button[data-action="submit-prompt"]'), "发送");
        await waitFor(
          () => query(container, '[data-message-local="submitted"]') !== null,
          "本地回显可见",
        );
        const echoBefore = query(container, '[data-message-local="submitted"]');
        const echoEntry = echoBefore?.closest("[data-timeline-key]") ?? null;
        const echoKey = echoEntry?.getAttribute("data-timeline-key") ?? null;
        expectThat(echoBefore !== null && echoKey !== null, "本地回显带稳定条目键");
        mounted.host?.completeHeldCommand("session.prompt", {
          status: "accepted",
          result: { sessionId: SESSION_ALPHA, turnId: LOCAL_TURN_ID },
        });
        await waitFor(
          () => query(container, '[data-message-local="accepted"]') !== null,
          "受理后本地回显进入已受理",
        );

        // 第二轮：思考与回复先落地（此时用户正文还没读回）。
        stage = "thought_and_reply";
        mounted.composition?.store.loadSession(SESSION_ALPHA);
        await waitFor(
          () => query(container, `[data-message-id="${THOUGHT_MESSAGE_ID}"]`) !== null &&
            query(container, `[data-message-id="${AGENT_MESSAGE_ID}"][data-message-volatile="false"]`) !== null,
          "思考与回复的正式正文就绪",
        );
        const thoughtNode = query(container, `[data-message-id="${THOUGHT_MESSAGE_ID}"]`);
        const replyNode = query(container, `[data-message-id="${AGENT_MESSAGE_ID}"]`);
        const thoughtEntry = thoughtNode?.closest("[data-timeline-key]") ?? null;
        const replyEntry = replyNode?.closest("[data-timeline-key]") ?? null;
        expectThat(thoughtNode !== null && replyNode !== null, "思考与回复节点存在");

        const expected = [
          `message:${THOUGHT_MESSAGE_ID}`,
          echoKey,
          `message:${AGENT_MESSAGE_ID}`,
        ];
        const assertAuthoritativeOrder = (when: string): void => {
          const order = [...queryAll(container, "[data-timeline-key]")]
            .map((node) => node.getAttribute("data-timeline-key"));
          expectEqual(
            order.join(" > "),
            expected.join(" > "),
            `${when}：正式条目必须按 Owner 序号 10/11/12 呈现`,
          );
        };

        // 第三轮：正式用户正文（序号 11，有 turnId）插进思考与回复之间。
        stage = "with_user";
        mounted.composition?.store.loadSession(SESSION_ALPHA);
        await waitFor(
          () => query(container, `[data-message-id="${LOCAL_MESSAGE_ID}"]`) !== null,
          "正式用户正文出现",
        );
        // 反例（旧实现）：带接替关联的正式消息被剔除出权威排序并按 turnId 前插到整轮之前，
        // 这里会变成「用户 11 > 思考 10 > 回复 12」。
        assertAuthoritativeOrder("用户正文有 turnId");
        // 接替者前插不得重挂已有正文：两个正式节点仍是原来的 DOM 节点（键控重排，不是重建）。
        expectThat(
          query(container, `[data-message-id="${THOUGHT_MESSAGE_ID}"]`)?.closest("[data-timeline-key]") === thoughtEntry,
          "用户正文插入不得重挂思考条目",
        );
        expectThat(
          query(container, `[data-message-id="${AGENT_MESSAGE_ID}"]`)?.closest("[data-timeline-key]") === replyEntry,
          "用户正文插入不得重挂回复条目",
        );
        // 本地回显被正式用户正文原位接替：同一个节点，换成正式标识。
        expectThat(echoBefore?.isConnected === true, "接替后本地回显节点不得被移除");
        expectEqual(echoBefore?.getAttribute("data-message-id"), LOCAL_MESSAGE_ID, "同一节点换成正式消息标识");
        expectEqual(echoBefore?.getAttribute("data-message-local"), "false", "接替后不再有本地阶段标注");
        expectEqual(
          echoBefore?.closest("[data-timeline-key]")?.getAttribute("data-timeline-key"),
          echoKey,
          "接替后仍沿用本地稳定键",
        );
        // 三条连续正式条目共享一段轮次资料（10/11/12 都在本轮）。
        expectEqual(
          echoEntry?.getAttribute("data-turn-group-entry-count"),
          "3",
          "本轮三条正式条目同属一段",
        );

        // 第四轮：同一条正式用户正文缺 `turnId`（关联早已确认）：顺序仍是 10/11/12。
        stage = "user_without_turn";
        mounted.composition?.store.loadSession(SESSION_ALPHA);
        await waitFor(
          () => query(container, `[data-message-id="${LOCAL_MESSAGE_ID}"]`)
            ?.closest("[data-timeline-key]")?.getAttribute("data-turn-group-id") === "unassigned",
          "缺少 turnId 的正式用户正文更新完成",
        );
        assertAuthoritativeOrder("用户正文缺少 turnId");
        expectThat(
          query(container, `[data-message-id="${LOCAL_MESSAGE_ID}"]`)?.closest("[data-timeline-key]") === echoEntry,
          "缺少 turnId 也不得重挂用户正文条目（分组资料变化不是父级变化）",
        );
        expectThat(
          query(container, `[data-message-id="${THOUGHT_MESSAGE_ID}"]`)?.closest("[data-timeline-key]") === thoughtEntry,
          "缺少 turnId 也不得重挂思考条目",
        );
        expectThat(
          query(container, `[data-message-id="${AGENT_MESSAGE_ID}"]`)?.closest("[data-timeline-key]") === replyEntry,
          "缺少 turnId 也不得重挂回复条目",
        );
        expectEqual(
          query(container, `[data-message-id="${LOCAL_MESSAGE_ID}"]`)?.closest("[data-timeline-key]")
            ?.getAttribute("data-timeline-key"),
          echoKey,
          "缺少 turnId 时仍沿用已确认的本地稳定键",
        );
      } finally {
        await mounted.unmount();
      }
    },
  },
];

export const CONVERSATION_RELIABILITY_CHECKS: readonly Check[] = checks;
