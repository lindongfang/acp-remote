/**
 * 会话标题的原地编辑状态机（`design.md` D4 / `pwa-web-client`「对话页顶部标题支持编辑」）。
 *
 * ## 只有四个阶段，且按 `sessionId` 绑定
 *
 * `viewing`（只读）→ `editing`（双击进入）→ `submitting`（已派发、等终态）→
 * `editing-error`（失败后保留输入）；取消回到 `viewing`。状态里始终记录它属于哪个会话，
 * 迟到的命令结果只在与 `requestId`、`sessionId` 同时对上时才改变编辑器——换一个会话或换了
 * 一次提交之后，旧结果不再改当前输入。
 *
 * ## 判定全部是纯函数
 *
 * 校验（trim 后非空、trim 前不超过 512 个 Unicode 标量值）、双击时间窗、Enter/Escape 与
 * composition 的按键动作、命令终态与权威摘要的会合，都不读时钟、不读浏览器事件，由仓库与
 * 组件分别喂入事实。这样行为测试不需要模拟 DOM，也不会出现「组件里还有第二份判定」。
 *
 * ## 「提交」不等于「保存成功」
 *
 * `accepted` 不退出编辑；只有 `completed` **且**当前权威摘要已反映这次保存（标题等于保存值，
 * 或版本比提交时更新）才退出。`failed`/`rejected` 保留输入并给出原因；`uncertain` 也保留，
 * 并在重连后用原 `requestId` 走 `command.status`，不换号重发。
 */

import type { Uuid } from "../protocol";
import { UNTITLED_SESSION_LABEL } from "../domain";

/** 标题上限：输入（trim 前）的 Unicode 标量值个数。 */
export const SESSION_TITLE_MAX_SCALARS = 512;

/** 未命名占位：它不是真实输入，编辑框为空时才显示。 */
export const SESSION_TITLE_PLACEHOLDER = UNTITLED_SESSION_LABEL;

/**
 * 双击判定窗口（毫秒）。
 *
 * 用两次点击的时间窗而不是浏览器 `dblclick` 事件：PWA/触屏与 React Native 的 Pressable
 * 只有点击，没有统一的 dblclick；把判定放在这里，组件与测试用的是同一份规则。
 */
export const SESSION_TITLE_DOUBLE_CLICK_WINDOW_MS = 400;

/** 编辑器的四个阶段（`design.md` D4）。 */
export type SessionTitleEditPhase = "viewing" | "editing" | "submitting" | "editing-error";

/** 一次提交的意图来源：Enter、失焦与离页。 */
export type SessionTitleCommitReason = "enter" | "blur" | "navigation";

/** 标题编辑器的内存状态（按 `sessionId` 绑定；不落盘，刷新后只有当前标题）。 */
export interface SessionTitleEditorState {
  readonly sessionId: Uuid | null;
  readonly phase: SessionTitleEditPhase;
  readonly draft: string;
  /**
   * 进入编辑时的有效标题（灰色占位，不是真实输入）；`null` = 未命名。
   *
   * 它只用于展示与「不输入则保留旧标题」的判定，不随其他设备更新而改写。
   */
  readonly originalTitle: string | null;
  /** 本次提交的稳定请求标识；只有提交中/失败重试时存在。 */
  readonly requestId: Uuid | null;
  /** 本次实际提交的保存值（trim 后）；用于「同值错误不重复失焦重试」。 */
  readonly submittedTitle: string | null;
  /** 本地拒绝或命令失败的原因；`editing-error` 时呈现，重新输入即收起。 */
  readonly error: string | null;
  /** 双击时明确不可用的原因（离线/未协商）；非空时从未进入可保存状态。 */
  readonly unavailableReason: string | null;
}

