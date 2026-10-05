/**
 * 目录详情页（R15）。
 *
 * 目录详情呈现「这个目录里的会话」与「在这个目录里新建会话」的入口（后者受 R16 的权限闸门
 * 控制）。别名失效与「目录存在但没有会话」是两种不同的呈现：前者说明链接已失效，
 * 后者说明目录是好的、只是还没有会话。
 */

import type { ReactElement } from "react";

import type { DirectoryDetailModel } from "../features/directory-model";
import { CONTENT_ORIGIN_LABELS } from "../features/directory-model";
import type { CreateSessionEntryModel } from "../features/create-session-model";
import { StatusPill } from "./StatusPill";

export interface DirectoryDetailProps {
  readonly model: DirectoryDetailModel;
  readonly createEntry: CreateSessionEntryModel;
  readonly onOpenSession: (sessionId: string) => void;
  readonly onCreateSession: () => void;
  readonly canCreate: boolean;
}

/** 目录详情。 */
export function DirectoryDetail({
  model,
  createEntry,
  onOpenSession,
  onCreateSession,
  canCreate,
}: DirectoryDetailProps): ReactElement {
  if (model.kind === "loading") {
    return (
      <section data-route="dir-detail">
        <p data-directory-detail-state="loading">尚未收到第一份快照，正在等待同步……</p>
      </section>
    );
  }

  if (model.kind === "unknown_directory") {
    return (
      <section data-route="dir-detail" data-directory-alias={model.alias}>
        <p data-directory-detail-state="unknown_directory" role="alert">
          目录「{model.alias}」不在最近一次同步的目录列表里。它可能已被删除，或这个链接来自更早的一次同步。
        </p>
      </section>
    );
  }

  return (
    <section data-route="dir-detail" data-directory-alias={model.directory.alias}>
      <header>
        {/* 目录项仍然只有展示名与别名：没有路径，也无处可填。 */}
        <h1 data-directory-display-name={model.directory.displayName}>{model.directory.displayName}</h1>
        <span data-directory-alias-label>{model.directory.alias}</span>
        <span data-directory-count="total">{model.directory.counts.total}</span>
        <span data-directory-count="active">{model.directory.counts.active}</span>
        <span data-directory-count="pending">{model.directory.counts.pending}</span>
        <p data-content-origin={model.contentOrigin}>{CONTENT_ORIGIN_LABELS[model.contentOrigin]}</p>
      </header>

      <button type="button" data-action="open-create-sheet" disabled={!canCreate} onClick={onCreateSession}>
        在这个目录里新建会话
      </button>
      {canCreate ? null : (
        <p data-create-entry-unavailable="true">{createEntry.permission.note}</p>
      )}

      {model.sessions.length === 0 ? (
        <p data-directory-sessions="empty">这个目录里还没有会话。</p>
      ) : (
        <ul data-directory-sessions="list">
          {model.sessions.map((session) => (
            <li key={session.id} data-session-id={session.id}>
              <button type="button" data-action="open-session" data-session-id={session.id} onClick={() => onOpenSession(session.id)}>
                <span data-session-title="true">{session.title}</span>
                <span data-session-agent="true">{session.agentName}</span>
              </button>
              <StatusPill state={session.state} />
              {session.originOfflineLabel === null ? null : (
                <span data-origin-offline="true">{session.originOfflineLabel}</span>
              )}
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
