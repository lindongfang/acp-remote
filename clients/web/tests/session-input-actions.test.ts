import { describe, expect, it, vi } from "vitest";
import { ManagementRequestError, type ManagementConnection } from "../src/management/client";
import {
  editSessionText,
  submitSessionText,
  SessionInputActionError,
} from "../src/management/session-input-actions";
import { encodeBase64url } from "../src/protocol/jws";

function id(byte: number): string {
  return encodeBase64url(new Uint8Array(16).fill(byte));
}

describe("session input actions", () => {
  it.each(["begin", "chunk", "finish", "status"] as const)(
    "preserves the original top-level error at %s even when cleanup fails",
    async (stage) => {
      const error = new ManagementRequestError("commit_unconfirmed");
      const uploadId = id(9);
      const connection = {
        beginInputUpload: vi.fn(async () => {
          if (stage === "begin") throw error;
          return { uploadId };
        }),
        sendInputData: vi.fn(async (_id: string, offset: bigint, bytes: Uint8Array) => {
          if (stage === "chunk") throw error;
          return { uploadId, nextOffset: offset + BigInt(bytes.length) };
        }),
        finishInputUpload: vi.fn(async () => {
          if (stage === "finish") throw error;
          return { uploadId };
        }),
        inputUploadStatus: vi.fn(async () => { throw error; }),
        releaseInputUpload: vi.fn(async () => { throw new Error("cleanup unavailable"); }),
        registerSessionInput: vi.fn(),
        consumeSessionInput: vi.fn(),
      };
      await expect(submitSessionText(connection as unknown as ManagementConnection, id(8), "body")).rejects.toBe(error);
      expect(connection.releaseInputUpload).toHaveBeenCalledTimes(stage === "begin" ? 0 : 1);
      expect(connection.registerSessionInput).not.toHaveBeenCalled();
      expect(connection.consumeSessionInput).not.toHaveBeenCalled();
      expect(connection.sendInputData).toHaveBeenCalledTimes(stage === "begin" ? 0 : 1);
      expect(connection.finishInputUpload).toHaveBeenCalledTimes(["begin", "chunk"].includes(stage) ? 0 : 1);
      expect(connection.inputUploadStatus).toHaveBeenCalledTimes(stage === "status" ? 1 : 0);
    },
  );
  it("uploads bounded chunks in order before registering and consuming", async () => {
    const targetId = id(1);
    const uploadId = id(2);
    const bodyId = id(3);
    const chunks: Uint8Array[] = [];
    let finished = 0n;
    let registeredMutation: unknown;
    const connection = {
      beginInputUpload: vi.fn(async () => ({ uploadId })),
      sendInputData: vi.fn(async (
        actualUploadId: string,
        offset: bigint,
        bytes: Uint8Array,
      ) => {
        expect(actualUploadId).toBe(uploadId);
        chunks.push(bytes.slice());
        return { uploadId, nextOffset: offset + BigInt(bytes.length) };
      }),
      inputUploadStatus: vi.fn(async () => ({
        uploadId,
        phase: "ready" as const,
        nextOffset: String(finished),
        bodyId,
      })),
      finishInputUpload: vi.fn(async (_uploadId: string, total: bigint) => {
        finished = total;
        return { uploadId };
      }),
      releaseInputUpload: vi.fn(async () => ({ released: true as const })),
      registerSessionInput: vi.fn(async (
        actualTargetId: string,
        mutation: unknown,
        requestId: string,
      ) => {
        expect(actualTargetId).toBe(targetId);
        registeredMutation = mutation;
        return {
          registrationId: requestId,
          state: "registered" as const,
          action: "input.submit" as const,
          targetId,
          registerAuthGeneration: "1",
          registeredAtMs: "1700000000000",
        };
      }),
      consumeSessionInput: vi.fn(async (
        actualTargetId: string,
        registrationId: string,
        actualBodyId: string | null,
      ) => {
        expect(actualTargetId).toBe(targetId);
        expect(actualBodyId).toBe(bodyId);
        return {
          kind: "resolved" as const,
          returned: false,
          request: {
            registrationId,
            state: "settled" as const,
            outcome: {
              kind: "queued" as const,
              itemId: id(10),
              revision: "1",
            },
          },
        };
      }),
    } as unknown as ManagementConnection;

    const text = "x".repeat(20_000);
    const result = await submitSessionText(connection, targetId, text);
    expect(chunks).toHaveLength(2);
    expect(chunks[0]?.length).toBe(16_384);
    expect(chunks[1]?.length).toBeGreaterThan(0);
    const uploaded = new Uint8Array(
      chunks.reduce((length, chunk) => length + chunk.length, 0),
    );
    let position = 0;
    for (const chunk of chunks) {
      uploaded.set(chunk, position);
      position += chunk.length;
    }
    expect(JSON.parse(new TextDecoder().decode(uploaded))).toEqual([
      { type: "text", text },
    ]);
    expect(registeredMutation).toEqual({ kind: "submit", bodyId });
    expect(result.fact.state).toBe("settled");
    expect(connection.releaseInputUpload).not.toHaveBeenCalled();
  });

  it("releases a failed edit upload and preserves the failure code", async () => {
    const targetId = id(4);
    const uploadId = id(5);
    const connection = {
      beginInputUpload: vi.fn(async () => ({ uploadId })),
      sendInputData: vi.fn(async (
        _uploadId: string,
        offset: bigint,
        bytes: Uint8Array,
      ) => ({
        uploadId,
        nextOffset: offset + BigInt(bytes.length),
      })),
      finishInputUpload: vi.fn(async (
        _uploadId: string,
        _total: bigint,
      ) => ({ uploadId })),
      inputUploadStatus: vi.fn(async () => ({
        uploadId,
        phase: "failed" as const,
        nextOffset: "7",
        error: "invalid_request",
      })),
      releaseInputUpload: vi.fn(async () => ({ released: true as const })),
    } as unknown as ManagementConnection;

    await expect(
      editSessionText(connection, targetId, id(6), "4", "replacement"),
    ).rejects.toMatchObject({
      name: "SessionInputActionError",
      code: "upload_failed",
    });
    expect(connection.releaseInputUpload).toHaveBeenCalledWith(uploadId);
    expect(connection.registerSessionInput).toBeUndefined();
  });

  it("surfaces pending and refused consume outcomes with detail", async () => {
    const targetId = id(11);
    const uploadId = id(12);
    const bodyId = id(13);
    const baseConnection = {
      beginInputUpload: vi.fn(async () => ({ uploadId })),
      sendInputData: vi.fn(async (_id: string, offset: bigint, bytes: Uint8Array) => ({
        uploadId,
        nextOffset: offset + BigInt(bytes.length),
      })),
      finishInputUpload: vi.fn(async () => ({ uploadId })),
      inputUploadStatus: vi.fn(async () => ({
        uploadId,
        phase: "ready" as const,
        nextOffset: "20",
        bodyId,
      })),
      releaseInputUpload: vi.fn(async () => ({ released: true as const })),
      registerSessionInput: vi.fn(async (
        actualTargetId: string,
        _mutation: unknown,
        requestId: string,
      ) => ({
        registrationId: requestId,
        state: "registered" as const,
        action: "input.submit" as const,
        targetId: actualTargetId,
        registerAuthGeneration: "1",
        registeredAtMs: "1700000000000",
      })),
    };

    const pendingConnection = {
      ...baseConnection,
      consumeSessionInput: vi.fn(async (_t: string, registrationId: string) => ({
        kind: "pending" as const,
        registrationId,
      })),
    } as unknown as ManagementConnection;
    await expect(
      submitSessionText(pendingConnection, targetId, "hello"),
    ).rejects.toMatchObject({
      name: "SessionInputActionError",
      code: "consume_pending",
    });

    const refusedConnection = {
      ...baseConnection,
      consumeSessionInput: vi.fn(async () => ({
        kind: "refused" as const,
        reason: "request_retired",
        returned: true,
      })),
    } as unknown as ManagementConnection;
    await expect(
      submitSessionText(refusedConnection, targetId, "hello"),
    ).rejects.toMatchObject({
      name: "SessionInputActionError",
      code: "consume_refused",
      detail: "request_retired",
    });
  });

  it("rejects whitespace-only input before allocating an upload", async () => {
    let called = false;
    const connection = {
      beginInputUpload: vi.fn(async () => {
        called = true;
        return { uploadId: id(7) };
      }),
    } as unknown as ManagementConnection;
    await expect(
      submitSessionText(connection, id(8), "  \n "),
    ).rejects.toThrowError(SessionInputActionError);
    expect(called).toBe(false);
  });
});
