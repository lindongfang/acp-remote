/**
 * 新建会话弹层（R16「会话创建是受限入口」）。
 *
 * ## 四条 MUST 的渲染落点
 *
 * 1. **载荷只含两个引用**：本组件没有路径、MCP 配置、凭据的任何输入框；Agent 候选来自
 *    `model.agents`（本机已配置的 Agent 目录），不是自由文本。
 * 2. **无权限时入口不可用且说明原因**：`data-create-permission="denied"` 的分支**照常渲染
 *    提交按钮**（`disabled`）并把说明摆在旁边——只隐藏按钮不作说明是不允许的。
 * 3. **创建中与终态分别呈现**：`creating` 分支显示「创建中」且按钮禁用（不接受重复提交）。
 * 4. **结果不确定显眼且不被自动消解**：`uncertain` 分支是一个独立的、醒目的区块，
 *    它只由用户点击「我知道了」才消失。
 */

import type { ReactElement } from "react";

import type { CreateSessionEntryModel, CreateSubmissionModel } from "../features/create-session-model";
import { isSubmissionAvailable } from "../features/create-session-model";
import { UNKNOWN_CONNECTION_LABEL } from "../domain";

export interface CreateSessionSheetProps {
  readonly model: CreateSessionEntryModel;
  /** 当前选中的 Agent 标识；`null` 表示尚未选择（此时不允许提交）。 */
  readonly selectedAgentId: string | null;
  readonly onSelectAgent: (agentId: string) => void;
  readonly onSubmit: (agentId: string) => void;
  readonly onDismissUncertain: () => void;
  readonly onClose: () => void;
}

/** 提交状态区块。 */
function Submission({
  submission,
  onDismissUncertain,
}: {
  readonly submission: CreateSubmissionModel;
  readonly onDismissUncertain: () => void;
}): ReactElement | null {
  if (submission.kind === "idle") return null;
  if (submission.kind === "creating") {
    return (
      <p data-create-submission="creating" role="status">
        创建中……正在等待服务器确认。
      </p>
    );
  }
  if (submission.kind === "uncertain") {
    return (
      <div data-create-submission="uncertain" role="alert">
        <strong>结果不确定</strong>
        <p>服务器无法确认这次创建是否已经生效。它不会自动消失，也不会随重连清除。</p>
        <button type="button" data-action="dismiss-uncertain" onClick={onDismissUncertain}>
          我知道了
        </button>
      </div>
    );
  }
  if (submission.kind === "created") {
    return (
      <p data-create-submission="created" data-created-session-id={submission.sessionId}>
        会话已创建。
      </p>
    );
  }
  if (submission.kind === "failed") {
    return (
      <p data-create-submission="failed" data-failure-code={submission.error.code} data-failure-retryable={String(submission.error.retryable)}>
        {submission.error.message}
      </p>
    );
  }
  return <p data-create-submission="accepted">{submission.acceptedAt === null ? "已接受" : `已接受（${submission.acceptedAt}）`}</p>;
}

/** 新建会话弹层。 */
export function CreateSessionSheet({
  model,
  selectedAgentId,
  onSelectAgent,
  onSubmit,
  onDismissUncertain,
  onClose,
}: CreateSessionSheetProps): ReactElement {
  const denied = model.permission.kind === "denied";
  const disabled = denied || selectedAgentId === null || !isSubmissionAvailable(model.submission);

  return (
    <section data-create-sheet={model.directoryAlias} data-create-permission={model.permission.kind}>
      <header>
        <h2>在「{model.directoryAlias}」里新建会话</h2>
        <button type="button" data-action="close-create-sheet" onClick={onClose}>
          关闭
        </button>
      </header>

      <p data-create-note="true">{model.permission.note}</p>
      {denied ? <p data-create-denied-reason="true">{model.permission.reason}</p> : null}

      <ul data-agent-options="true">
        {model.agents.map((agent) => (
          <li key={agent.id}>
            <button
              type="button"
              data-action="select-agent"
              data-agent-id={agent.id}
              data-selected={String(agent.id === selectedAgentId)}
              onClick={() => {
                onSelectAgent(agent.id);
              }}
            >
              {/* 同名 Agent 以标识区分：展示名之外必须能看到 id。 */}
              <span data-agent-display-name={agent.displayName}>{agent.displayName}</span>
              <span data-agent-id-label>{agent.id}</span>
              {agent.isDefault ? <span data-agent-default="true">默认</span> : null}
              <span data-agent-connection={agent.connection}>
                {agent.connection === "unknown" ? UNKNOWN_CONNECTION_LABEL : agent.connection}
              </span>
            </button>
          </li>
        ))}
      </ul>

      <Submission submission={model.submission} onDismissUncertain={onDismissUncertain} />

      <button
        type="button"
        data-action="submit-create"
        disabled={disabled}
        onClick={() => {
          if (selectedAgentId !== null) onSubmit(selectedAgentId);
        }}
      >
        创建会话
      </button>
    </section>
  );
}
