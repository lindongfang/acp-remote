/**
 * 对话过程状态（`design.md` D3；spec「对话过程持续可见」）。
 *
 * ## 一个互斥阶段，一条紧凑过程行
 *
 * 对话页同时可能知道好几件事：附件还在上传、命令在等服务端确认、会话正在恢复、当前轮次
 * 没有输出、或者已经有思考/工具/回复。这里把它们归约成**一个**互斥阶段，而不是让页面自己
 * 用一堆布尔值拼「该显示哪一条」。阶段之间的优先级是：
 *
 * 1. `submitting`：本次提交尚未被服务端受理（附件上传/封存或命令待确认）；
 * 2. 实际内容（`thought` / `tool` / `reply` / `attention`）：有真实条目可看时以它为准；
 * 3. `restoring`：会话资源正在恢复（且没有实际内容占屏）；
 * 4. `awaiting`：轮次已受理但还没有任何实际内容。
 *
 * ## 只陈述已知事实
 *
 * `awaiting` 的文案是「等待响应」，MUST NOT 写成「Agent 正在思考」——没有思考事件时客户端
 * 并不知道 Agent 在做什么（spec 明确禁止编造思考正文，也不因一段时间无输出就断言思考）。
 * 上传/封存阶段同样明确「Agent 尚未收到任何内容」。恢复提示只出现在真正处于 `activating`
 * 的会话上；恢复成功（`ready`）后输入区与这里的提示一起消失。
 *
 * ## 只有需要的过程阶段才占行
 *
 * `label` 仅在 `submitting` / `restoring` / `awaiting` 时非空；`thought` / `tool` / `reply` /
 * `attention` / `none` 为 `null`——有实际内容时由思考折叠块、工具卡、回复气泡或交互卡自己
 * 表达，过程行不重复。它出现在时间线末尾（不是顶部重复条），与失败条、取消说明、授权/提问
 * 卡片并列而不相互遮挡。
 */

import type { SentPromptPhase, SentPromptView, TimelineEntry } from "./conversation-timeline";

/** 互斥阶段。只有前三个过程阶段需要额外一行文案。 */
export type ConversationProcessPhase =
  | "none"
  | "submitting"
  | "restoring"
  | "awaiting"
  | "thought"
  | "tool"
  | "reply"
  | "attention";

/** 过程行模型。`label === null` 表示这个阶段不占视觉行。 */
export interface ConversationProcessModel {
  readonly phase: ConversationProcessPhase;
  readonly label: string | null;
  readonly detail: string | null;
}

/** 由实际内容得出的阶段（不占行）。 */
export type ContentPhase = "thought" | "tool" | "reply" | "attention";

/** 一个不占行的阶段。 */
const ABSENT: ConversationProcessModel = { phase: "none", label: null, detail: null };

/** 轮次的终态：到达这些状态后不再有「等待响应」。 */
const TERMINAL_TURN_STATES: Readonly<Record<string, true>> = {
  completed: true,
  failed: true,
  cancelled: true,
};

/** 本地提交中、尚未被服务端受理的阶段（决定过程行怎么描述）。 */
const IN_FLIGHT_PHASES: Readonly<Record<string, true>> = {
  uploading: true,
  sealing: true,
  submitted: true,
};

/** 条目所属的轮次标识（只认可靠标识）。 */
function turnIdOf(entry: TimelineEntry): string | null {
  switch (entry.kind) {
    case "message":
      return entry.message.turnId;
    case "turn":
      return entry.turnId;
    case "tool_call":
      return entry.turnId;
    case "permission_requested":
    case "elicitation_requested":
      return entry.turnId;
    case "pending_interaction":
      return entry.interaction.turnId;
    default:
      return null;
  }
}

/**
 * 条目的「实际过程内容」归类。
 *
 * `attention` 是等待用户处理的卡片（授权/提问）：它已经占了时间线，过程行必须让位，
 * 否则会出现「等待响应」与「请授权」两条并排的重复反馈。
 */
