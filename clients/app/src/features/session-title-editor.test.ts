/**
 * 会话标题的纯状态机（`design.md` D4）。
 *
 * ## 判别力
 *
 * - **未命名不是真实输入**：`null`/空白有效标题只能得到空草稿，保存空输入必须被拒绝；
 * - **长度按输入、trim 后为空拒绝**：512 个标量值通过、513 个拒绝，且不截断；
 * - **composition 优先于 Enter**：`isComposing`/229 期间按忽略处理；
 * - **同值错误不重复失焦重试**：失败的提交在失焦/离页路径上不再派发（显式 Enter 才重试）；
 * - **终态 + 权威摘要**：标题相等或版本更新才算确认，提交时没有摘要时不靠版本猜测。
 */

import { describe, expect, it } from "vitest";

import {
  SESSION_TITLE_DOUBLE_CLICK_WINDOW_MS,
  SESSION_TITLE_MAX_SCALARS,
  beginTitleEditing,
  editTitleDraft,
  initialSessionTitleEditorState,
  isSessionTitleDoubleClick,
  markTitleEditError,
  markTitleSubmitting,
  sessionTitleKeyAction,
  sessionTitleScalarCount,
  sessionTitleVersionAdvanced,
  shouldAttemptTitleCommit,
  titleCommitReflected,
  validateSessionTitle,
} from "./session-title-editor";
import type { SessionTitlePendingCommit } from "./session-title-editor";

const SESSION_ID = "aaaaaaaa-0000-4000-8000-000000000001";
const REQUEST_ID = "00000000-0000-4000-8000-000000000001";

describe("标题校验", () => {
  it("保存值是 trim 后的文本", () => {
    expect(validateSessionTitle("  排查登录失败  ")).toEqual({ ok: true, title: "排查登录失败" });
  });

  it("trim 后为空拒绝（未命名占位不是真实输入）", () => {
    const result = validateSessionTitle("   \t\n ");
    expect(result.ok).toBe(false);
    if (!result.ok) expect(result.reason).toContain("不能为空");
    expect(validateSessionTitle("").ok).toBe(false);
  });

  it("按 Unicode 标量值计：512 通过、513 拒绝且不截断", () => {
    expect(sessionTitleScalarCount("😀")).toBe(1);
    expect(validateSessionTitle("😀".repeat(SESSION_TITLE_MAX_SCALARS)).ok).toBe(true);
    const tooLong = validateSessionTitle("😀".repeat(SESSION_TITLE_MAX_SCALARS + 1));
    expect(tooLong.ok).toBe(false);
    if (!tooLong.ok) expect(tooLong.reason).toContain(String(SESSION_TITLE_MAX_SCALARS));
  });

  it("长度按输入（trim 前）计：512 个字符两侧带空格同样超限", () => {
    const result = validateSessionTitle(`${"a".repeat(SESSION_TITLE_MAX_SCALARS)} `);
    expect(result.ok).toBe(false);
  });
});

describe("双击与按键", () => {
  it("只有时间窗内的第二次点击才算双击", () => {
    expect(isSessionTitleDoubleClick(null, 1000)).toBe(false);
    expect(isSessionTitleDoubleClick(1000, 1000)).toBe(true);
    expect(isSessionTitleDoubleClick(1000, 1000 + SESSION_TITLE_DOUBLE_CLICK_WINDOW_MS)).toBe(true);
    expect(isSessionTitleDoubleClick(1000, 1000 + SESSION_TITLE_DOUBLE_CLICK_WINDOW_MS + 1)).toBe(false);
    // 时钟回退不构成双击。
    expect(isSessionTitleDoubleClick(2000, 1000)).toBe(false);
  });

  it("composition 期间 Enter 不提交；组合结束后才提交；Escape 取消", () => {
    expect(sessionTitleKeyAction({ key: "Enter", isComposing: true })).toBe("ignore");
    expect(sessionTitleKeyAction({ key: "Enter", keyCode: 229 })).toBe("ignore");
    expect(sessionTitleKeyAction({ key: "Enter", isComposing: false })).toBe("submit");
    expect(sessionTitleKeyAction({ key: "Escape" })).toBe("cancel");
    expect(sessionTitleKeyAction({ key: "a" })).toBe("ignore");
  });
});

