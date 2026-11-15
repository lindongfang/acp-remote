import type { ViewDataFrame } from "../views/binary";
import { ViewDecodeError } from "../views/errors";
import {
  ViewRecordAssembler,
  type CompletedViewRecord,
} from "../views/records";

export type QueuedBodyReadErrorCode =
  | "body_too_large"
  | "invalid_stream";

export class QueuedBodyReadError extends Error {
  readonly code: QueuedBodyReadErrorCode;

  constructor(code: QueuedBodyReadErrorCode) {
    super(code);
    this.name = "QueuedBodyReadError";
    this.code = code;
  }
}

export type QueuedBodyReadUpdate =
  | { readonly state: "pending" }
  | { readonly state: "complete"; readonly preview: string };

/**
 * Joins the protocol record assembler to the one raw record that a queued
 * input body may display. A snapshot-end frame completes the body.
 */
export class QueuedBodyReader {
  readonly #expectedTotal: bigint;
  readonly #assembler: ViewRecordAssembler;
  #body: CompletedViewRecord | null = null;
  #sealed = false;

  constructor(streamId: string, expectedTotal: bigint) {
    if (
      typeof expectedTotal !== "bigint" ||
      expectedTotal < 0n ||
      expectedTotal > BigInt(Number.MAX_SAFE_INTEGER)
    ) {
      throw new QueuedBodyReadError("body_too_large");
    }
    try {
      this.#assembler = new ViewRecordAssembler(streamId);
    } catch {
      throw new QueuedBodyReadError("invalid_stream");
    }
    this.#expectedTotal = expectedTotal;
  }

  accept(frame: ViewDataFrame): QueuedBodyReadUpdate {
    if (this.#sealed) {
      throw new QueuedBodyReadError("invalid_stream");
    }
    try {
      const completed = this.#assembler.accept(frame);
      if (completed === null) return { state: "pending" };
      if (completed.kind === "original_raw") {
        if (this.#body !== null) {
          throw new QueuedBodyReadError("invalid_stream");
        }
        this.#body = completed;
        return { state: "pending" };
      }
      if (completed.kind !== "snapshot_end") {
        throw new QueuedBodyReadError("invalid_stream");
      }
      const body = this.#body;
      if (
        body === null ||
        body.length !== this.#expectedTotal
      ) {
        throw new QueuedBodyReadError("invalid_stream");
      }
      const preview = previewBody(body.bytes());
      this.#sealed = true;
      this.#assembler.seal();
      return Object.freeze({ state: "complete", preview });
    } catch (error) {
      this.#sealed = true;
      this.#assembler.seal();
      if (error instanceof QueuedBodyReadError) throw error;
      if (error instanceof ViewDecodeError) {
        throw new QueuedBodyReadError("invalid_stream");
      }
      throw new QueuedBodyReadError("invalid_stream");
    }
  }

  seal(): void {
    if (this.#sealed) return;
    this.#sealed = true;
    this.#assembler.seal();
    this.#body = null;
  }
}

function previewBody(bytes: Uint8Array): string {
  let raw: string;
  try {
    raw = new TextDecoder("utf-8", { fatal: true }).decode(bytes);
  } catch {
    throw new QueuedBodyReadError("invalid_stream");
  }
  try {
    return JSON.stringify(JSON.parse(raw), null, 2);
  } catch {
    return raw;
  }
}
