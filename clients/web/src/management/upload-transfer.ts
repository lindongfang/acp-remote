import { CatalogActionError } from "./catalog-documents";
import {
  INPUT_CHUNK_BYTES,
  type InputAcknowledgement,
} from "./input-transfer";
import type { CatalogUploadStatus } from "./session-protocol";

export { INPUT_CHUNK_BYTES };
export const CATALOG_PREPARATION_POLL_MS = 250;
export const CATALOG_PREPARATION_TIMEOUT_MS = 30_000;

export interface PendingInputAcknowledgement {
  readonly uploadId: string;
  readonly expectedNextOffset: bigint;
  readonly resolve: (acknowledgement: InputAcknowledgement) => void;
  readonly reject: (error: Error) => void;
  readonly timeout: ReturnType<typeof globalThis.setTimeout>;
}

export class InflightInputAcknowledgementTracker {
  #pending: PendingInputAcknowledgement | null = null;

  get pending(): PendingInputAcknowledgement | null {
    return this.#pending;
  }

  track(pending: PendingInputAcknowledgement): void {
    this.#pending = pending;
  }

  accept(ack: InputAcknowledgement): "matched" | "mismatch" | "no_pending" {
    const pending = this.#pending;
    if (pending === null) {
      return "no_pending";
    }
    if (
      ack.uploadId !== pending.uploadId ||
      ack.nextOffset !== pending.expectedNextOffset
    ) {
      return "mismatch";
    }
    this.#pending = null;
    globalThis.clearTimeout(pending.timeout);
    pending.resolve(ack);
    return "matched";
  }

  reject(error: Error): void {
    const pending = this.#pending;
    if (pending === null) {
      return;
    }
    this.#pending = null;
    globalThis.clearTimeout(pending.timeout);
    pending.reject(error);
  }

  clear(): void {
    if (this.#pending !== null) {
      globalThis.clearTimeout(this.#pending.timeout);
      this.#pending = null;
    }
  }
}

export async function uploadBinaryChunks(
  uploadId: string,
  bytes: Uint8Array,
  sendChunk: (
    uploadId: string,
    offset: bigint,
    chunk: Uint8Array,
  ) => Promise<{ readonly uploadId: string; readonly nextOffset: bigint }>,
  chunkSize: number = INPUT_CHUNK_BYTES,
): Promise<bigint> {
  let offset = 0n;
  while (offset < BigInt(bytes.length)) {
    const start = Number(offset);
    const chunk = bytes.subarray(
      start,
      Math.min(start + chunkSize, bytes.length),
    );
    const ack = await sendChunk(uploadId, offset, chunk);
    if (ack.uploadId !== uploadId || ack.nextOffset <= offset) {
      throw new CatalogActionError("upload_failed");
    }
    offset = ack.nextOffset;
  }
  if (offset !== BigInt(bytes.length)) {
    throw new CatalogActionError("upload_failed");
  }
  return offset;
}

export async function waitForCatalogUploadReady(
  uploadId: string,
  checkStatus: (uploadId: string) => Promise<CatalogUploadStatus>,
  timeoutMs: number = CATALOG_PREPARATION_TIMEOUT_MS,
  pollIntervalMs: number = CATALOG_PREPARATION_POLL_MS,
): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const status = await checkStatus(uploadId);
    if (status.uploadId !== uploadId) {
      throw new CatalogActionError("upload_failed");
    }
    if (status.phase === "ready") {
      break;
    }
    if (status.phase === "failed") {
      throw new CatalogActionError("upload_failed", status.error);
    }
    if (Date.now() >= deadline) {
      throw new CatalogActionError("preparation_timeout");
    }
    await new Promise<void>((resolve) => {
      globalThis.setTimeout(resolve, pollIntervalMs);
    });
  }
}

export async function safeReleaseUpload(
  releaseFn: () => Promise<unknown>,
): Promise<void> {
  try {
    await releaseFn();
  } catch {
    // Upload buffer may already be released or connection closed.
  }
}