/** 一条已派发、尚未以「终态 + 权威摘要」收束的标题命令。 */
export interface SessionTitlePendingCommit {
  readonly requestId: Uuid;
  readonly sessionId: Uuid;
  /** trim 后的保存值。 */
  readonly title: string;
  /** 提交时对应摘要的版本（用于识别更晚的权威摘要）；提交时没有摘要为 `null`。 */
  readonly baseVersion: string | null;
  /** 用户已离开页面：失败只记可归因提示，不恢复输入、不跳回。 */
  readonly leftPage: boolean;
  /** `completed` = 终态已到、等待摘要；`uncertain` = 结果未知，重连后查询原请求。 */
  readonly terminal: "completed" | "uncertain" | null;
}

/** 组件消费的编辑器视图；`null` = 不呈现任何标题编辑/提示界面。 */
export interface SessionTitleEditorView {
  readonly sessionId: Uuid;
  readonly phase: SessionTitleEditPhase;
  readonly draft: string;
  readonly placeholder: string;
  readonly error: string | null;
  readonly notice: string | null;
  /** 提交期间不再接受第二个保存意图。 */
  readonly submitting: boolean;
}

export function initialSessionTitleEditorState(): SessionTitleEditorState {
  return {
    sessionId: null,
    phase: "viewing",
    draft: "",
    originalTitle: null,
    requestId: null,
    submittedTitle: null,
    error: null,
    unavailableReason: null,
  };
}

/** Unicode 标量值个数（与 `for…of` 的码点语义一致，不是 UTF-16 长度）。 */
export function sessionTitleScalarCount(text: string): number {
  let count = 0;
  for (const scalar of text) {
    // 迭代按码点切分；每个切片的长度 ≥ 1（代理对在同一个切片里）。
    if (scalar.length > 0) count += 1;
  }
  return count;
}

/** 校验结果：`ok` 时给出**要发送的** trim 后文本。 */
export type SessionTitleValidation =
  | { readonly ok: true; readonly title: string }
  | { readonly ok: false; readonly reason: string };

/**
 * 校验输入。
 *
 * 长度按**输入（trim 前）**计：超长不截断、直接拒绝；trim 后为空同样拒绝。保存值是 trim 后的
 * 文本（与服务端一致）。
 */
export function validateSessionTitle(draft: string): SessionTitleValidation {
  if (sessionTitleScalarCount(draft) > SESSION_TITLE_MAX_SCALARS) {
    return {
      ok: false,
      reason: `标题最多 ${SESSION_TITLE_MAX_SCALARS} 个字符（按 Unicode 标量值计），不会自动截断。`,
    };
  }
  const title = draft.trim();
  if (title.length === 0) {
    return { ok: false, reason: "标题不能为空（只输入空白不算标题）。" };
  }
  return { ok: true, title };
}

/** 两次点击是否构成一次双击。 */
export function isSessionTitleDoubleClick(previousAtMs: number | null, nowMs: number): boolean {
  if (previousAtMs === null) return false;
  const elapsed = nowMs - previousAtMs;
  return elapsed >= 0 && elapsed <= SESSION_TITLE_DOUBLE_CLICK_WINDOW_MS;
}

/** `onKeyPress` 的宿主事件里与标题编辑有关的字段。 */
export interface SessionTitleKeyLike {
  readonly key?: string;
  /** 输入法正在组词（宿主 keydown 上的 `isComposing`）。 */
  readonly isComposing?: boolean;
  /** 部分浏览器在组词期间给出 229。 */
  readonly keyCode?: number;
}

export type SessionTitleKeyAction = "ignore" | "submit" | "cancel";

/**
 * 按键动作：composition 期间的 Enter 不提交；Enter 提交、Escape 取消，其余忽略。
 *
 * `keyCode === 229` 与 `isComposing` 是同一件事的两种宿主表达，任一条成立都按组词处理。
 */
export function sessionTitleKeyAction(key: SessionTitleKeyLike): SessionTitleKeyAction {
  if (key.isComposing === true || key.keyCode === 229) return "ignore";
  if (key.key === "Escape") return "cancel";
  if (key.key === "Enter") return "submit";
  return "ignore";
}

