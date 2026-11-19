import type { IdentityRecord } from "../pairing/identity";
import type { ManagementConnection } from "../management/client";
import type { SessionQueueEntry } from "../management/session-protocol";
import {
  QueuedBodyReadError,
  QueuedBodyReader,
  type QueuedBodyReadUpdate,
} from "../management/session-body-read";
import type { ViewDataFrame } from "../views/binary";
import { managementErrorMessage } from "./error-formatting";

export interface ActiveBodyRead {
  readonly streamId: string;
  readonly reader: QueuedBodyReaderLike;
}

export interface QueuedBodyReaderLike {
  accept(frame: ViewDataFrame): QueuedBodyReadUpdate;
  seal(): void;
}

export type QueuedBodyReaderFactory = (
  streamId: string,
  totalBytes: bigint,
) => QueuedBodyReaderLike;

export interface QueuedBodyHostPort {
  readonly getManagement: () => ManagementConnection | null;
  readonly getManagementGeneration: () => number;
  readonly getInspectedQueueTargetId: () => string | null;
  readonly renderDashboard: (
    record: IdentityRecord,
    notice?: string,
    failure?: string,
  ) => void;
  readonly readerFactory?: QueuedBodyReaderFactory;
  readonly formatError?: (error: unknown) => string;
  readonly timerApi?: {
    setTimeout: (fn: () => void, ms: number) => number;
    clearTimeout: (id: number) => void;
  };
}

export class QueuedBodyController {
  readonly #port: QueuedBodyHostPort;

  #activeBodyRead: ActiveBodyRead | null = null;
  #bodyReadTimer: number | null = null;
  #bodyReadAttempt = 0;
  #bodyPreview: string | null = null;

  constructor(port: QueuedBodyHostPort) {
    this.#port = port;
  }

  get activeBodyRead(): ActiveBodyRead | null {
    return this.#activeBodyRead;
  }

  get bodyPreview(): string | null {
    return this.#bodyPreview;
  }

  get bodyReadAttempt(): number {
    return this.#bodyReadAttempt;
  }

  reset(): void {
    ++this.#bodyReadAttempt;
    if (this.#activeBodyRead !== null) {
      this.#activeBodyRead.reader.seal();
      try {
        this.#port.getManagement()?.detachSessionStream(this.#activeBodyRead.streamId);
      } catch {}
    }
    this.#activeBodyRead = null;
    this.#bodyPreview = null;
    this.#clearTimer();
  }

  cancelActiveBodyRead(): void {
    this.reset();
  }

  async openQueueBody(
    record: IdentityRecord,
    targetId: string,
    entry: SessionQueueEntry,
    submit: HTMLButtonElement,
  ): Promise<void> {
    const connection = this.#port.getManagement();
    const generation = this.#port.getManagementGeneration();
    if (connection === null) {
      this.#port.renderDashboard(
        record,
        undefined,
        "管理连接已经关闭，无法读取正文。",
      );
      return;
    }
    const attempt = ++this.#bodyReadAttempt;
    const previous = this.#activeBodyRead;
    this.#activeBodyRead = null;
    this.#bodyPreview = null;
    this.#clearTimer();
    if (previous !== null) {
      previous.reader.seal();
      try {
        connection.detachSessionStream(previous.streamId);
      } catch {
        /* Previous read is already locally sealed. */
      }
    }
    let openedStreamId: string | null = null;
    try {
      const descriptor = await connection.openSessionBody(
        targetId,
        entry.itemId,
        entry.revision,
      );
      if (
        generation !== this.#port.getManagementGeneration() ||
        this.#port.getManagement() !== connection ||
        this.#port.getInspectedQueueTargetId() !== targetId ||
        attempt !== this.#bodyReadAttempt
      ) {
        try {
          connection.detachSessionStream(descriptor.streamId);
        } catch {
          /* Stale response belongs to original connection. */
        }
        return;
      }
      openedStreamId = descriptor.streamId;
      const factory =
        this.#port.readerFactory ??
        ((streamId, totalBytes) => new QueuedBodyReader(streamId, totalBytes));
      const reader = factory(descriptor.streamId, BigInt(descriptor.totalBytes));
      this.#activeBodyRead = {
        streamId: descriptor.streamId,
        reader,
      };
      this.#bodyPreview = null;