describe("编辑阶段", () => {
  it("旧标题只作为占位；未命名/空白得到空占位（占位不是真实输入）", () => {
    const named = beginTitleEditing(initialSessionTitleEditorState(), {
      sessionId: SESSION_ID,
      effectiveTitle: "既定标题",
      unavailableReason: null,
    });
    expect(named.phase).toBe("editing");
    expect(named.draft).toBe("");
    expect(named.originalTitle).toBe("既定标题");

    const unnamed = beginTitleEditing(initialSessionTitleEditorState(), {
      sessionId: SESSION_ID,
      effectiveTitle: null,
      unavailableReason: null,
    });
    expect(unnamed.draft).toBe("");
    expect(unnamed.originalTitle).toBeNull();
  });

  it("不可用时只记录原因，从未进入可保存状态", () => {
    const state = beginTitleEditing(initialSessionTitleEditorState(), {
      sessionId: SESSION_ID,
      effectiveTitle: "既定标题",
      unavailableReason: "当前离线",
    });
    expect(state.phase).toBe("viewing");
    expect(state.unavailableReason).toBe("当前离线");
    expect(state.draft).toBe("");
  });

  it("提交中不再接受第二个保存意图；同值错误后的失焦/离页不重试，Enter 可重试", () => {
    const editing = beginTitleEditing(initialSessionTitleEditorState(), {
      sessionId: SESSION_ID,
      effectiveTitle: "旧标题",
      unavailableReason: null,
    });
    expect(shouldAttemptTitleCommit(editing, SESSION_ID, "blur")).toBe(true);

    // 真实顺序：先在编辑阶段改草稿，再提交；提交中 `editTitleDraft` 是 no-op。
    const submitting = markTitleSubmitting(editTitleDraft(editing, SESSION_ID, "新标题"), {
      sessionId: SESSION_ID,
      requestId: REQUEST_ID,
      title: "新标题",
    });
    expect(submitting.phase).toBe("submitting");
    expect(shouldAttemptTitleCommit(submitting, SESSION_ID, "enter")).toBe(false);
    expect(shouldAttemptTitleCommit(submitting, SESSION_ID, "blur")).toBe(false);

    const failed = markTitleEditError(editTitleDraft(submitting, SESSION_ID, "新标题"), {
      sessionId: SESSION_ID,
      reason: "失败",
      requestId: null,
      submittedTitle: "新标题",
    });
    expect(shouldAttemptTitleCommit(failed, SESSION_ID, "blur")).toBe(false);
    expect(shouldAttemptTitleCommit(failed, SESSION_ID, "navigation")).toBe(false);
    expect(shouldAttemptTitleCommit(failed, SESSION_ID, "enter")).toBe(true);
  });

  it("版本比较解析失败时不强行退出", () => {
    expect(sessionTitleVersionAdvanced("2", "1")).toBe(true);
    expect(sessionTitleVersionAdvanced("1", "1")).toBe(false);
    expect(sessionTitleVersionAdvanced("not-a-number", "1")).toBe(false);
    expect(sessionTitleVersionAdvanced("2", null)).toBe(false);
  });

  it("权威摘要：标题相等或版本更新才算确认；提交时没有摘要只认标题相等", () => {
    const commit: SessionTitlePendingCommit = {
      requestId: REQUEST_ID,
      sessionId: SESSION_ID,
      title: "保存值",
      baseVersion: "3",
      leftPage: false,
      terminal: "completed",
    };
    expect(titleCommitReflected(commit, { title: "保存值", version: "3" })).toBe(true);
    expect(titleCommitReflected(commit, { title: "别的值", version: "4" })).toBe(true);
    expect(titleCommitReflected(commit, { title: "别的值", version: "3" })).toBe(false);
    expect(titleCommitReflected({ ...commit, baseVersion: null }, { title: "别的值", version: "9" })).toBe(false);
    expect(titleCommitReflected({ ...commit, baseVersion: null }, { title: "保存值", version: "9" })).toBe(true);
  });
});