/** 是否应当为这次意图尝试派发保存（不判断输入是否合法）。 */
export function shouldAttemptTitleCommit(
  editor: SessionTitleEditorState,
  sessionId: Uuid,
  reason: SessionTitleCommitReason,
): boolean {
  if (editor.sessionId !== sessionId) return false;
  if (editor.phase !== "editing" && editor.phase !== "editing-error") return false;
  // 同值错误后的重复失焦/离页不再派发；显式 Enter 仍允许用户重试。
  if (
    reason !== "enter" &&
    editor.phase === "editing-error" &&
    editor.submittedTitle !== null &&
    editor.draft.trim() === editor.submittedTitle
  ) {
    return false;
  }
  return true;
}

/**
 * 进入编辑。
 *
 * `effectiveTitle` 是**服务端下发的有效标题原值**（不是展示文案）：`null`/空白表示未命名。
 * 输入框始终为空，旧标题只作为灰色占位；不输入直接确认时保留原值、不派发命令。不可用时只
 * 记录原因，阶段仍是 `viewing`——从未进入可保存状态。
 */
export function beginTitleEditing(
  editor: SessionTitleEditorState,
  input: {
    readonly sessionId: Uuid;
    readonly effectiveTitle: string | null;
    readonly unavailableReason: string | null;
  },
): SessionTitleEditorState {
  // 同一会话的再次双击不重置草稿；提交中也不重新进入。
  if (editor.sessionId === input.sessionId && editor.phase !== "viewing") return editor;
  if (input.unavailableReason !== null) {
    return {
      ...initialSessionTitleEditorState(),
      sessionId: input.sessionId,
      unavailableReason: input.unavailableReason,
    };
  }
  // 旧标题只作为灰色占位，不预填成真实输入：用户不输入时保留原值，而不是把旧标题
  // 当成一次「保存」再落库。
  return {
    sessionId: input.sessionId,
    phase: "editing",
    draft: "",
    originalTitle: input.effectiveTitle !== null && input.effectiveTitle.trim().length > 0
      ? input.effectiveTitle
      : null,
    requestId: null,
    submittedTitle: null,
    error: null,
    unavailableReason: null,
  };
}

/** 更新草稿；一旦修改就离开上一次失败态（错误随之收起）。 */
export function editTitleDraft(
  editor: SessionTitleEditorState,
  sessionId: Uuid,
  draft: string,
): SessionTitleEditorState {
  if (editor.sessionId !== sessionId) return editor;
  if (editor.phase !== "editing" && editor.phase !== "editing-error") return editor;
  if (editor.phase === "editing" && editor.draft === draft) return editor;
  // 改了文字就是一次新的意图：不再引用上一次失败的请求（它的迟到结果只作为可归因提示）。
  return {
    ...editor,
    phase: "editing",
    draft,
    requestId: null,
    submittedTitle: null,
    error: null,
  };
}

/** Escape：退出编辑并丢弃草稿；提交中不撤销已发出的命令。 */
export function cancelTitleEditing(
  editor: SessionTitleEditorState,
  sessionId: Uuid,
): SessionTitleEditorState {
  if (editor.sessionId !== sessionId) return editor;
  if (editor.phase !== "editing" && editor.phase !== "editing-error") return editor;
  return {
    ...editor,
    phase: "viewing",
    draft: "",
    originalTitle: null,
    requestId: null,
    submittedTitle: null,
    error: null,
  };
}

/** 已派发：绑定原会话与 requestId，进入提交中。 */
export function markTitleSubmitting(
  editor: SessionTitleEditorState,
  input: {
    readonly sessionId: Uuid;
    readonly requestId: Uuid;
    readonly title: string;
  },
): SessionTitleEditorState {
  if (editor.sessionId !== input.sessionId) return editor;
  return {
    ...editor,
    phase: "submitting",
    requestId: input.requestId,
    submittedTitle: input.title,
    error: null,
    unavailableReason: null,
  };
}

