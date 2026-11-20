import { HistoryPageController } from "./history-page-controller";
import type { IdentityRecord } from "../pairing/identity";
import type { ManagementConnection } from "../management/client";
import type {
  SessionViewOpened,
  SourceLaunchSelectionInput,
} from "../management/session-protocol";
import {
  SessionViewDisplay,
  type ViewDisplayTransport,
} from "../management/session-view-display";
import type { ViewDataFrame } from "../views/binary";
import { managementErrorMessage } from "./error-formatting";

export type SharedViewOpenReason =
  | "first_open"
  | "explicit_refresh"
  | "reopen"
  | "reconnect"
  | "reacquire";

export type SharedViewSpec =
  | { readonly kind: "target"; readonly targetId: string }
  | {
      readonly kind: "source";
      readonly selection: SourceLaunchSelectionInput;
      readonly nativeSessionId: string;
    };

export interface SharedViewDisplayLike {
  readonly streamId: string;
  readonly detached?: boolean;
  readonly element:
    | HTMLElement
    | {
        dataset: Record<string, string>;
        removeAttribute: (key: string) => void;
      };
  start(): void;
  suspend(): void;
  dispose(): void;
  accept?(frame: ViewDataFrame): void;
  beginStagingRebind?(opened: SessionViewOpened, transport: ViewDisplayTransport): boolean;
  acceptStagedFrame?(frame: ViewDataFrame): boolean;
  abortStagingRebind?(reason?: string): void;
}

export type SharedViewDisplayFactory = (
  doc: Document,
  opened: SessionViewOpened,
  callbacks: ViewDisplayTransport,
) => SharedViewDisplayLike;

export interface SharedViewHostPort {
  readonly getManagement: () => ManagementConnection | null;
  readonly getManagementGeneration: () => number;
  readonly getInspectedQueueTargetId: () => string | null;
  readonly getSharedViewHost: () => { append: (element: HTMLElement) => void };
  readonly getDocument?: () => Document;
  readonly displayFactory?: SharedViewDisplayFactory;
  readonly persistSourceViewSpec?: (
    selection: SourceLaunchSelectionInput,
    nativeSessionId: string,
  ) => void;
  readonly clearPersistedSourceViewSpec?: () => void;
  readonly renderDashboard: (
    record: IdentityRecord,
    notice?: string,
    failure?: string,
  ) => void;
  readonly formatError?: (error: unknown) => string;
}

export function releaseSharedDisplay(
  display: Pick<SharedViewDisplayLike, "streamId"> & { readonly detached?: boolean },
  connection: {
    detachSessionStream: (streamId: string) => void;
    closeSessionView: (streamId: string) => Promise<unknown>;
  } | null,
): void {
  if (connection === null) return;
  if (display.detached !== true) {
    try {
      connection.detachSessionStream(display.streamId);
    } catch {
      /* Best effort on the original transport. */
    }
  }
  void connection.closeSessionView(display.streamId).catch(() => undefined);
}

export class SharedViewController {
  readonly #port: SharedViewHostPort;

  #openedSessionView: SessionViewOpened | null = null;
  #sharedSessionDisplay: SharedViewDisplayLike | null = null;
  #sharedSessionConnection: ManagementConnection | null = null;
  #sharedViewTargetId: string | null = null;
  #sharedViewSpec: SharedViewSpec | null = null;
  #sharedViewAttempt = 0;
  #sharedViewStaleNotice: string | null = null;
  readonly #staleSharedDisplays = new Set<SharedViewDisplayLike>();

  constructor(port: SharedViewHostPort) {
    this.#port = port;
  }

