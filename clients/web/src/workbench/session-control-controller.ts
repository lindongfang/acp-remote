import { ManagementRequestError, type ManagementConnection } from "../management/client";
import type { SessionControlChangedFrame } from "../management/protocol";
import type { SessionControlSnapshot } from "../management/session-protocol";
import type { IdentityRecord } from "../pairing/identity";
import {
  button,
  element,
  paragraph,
  statusLine,
} from "./dom-primitives";
import { managementErrorMessage } from "./error-formatting";

export interface SessionControlIntegrationPort {
  readonly getControlTargetId: () => string | null;
  readonly renderControlBanner: (
    record: IdentityRecord,
    targetId: string,
  ) => HTMLElement;
  readonly inspectSessionControl: (
    targetId: string,
    generation: number,
  ) => Promise<unknown>;
  readonly canWriteTarget: (
    targetId: string | null,
    record?: IdentityRecord | null,
  ) => boolean;
  readonly getControlEpoch: (targetId: string | null) => string | undefined;
  readonly onSessionNotControlled: (
    record: IdentityRecord,
    targetId: string,
    generation: number,
    error: unknown,
    failureMessage: string,
  ) => Promise<boolean>;
}

export interface SessionControlHostPort {
  readonly getManagement: () => ManagementConnection | null;
  readonly getManagementGeneration: () => number;
  readonly getCurrentRecord: () => IdentityRecord | null;
  readonly getActiveControlTargetId: () => string | null;
  readonly getDocument?: () => Document;
  readonly renderDashboard: (
    record: IdentityRecord,
    notice?: string,
    failure?: string,
  ) => void;
  readonly formatError?: (error: unknown) => string;
}

export type SessionControlOwnership =
  | "self"
  | "other"
  | "uncontrolled"
  | "unknown";

function rootKeyFor(sourceId: string, rootNativeSessionId: string): string {
  return `${sourceId}\0${rootNativeSessionId}`;
}

export class SessionControlController {
  readonly #port: SessionControlHostPort;

  #rootSnapshots = new Map<string, SessionControlSnapshot>();
  #targetToRootKey = new Map<string, string>();
  #actionInFlight = false;

  constructor(port: SessionControlHostPort) {
    this.#port = port;
  }

