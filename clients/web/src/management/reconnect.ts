import { ManagementConnectionError, ManagementRequestError } from "./client";

export class ManagementReconnect {
  readonly #retry: () => void;
  #timer: ReturnType<typeof globalThis.setTimeout> | null = null;
  #attempt = 0;
  #delay: number | null = null;
  #nextAttemptAt: number | null = null;

  constructor(retry: () => void) {
    this.#retry = retry;
  }

  schedule(error: unknown): number | null {
    const minimumDelay = retryDelay(error);
    if (minimumDelay === null) {
      this.reset();
      return null;
    }
    if (this.#timer !== null) {
      return this.#delay;
    }
    this.#delay = Math.max(
      minimumDelay,
      Math.min(2_000 * 2 ** Math.min(this.#attempt, 4), 30_000),
    );
    this.#attempt += 1;
    this.#nextAttemptAt = Date.now() + this.#delay;
    this.#arm(this.#delay);
    return this.#delay;
  }

  resume(): void {
    if (this.#timer !== null) return;
    const remaining = Math.max((this.#nextAttemptAt ?? Date.now()) - Date.now(), 0);
    if (remaining > 0) {
      this.#delay = remaining;
      this.#arm(remaining);
    } else {
      this.#nextAttemptAt = null;
      this.#retry();
    }
  }

  #arm(delay: number): void {
    this.#timer = globalThis.setTimeout(() => {
      this.#timer = null;
      this.#delay = null;
      this.#nextAttemptAt = null;
      this.#retry();
    }, delay);
  }

  cancel(): void {
    if (this.#timer !== null) {
      globalThis.clearTimeout(this.#timer);
      this.#timer = null;
    }
    this.#delay = null;
  }

  reset(): void {
    this.cancel();
    this.#attempt = 0;
    this.#nextAttemptAt = null;
  }
}

function retryDelay(error: unknown): number | null {
  if (error instanceof ManagementConnectionError) {
    if (error.code === "revoked") return null;
    if (error.closeCode === 4429 || error.closeCode === 4409) return 60_000;
    if (
      error.code === "request_timeout" ||
      (error.code === "connection_closed" &&
        (error.closeCode === null ||
          [1001, 1006, 1011, 1012, 1013, 4408, 4500].includes(error.closeCode)))
    ) {
      return 2_000;
    }
  }
  if (error instanceof ManagementRequestError) {
    if (error.code === "rate_limited") return 60_000;
    if (["storage_unavailable", "node_stopping"].includes(error.code)) return 2_000;
  }
  return null;
}