  get openedSessionView(): SessionViewOpened | null { return this.#openedSessionView; }
  get sharedSessionDisplay(): SharedViewDisplayLike | null { return this.#sharedSessionDisplay; }
  get sharedSessionConnection(): ManagementConnection | null { return this.#sharedSessionConnection; }
  get sharedViewTargetId(): string | null { return this.#sharedViewTargetId; }
  get sharedViewSpec(): SharedViewSpec | null { return this.#sharedViewSpec; }
  get sharedViewAttempt(): number { return this.#sharedViewAttempt; }
  get sharedViewStaleNotice(): string | null { return this.#sharedViewStaleNotice; }
  get staleSharedDisplays(): ReadonlySet<SharedViewDisplayLike> { return this.#staleSharedDisplays; }

  restoreSharedViewSpec(spec: SharedViewSpec): void {
    this.#sharedViewSpec = spec;
  }

  restorePersistedSourceViewIfIdle(
    loadPersisted: () => Extract<SharedViewSpec, { readonly kind: "source" }> | null,
    inspectedQueueTargetId: string | null,
  ): Extract<SharedViewSpec, { readonly kind: "source" }> | null {
    if (
      this.#sharedViewSpec === null &&
      this.#sharedViewTargetId === null &&
      inspectedQueueTargetId === null
    ) {
      const persisted = loadPersisted();
      if (persisted !== null) {
        this.#sharedViewSpec = persisted;
        return persisted;
      }
    }
    return null;
  }

  acceptViewFrame(frame: ViewDataFrame): boolean {
    if (this.#sharedSessionDisplay !== null) {
      if (this.#sharedSessionDisplay.acceptStagedFrame?.(frame) === true) {
        return true;
      }
      if (frame.streamId === this.#sharedSessionDisplay.streamId) {
        this.#sharedSessionDisplay.accept?.(frame);
        return true;
      }
    }
    return false;
  }

  suspendSharedView(): void {
    ++this.#sharedViewAttempt;
    this.#sharedSessionDisplay?.suspend();
  }

  disposeSharedViews(): void {
    ++this.#sharedViewAttempt;
    this.#sharedViewTargetId = null;
    if (this.#sharedSessionDisplay !== null) {
      releaseSharedDisplay(this.#sharedSessionDisplay, this.#sharedSessionConnection);
      this.#sharedSessionDisplay.dispose();
    }
    for (const stale of this.#staleSharedDisplays) stale.dispose();
    this.#staleSharedDisplays.clear();
    this.#sharedSessionDisplay = null;
    this.#sharedSessionConnection = null;
    this.#openedSessionView = null;
  }

  clearSharedSourceViewSpec(): void {
    this.#sharedViewSpec = null;
    this.#sharedViewStaleNotice = null;
    this.#port.clearPersistedSourceViewSpec?.();
  }

  async openTargetSharedView(
    record: IdentityRecord,
    targetId: string,
    submit: HTMLButtonElement | null,
    reason: SharedViewOpenReason = "first_open",
  ): Promise<void> {
    this.clearSharedSourceViewSpec();
    this.#sharedViewSpec = { kind: "target", targetId };
    this.#sharedViewStaleNotice = null;
    await this.openSharedSessionView(record, targetId, submit, reason);
  }

  async closeTargetSharedView(
    record: IdentityRecord,
    opened: SessionViewOpened,
    submit: HTMLButtonElement,
  ): Promise<void> {
    if (this.#sharedViewSpec?.kind === "target") {
      this.#sharedViewSpec = null;
      this.#sharedViewStaleNotice = null;
    }
    await this.closeSharedSessionView(record, opened, submit);
  }

  #instantiateDisplay(
    record: IdentityRecord,
    opened: SessionViewOpened,
    connection: ManagementConnection,
    clearStaleOnSnapshot: boolean,
    turns?: string,
  ): SharedViewDisplayLike {
    const factory =
      this.#port.displayFactory ??
      ((doc, openedView, callbacks) =>
        new SessionViewDisplay(doc, openedView, callbacks));
    const doc = this.#port.getDocument?.() ?? globalThis.document;
    let display!: SharedViewDisplayLike;
    let history: HistoryPageController | undefined;
    display = factory(doc, opened, {
      turnStarted: turn => history?.turnStarted(turn),
      credit: (streamId, sequence) =>
        connection.sendSessionViewCredit(streamId, sequence),
      detach: (streamId) => connection.detachSessionStream(streamId),
      reopenRequested: full => {
        if (this.#sharedSessionDisplay !== display) return;
        const spec = this.#sharedViewSpec;
        if (opened.page && spec?.kind === "target") {
          void this.openSharedSessionView(record, spec.targetId, null, "reopen", full ? null : {});
        } else if (opened.page && spec?.kind === "source") {
          void this.openSharedSourceView(record, spec.selection, spec.nativeSessionId, null, "reopen", full ? null : {});
        } else void this.rebindSharedView(record, opened);
      },
      retrySyncRequested: () => {
        const spec = this.#sharedViewSpec;
        if (spec?.kind === "target") {
          void this.openSharedSessionView(record, spec.targetId, null, "explicit_refresh");
        } else if (spec?.kind === "source") {
          void this.openSharedSourceView(
            record,
            spec.selection,
            spec.nativeSessionId,
            null,
            "explicit_refresh",
          );
        }
      },
      snapshotReady: () => {
        if (this.#sharedSessionDisplay !== display) return;
        if (clearStaleOnSnapshot) {
          if ("removeAttribute" in display.element) {
            display.element.removeAttribute("data-stale");
          }
          this.#sharedViewStaleNotice = null;
        }
        for (const stale of this.#staleSharedDisplays) stale.dispose();
        this.#staleSharedDisplays.clear();
      },
    });
    const spec = this.#sharedViewSpec;
    if (!opened.page || !spec || !(display instanceof SessionViewDisplay)) return display;
    const root = display;
    let retired = false;
    const controller = new HistoryPageController(doc, spec, connection, {
      credit: (id, seq) => connection.sendSessionViewCredit(id, seq),
      detach: id => connection.detachSessionStream(id),
    }, undefined, turns => {
      if (spec.kind === "target") void this.openSharedSessionView(record, spec.targetId, null, "explicit_refresh", turns ? { turns } : {});
      else void this.openSharedSourceView(record, spec.selection, spec.nativeSessionId, null, "explicit_refresh", turns ? { turns } : {});
    }, () => this.#sharedSessionDisplay === display && this.#port.getManagement() === connection && this.#sharedViewSpec === spec, () => {
      retired = true;
      releaseSharedDisplay(root, connection);
      root.dispose();
    });
    history = controller;
    controller.attach(opened, root);
    controller.view.turns.value = turns ?? "";
    display = {
      element: controller.element, streamId: root.streamId,
      get detached() { return retired || root.detached; },
      start: () => root.start(),
      accept: frame => root.accept(frame),
      acceptStagedFrame: frame => controller.accept(frame),
      suspend: () => { root.suspend(); controller.suspend(); },
      dispose: () => { controller.dispose(); root.dispose(); },
    };
    return display;
  }

  async openSharedSessionView(
    record: IdentityRecord,
    targetId: string,
    submit: HTMLButtonElement | null,
    reason: SharedViewOpenReason = "first_open",
    page: { turns?: string } | null = {},
  ): Promise<void> {
    const connection = this.#port.getManagement();
    const generation = this.#port.getManagementGeneration();
    const attempt = ++this.#sharedViewAttempt;
    this.#sharedViewTargetId = targetId;
    if (connection === null) {
      this.#port.renderDashboard(record, undefined, "管理连接已经关闭，无法打开共享视图。");
      return;
    }
    try {
      const opened = await connection.openSessionView(targetId, reason, page ?? undefined);
      if (
        generation !== this.#port.getManagementGeneration() ||
        this.#port.getManagement() !== connection ||
        attempt !== this.#sharedViewAttempt ||
        this.#sharedViewTargetId !== targetId ||
        this.#port.getInspectedQueueTargetId() !== targetId
      ) {
        releaseSharedDisplay(opened, connection);
        return;
      }
      const previous = this.#sharedSessionDisplay;
      const previousConnection = this.#sharedSessionConnection;
      const previousOpened = this.#openedSessionView;
      let display: SharedViewDisplayLike | null = null;
      try {
        display = this.#instantiateDisplay(record, opened, connection, false, page?.turns);
        this.#sharedSessionDisplay = display;
        this.#sharedSessionConnection = connection;
        this.#openedSessionView = opened;
        this.#port.getSharedViewHost().append(display.element as HTMLElement);
        display.start();
      } catch (error) {
        if (display !== null && this.#sharedSessionDisplay === display) {
          this.#sharedSessionDisplay = previous;
          this.#sharedSessionConnection = previousConnection;
          this.#openedSessionView = previousOpened;
        }
        releaseSharedDisplay(opened, connection);
        display?.dispose();
        throw error;
      }
      if (previous !== null) {
        previous.suspend();
        releaseSharedDisplay(previous, previousConnection);
        this.#staleSharedDisplays.add(previous);
      }
      this.#port.renderDashboard(record, "共享视图已打开，正在读取快照及后续增量。");
    } catch (error) {
      if (
        generation !== this.#port.getManagementGeneration() ||
        attempt !== this.#sharedViewAttempt
      ) {
        return;
      }
      if (submit !== null) submit.disabled = false;
      const format = this.#port.formatError ?? managementErrorMessage;
      this.#port.renderDashboard(record, undefined, `共享视图未打开：${format(error)}`);
    }
  }

  async closeSharedSessionView(
    record: IdentityRecord,
    opened: SessionViewOpened,
    submit: HTMLButtonElement,
  ): Promise<void> {
    const connection = this.#sharedSessionConnection;
    const generation = this.#port.getManagementGeneration();
    const display = this.#sharedSessionDisplay;
    if (display === null || display.streamId !== opened.streamId) return;
    const attempt = ++this.#sharedViewAttempt;
    this.#sharedViewTargetId = null;
    this.#sharedSessionDisplay = null;
    this.#sharedSessionConnection = null;
    this.#openedSessionView = null;
    display.dispose();
    for (const stale of this.#staleSharedDisplays) stale.dispose();
    this.#staleSharedDisplays.clear();
    if (connection === null) {
      this.#port.renderDashboard(
        record,
        undefined,
        "本地视图已释放；管理连接已关闭，未确认远端关闭。",
      );
      return;
    }
    try {
      const closed = await connection.closeSessionView(opened.streamId);
      if (
        generation !== this.#port.getManagementGeneration() ||
        attempt !== this.#sharedViewAttempt
      ) {
        return;
      }
      this.#port.renderDashboard(
        record,
        `共享视图已关闭，状态 ${closed.state}，最后确认分块 ${closed.lastCommittedPacket}。`,
      );
    } catch (error) {
      if (
        generation !== this.#port.getManagementGeneration() ||
        attempt !== this.#sharedViewAttempt
      ) {
        return;
      }
      if (!display.detached) {
        try {
          connection.detachSessionStream(opened.streamId);
        } catch {}
      }
      submit.disabled = false;
      const format = this.#port.formatError ?? managementErrorMessage;
      this.#port.renderDashboard(record, undefined, `共享视图关闭未完成：${format(error)}`);
    }
  }

  async rebindSharedView(
    record: IdentityRecord,
    previousOpened: SessionViewOpened,
  ): Promise<void> {
    const connection = this.#port.getManagement();
    const generation = this.#port.getManagementGeneration();
    const previous = this.#sharedSessionDisplay;
    const previousConnection = this.#sharedSessionConnection;
    if (
      connection === null ||
      previous === null ||
      previous.streamId !== previousOpened.streamId
    ) {
      return;
    }
    const attempt = ++this.#sharedViewAttempt;
    try {
      const reopened = await connection.rebindSessionView(previousOpened.streamId);
      if (
        generation !== this.#port.getManagementGeneration() ||
        this.#port.getManagement() !== connection ||
        attempt !== this.#sharedViewAttempt ||
        this.#sharedSessionDisplay !== previous
      ) {
        releaseSharedDisplay(reopened, connection);
        return;
      }
      if (previous.beginStagingRebind !== undefined) {
        const stagedTransport: ViewDisplayTransport = {
          credit: (streamId, sequence) =>
            connection.sendSessionViewCredit(streamId, sequence),
          detach: (streamId) => connection.detachSessionStream(streamId),
          reopenRequested: () => {
            void this.rebindSharedView(record, reopened);
          },
          retrySyncRequested: () => {
            const spec = this.#sharedViewSpec;
            if (spec?.kind === "target") {
              void this.openSharedSessionView(record, spec.targetId, null, "explicit_refresh");
            } else if (spec?.kind === "source") {
              void this.openSharedSourceView(
                record,
                spec.selection,
                spec.nativeSessionId,
                null,
                "explicit_refresh",
              );
            }
          },
          snapshotReady: () => {
            if (this.#sharedSessionDisplay !== previous) return;
            this.#sharedSessionConnection = connection;
            this.#openedSessionView = reopened;
            this.#sharedViewStaleNotice = null;
            for (const stale of this.#staleSharedDisplays) stale.dispose();
            this.#staleSharedDisplays.clear();
          },
        };
        if (previous.beginStagingRebind(reopened, stagedTransport)) {
          this.#port.renderDashboard(record, "正在后台准备最新会话快照，准备就绪后将无缝切换。");
          return;
        }
      }
      let display: SharedViewDisplayLike | null = null;
      try {
        display = this.#instantiateDisplay(record, reopened, connection, true);
        this.#sharedSessionDisplay = display;
        this.#sharedSessionConnection = connection;
        this.#openedSessionView = reopened;
        this.#sharedViewStaleNotice = null;
        this.#port.getSharedViewHost().append(display.element as HTMLElement);
        display.start();
      } catch (error) {
        if (display !== null && this.#sharedSessionDisplay === display) {
          this.#sharedSessionDisplay = previous;
          this.#sharedSessionConnection = previousConnection;
          this.#openedSessionView = previousOpened;
        }
        releaseSharedDisplay(reopened, connection);
        display?.dispose();
        throw error;
      }
      previous.suspend();
      this.#staleSharedDisplays.add(previous);
      this.#port.renderDashboard(record, "共享视图已按最新状态重新绑定并刷新。");
    } catch (error) {
      if (
        generation !== this.#port.getManagementGeneration() ||
        attempt !== this.#sharedViewAttempt
      ) {
        return;
      }
      if ("dataset" in previous.element) previous.element.dataset.stale = "true";
      const format = this.#port.formatError ?? managementErrorMessage;
      this.#sharedViewStaleNotice = `共享视图重绑定失败，当前保留旧视图状态：${format(error)}`;
      this.#port.renderDashboard(record, undefined, this.#sharedViewStaleNotice);
    }
  }

  async openSharedSourceView(
    record: IdentityRecord,
    selection: SourceLaunchSelectionInput,
    nativeSessionId: string,
    submit: HTMLButtonElement | null,
    reason: SharedViewOpenReason = "first_open",
    page: { turns?: string } | null = {},
  ): Promise<void> {
    const connection = this.#port.getManagement();
    const generation = this.#port.getManagementGeneration();
    const attempt = ++this.#sharedViewAttempt;
    this.#sharedViewTargetId = null;
    this.#sharedViewSpec = { kind: "source", selection, nativeSessionId };
    this.#port.persistSourceViewSpec?.(selection, nativeSessionId);
    if (connection === null) {
      if (submit !== null) submit.disabled = false;
      const keepStale =
        (reason === "reconnect" || reason === "explicit_refresh") &&
        this.#sharedSessionDisplay !== null;
      this.#sharedViewStaleNotice = keepStale
        ? "管理连接已关闭，未能从来源重新校验；当前保留旧缓存视图。"
        : "管理连接已经关闭，无法打开来源只读视图。";
      if (keepStale && this.#sharedSessionDisplay !== null && "dataset" in this.#sharedSessionDisplay.element) {
        this.#sharedSessionDisplay.element.dataset.stale = "true";
      }
      this.#port.renderDashboard(record, undefined, this.#sharedViewStaleNotice);
      return;
    }
    try {
      const opened = await connection.openSourceSessionView(selection, nativeSessionId, reason, page ?? undefined);
      if (
        generation !== this.#port.getManagementGeneration() ||
        this.#port.getManagement() !== connection ||
        attempt !== this.#sharedViewAttempt ||
        this.#sharedViewSpec?.kind !== "source" ||
        this.#sharedViewSpec.nativeSessionId !== nativeSessionId
      ) {
        releaseSharedDisplay(opened, connection);
        return;
      }
      const previous = this.#sharedSessionDisplay;
      const previousConnection = this.#sharedSessionConnection;
      const previousOpened = this.#openedSessionView;
      let display: SharedViewDisplayLike | null = null;
      try {
        display = this.#instantiateDisplay(record, opened, connection, true, page?.turns);
        this.#sharedSessionDisplay = display;
        this.#sharedSessionConnection = connection;
        this.#openedSessionView = opened;
        this.#sharedViewStaleNotice = null;
        this.#port.getSharedViewHost().append(display.element as HTMLElement);
        display.start();
      } catch (error) {
        if (display !== null && this.#sharedSessionDisplay === display) {
          this.#sharedSessionDisplay = previous;
          this.#sharedSessionConnection = previousConnection;
          this.#openedSessionView = previousOpened;
        }
        releaseSharedDisplay(opened, connection);
        display?.dispose();
        throw error;
      }
      if (previous !== null) {
        previous.suspend();
        releaseSharedDisplay(previous, previousConnection);
        this.#staleSharedDisplays.add(previous);
      }
      this.#port.renderDashboard(
        record,
        opened.status.historyIncomplete
          ? "来源只读视图已打开（当前模式仅包含实时连接与有限缓存，无法补全未缓存前文）。"
          : "来源只读视图已打开，正在读取快照及后续增量。",
      );
    } catch (error) {
      if (
        generation !== this.#port.getManagementGeneration() ||
        attempt !== this.#sharedViewAttempt
      ) {
        return;
      }
      if (submit !== null) submit.disabled = false;
      const format = this.#port.formatError ?? managementErrorMessage;
      const failureText = format(error);
      if (reason === "reconnect" || reason === "explicit_refresh") {
        if (this.#sharedSessionDisplay !== null && "dataset" in this.#sharedSessionDisplay.element) {
          this.#sharedSessionDisplay.element.dataset.stale = "true";
        }
        this.#sharedViewStaleNotice = `来源只读视图重新校验失败，当前保留旧视图状态：${failureText}`;
        this.#port.renderDashboard(record, undefined, this.#sharedViewStaleNotice);
        return;
      }
      this.clearSharedSourceViewSpec();
      this.#port.renderDashboard(record, undefined, `来源只读视图未打开：${failureText}`);
    }
  }

  async closeSharedSourceView(
    record: IdentityRecord,
    opened: SessionViewOpened,
    submit: HTMLButtonElement,
  ): Promise<void> {
    this.clearSharedSourceViewSpec();
    await this.closeSharedSessionView(record, opened, submit);
  }
}
