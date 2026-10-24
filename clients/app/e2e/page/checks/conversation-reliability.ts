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
 * - 复制标签页不接管原准备；显式放弃后换目录新建得到**新 ID**。
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
import { commandBody } from "./conversation-timeline";
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
/** 「放弃后新 ID」用例里的替换准备（后端分配的第二份）。 */
const REPLACEMENT_SESSION_ID = "eeeeeeee-0000-4000-8000-00000000c2b2";
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
        await waitFor(
          () => query(container, '[data-process-row="awaiting"]') !== null,
          "无首个输出时出现等待行",
        );
        const awaiting = textOf(query(container, '[data-process-row="awaiting"]'));
        expectThat(awaiting.includes("等待响应"), `等待行必须说「等待响应」：${awaiting}`);
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
    name: "CR-6 显式放弃后换目录新建得到新 ID：release 旧资源且草稿不复活",
    async run() {
      const identity = await createFakeHostIdentity(HOST_ID);
      await seedPairedHost(identity);
      let allocations = 0;
      const mounted = await mountScenario({
        ...reliabilityScenario({
          selectPreparation: true,
          preparationResult: (body) => {
            if (body["action"] === "allocate") {
              allocations += 1;
              const sessionId = allocations === 1 ? PREPARED_SESSION_ID : REPLACEMENT_SESSION_ID;
              return readyPreparationResult({ ...body, sessionId }, receiptFor(sessionId));
            }
            const sessionId = typeof body["sessionId"] === "string" ? body["sessionId"] : PREPARED_SESSION_ID;
            return readyPreparationResult(body, receiptFor(sessionId));
          },
        }),
        identity,
        route: DIRS_ROUTE,
        extraRoutes: [sessionRoute(PREPARED_SESSION_ID), sessionRoute(REPLACEMENT_SESSION_ID), DIRS_ROUTE],
      });
      const container = mounted.container;
      const sent = (): readonly string[] => mounted.host?.sent ?? [];
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
        await waitFor(
          () => query(container, '[data-route="preparation-abandon"]') !== null,
          "未转正临时会话提供显式放弃入口",
        );
        // 在准备页写一条草稿：放弃必须清掉它。
        typeIntoComposer(container, "要放弃的草稿");

        // 显式放弃：先点入口，再确认。
        const abandonBar = query(container, '[data-route="preparation-abandon"]');
        const buttonByText = (label: string): Element | null =>
          queryAll(abandonBar, '[role="button"]').find((node) => textOf(node).includes(label)) ?? null;
        click(buttonByText("放弃新会话"), "放弃新会话");
        await waitFor(
          () => textOf(abandonBar).includes("无法恢复"),
          "确认前说明后果",
        );
        click(buttonByText("确认放弃"), "确认放弃");
        await waitFor(() => currentPath() === "/", "放弃后回到目录页");

        const release = preparationBodies(sent()).find(
          (body) => body["action"] === "release" && body["sessionId"] === PREPARED_SESSION_ID,
        );
        expectThat(release !== undefined, "放弃必须 release 旧准备的临时资源");
        expectEqual(
          mounted.composition?.store.preparationPage(PREPARED_SESSION_ID) ?? null,
          null,
          "放弃后旧准备退出槽位",
        );
        expectEqual(
          mounted.composition?.store.activePreparationSessionId() ?? null,
          null,
          "放弃后本标签页没有准备槽位",
        );

        // 再新建：同一目录也必须得到**新 ID**，旧草稿不得复活。
        await waitFor(
          () => query(container, `[data-directory-id="${DIRECTORY_ID_ALPHA}"]`) !== null,
          "回到目录页后目录卡片重现",
        );
        click(
          query(query(container, `[data-directory-id="${DIRECTORY_ID_ALPHA}"]`), '[data-action="create-session"]'),
          "放弃后再次新建",
        );
        await waitFor(() => query(container, "[data-create-sheet]") !== null, "创建弹层（第二次）");
        click(query(container, 'button[data-agent-id="codex-a"]'), "选择 Agent（第二次）");
        await waitFor(
          () => query(container, '[data-action="submit-create"]')?.getAttribute("aria-disabled") !== "true",
          "允许提交（第二次）",
        );
        click(query(container, '[data-action="submit-create"]'), "创建会话（第二次）");
        await waitFor(
          () => currentPath() === `/session/${REPLACEMENT_SESSION_ID}`,
          "新目录新建得到新 ID",
        );
        await waitFor(
          () => query(container, "[data-composer-input]") !== null,
          "新准备的输入区就绪",
        );
        await waitFor(
          () => composerTextArea(container)?.value === "",
          "旧草稿不得复活",
        );
        expectEqual(allocations, 2, "两次新建各分配一次（第一次已释放）");
      } finally {
        await mounted.unmount();
      }
    },
  },
];

export const CONVERSATION_RELIABILITY_CHECKS: readonly Check[] = checks;
