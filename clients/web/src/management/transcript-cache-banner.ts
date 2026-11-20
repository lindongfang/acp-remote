import type {
  SessionViewStatus,
  TranscriptCacheState,
} from "./session-protocol";

export interface TranscriptCacheBannerCallbacks {
  readonly onRetrySync?: () => void;
}

export type BannerStateInput = Pick<
  SessionViewStatus,
  | "transcriptSource"
  | "syncState"
  | "capabilityBoundary"
  | "continuable"
  | "cacheWarning"
>;

/**
 * Displays explicit cache-vs-live origin, revalidation/degradation status, capability boundaries,
 * non-blocking local write warnings, and manual resync trigger (`explicit_refresh`).
 */
export class TranscriptCacheBanner {
  readonly element: HTMLElement;
  readonly #message: HTMLParagraphElement;
  readonly #warning: HTMLParagraphElement;
  readonly #retryButton: HTMLButtonElement;
  readonly #listeners = new AbortController();
  #onRetrySync: (() => void) | undefined;
  #disposed = false;

  constructor(owner: Document, callbacks: TranscriptCacheBannerCallbacks = {}) {
    this.#onRetrySync = callbacks.onRetrySync;
    this.element = owner.createElement("aside");
    this.element.className = "transcript-cache-banner";
    this.element.setAttribute("aria-label", "会话历史状态与来源");
    this.element.hidden = true;

    this.#message = owner.createElement("p");
    this.#message.className = "status-line status-notice";
    this.#message.setAttribute("role", "status");

    this.#warning = owner.createElement("p");
    this.#warning.className = "status-line status-error";
    this.#warning.setAttribute("role", "alert");
    this.#warning.hidden = true;

    this.#retryButton = owner.createElement("button");
    this.#retryButton.type = "button";
    this.#retryButton.className = "secondary-button";
    this.#retryButton.textContent = "重试同步";
    this.#retryButton.hidden = true;
    this.#retryButton.addEventListener(
      "click",
      () => {
        if (this.#disposed) return;
        this.#retryButton.disabled = true;
        this.#onRetrySync?.();
      },
      { signal: this.#listeners.signal },
    );

    this.element.append(this.#message, this.#retryButton, this.#warning);
  }

  setCallbacks(callbacks: TranscriptCacheBannerCallbacks): void {
    this.#onRetrySync = callbacks.onRetrySync;
  }

  update(state: BannerStateInput | TranscriptCacheState): void {
    if (this.#disposed) return;
    const source = state.transcriptSource ?? "live";
    const syncState = state.syncState ?? "synced";
    const boundary = state.capabilityBoundary ?? "unknown";
    const continuable = state.continuable ?? true;
    const cacheWarning = state.cacheWarning ?? false;

    this.element.dataset.transcriptSource = source;
    this.element.dataset.syncState = syncState;
    this.element.dataset.capabilityBoundary = boundary;
    this.element.dataset.continuable = String(continuable);
    this.element.dataset.cacheWarning = String(cacheWarning);

    const statusCopy = describeCacheState(source, syncState, boundary, continuable);
    const showRetry =
      syncState === "degraded_unreachable" || syncState === "degraded_capacity_full";

    this.#message.textContent = statusCopy ?? "";
    this.#message.hidden = statusCopy === null;
    this.#retryButton.hidden = !showRetry;
    this.#retryButton.disabled = false;

    this.#warning.textContent = cacheWarning
      ? "本地展示缓存写入降级告警：当前实时会话与审批不受影响，但断连后可能无法完整保留最新展示快照。"
      : "";
    this.#warning.hidden = !cacheWarning;

    this.element.hidden = statusCopy === null && !cacheWarning;
  }

  dispose(): void {
    if (this.#disposed) return;
    this.#disposed = true;
    this.#listeners.abort();
    this.element.replaceChildren();
    this.element.remove();
  }
}

function describeCacheState(
  source: NonNullable<BannerStateInput["transcriptSource"]>,
  syncState: NonNullable<BannerStateInput["syncState"]>,
  boundary: NonNullable<BannerStateInput["capabilityBoundary"]>,
  continuable: boolean,
): string | null {
  if (syncState === "native_unrecoverable") {
    return "原生会话已删除或不可恢复，当前显示本地历史快照（只读）。";
  }
  if (!continuable || boundary === "pure_memory") {
    return "当前会话仅支持只读历史快照，无法继续发送新输入或恢复原生上下文。";
  }
  switch (syncState) {
    case "revalidating":
      return "正在显示本地缓存快照，后台正在向原生 Agent 校验最新状态…";
    case "degraded_unreachable":
      return "原生来源暂不可达，当前显示本地缓存快照。";
    case "degraded_capacity_full":
      return "节点活动会话槽位已满（50/50），当前显示本地缓存只读快照；请先释放空闲会话或稍后重试同步。";
    case "idle":
    case "synced":
      if (source === "hybrid_resumed") {
        return "已通过原生 resume 续接，前文来自本地缓存快照，后续回复实时追加。";
      }
      return source === "transcript_cache"
        ? "当前显示本地缓存快照（来源能力仅支持只读回看或延迟恢复）。"
        : null;
  }
}
