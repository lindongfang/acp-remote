import type { SessionResumeResult } from "./session-protocol";

export interface SessionResumeOperation {
  readonly targetId: string;
  readonly registrationId: string;
  readonly consumeRequestId: string;
}

export interface SessionResumeIds {
  readonly registrationId: string;
  readonly consumeRequestId: string;
}

/**
 * Keeps one explicit-resume operation pending until the node settles it.
 * A transport failure therefore retries the same registration and consume
 * identities instead of accepting the old inputs under a new operation.
 */
export class SessionResumeCoordinator {
  #pending: SessionResumeOperation | null = null;

  clear(): void {
    this.#pending = null;
  }

  async run(
    targetId: string,
    createIds: () => SessionResumeIds,
    register: (targetId: string, registrationId: string) => Promise<unknown>,
    consume: (
      targetId: string,
      registrationId: string,
      consumeRequestId: string,
    ) => Promise<SessionResumeResult>,
  ): Promise<SessionResumeResult> {
    const operation = this.#operation(targetId, createIds);
    await register(targetId, operation.registrationId);
    const result = await consume(
      targetId,
      operation.registrationId,
      operation.consumeRequestId,
    );
    if (this.#pending === operation) {
      this.#pending = null;
    }
    return result;
  }

  #operation(
    targetId: string,
    createIds: () => SessionResumeIds,
  ): SessionResumeOperation {
    if (this.#pending?.targetId === targetId) {
      return this.#pending;
    }
    const ids = createIds();
    const operation = Object.freeze({
      targetId,
      registrationId: ids.registrationId,
      consumeRequestId: ids.consumeRequestId,
    });
    this.#pending = operation;
    return operation;
  }
}