/**
 * 本地拒绝（非法输入/离线/未协商）：保留输入框与输入，显示原因。
 *
 * `requestId` 与 `submittedTitle` 由调用方按场景给出，其余字段保持不变。
 */
export function markTitleEditError(
  editor: SessionTitleEditorState,
  input: {
    readonly sessionId: Uuid;
    readonly reason: string;
    readonly requestId?: Uuid | null;
    readonly submittedTitle?: string | null;
  },
): SessionTitleEditorState {
  if (editor.sessionId !== input.sessionId) return editor;
  const requestId = input.requestId === undefined ? editor.requestId : input.requestId;
  const submittedTitle = input.submittedTitle === undefined ? editor.submittedTitle : input.submittedTitle;
  // 同一非法输入/同一失败原因的重复失焦不改变状态（也就不会触发新的提交意图）。
  if (
    editor.phase === "editing-error" &&
    editor.error === input.reason &&
    editor.requestId === requestId &&
    editor.submittedTitle === submittedTitle
  ) {
    return editor;
  }
  return {
    ...editor,
    phase: "editing-error",
    requestId,
    submittedTitle,
    error: input.reason,
  };
}

/** 放弃输入（离页）：不保存跨页草稿，阶段回到只读。 */
export function abandonTitleEditing(
  editor: SessionTitleEditorState,
  sessionId: Uuid,
): SessionTitleEditorState {
  if (editor.sessionId !== sessionId) return editor;
  return {
    ...editor,
    phase: "viewing",
    draft: "",
    originalTitle: null,
    requestId: null,
    submittedTitle: null,
    error: null,
  };
}

/** `DecimalString` 版本是否比提交时更新；解析失败按「没有更新」处理，不强行退出。 */
export function sessionTitleVersionAdvanced(current: string, base: string | null): boolean {
  if (base === null) return false;
  try {
    return BigInt(current) > BigInt(base);
  } catch {
    return false;
  }
}

/**
 * 权威摘要是否已反映这次保存。
 *
 * 两条独立证据：标题等于保存值（同值保存不会推进版本），或摘要版本比提交时更新。
 * 提交时没有摘要（`baseVersion === null`）时只认标题相等，避免把更旧的在途快照当确认。
 */
export function titleCommitReflected(
  commit: SessionTitlePendingCommit,
  summary: { readonly title: string | null; readonly version: string },
): boolean {
  if (summary.title !== null && summary.title.length > 0 && summary.title === commit.title) return true;
  return sessionTitleVersionAdvanced(summary.version, commit.baseVersion);
}

/**
 * 构造组件视图；编辑器不属于当前会话、且没有可归因提示时为 `null`（不渲染编辑界面）。
 */
export function buildSessionTitleEditorView(input: {
  readonly sessionId: Uuid;
  readonly editor: SessionTitleEditorState;
  readonly notice: string | null;
}): SessionTitleEditorView | null {
  const belongs = input.editor.sessionId === input.sessionId;
  const open = belongs && input.editor.phase !== "viewing";
  const notice = input.notice ?? (belongs ? input.editor.unavailableReason : null);
  if (!open) {
    if (notice === null) return null;
    return {
      sessionId: input.sessionId,
      phase: "viewing",
      draft: "",
      placeholder: SESSION_TITLE_PLACEHOLDER,
      error: null,
      notice,
      submitting: false,
    };
  }
  return {
    sessionId: input.sessionId,
    phase: input.editor.phase,
    draft: input.editor.draft,
    // 旧标题以灰色占位呈现；未命名才用「未命名会话」文案。
    placeholder: input.editor.originalTitle ?? SESSION_TITLE_PLACEHOLDER,
    error: input.editor.error,
    notice: null,
    submitting: input.editor.phase === "submitting",
  };
}
