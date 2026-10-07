/**
 * 目录列表（R15）。
 *
 * ## 三条 MUST 在渲染上的落点
 *
 * - **只显示展示名与别名**：一行里只有 `displayName` 与 `alias` 两段文字，加上三项会话汇总。
 *   组件**没有**任何接收路径的 props，因此「顺手把 path 也显示出来」没有表达方式。
 * - **同名以别名区分**：`alias` 始终渲染（不放在「同名时才显示」的分支里）——
 *   那种分支正是路径消歧得以藏身的地方。
 * - **两种空态可区分**：`EmptyState` 的 `kind` 取自 `DirectoryEmptyState`，且标题与指引
 *   来自两套不同的词表。
 *
 * 组件是纯函数：所有交互通过回调上抛，页面负责导航。
 */

import type { ReactElement } from "react";

import type { DirectoryPageModel } from "../features/directory-model";
import { CONTENT_ORIGIN_LABELS, DIRECTORY_EMPTY_COPY } from "../features/directory-model";
import { EmptyState } from "./EmptyState";

export interface DirectoryListProps {
  readonly model: DirectoryPageModel;
  /** 进入某个目录详情（页面负责导航）。 */
  readonly onOpenDirectory: (alias: string) => void;
}

/** 内容来源标注：断连时必须看得出这些内容来自上次同步。 */
function ContentOriginNote({ model }: { readonly model: DirectoryPageModel }): ReactElement {
  if (model.kind === "loading") return <></>;
  return <p data-content-origin={model.contentOrigin}>{CONTENT_ORIGIN_LABELS[model.contentOrigin]}</p>;
}

/** 目录列表。 */
export function DirectoryList({ model, onOpenDirectory }: DirectoryListProps): ReactElement {
  if (model.kind === "loading") {
    return (
      <section data-route="dirs">
        <p data-directory-state="loading">尚未收到第一份快照，正在等待同步……</p>
      </section>
    );
  }

  if (model.kind === "empty") {
    const copy = DIRECTORY_EMPTY_COPY[model.empty];
    return (
      <section data-route="dirs">
        <ContentOriginNote model={model} />
        <EmptyState kind={model.empty} title={copy.title} guidance={copy.guidance} />
      </section>
    );
  }

  return (
    <section data-route="dirs">
      <ContentOriginNote model={model} />
      <ul data-directory-list="directories">
        {model.directories.map((directory) => (
          <li key={directory.alias} data-directory-alias={directory.alias}>
            <button
              type="button"
              data-action="open-directory"
              data-directory-alias={directory.alias}
              onClick={() => {
                onOpenDirectory(directory.alias);
              }}
            >
              {/* 展示名可以重复；别名始终展示，是区分同名目录的唯一依据。 */}
              <span data-directory-display-name={directory.displayName}>{directory.displayName}</span>
              <span data-directory-alias-label>{directory.alias}</span>
            </button>
            <span data-directory-count="total">{directory.counts.total}</span>
            <span data-directory-count="active">{directory.counts.active}</span>
            <span data-directory-count="pending">{directory.counts.pending}</span>
          </li>
        ))}
      </ul>
    </section>
  );
}
