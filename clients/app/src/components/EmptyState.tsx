/**
 * 空态展示（共享展示原语，WP5a 只建骨架，WP7 接管实现与后续改动）。
 *
 * `pwa-web-client` 的「两种空态可区分」要求「目录存在但无会话」与「本机没有任何目录」
 * 在文案与操作指引上不同。本组件承载这个**区分**，文案由调用方（WP7 的目录页）给出，
 * 因此两种空态无法被合并成一句话。
 */
import type { ReactElement } from "react";


export interface EmptyStateProps {
  /** 空态的种类标识；与 `src/domain/` 的 `DirectoryEmptyState` 对齐。 */
  readonly kind: string;
  /** 主文案。 */
  readonly title: string;
  /** 操作指引；两种空态必须给出不同内容。 */
  readonly guidance: string;
}

/** 渲染一个空态。 */
export function EmptyState({ kind, title, guidance }: EmptyStateProps): ReactElement {
  return (
    <div data-empty-state={kind}>
      <h2>{title}</h2>
      <p>{guidance}</p>
    </div>
  );
}