function contentKindOf(entry: TimelineEntry): ContentPhase | null {
  switch (entry.kind) {
    case "message":
      if (entry.channel === "thought") return "thought";
      return entry.channel === "agent" ? "reply" : null;
    case "tool_call":
    case "terminal_output":
    case "plan":
      return "tool";
    case "permission_requested":
    case "elicitation_requested":
    case "pending_interaction":
      return "attention";
    default:
      // 文件改动/模式/配置卡与降级卡不证明 Agent 正在做什么：不参与「等待还是已输出」的判定。
      return null;
  }
}

/** 取最新的一条本地提交（按时间；同一时刻按 requestId 稳定）。 */
function latestPrompt(prompts: readonly SentPromptView[]): SentPromptView | null {
  let latest: SentPromptView | null = null;
  for (const prompt of prompts) {
    if (latest === null || prompt.createdAt >= latest.createdAt) latest = prompt;
  }
  return latest;
}

/**
 * 归约过程状态。
 *
 * @param restoring 当前会话资源是否正在恢复（`activation.kind === "activating"`）。
 * @param prompts 本设备已提交、尚未被权威消息接替的输入。
 * @param activeTurnId 当前活动轮次（来自轮次卡片的非终态事实）；无则为 `null`。
 * @param entries 时间线条目（已排序；本地条目也在里面）。
 */
export function buildConversationProcess(input: {
  readonly restoring: boolean;
  readonly prompts: readonly SentPromptView[];
  readonly activeTurnId: string | null;
  readonly entries: readonly TimelineEntry[];
}): ConversationProcessModel {
  // 1. 本次提交还没被服务端受理：上传/封存/待确认。此时说「等待响应」会假装它已被受理。
  const latest = latestPrompt(input.prompts);
  if (latest !== null && IN_FLIGHT_PHASES[latest.phase] === true) {
    return submittingModel(latest.phase);
  }

  // 2. 当前轮次：先取活动轮次，否则退回**本次本地提交**上的可靠 turnId。
  // 历史消息上的 turnId 缺少配套轮次记录时不能当成「正在运行」：那会让空闲会话永远显示等待。
  let latestLocalTurnId: string | null = null;
  for (const entry of input.entries) {
    if (entry.kind === "message" && entry.channel === "user" && entry.local != null &&
        entry.message.turnId !== null) {
      latestLocalTurnId = entry.message.turnId;
    }
  }
  const turnId = input.activeTurnId ?? latestLocalTurnId;
  if (turnId !== null) {
    // 3. 已有实际内容就以内容为准（最后一条决定当前正在发生什么）。
    let contentKind: ContentPhase | null = null;
    let terminal = false;
    for (const entry of input.entries) {
      if (turnIdOf(entry) !== turnId) continue;
      const kind = contentKindOf(entry);
      if (kind !== null) contentKind = kind;
      if (entry.kind === "turn" && TERMINAL_TURN_STATES[entry.state] === true) terminal = true;
    }
    if (contentKind !== null) {
      return { phase: contentKind, label: null, detail: null };
    }
    // 4. 轮次未终结且没有任何实际内容：这是「等待响应」的唯一依据。
    if (!terminal) {
      return {
        phase: "awaiting",
        label: "等待响应",
        detail: "消息已受理，Agent 尚未返回内容。",
      };
    }
  }

  // 5. 恢复中（且没有实际内容占屏）：恢复完成即消失，不另留说明。
  if (input.restoring) {
    return {
      phase: "restoring",
      label: "会话恢复中",
      detail: "恢复完成前不会发送新消息；历史可以照常阅读。",
    };
  }
  return ABSENT;
}

/** 提交中阶段的文案：只描述已知事实（上传/封存/待确认），不假装 Agent 已收到。 */
function submittingModel(phase: SentPromptPhase): ConversationProcessModel {
  if (phase === "uploading") {
    return {
      phase: "submitting",
      label: "附件上传中",
      detail: "Agent 尚未收到任何内容；全部附件封存后才会提交这一条。",
    };
  }
  if (phase === "sealing") {
    return {
      phase: "submitting",
      label: "附件封存中",
      detail: "Agent 尚未收到任何内容；封存完成后才会提交这一条。",
    };
  }
  return { phase: "submitting", label: "已提交，等待服务端确认", detail: null };
}