      const timerApi = this.#port.timerApi ?? globalThis;
      this.#bodyReadTimer = timerApi.setTimeout(() => {
        if (
          attempt === this.#bodyReadAttempt &&
          generation === this.#port.getManagementGeneration() &&
          this.#port.getManagement() === connection
        ) {
          this.failQueueBodyRead(record, generation, "读取正文超时。");
        }
      }, 30_000);

      connection.sendSessionViewCredit(descriptor.streamId, 0n);
      this.#port.renderDashboard(record, "正在逐块读取队列正文…");
    } catch (error) {
      if (
        generation !== this.#port.getManagementGeneration() ||
        this.#port.getManagement() !== connection ||
        this.#port.getInspectedQueueTargetId() !== targetId ||
        attempt !== this.#bodyReadAttempt
      ) {
        return;
      }
      const format = this.#port.formatError ?? managementErrorMessage;
      const detail =
        error instanceof QueuedBodyReadError && error.code === "body_too_large"
          ? "正文过大，无法在当前页面安全显示。"
          : format(error);
      if (
        openedStreamId !== null &&
        this.#activeBodyRead?.streamId === openedStreamId
      ) {
        this.failQueueBodyRead(record, generation, detail);
        submit.disabled = false;
        return;
      }
      if (openedStreamId !== null) {
        try {
          connection.detachSessionStream(openedStreamId);
        } catch {
          // The connection may already have ended while opening the body.
        }
      }
      submit.disabled = false;
      this.#port.renderDashboard(
        record,
        undefined,
        `正文读取未完成：${detail}`,
      );
    }
  }

  acceptViewFrame(
    record: IdentityRecord,
    generation: number,
    frame: ViewDataFrame,
  ): boolean {
    if (generation !== this.#port.getManagementGeneration()) return false;
    const read = this.#activeBodyRead;
    if (read === null || frame.streamId !== read.streamId) return false;
    let update: QueuedBodyReadUpdate;
    try {
      update = read.reader.accept(frame);
    } catch (error) {
      const reason =
        error instanceof QueuedBodyReadError && error.code === "body_too_large"
          ? "正文过大，无法在当前页面安全显示。"
          : "正文流序号、偏移或内容不一致。";
      this.failQueueBodyRead(record, generation, reason);
      return true;
    }
    try {
      this.#port.getManagement()?.sendSessionViewCredit(
        frame.streamId,
        frame.packetSequence,
      );
    } catch (error) {
      const format = this.#port.formatError ?? managementErrorMessage;
      this.failQueueBodyRead(
        record,
        generation,
        `无法确认正文分块：${format(error)}`,
      );
      return true;
    }
    if (update.state === "complete") {
      this.#bodyPreview = update.preview;
      read.reader.seal();
      this.#activeBodyRead = null;
      this.#clearTimer();
      this.#port.renderDashboard(
        record,
        "队列输入正文已完整读取并显示。",
      );
    }
    return true;
  }

  failQueueBodyRead(
    record: IdentityRecord,
    generation: number,
    detail: string,
  ): void {
    if (this.#activeBodyRead !== null) {
      this.#activeBodyRead.reader.seal();
      try {
        this.#port.getManagement()?.detachSessionStream(this.#activeBodyRead.streamId);
      } catch {
        // The local read is discarded even when detach cannot be delivered.
      }
    }
    this.#activeBodyRead = null;
    this.#bodyPreview = null;
    this.#clearTimer();
    if (generation === this.#port.getManagementGeneration()) {
      this.#port.renderDashboard(record, undefined, detail);
    }
  }

  #clearTimer(): void {
    if (this.#bodyReadTimer !== null) {
      const timerApi = this.#port.timerApi ?? globalThis;
      timerApi.clearTimeout(this.#bodyReadTimer);
      this.#bodyReadTimer = null;
    }
  }
}
