import type {
  SessionPriorityFailure,
  SessionQueueEntry,
  SessionQueueSnapshot,
  SessionStopReason,
} from "./session-protocol";

export interface SessionQueueViewOptions {
  readonly writeDisabled?: boolean;
  readonly onResume?: (button: HTMLButtonElement) => void | Promise<void>;
  readonly onEdit?: (entry: SessionQueueEntry) => void;
  readonly onCancel?: (
    entry: SessionQueueEntry,
    button: HTMLButtonElement,
  ) => void | Promise<void>;
  readonly onPrioritize?: (
    entry: SessionQueueEntry,
    button: HTMLButtonElement,
  ) => void | Promise<void>;
  readonly onOpenBody?: (
    entry: SessionQueueEntry,
    button: HTMLButtonElement,
  ) => void | Promise<void>;
}

export function sessionQueueSnapshotView(
  snapshot: SessionQueueSnapshot,
  options: SessionQueueViewOptions = {},
): DocumentFragment {
  const fragment = document.createDocumentFragment();
  if (snapshot.stopReason !== null) {
    fragment.append(
      statusLine(
        `执行已停止：${stopReasonLabel(snapshot.stopReason)}。旧队列已暂停，成功的新输入不会自动恢复旧项。`,
        "error",
      ),
    );
    fragment.append(
      resumeButton(snapshot.targetId, options.onResume, options.writeDisabled),
    );
  } else if (snapshot.priorityFailure !== null) {
    fragment.append(
      statusLine(priorityFailureLabel(snapshot.priorityFailure), "error"),
    );
    fragment.append(
      resumeButton(snapshot.targetId, options.onResume, options.writeDisabled),
    );
  } else if (!snapshot.failureSuspended) {
    fragment.append(statusLine("当前没有待继续的已停止队列。", "notice"));
  }

  fragment.append(
    queueEntryList("正在执行", snapshot.active === null ? [] : [snapshot.active], options),
    queueEntryList("等待队列", snapshot.waiting, options),
  );
  if (snapshot.priority !== null) {
    fragment.append(
      statusLine(`已选择优先项 ${snapshot.priority}；它仍需满足前台互斥。`, "notice"),
    );
  }
  if (snapshot.processing !== null) {
    fragment.append(
      statusLine(
        `队列管理操作 ${snapshot.processing.phase}，registration ${snapshot.processing.registrationId}。`,
        "working",
      ),
    );
  }
  return fragment;
}

function resumeButton(
  targetId: string,
  onResume: SessionQueueViewOptions["onResume"],
  writeDisabled = false,
): HTMLButtonElement {
  const resume = button("继续旧队列");
  resume.dataset.sessionAction = "resume";
  resume.dataset.targetId = targetId;
  if (onResume === undefined || writeDisabled) {
    resume.disabled = true;
  } else {
    resume.addEventListener("click", () => {
      resume.disabled = true;
      void onResume(resume);
    });
  }
  return resume;
}

function queueEntryList(
  heading: string,
  entries: readonly SessionQueueEntry[],
  options: SessionQueueViewOptions = {},
): HTMLElement {
  const section = element("section");
  section.append(element("h4", undefined, heading));
  if (entries.length === 0) {
    section.append(paragraph("无", "empty-state"));
    return section;
  }
  for (const entry of entries) {
    const card = element("article", "device-card");
    card.dataset.queueEntry = entry.itemId;
    card.append(
      element("h4", undefined, entry.itemId),
      detailBlock("revision", entry.revision),
      detailBlock("提交客户端", entry.submitterId),
      detailBlock("编辑客户端", entry.editorId),
      statusLine(
        entry.suspended ? "已暂停，需显式继续" : "可调度",
        entry.suspended ? "error" : "notice",
      ),
    );
    const actions = element("div", "action-row");
    if (options.onOpenBody !== undefined) {
      actions.append(
        entryAction("读取正文", "body", entry, options.onOpenBody),
      );
    }
    if (options.onEdit !== undefined) {
      const edit = button("编辑");
      edit.dataset.sessionAction = "edit";
      edit.dataset.queueEntry = entry.itemId;
      edit.disabled = options.writeDisabled === true;
      edit.addEventListener("click", () => options.onEdit?.(entry));
      actions.append(edit);
    }
    if (options.onPrioritize !== undefined) {
      actions.append(
        entryAction(
          "立即发送",
          "prioritize",
          entry,
          options.onPrioritize,
          undefined,
          options.writeDisabled === true,
        ),
      );
    }
    if (options.onCancel !== undefined) {
      actions.append(
        entryAction(
          "取消",
          "cancel",
          entry,
          options.onCancel,
          "danger-button",
          options.writeDisabled === true,
        ),
      );
    }
    if (actions.children.length > 0) {
      card.append(actions);
    }
    section.append(card);
  }
  return section;
}

function entryAction(
  text: string,
  action: "body" | "prioritize" | "cancel",
  entry: SessionQueueEntry,
  handler: (
    entry: SessionQueueEntry,
    button: HTMLButtonElement,
  ) => void | Promise<void>,
  className?: string,
  disabled = false,
): HTMLButtonElement {
  const value = button(text);
  if (className !== undefined) {
    value.className = className;
  }
  value.disabled = disabled;
  value.dataset.sessionAction = action;
  value.dataset.queueEntry = entry.itemId;
  value.addEventListener("click", () => {
    value.disabled = true;
    void handler(entry, value);
  });
  return value;
}

export function stopReasonLabel(reason: SessionStopReason): string {
  switch (reason) {
    case "max_tokens":
      return "达到 token 上限";
    case "max_turn_requests":
      return "达到请求轮数上限";
    case "refusal":
      return "模型拒绝";
    case "cancelled":
      return "用户取消";
    case "error":
      return "执行错误";
    case "unknown":
      return "停止原因关联未确认";
    case "end_turn":
      return "本轮正常结束";
  }
}

export function priorityFailureLabel(reason: SessionPriorityFailure): string {
  switch (reason) {
    case "selection_changed":
      return "已选择立即发送的旧项已变更或失效；未回退发送其他输入，余项保持暂停，请确认后继续旧队列。";
    case "interrupt_unconfirmed":
      return "中断结果未确认；未派发优先项或回退其他输入，余项保持暂停，请核验后继续旧队列。";
  }
}

function paragraph(text: string, className?: string): HTMLElement {
  return element("p", className, text);
}

function statusLine(
  text: string,
  tone: "working" | "notice" | "error",
): HTMLElement {
  const line = element("p", `status-line status-${tone}`);
  line.setAttribute("role", tone === "error" ? "alert" : "status");
  const marker = element("span", "status-marker");
  marker.setAttribute("aria-hidden", "true");
  line.append(marker, document.createTextNode(text));
  return line;
}

function detailBlock(label: string, value: string): HTMLElement {
  const block = element("div", "detail-block");
  block.append(
    element("span", "detail-label", label),
    element("code", undefined, value),
  );
  return block;
}

function button(text: string): HTMLButtonElement {
  const value = element("button", undefined, text) as HTMLButtonElement;
  value.type = "button";
  return value;
}

function element<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  className?: string,
  text?: string,
): HTMLElementTagNameMap[K] {
  const value = document.createElement(tag);
  if (className !== undefined) {
    value.className = className;
  }
  if (text !== undefined) {
    value.textContent = text;
  }
  return value;
}
