import { randomId128 } from "../protocol/pairing";
import type { ManagementConnection } from "./client";
import type {
  InputUploadStatus,
  QueueMutationDto,
  SessionConsumeResult,
  SessionInputFact,
  SessionRegistration,
} from "./session-protocol";

const INPUT_CHUNK_BYTES = 16_384;
const PREPARATION_TIMEOUT_MS = 30_000;
const PREPARATION_POLL_MS = 250;
const ENCODER = new TextEncoder();

export interface CompletedSessionUpload {
  readonly uploadId: string;
  readonly bodyId: string;
  readonly totalBytes: bigint;
}

export interface RegisteredSessionInput {
  readonly upload: CompletedSessionUpload;
  readonly registration: SessionRegistration;
  readonly fact: SessionInputFact;
}

export type SessionInputActionErrorCode =
  | "empty_input"
  | "upload_failed"
  | "preparation_timeout"
  | "consume_pending"
  | "consume_refused";

export class SessionInputActionError extends Error {
  readonly code: SessionInputActionErrorCode;
  readonly detail: string | null;

  constructor(
    code: SessionInputActionErrorCode,
    detail: string | null = null,
  ) {
    super(code);
    this.name = "SessionInputActionError";
    this.code = code;
    this.detail = detail;
  }
}

export function encodeTextInput(text: string): Uint8Array {
  if (typeof text !== "string" || text.trim().length === 0) {
    throw new SessionInputActionError("empty_input");
  }
  return ENCODER.encode(JSON.stringify([{ type: "text", text }]));
}

export async function uploadSessionText(
  connection: ManagementConnection,
  targetId: string,
  purpose: "submit" | "edit",
  text: string,
): Promise<CompletedSessionUpload> {
  const bytes = encodeTextInput(text);
  const { uploadId } = await connection.beginInputUpload(targetId, purpose);
  try {
    let offset = 0n;
    while (offset < BigInt(bytes.length)) {
      const start = Number(offset);
      const chunk = bytes.subarray(
        start,
        Math.min(start + INPUT_CHUNK_BYTES, bytes.length),
      );
      const acknowledgement = await connection.sendInputData(
        uploadId,
        offset,
        chunk,
      );
      if (
        acknowledgement.uploadId !== uploadId ||
        acknowledgement.nextOffset <= offset
      ) {
        throw new SessionInputActionError("upload_failed");
      }
      offset = acknowledgement.nextOffset;
    }
    if (offset !== BigInt(bytes.length)) {
      throw new SessionInputActionError("upload_failed");
    }
    await connection.finishInputUpload(uploadId, offset);
    const status = await waitForPreparedUpload(connection, uploadId);
    if (status.phase !== "ready") {
      throw new SessionInputActionError("upload_failed");
    }
    return Object.freeze({
      uploadId,
      bodyId: status.bodyId,
      totalBytes: offset,
    });
  } catch (error) {
    try {
      await connection.releaseInputUpload(uploadId);
    } catch {
      // A transport failure can make the upload unavailable to release.
    }
    throw error;
  }
}

export async function submitSessionText(
  connection: ManagementConnection,
  targetId: string,
  text: string,
): Promise<RegisteredSessionInput> {
  const upload = await uploadSessionText(
    connection,
    targetId,
    "submit",
    text,
  );
  try {
    return await registerAndConsume(
      connection,
      targetId,
      { kind: "submit", bodyId: upload.bodyId },
      upload,
    );
  } catch (error) {
    await releaseAfterRegistrationFailure(connection, upload.uploadId);
    throw error;
  }
}

export async function editSessionText(
  connection: ManagementConnection,
  targetId: string,
  itemId: string,
  expectedRevision: string,
  text: string,
): Promise<RegisteredSessionInput> {
  const upload = await uploadSessionText(
    connection,
    targetId,
    "edit",
    text,
  );
  try {
    return await registerAndConsume(
      connection,
      targetId,
      {
        kind: "edit",
        itemId,
        expectedRevision,
        bodyId: upload.bodyId,
      },
      upload,
    );
  } catch (error) {
    await releaseAfterRegistrationFailure(connection, upload.uploadId);
    throw error;
  }
}

export async function cancelSessionInput(
  connection: ManagementConnection,
  targetId: string,
  itemId: string,
  expectedRevision: string,
): Promise<SessionInputFact> {
  return await registerControlAndConsume(connection, targetId, {
    kind: "cancel",
    itemId,
    expectedRevision,
  });
}

export async function prioritizeSessionInput(
  connection: ManagementConnection,
  targetId: string,
  itemId: string,
  expectedRevision: string,
): Promise<SessionInputFact> {
  return await registerControlAndConsume(connection, targetId, {
    kind: "prioritize",
    itemId,
    expectedRevision,
  });
}

async function waitForPreparedUpload(
  connection: ManagementConnection,
  uploadId: string,
): Promise<InputUploadStatus> {
  const deadline = Date.now() + PREPARATION_TIMEOUT_MS;
  for (;;) {
    const status = await connection.inputUploadStatus(uploadId);
    if (status.uploadId !== uploadId) {
      throw new SessionInputActionError("upload_failed");
    }
    if (status.phase === "ready") {
      return status;
    }
    if (status.phase === "failed") {
      throw new SessionInputActionError("upload_failed");
    }
    if (Date.now() >= deadline) {
      throw new SessionInputActionError("preparation_timeout");
    }
    await delay(PREPARATION_POLL_MS);
  }
}

async function registerControlAndConsume(
  connection: ManagementConnection,
  targetId: string,
  mutation: Extract<
    QueueMutationDto,
    { readonly kind: "cancel" | "prioritize" }
  >,
): Promise<SessionInputFact> {
  const registrationId = randomId128();
  const registration = await connection.registerSessionInput(
    targetId,
    mutation,
    registrationId,
  );
  const consumed = await connection.consumeSessionInput(
    targetId,
    registration.registrationId,
    null,
  );
  return extractConsumedFact(consumed);
}

async function registerAndConsume(
  connection: ManagementConnection,
  targetId: string,
  mutation: Extract<
    QueueMutationDto,
    { readonly kind: "submit" | "edit" }
  >,
  upload: CompletedSessionUpload,
): Promise<RegisteredSessionInput> {
  const registrationId = randomId128();
  const registration = await connection.registerSessionInput(
    targetId,
    mutation,
    registrationId,
  );
  const consumed = await connection.consumeSessionInput(
    targetId,
    registration.registrationId,
    upload.bodyId,
  );
  const fact = extractConsumedFact(consumed);
  return Object.freeze({ upload, registration, fact });
}

function extractConsumedFact(consumed: SessionConsumeResult): SessionInputFact {
  switch (consumed.kind) {
    case "resolved":
      return consumed.request;
    case "observed":
      return consumed.fact;
    case "pending":
      throw new SessionInputActionError("consume_pending", consumed.registrationId);
    case "refused":
      throw new SessionInputActionError("consume_refused", consumed.reason);
  }
}

function delay(milliseconds: number): Promise<void> {
  return new Promise((resolve) => {
    globalThis.setTimeout(resolve, milliseconds);
  });
}

async function releaseAfterRegistrationFailure(
  connection: ManagementConnection,
  uploadId: string,
): Promise<void> {
  try {
    await connection.releaseInputUpload(uploadId);
  } catch {
    // The result is already unknown after a transport or node failure.
  }
}