  #doc(): Document {
    return this.#port.getDocument?.() ?? globalThis.document;
  }

  /** Clears per-connection snapshots so reconnect/restart starts fresh. */
  resetForConnection(): void {
    this.#rootSnapshots.clear();
    this.#targetToRootKey.clear();
    this.#actionInFlight = false;
  }

  /** Resets all cached control state when session state is torn down. */
  resetAll(): void {
    this.resetForConnection();
  }

  snapshotForTarget(targetId: string | null): SessionControlSnapshot | null {
    if (targetId === null) {
      return null;
    }
    const rootKey = this.#targetToRootKey.get(targetId);
    if (rootKey !== undefined) {
      return this.#rootSnapshots.get(rootKey) ?? null;
    }
    for (const snapshot of this.#rootSnapshots.values()) {
      if (snapshot.targetId === targetId || snapshot.rootTargetId === targetId) {
        return snapshot;
      }
    }
    return null;
  }

  controlEpochForTarget(targetId: string | null): string | undefined {
    return this.snapshotForTarget(targetId)?.controlEpoch;
  }

  ownershipForTarget(
    targetId: string | null,
    localClientId: string | null,
  ): SessionControlOwnership {
    if (targetId === null || localClientId === null) {
      return "unknown";
    }
    const snapshot = this.snapshotForTarget(targetId);
    if (snapshot === null) {
      return "unknown";
    }
    if (snapshot.controllerClientId === null) {
      return "uncontrolled";
    }
    return snapshot.controllerClientId === localClientId ? "self" : "other";
  }

  canWriteTarget(targetId: string | null, record?: IdentityRecord | null): boolean {
    if (targetId === null) {
      return true;
    }
    const connection = this.#port.getManagement();
    if (connection === null) {
      return false;
    }
    const localClientId =
      connection.clientId ??
      record?.clientId ??
      this.#port.getCurrentRecord()?.clientId ??
      null;
    return this.ownershipForTarget(targetId, localClientId) === "self";
  }

  #mergeSnapshot(
    incoming: SessionControlSnapshot,
    queriedTargetId: string | null,
  ): SessionControlSnapshot {
    const key = rootKeyFor(incoming.sourceId, incoming.rootNativeSessionId);
    if (queriedTargetId !== null) {
      this.#targetToRootKey.set(queriedTargetId, key);
    }
    if (incoming.targetId !== null) {
      this.#targetToRootKey.set(incoming.targetId, key);
    }
    if (incoming.rootTargetId !== null) {
      this.#targetToRootKey.set(incoming.rootTargetId, key);
    }
    const existing = this.#rootSnapshots.get(key);
    if (
      existing !== undefined &&
      BigInt(incoming.controlEpoch) < BigInt(existing.controlEpoch)
    ) {
      return existing;
    }
    const merged: SessionControlSnapshot = {
      ...incoming,
      targetId:
        incoming.targetId ??
        queriedTargetId ??
        existing?.targetId ??
        incoming.rootTargetId ??
        null,
      rootTargetId: incoming.rootTargetId ?? existing?.rootTargetId ?? null,
    };
    this.#rootSnapshots.set(key, merged);
    return merged;
  }

  /** Fetches the current control snapshot for `targetId` without claiming control. */
  async inspectTarget(
    targetId: string,
    generation: number,
  ): Promise<SessionControlSnapshot | null> {
    const connection = this.#port.getManagement();
    if (
      connection === null ||
      generation !== this.#port.getManagementGeneration() ||
      typeof connection.inspectSessionControl !== "function"
    ) {
      return null;
    }
    const snapshot = await connection.inspectSessionControl(targetId);
    if (
      generation !== this.#port.getManagementGeneration() ||
      this.#port.getManagement() !== connection
    ) {
      return null;
    }
    return this.#mergeSnapshot(snapshot, targetId);
  }

  /** Re-inspects control state (for example after `session_not_controlled`) and re-renders. */
  async refreshTargetAndRender(
    record: IdentityRecord,
    targetId: string,
    generation: number,
    notice?: string,
    failure?: string,
  ): Promise<void> {
    try {
      await this.inspectTarget(targetId, generation);
    } catch {
      // Keep the caller's error message visible even if follow-up inspect fails.
    }
    if (generation === this.#port.getManagementGeneration()) {
      this.#port.renderDashboard(record, notice, failure);
    }
  }

  /** Applies a `session.control.changed` server push using monotonic epoch ordering. */
  handleControlChanged(
    record: IdentityRecord,
    generation: number,
    frame: SessionControlChangedFrame,
  ): void {
    if (generation !== this.#port.getManagementGeneration()) {
      return;
    }
    const key = rootKeyFor(frame.sourceId, frame.rootNativeSessionId);
    const activeTargetId = this.#port.getActiveControlTargetId();
    const existing = this.#rootSnapshots.get(key);
    if (
      existing !== undefined &&
      BigInt(frame.controlEpoch) < BigInt(existing.controlEpoch)
    ) {
      return;
    }
    const merged = this.#mergeSnapshot(
      {
        targetId: existing?.targetId ?? frame.rootTargetId,
        rootTargetId: frame.rootTargetId,
        sourceId: frame.sourceId,
        rootNativeSessionId: frame.rootNativeSessionId,
        controllerClientId: frame.controllerClientId,
        controllerDisplayName: frame.controllerDisplayName,
        controlEpoch: frame.controlEpoch,
      },
      null,
    );
    const activeMatches =
      activeTargetId !== null &&
      (this.#targetToRootKey.get(activeTargetId) === key ||
        activeTargetId === merged.rootTargetId ||
        activeTargetId === merged.targetId);
    if (!activeMatches) {
      return;
    }
    const localClientId =
      this.#port.getManagement()?.clientId ?? record.clientId ?? null;
    const notice =
      merged.controllerClientId === null
        ? `会话控制权已释放为只读观察态（代际 ${merged.controlEpoch}）。`
        : merged.controllerClientId === localClientId
          ? `本设备已持有会话控制权（代际 ${merged.controlEpoch}）。`
          : `会话控制权已由 ${merged.controllerDisplayName ?? merged.controllerClientId} 接管（代际 ${merged.controlEpoch}），本端已切换为只读观察。`;
    this.#port.renderDashboard(record, notice);
  }

  /** Renders the 3-state session control banner and takeover/release action button. */
  renderControlBanner(record: IdentityRecord, targetId: string): HTMLElement {
    const doc = this.#doc();
    const connection = this.#port.getManagement();
    const localClientId = connection?.clientId ?? record.clientId ?? null;
    const snapshot = this.snapshotForTarget(targetId);
    const ownership = this.ownershipForTarget(targetId, localClientId);

    const banner = element("div", "session-control-banner", undefined, doc);
    banner.dataset.controlState = ownership;
    banner.dataset.targetId = targetId;
    if (snapshot !== null) {
      banner.dataset.controlEpoch = snapshot.controlEpoch;
    }

    if (ownership === "self" && snapshot !== null) {
      banner.append(
        statusLine(
          `当前由本设备接管 · 代际 ${snapshot.controlEpoch}`,
          "notice",
          doc,
        ),
      );
      const release = button("释放控制权", "secondary-button", doc);
      release.dataset.controlAction = "release";
      release.disabled = connection === null || this.#actionInFlight;
      release.addEventListener("click", () => {
        release.disabled = true;
        void this.releaseTarget(record, targetId, snapshot.controlEpoch, release);
      });
      banner.append(release);
      return banner;
    }

    if (ownership === "other" && snapshot !== null) {
      const holder =
        snapshot.controllerDisplayName ?? snapshot.controllerClientId ?? "其他设备";
      banner.append(
        statusLine(
          `只读观察 · 当前由 ${holder} 接管 · 代际 ${snapshot.controlEpoch}`,
          "working",
          doc,
        ),
      );
      const preempt = button("顶号接管会话", "submit", doc);
      preempt.dataset.controlAction = "takeover";
      preempt.disabled = connection === null || this.#actionInFlight;
      preempt.addEventListener("click", () => {
        preempt.disabled = true;
        void this.takeoverTarget(record, targetId, preempt);
      });
      banner.append(preempt);
      return banner;
    }

    if (ownership === "uncontrolled" && snapshot !== null) {
      banner.append(
        statusLine(
          `只读观察 · 当前无人接管 · 代际 ${snapshot.controlEpoch}`,
          "notice",
          doc,
        ),
      );
      const take = button("接管会话", "submit", doc);
      take.dataset.controlAction = "takeover";
      take.disabled = connection === null || this.#actionInFlight;
      take.addEventListener("click", () => {
        take.disabled = true;
        void this.takeoverTarget(record, targetId, take);
      });
      banner.append(take);
      return banner;
    }

    banner.append(paragraph("只读观察 · 正在读取会话控制权…", "field-help", doc));
    const take = button("接管会话", "submit", doc);
    take.dataset.controlAction = "takeover";
    take.disabled = connection === null || this.#actionInFlight;
    take.addEventListener("click", () => {
      take.disabled = true;
      void this.takeoverTarget(record, targetId, take);
    });
    banner.append(take);
    return banner;
  }

  /** Explicitly claims or preempts control for `targetId`. */
  async takeoverTarget(
    record: IdentityRecord,
    targetId: string,
    submit?: HTMLButtonElement,
  ): Promise<void> {
    const connection = this.#port.getManagement();
    const generation = this.#port.getManagementGeneration();
    if (connection === null) {
      if (submit !== undefined) submit.disabled = false;
      this.#port.renderDashboard(
        record,
        undefined,
        "管理连接已经关闭，无法接管会话控制权。",
      );
      return;
    }
    this.#actionInFlight = true;
    try {
      const snapshot = await connection.takeoverSessionControl(targetId);
      if (
        generation !== this.#port.getManagementGeneration() ||
        this.#port.getManagement() !== connection
      ) {
        return;
      }
      this.#actionInFlight = false;
      const merged = this.#mergeSnapshot(snapshot, targetId);
      this.#port.renderDashboard(
        record,
        `当前由本设备接管 · 代际 ${merged.controlEpoch}。`,
      );
    } catch (error) {
      if (generation !== this.#port.getManagementGeneration()) return;
      this.#actionInFlight = false;
      if (submit !== undefined) submit.disabled = false;
      const format = this.#port.formatError ?? managementErrorMessage;
      await this.refreshTargetAndRender(
        record,
        targetId,
        generation,
        undefined,
        `接管控制权失败：${format(error)}`,
      );
    }
  }

  /** Creates an integration port shared with sibling queue and approval controllers. */
  createIntegrationPort(): SessionControlIntegrationPort {
    return {
      getControlTargetId: () => this.#port.getActiveControlTargetId(),
      renderControlBanner: (record, targetId) =>
        this.renderControlBanner(record, targetId),
      inspectSessionControl: (targetId, generation) =>
        this.inspectTarget(targetId, generation),
      canWriteTarget: (targetId, record) =>
        this.canWriteTarget(targetId, record),
      getControlEpoch: (targetId) => this.controlEpochForTarget(targetId),
      onSessionNotControlled: async (
        record,
        targetId,
        generation,
        error,
        failureMessage,
      ) => {
        const isNotControlled =
          (error instanceof ManagementRequestError &&
            error.code === "session_not_controlled") ||
          (error instanceof Error &&
            "detail" in error &&
            (error as { detail?: unknown }).detail === "session_not_controlled");
        if (!isNotControlled) return false;
        await this.refreshTargetAndRender(
          record,
          targetId,
          generation,
          undefined,
          failureMessage,
        );
        return true;
      },
    };
  }

  /** Releases current device's control of `targetId`. */
  async releaseTarget(
    record: IdentityRecord,
    targetId: string,
    expectedControlEpoch?: string,
    submit?: HTMLButtonElement,
  ): Promise<void> {
    const connection = this.#port.getManagement();
    const generation = this.#port.getManagementGeneration();
    if (connection === null) {
      if (submit !== undefined) submit.disabled = false;
      this.#port.renderDashboard(
        record,
        undefined,
        "管理连接已经关闭，无法释放会话控制权。",
      );
      return;
    }
    this.#actionInFlight = true;
    try {
      const epoch = expectedControlEpoch ?? this.controlEpochForTarget(targetId);
      const snapshot = await connection.releaseSessionControl(targetId, epoch);
      if (
        generation !== this.#port.getManagementGeneration() ||
        this.#port.getManagement() !== connection
      ) {
        return;
      }
      this.#actionInFlight = false;
      const merged = this.#mergeSnapshot(snapshot, targetId);
      this.#port.renderDashboard(
        record,
        `已释放会话控制权（当前代际 ${merged.controlEpoch}）。`,
      );
    } catch (error) {
      if (generation !== this.#port.getManagementGeneration()) return;
      this.#actionInFlight = false;
      if (submit !== undefined) submit.disabled = false;
      const format = this.#port.formatError ?? managementErrorMessage;
      await this.refreshTargetAndRender(
        record,
        targetId,
        generation,
        undefined,
        `释放控制权失败：${format(error)}`,
      );
    }
  }
}
