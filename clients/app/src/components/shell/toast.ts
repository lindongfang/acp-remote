/**
 * 全局提示（外形取自原型的 `.toast`：底部居中的一条深色胶囊，1.8 秒后自行消失）。
 *
 * ## 为什么是模块级订阅源而不是 store 字段
 *
 * 提示由**任意位置**的交互回调触发（主题开关这类外壳自己的动作、后续组件里的复制结果等），
 * 而它既不参与任何数据判定、也不跨路由存活——因此不进 `ClientStore`（那会让「提示」变成
 * 可导出状态的一部分）。形状与主题偏好仓库一致：一个模块级订阅源 + `useSyncExternalStore`。
 *
 * 收起用**序号**判定而不是保存定时器句柄：连续两条提示只由最后一条的计时负责收起，
 * 既不留下悬空的句柄，也不需要 `clearTimeout` 的分支。
 */

import { useCallback, useSyncExternalStore } from "react";

/** 提示的驻留时长（毫秒）：与原型 `toast()` 的 1800ms 一致。 */
export const TOAST_DURATION_MS = 1800;

/** 提示的读写与订阅面。 */
export interface ToastStore {
  /** 显示一条提示；再次调用会把它重新起算（原型同此）。 */
  show(message: string): void;
  /** 当前提示文本；`null` = 没有提示在驻留。 */
  get(): string | null;
  subscribe(listener: () => void): () => void;
}

/**
 * 建一个提示仓库。
 *
 * `durationMs` 由调用方给出（生产用 {@link TOAST_DURATION_MS}，测试用自己的时长或假计时器），
 * 因此「驻留后自行收起」是可断言的，而不是靠等待 1.8 秒。
 */
export function createToastStore(durationMs: number): ToastStore {
  let message: string | null = null;
  let generation = 0;
  const listeners = new Set<() => void>();
  const notify = (): void => {
    for (const listener of [...listeners]) listener();
  };
  return {
    show(next: string): void {
      message = next;
      generation += 1;
      const mine = generation;
      setTimeout(() => {
        if (mine !== generation) return;
        message = null;
        notify();
      }, durationMs);
      notify();
    },
    get(): string | null {
      return message;
    },
    subscribe(listener: () => void): () => void {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
  };
}

/** 进程内唯一的提示仓库：外壳与后续组件共用一个出口。 */
export const toastStore: ToastStore = createToastStore(TOAST_DURATION_MS);

/** 当前提示文本（订阅式读取；显示走 `toastStore.show()`）。 */
export function useToastMessage(): string | null {
  const subscribe = useCallback((listener: () => void) => toastStore.subscribe(listener), []);
  const snapshot = useCallback(() => toastStore.get(), []);
  // 静态渲染（`renderToStaticMarkup`）没有提示可显示，与客户端首帧一致。
  return useSyncExternalStore(subscribe, snapshot, () => null);
}
