import { randomId128 } from "../protocol/pairing";
import type { JsonValue } from "../protocol/jws";
import {
  CONTROL_PATH,
  CONTROL_PROTOCOL,
  type AuthBinding,
  type AuthenticatedFrame,
  type ChallengeFrame,
  type DevicePage,
  type ExpectedIdentity,
  type HelloFrame,
  type ManagementErrorCode,
  type ManagementFrame,
  type NodeInfo,
  type RevokeResult,
  type SessionControlChangedFrame,
  ManagementProtocolError,
  createClientProof,
  createHello,
  parseDevicePage,
  parseManagementFrame,
  parseNodeInfoResult,
  parseRevokeResult,
  parseSubscribeResult,
  randomNonce,
  verifyChallenge,
} from "./protocol";
import {
  decodeSessionServerBinary,
  parseApprovalDecisionResult,
  parseApprovalListResult,
  parseApprovalRegistration,
  parseApprovalResult,
  parseApprovalStatusResult,
  parseCatalogFactMetadata,
  parseCatalogInventory,
  parseCatalogReceiptMetadata,
  parseCatalogRegistration,
  parseCatalogUploadBeginResult,
  parseCatalogUploadFinishResult,
  parseCatalogUploadReleaseResult,
  parseCatalogUploadStatus,
  parseInputUploadBeginResult,
  parseInputUploadFinishResult,
  parseInputUploadReleaseResult,
  parseInputUploadStatus,
  parseNativeReleaseRegisterResult,
  parseNativeReleaseStatusResult,
  parseSessionCapacity,
  parseProductTargetRecord,
  parseSessionCommand,
  parseSessionConsumeResult,
  parseSessionControlSnapshot,
  parseSessionQueue,
  parseSessionRegistration,
  parseSessionQueuedBodyDescriptor,
  parseSessionViewClosed,
  parseSessionViewOpened,
  parseSourceSessionFact,
  parseSourceSessionRegistration,
  parseSourceSessionResultDescriptor,
  parseSourceSessionResultReleaseResult,
  parseWorkspaceRecord,
  requireSessionId,
  requireSessionU64,
  type ApprovalDecisionResult,
  type ApprovalListResult,
  type ApprovalRegistration,
  type ApprovalStatus,
  type ApprovalView,
  type CatalogFactMetadata,
  type CatalogInventory,
  type CatalogReceiptMetadata,
  type CatalogRegistration,
  type CatalogUploadBeginResult,
  type CatalogUploadFinishResult,
  type CatalogUploadReleaseResult,
  type CatalogUploadStatus,
  type InputUploadBeginResult,
  type InputUploadFinishResult,
  type InputUploadReleaseResult,
  type InputUploadStatus,
  type NativeReleaseRegistered,
  type NativeReleaseStatus,
  type SessionCapacity,
  type SessionControlSnapshot,
  type ProductTargetRecord,
  type QueueMutationDto,
  type SessionConsumeResult,
  type SessionQueuedBodyDescriptor,
  type SessionQueueSnapshot,
  type SessionRegistration,
  type SessionResumeResult,
  type SessionServerBinary,
  type SessionViewOpened,
  type SessionViewClosed,
  type SourceLaunchSelectionInput,
  type SourceOperationInput,
  type SourceSessionFact,
  type SourceSessionRegistration,
  type SourceSessionResultDescriptor,
  type SourceSessionResultReleaseResult,
  type WorkspaceRecord,
} from "./session-protocol";
import { encodeInputData } from "./input-transfer";
import {
  InflightInputAcknowledgementTracker,
  uploadBinaryChunks,
  waitForCatalogUploadReady,
  safeReleaseUpload,
} from "./upload-transfer";
import {
  CatalogActionError,
  utf16leBase64,
  buildInstallHarnessDocument,
  buildBindTargetInitialDocument,
} from "./catalog-documents";
export {
  CatalogActionError,
  utf16leBase64,
  buildInstallHarnessDocument,
  buildBindTargetInitialDocument,
};
export type {
  CatalogLaunchEnvInput,
  CatalogAdditionalDirectoryInput,
  CatalogProfileRecipeInput,
  InstallHarnessDocumentInput,
  BindTargetInitialDocumentInput,
} from "./catalog-documents";
import {
  encodeViewCredit,
  encodeViewDetach,
  type ViewDataFrame,
} from "../views/binary";

const AUTHENTICATION_TIMEOUT_MS = 20_000;
const REQUEST_TIMEOUT_MS = 15_000;
const CATALOG_ENCODER = new TextEncoder();
const NORMAL_CLOSURE = 1000;
const PROTOCOL_CLOSURE = 4400;
const SOCKET_OPEN = 1;

export type ManagementConnectionErrorCode =
  | "not_connected"
  | "connection_closed"
  | "request_timeout"
  | "revoked";

export class ManagementConnectionError extends Error {
  readonly code: ManagementConnectionErrorCode;
  readonly closeCode: number | null;
  readonly nodeVerified: boolean;

  constructor(
    code: ManagementConnectionErrorCode,
    closeCode: number | null = null,
    nodeVerified = false,
  ) {
    super(code);
    this.name = "ManagementConnectionError";
    this.code = code;
    this.closeCode = closeCode;
    this.nodeVerified = nodeVerified;
  }
}

export class ManagementRequestError extends Error {
  readonly code: ManagementErrorCode;

  constructor(code: ManagementErrorCode) {
    super(code);
    this.name = "ManagementRequestError";
    this.code = code;
  }
}

export class SessionConsumeOutcomeError extends Error {
  readonly code: "consume_pending" | "consume_refused";
  readonly detail: string;

  constructor(code: "consume_pending" | "consume_refused", detail: string) {
    super(detail);
    this.name = "SessionConsumeOutcomeError";
    this.code = code;
    this.detail = detail;
  }
}

export interface ManagementClose {
  readonly code: number;
  readonly reason: string;
  readonly wasClean: boolean;
  readonly error: Error;
}

export interface ManagementConnectionCallbacks {
  readonly onDevicesChanged?: (revision: number) => void;
  readonly onSessionControlChanged?: (frame: SessionControlChangedFrame) => void;
  readonly onViewData?: (frame: ViewDataFrame) => void;
  readonly onClose?: (close: ManagementClose) => void;
}

export interface ManagementConnectionOptions {
  readonly origin: string;
  readonly socketFactory?: (
    url: string,
    protocols: readonly string[],
  ) => WebSocket;
  readonly callbacks?: ManagementConnectionCallbacks;
}

type ConnectionState =
  | "idle"
  | "opening"
  | "awaiting_challenge"
  | "verifying_challenge"
  | "awaiting_authentication"
  | "authenticated"
  | "closing"
  | "closed";

interface PendingRequest {
  readonly resolve: (result: unknown) => void;
  readonly reject: (error: Error) => void;
  readonly parse: (value: JsonValue) => unknown | Promise<unknown>;
  readonly timeout: number;
}

export class ManagementConnection {
  readonly #origin: string;
  readonly #identity: ExpectedIdentity;
  readonly #privateKey: CryptoKey;
  readonly #socketFactory: (
    url: string,
    protocols: readonly string[],
  ) => WebSocket;
  readonly #callbacks: ManagementConnectionCallbacks;
  readonly #pending = new Map<string, PendingRequest>();
  #inbound: Promise<void> = Promise.resolve();
  readonly #inputAckTracker = new InflightInputAcknowledgementTracker();
  #state: ConnectionState = "idle";
  #socket: WebSocket | null = null;
  #hello: HelloFrame | null = null;
  #challenge: ChallengeFrame | null = null;
  #clientId: string | null = null;
  #authenticationTimer: number | null = null;
  #connectResolve: ((value: AuthenticatedFrame) => void) | null = null;
  #connectReject: ((error: Error) => void) | null = null;
  #closeNotified = false;
  #failure: Error | null = null;

  constructor(
    identity: ExpectedIdentity,
    privateKey: CryptoKey,
    options: ManagementConnectionOptions,
  ) {
    this.#identity = identity;
    this.#privateKey = privateKey;
    this.#origin = options.origin;
    this.#socketFactory =
      options.socketFactory ??
      ((url, protocols) => new WebSocket(url, [...protocols]));
    this.#callbacks = options.callbacks ?? {};
  }

  get clientId(): string | null {
    return this.#clientId;
  }

  async connect(): Promise<AuthenticatedFrame> {
    if (this.#state !== "idle") {
      throw new ManagementConnectionError("connection_closed");
    }
    const url = `${this.#origin.replace(/^https:/u, "wss:")}${CONTROL_PATH}`;
    let socket: WebSocket;
    try {
      socket = this.#socketFactory(url, [CONTROL_PROTOCOL]);
    } catch {
      this.#state = "closed";
      throw new ManagementConnectionError("connection_closed");
    }
    this.#socket = socket;
    socket.binaryType = "arraybuffer";
    this.#state = "opening";

    const connected = new Promise<AuthenticatedFrame>((resolve, reject) => {
      this.#connectResolve = resolve;
      this.#connectReject = reject;
    });
    socket.addEventListener("open", () => this.#onOpen());
    socket.addEventListener("message", (event) => this.#enqueueMessage(event.data));
    socket.addEventListener("error", () => this.#onTransportError());
    socket.addEventListener("close", (event) => this.#onClose(event));
    this.#authenticationTimer = globalThis.setTimeout(
      () =>
        this.#failProtocol(
          new ManagementConnectionError("request_timeout"),
          "authentication timeout",
        ),
      AUTHENTICATION_TIMEOUT_MS,
    );
    return await connected;
  }

  async nodeInfo(): Promise<NodeInfo> {
    return await this.#command("node.info", {}, (value) =>
      parseNodeInfoResult(value, this.#identity),
    );
  }

  async listDevices(cursor: string | null): Promise<DevicePage> {
    return await this.#command("device.list", { cursor }, parseDevicePage);
  }

  async revokeDevice(clientId: string): Promise<RevokeResult> {
    return await this.#command(
      "device.revoke",
      { clientId },
      parseRevokeResult,
    );
  }

  async subscribeDevices(): Promise<void> {
    await this.#request({
      v: 1,
      type: "subscribe",
      requestId: randomId128(),
      resource: "devices",
    }, parseSubscribeResult);
  }

  async inspectSessionQueue(targetId: string): Promise<SessionQueueSnapshot> {
    return await this.#command(
      "input.queue",
      { targetId: requireSessionId(targetId) },
      parseSessionQueue,
    );
  }

  async registerSessionResume(
    targetId: string,
    requestId: string,
    expectedControlEpoch?: string,
  ): Promise<SessionRegistration> {
    const payload: Record<string, JsonValue> = {
      targetId: requireSessionId(targetId),
      operation: { kind: "resume" },
    };
    if (expectedControlEpoch !== undefined) {
      payload.expectedControlEpoch = requireSessionU64(expectedControlEpoch);
    }
    const parsed = await this.#commandWithId(
      "input.register",
      payload,
      requireSessionId(requestId),
      parseSessionRegistration,
    );
    if (parsed.registrationId !== requestId) {
      throw new ManagementProtocolError("invalid_response");
    }
    return parsed;
  }

  async consumeSessionResume(
    targetId: string,
    registrationId: string,
    requestId: string,
    expectedControlEpoch?: string,
  ): Promise<SessionResumeResult> {
    const canonicalRegistrationId = requireSessionId(registrationId);
    const payload: Record<string, JsonValue> = {
      targetId: requireSessionId(targetId),
      registrationId: canonicalRegistrationId,
      bodyId: null,
    };
    if (expectedControlEpoch !== undefined) {
      payload.expectedControlEpoch = requireSessionU64(expectedControlEpoch);
    }
    const parsed = await this.#commandWithId(
      "input.consume",
      payload,
      requireSessionId(requestId),
      parseSessionConsumeResult,
    );
    switch (parsed.kind) {
      case "resolved": {
        const fact = parsed.request;
        if (fact.registrationId !== canonicalRegistrationId) {
          throw new ManagementProtocolError("invalid_response");
        }
        if (fact.state === "settled" && fact.outcome.kind === "resumed") {
          return Object.freeze({
            registrationId: fact.registrationId,
            restored: fact.outcome.restored,
            returned: parsed.returned,
          });
        }
        if (fact.state === "settled" && fact.outcome.kind === "rejected") {
          throw new SessionConsumeOutcomeError("consume_refused", fact.outcome.reason);
        }
        throw new ManagementProtocolError("invalid_response");
      }
      case "observed": {
        const fact = parsed.fact;
        if (fact.registrationId !== canonicalRegistrationId) {
          throw new ManagementProtocolError("invalid_response");
        }
        if (fact.state === "settled" && fact.outcome.kind === "resumed") {
          return Object.freeze({
            registrationId: fact.registrationId,
            restored: fact.outcome.restored,
            returned: false,
          });
        }
        if (fact.state === "registered" || fact.state === "applying") {
          throw new SessionConsumeOutcomeError("consume_pending", fact.registrationId);
        }
        if (fact.state === "settled" && fact.outcome.kind === "rejected") {
          throw new SessionConsumeOutcomeError("consume_refused", fact.outcome.reason);
        }
        if (fact.state === "interrupted" || fact.state === "retired") {
          throw new SessionConsumeOutcomeError("consume_refused", fact.state);
        }
        throw new ManagementProtocolError("invalid_response");
      }
      case "pending":
        if (parsed.registrationId !== canonicalRegistrationId) {
          throw new ManagementProtocolError("invalid_response");
        }
        throw new SessionConsumeOutcomeError("consume_pending", parsed.registrationId);
      case "refused":
        throw new SessionConsumeOutcomeError("consume_refused", parsed.reason);
    }
  }

  async beginInputUpload(
    targetId: string,
    purpose: "submit" | "edit",
  ): Promise<InputUploadBeginResult> {
    return await this.#command(
      "input.upload.begin",
      {
        targetId: requireSessionId(targetId),
        purpose,
      },
      parseInputUploadBeginResult,
    );
  }

  async sendInputData(
    uploadId: string,
    offset: bigint,
    bytes: Uint8Array,
  ): Promise<{ readonly uploadId: string; readonly nextOffset: bigint }> {
    if (this.#state !== "authenticated") {
      throw new ManagementConnectionError("not_connected");
    }
    if (this.#inputAckTracker.pending !== null) {
      throw new ManagementProtocolError("invalid_response");
    }
    const canonicalUploadId = requireSessionId(uploadId);
    const wire = encodeInputData(canonicalUploadId, offset, bytes);
    const expectedNextOffset = offset + BigInt(bytes.length);
    requireSessionU64(expectedNextOffset.toString());
    return await new Promise((resolve, reject) => {
      const timeout = globalThis.setTimeout(() => {
        this.#rejectInputAcknowledgement(
          new ManagementConnectionError("request_timeout"),
        );
        this.#failProtocol(
          new ManagementConnectionError("request_timeout"),
          "input acknowledgement timeout",
        );
      }, REQUEST_TIMEOUT_MS);
      this.#inputAckTracker.track({
        uploadId: canonicalUploadId,
        expectedNextOffset,
        resolve: (acknowledgement) =>
          resolve({
            uploadId: acknowledgement.uploadId,
            nextOffset: acknowledgement.nextOffset,
          }),
        reject,
        timeout,
      });
      try {
        this.#sendBinary(wire);
      } catch {
        this.#rejectInputAcknowledgement(
          new ManagementConnectionError("connection_closed"),
        );
        this.#failProtocol(
          new ManagementConnectionError("connection_closed"),
          "input send failed",
        );
      }
    });
  }

  async inputUploadStatus(uploadId: string): Promise<InputUploadStatus> {
    const canonicalUploadId = requireSessionId(uploadId);
    const parsed = await this.#command(
      "input.upload.status",
      { uploadId: canonicalUploadId },
      parseInputUploadStatus,
    );
    if (parsed.uploadId !== canonicalUploadId) {
      throw new ManagementProtocolError("invalid_response");
    }
    return parsed;
  }

  async finishInputUpload(
    uploadId: string,
    totalBytes: bigint,
  ): Promise<InputUploadFinishResult> {
    const canonicalUploadId = requireSessionId(uploadId);
    const parsed = await this.#command(
      "input.upload.finish",
      {
        uploadId: canonicalUploadId,
        totalBytes: requireSessionU64(totalBytes.toString()),
      },
      parseInputUploadFinishResult,
    );
    if (parsed.uploadId !== canonicalUploadId) {
      throw new ManagementProtocolError("invalid_response");
    }
    return parsed;
  }

  async releaseInputUpload(
    uploadId: string,
  ): Promise<InputUploadReleaseResult> {
    const canonicalUploadId = requireSessionId(uploadId);
    return await this.#command(
      "input.upload.release",
      { uploadId: canonicalUploadId },
      parseInputUploadReleaseResult,
    );
  }

  async beginCatalogUpload(): Promise<CatalogUploadBeginResult> {
    return await this.#command(
      "catalog.upload.begin",
      {},
      parseCatalogUploadBeginResult,
    );
  }

  async finishCatalogUpload(
    uploadId: string,
    totalBytes: bigint,
  ): Promise<CatalogUploadFinishResult> {
    const canonicalUploadId = requireSessionId(uploadId);
    const parsed = await this.#command(
      "catalog.upload.finish",
      {
        uploadId: canonicalUploadId,
        totalBytes: requireSessionU64(totalBytes.toString()),
      },
      parseCatalogUploadFinishResult,
    );
    if (parsed.uploadId !== canonicalUploadId) {
      throw new ManagementProtocolError("invalid_response");
    }
    return parsed;
  }

  async catalogUploadStatus(uploadId: string): Promise<CatalogUploadStatus> {
    const canonicalUploadId = requireSessionId(uploadId);
    const parsed = await this.#command(
      "catalog.upload.status",
      { uploadId: canonicalUploadId },
      parseCatalogUploadStatus,
    );
    if (parsed.uploadId !== canonicalUploadId) {
      throw new ManagementProtocolError("invalid_response");
    }
    return parsed;
  }

  async releaseCatalogUpload(
    uploadId: string,
  ): Promise<CatalogUploadReleaseResult> {
    const canonicalUploadId = requireSessionId(uploadId);
    return await this.#command(
      "catalog.upload.release",
      { uploadId: canonicalUploadId },
      parseCatalogUploadReleaseResult,
    );
  }

  async registerCatalogDocument(
    documentUploadId: string,
  ): Promise<CatalogRegistration> {
    const canonicalUploadId = requireSessionId(documentUploadId);
    return await this.#command(
      "catalog.register",
      { documentUploadId: canonicalUploadId },
      parseCatalogRegistration,
    );
  }

  async applyCatalogRequest(
    catalogRequestId: string,
  ): Promise<CatalogReceiptMetadata> {
    const canonicalRequestId = requireSessionId(catalogRequestId);
    const parsed = await this.#command(
      "catalog.apply",
      { catalogRequestId: canonicalRequestId },
      parseCatalogReceiptMetadata,
    );
    if (parsed.fact.catalogRequestId !== canonicalRequestId) {
      throw new ManagementProtocolError("invalid_response");
    }
    return parsed;
  }

  async inspectCatalogRequest(
    catalogRequestId: string,
  ): Promise<CatalogFactMetadata> {
    const canonicalRequestId = requireSessionId(catalogRequestId);
    const parsed = await this.#command(
      "catalog.request",
      { catalogRequestId: canonicalRequestId },
      parseCatalogFactMetadata,
    );
    if (parsed.catalogRequestId !== canonicalRequestId) {
      throw new ManagementProtocolError("invalid_response");
    }
    return parsed;
  }

  async executeCatalogDocument(
    document: Record<string, unknown>,
  ): Promise<CatalogReceiptMetadata> {
    const bytes = CATALOG_ENCODER.encode(JSON.stringify(document));
    const { uploadId } = await this.beginCatalogUpload();
    let registration: CatalogRegistration;
    try {
      await uploadBinaryChunks(
        uploadId,
        bytes,
        (id, offset, chunk) => this.sendInputData(id, offset, chunk),
      );
      await this.finishCatalogUpload(uploadId, BigInt(bytes.length));
      await waitForCatalogUploadReady(
        uploadId,
        (id) => this.catalogUploadStatus(id),
      );
      registration = await this.registerCatalogDocument(uploadId);
    } catch (error) {
      await safeReleaseUpload(() => this.releaseCatalogUpload(uploadId));
      throw error;
    }
    const receipt = await this.applyCatalogRequest(registration.catalogRequestId);
    if (receipt.fact.state !== "committed") {
      throw new CatalogActionError("request_rejected", receipt.fact.state);
    }
    return receipt;
  }

  async registerWorkspace(path: string): Promise<WorkspaceRecord> {
    const command = parseSessionCommand(
      JSON.stringify({
        v: 1,
        type: "command",
        requestId: randomId128(),
        action: "workspace.register",
        payload: { path },
      }),
    );
    if (command.action !== "workspace.register") {
      throw new ManagementProtocolError("invalid_response");
    }
    return await this.#commandWithId(
      command.action,
      command.payload,
      command.requestId,
      parseWorkspaceRecord,
    );
  }

  async registerTarget(
    sourceId: string,
    nativeSessionId: string,
    workspaceId: string,
  ): Promise<ProductTargetRecord> {
    const command = parseSessionCommand(
      JSON.stringify({
        v: 1,
        type: "command",
        requestId: randomId128(),
        action: "target.register",
        payload: {
          sourceId: requireSessionId(sourceId),
          nativeSessionId,
          workspaceId: requireSessionId(workspaceId),
        },
      }),
    );
    if (command.action !== "target.register") {
      throw new ManagementProtocolError("invalid_response");
    }
    return await this.#commandWithId(
      command.action,
      command.payload,
      command.requestId,
      parseProductTargetRecord,
    );
  }

  async inspectCatalogInventory(): Promise<CatalogInventory> {
    return await this.#command(
      "catalog.inventory",
      {},
      parseCatalogInventory,
    );
  }

  async registerSessionInput(
    targetId: string,
    mutation: QueueMutationDto,
    requestId: string,
    expectedControlEpoch?: string,
  ): Promise<SessionRegistration> {
    const canonicalRequestId = requireSessionId(requestId);
    const payload: Record<string, JsonValue> = {
      targetId: requireSessionId(targetId),
      operation: mutation,
    };
    if (expectedControlEpoch !== undefined) {
      payload.expectedControlEpoch = requireSessionU64(expectedControlEpoch);
    }
    const command = parseSessionCommand(JSON.stringify({
      v: 1,
      type: "command",
      requestId: canonicalRequestId,
      action: "input.register",
      payload,
    }));
    if (command.action !== "input.register") {
      throw new ManagementProtocolError("invalid_response");
    }
    const parsed = await this.#commandWithId(
      command.action,
      command.payload,
      command.requestId,
      parseSessionRegistration,
    );
    if (parsed.registrationId !== canonicalRequestId) {
      throw new ManagementProtocolError("invalid_response");
    }
    return parsed;
  }

  async consumeSessionInput(
    targetId: string,
    registrationId: string,
    bodyId: string | null,
    expectedControlEpoch?: string,
  ): Promise<SessionConsumeResult> {
    const canonicalRegistrationId = requireSessionId(registrationId);
    const payload: Record<string, JsonValue> = {
      targetId: requireSessionId(targetId),
      registrationId: canonicalRegistrationId,
      bodyId: bodyId === null ? null : requireSessionId(bodyId),
    };
    if (expectedControlEpoch !== undefined) {
      payload.expectedControlEpoch = requireSessionU64(expectedControlEpoch);
    }
    const parsed = await this.#command(
      "input.consume",
      payload,
      parseSessionConsumeResult,
    );
    if (
      (parsed.kind === "resolved" && parsed.request.registrationId !== canonicalRegistrationId) ||
      (parsed.kind === "observed" && parsed.fact.registrationId !== canonicalRegistrationId) ||
      (parsed.kind === "pending" && parsed.registrationId !== canonicalRegistrationId)
    ) {
      throw new ManagementProtocolError("invalid_response");
    }
    return parsed;
  }

  async openSessionBody(
    targetId: string,
    itemId: string,
    expectedRevision: string,
  ): Promise<SessionQueuedBodyDescriptor> {
    const canonicalTargetId = requireSessionId(targetId);
    const descriptor = await this.#command(
      "input.body.open",
      {
        targetId: canonicalTargetId,
        itemId: requireSessionId(itemId),
        expectedRevision: requireSessionU64(expectedRevision),
      },
      parseSessionQueuedBodyDescriptor,
    );
    if (descriptor.targetId !== canonicalTargetId) {
      throw new ManagementProtocolError("invalid_response");
    }
    return descriptor;
  }

  async openSessionView(
    targetId: string,
    reason:
      | "first_open"
      | "explicit_refresh"
      | "reopen"
      | "reconnect"
      | "reacquire",
  ): Promise<SessionViewOpened> {
    return await this.#command(
      "view.open",
      {
        targetId: requireSessionId(targetId),
        reason,
      },
      parseSessionViewOpened,
    );
  }

  async openSourceSessionView(
    selection: SourceLaunchSelectionInput,
    nativeSessionId: string,
    reason:
      | "first_open"
      | "explicit_refresh"
      | "reopen"
      | "reconnect"
      | "reacquire",
  ): Promise<SessionViewOpened> {
    const command = parseSessionCommand(JSON.stringify({
      v: 1,
      type: "command",
      requestId: randomId128(),
      action: "view.open_source",
      payload: {
        selection,
        nativeSessionId,
        reason,
      },
    }));
    if (command.action !== "view.open_source") {
      throw new ManagementProtocolError("invalid_response");
    }
    return await this.#commandWithId(
      command.action,
      command.payload,
      command.requestId,
      parseSessionViewOpened,
    );
  }

  async rebindSessionView(streamId: string): Promise<SessionViewOpened> {
    const canonicalStreamId = requireSessionId(streamId);
    return await this.#command(
      "view.rebind",
      { streamId: canonicalStreamId },
      parseSessionViewOpened,
    );
  }

  async closeSessionView(streamId: string): Promise<SessionViewClosed> {
    const canonicalStreamId = requireSessionId(streamId);
    const parsed = await this.#command(
      "view.close",
      { streamId: canonicalStreamId },
      parseSessionViewClosed,
    );
    if (parsed.streamId !== canonicalStreamId) {
      throw new ManagementProtocolError("invalid_response");
    }
    return parsed;
  }

  async listApprovals(): Promise<ApprovalListResult> {
    const parsed = await this.#command(
      "approval.list",
      {},
      parseApprovalListResult,
    );
    for (const approval of parsed.items) {
      if (approval.approvalId !== approval.status.approvalId) {
        throw new ManagementProtocolError("invalid_response");
      }
    }
    return parsed;
  }

  async readApproval(approvalId: string): Promise<ApprovalView> {
    const canonicalApprovalId = requireSessionId(approvalId);
    const parsed = await this.#command(
      "approval.read",
      { approvalId: canonicalApprovalId },
      parseApprovalResult,
    );
    if (
      parsed.approvalId !== canonicalApprovalId ||
      parsed.status.approvalId !== canonicalApprovalId
    ) {
      throw new ManagementProtocolError("invalid_response");
    }
    return parsed;
  }

  async registerApproval(
    approvalId: string,
    choiceId: string,
    expectedControlEpoch?: string,
  ): Promise<ApprovalRegistration> {
    const canonicalApprovalId = requireSessionId(approvalId);
    const canonicalChoiceId = requireSessionId(choiceId);
    const payload: Record<string, JsonValue> = {
      approvalId: canonicalApprovalId,
      choiceId: canonicalChoiceId,
    };
    if (expectedControlEpoch !== undefined) {
      payload.expectedControlEpoch = requireSessionU64(expectedControlEpoch);
    }
    const parsed = await this.#command(
      "approval.register",
      payload,
      parseApprovalRegistration,
    );
    if (
      parsed.approvalId !== canonicalApprovalId ||
      parsed.choiceId !== canonicalChoiceId
    ) {
      throw new ManagementProtocolError("invalid_response");
    }
    return parsed;
  }

  async decideApproval(
    approvalId: string,
    approvalRegistrationId: string,
    expectedControlEpoch?: string,
  ): Promise<ApprovalDecisionResult> {
    const canonicalApprovalId = requireSessionId(approvalId);
    const canonicalRegistrationId =
      requireSessionId(approvalRegistrationId);
    const payload: Record<string, JsonValue> = {
      approvalRegistrationId: canonicalRegistrationId,
    };
    if (expectedControlEpoch !== undefined) {
      payload.expectedControlEpoch = requireSessionU64(expectedControlEpoch);
    }
    const parsed = await this.#command(
      "approval.decide",
      payload,
      parseApprovalDecisionResult,
    );
    if (
      parsed.approvalId !== canonicalApprovalId ||
      parsed.approvalRegistrationId !== canonicalRegistrationId
    ) {
      throw new ManagementProtocolError("invalid_response");
    }
    return parsed;
  }

  async inspectApprovalStatus(approvalId: string): Promise<ApprovalStatus> {
    const canonicalApprovalId = requireSessionId(approvalId);
    const parsed = await this.#command(
      "approval.status",
      { approvalId: canonicalApprovalId },
      parseApprovalStatusResult,
    );
    if (parsed.approvalId !== canonicalApprovalId) {
      throw new ManagementProtocolError("invalid_response");
    }
    return parsed;
  }

  async registerNativeRelease(
    targetId: string,
  ): Promise<NativeReleaseRegistered> {
    const canonicalTargetId = requireSessionId(targetId);
    const parsed = await this.#command(
      "native.release.register",
      { targetId: canonicalTargetId },
      parseNativeReleaseRegisterResult,
    );
    if (parsed.targetId !== canonicalTargetId) {
      throw new ManagementProtocolError("invalid_response");
    }
    return parsed;
  }

  async releaseNativeSession(
    releaseOperationId: string,
  ): Promise<NativeReleaseStatus> {
    const canonicalOperationId = requireSessionId(releaseOperationId);
    const parsed = await this.#command(
      "native.release",
      { releaseOperationId: canonicalOperationId },
      parseNativeReleaseStatusResult,
    );
    if (parsed.releaseOperationId !== canonicalOperationId) {
      throw new ManagementProtocolError("invalid_response");
    }
    return parsed;
  }

  async inspectSessionCapacity(): Promise<SessionCapacity> {
    return this.#command("session.capacity.inspect", {}, parseSessionCapacity);
  }

  async inspectSessionControl(targetId: string): Promise<SessionControlSnapshot> {
    return await this.#command(
      "session.control.inspect",
      { targetId: requireSessionId(targetId) },
      parseSessionControlSnapshot,
    );
  }

  async takeoverSessionControl(targetId: string): Promise<SessionControlSnapshot> {
    return await this.#command(
      "session.control.takeover",
      { targetId: requireSessionId(targetId) },
      parseSessionControlSnapshot,
    );
  }

  async releaseSessionControl(
    targetId: string,
    expectedControlEpoch?: string,
  ): Promise<SessionControlSnapshot> {
    const payload: Record<string, JsonValue> = {
      targetId: requireSessionId(targetId),
    };
    if (expectedControlEpoch !== undefined) {
      payload.expectedControlEpoch = requireSessionU64(expectedControlEpoch);
    }
    return await this.#command(
      "session.control.release",
      payload,
      parseSessionControlSnapshot,
    );
  }

  async inspectNativeReleaseStatus(
    releaseOperationId: string,
  ): Promise<NativeReleaseStatus> {
    const canonicalOperationId = requireSessionId(releaseOperationId);
    const parsed = await this.#command(
      "native.release.status",
      { releaseOperationId: canonicalOperationId },
      parseNativeReleaseStatusResult,
    );
    if (parsed.releaseOperationId !== canonicalOperationId) {
      throw new ManagementProtocolError("invalid_response");
    }
    return parsed;
  }

  async registerSourceSession(
    selection: SourceLaunchSelectionInput,
    operation: SourceOperationInput,
    requestId = randomId128(),
  ): Promise<SourceSessionRegistration> {
    const canonicalRequestId = requireSessionId(requestId);
    const command = parseSessionCommand(JSON.stringify({
      v: 1,
      type: "command",
      requestId: canonicalRequestId,
      action: "source.session.register",
      payload: {
        selection,
        operation,
      },
    }));
    if (command.action !== "source.session.register") {
      throw new ManagementProtocolError("invalid_response");
    }
    const parsed = await this.#commandWithId(
      command.action,
      command.payload,
      command.requestId,
      parseSourceSessionRegistration,
    );
    if (
      parsed.action !== operation.kind ||
      parsed.sourceId !== command.payload.selection.sourceId ||
      parsed.harnessId !== command.payload.selection.harnessId ||
      parsed.profileId !== command.payload.selection.profileId ||
      parsed.profileRevision !== command.payload.selection.profileRevision ||
      parsed.workspaceId !== command.payload.selection.workspaceId
    ) {
      throw new ManagementProtocolError("invalid_response");
    }
    return parsed;
  }

  async executeSourceSession(
    sourceOperationId: string,
  ): Promise<SourceSessionFact> {
    const canonicalOperationId = requireSessionId(sourceOperationId);
    const parsed = await this.#command(
      "source.session.execute",
      { sourceOperationId: canonicalOperationId },
      parseSourceSessionFact,
    );
    if (parsed.registration.sourceOperationId !== canonicalOperationId) {
      throw new ManagementProtocolError("invalid_response");
    }
    return parsed;
  }

  async inspectSourceSessionStatus(
    sourceOperationId: string,
  ): Promise<SourceSessionFact> {
    const canonicalOperationId = requireSessionId(sourceOperationId);
    const parsed = await this.#command(
      "source.session.status",
      { sourceOperationId: canonicalOperationId },
      parseSourceSessionFact,
    );
    if (parsed.registration.sourceOperationId !== canonicalOperationId) {
      throw new ManagementProtocolError("invalid_response");
    }
    return parsed;
  }

  async reconcileSourceSession(
    sourceOperationId: string,
  ): Promise<SourceSessionFact> {
    const canonicalOperationId = requireSessionId(sourceOperationId);
    const parsed = await this.#command(
      "source.session.reconcile",
      { sourceOperationId: canonicalOperationId },
      parseSourceSessionFact,
    );
    if (parsed.registration.sourceOperationId !== canonicalOperationId) {
      throw new ManagementProtocolError("invalid_response");
    }
    return parsed;
  }

  async openSourceSessionResult(
    sourceOperationId: string,
  ): Promise<SourceSessionResultDescriptor> {
    const canonicalOperationId = requireSessionId(sourceOperationId);
    return await this.#command(
      "source.session.result.open",
      { sourceOperationId: canonicalOperationId },
      parseSourceSessionResultDescriptor,
    );
  }

  async releaseSourceSessionResult(
    sourceOperationId: string,
  ): Promise<SourceSessionResultReleaseResult> {
    const canonicalOperationId = requireSessionId(sourceOperationId);
    return await this.#command(
      "source.session.result.release",
      { sourceOperationId: canonicalOperationId },
      parseSourceSessionResultReleaseResult,
    );
  }

  sendSessionViewCredit(streamId: string, packetSequence: bigint): void {
    if (this.#state !== "authenticated") {
      throw new ManagementConnectionError("not_connected");
    }
    this.#sendBinary(encodeViewCredit(streamId, packetSequence));
  }

  detachSessionStream(streamId: string): void {
    if (this.#state !== "authenticated") {
      throw new ManagementConnectionError("not_connected");
    }
    this.#sendBinary(encodeViewDetach(streamId));
  }

  disconnect(): void {
    if (this.#state === "closed" || this.#state === "closing") {
      return;
    }
    this.#state = "closing";
    this.#clearAuthenticationTimer();
    this.#socket?.close(NORMAL_CLOSURE, "client closing");
  }

  #onOpen(): void {
    if (this.#state !== "opening") {
      this.#failProtocol(
        new ManagementProtocolError("invalid_frame"),
        "unexpected open",
      );
      return;
    }
    try {
      const hello = createHello(
        this.#identity.clientId,
        this.#identity.clientPublicKey,
        randomNonce(),
      );
      this.#hello = hello;
      this.#state = "awaiting_challenge";
      this.#send(hello);
    } catch {
      this.#failProtocol(
        new ManagementProtocolError("invalid_frame"),
        "cannot create hello",
      );
    }
  }

  #enqueueMessage(data: unknown): void {
    this.#inbound = this.#inbound
      .then(() => this.#processMessage(data))
      .catch(() => {
        // #processMessage records protocol failures on the connection.
      });
  }

  #processMessage(data: unknown): void {
    if (typeof data !== "string") {
      if (data instanceof ArrayBuffer) {
        this.#handleBinary(new Uint8Array(data));
        return;
      }
      if (ArrayBuffer.isView(data)) {
        this.#handleBinary(
          new Uint8Array(data.buffer, data.byteOffset, data.byteLength),
        );
        return;
      }
      this.#failProtocol(
        new ManagementProtocolError("invalid_frame"),
        "binary frame",
      );
      return;
    }
    let frame: ManagementFrame;
    try {
      frame = parseManagementFrame(data);
    } catch {
      this.#failProtocol(
        new ManagementProtocolError("invalid_frame"),
        "invalid frame",
      );
      return;
    }

    if (this.#state === "awaiting_challenge") {
      if (frame.type !== "challenge") {
        this.#failProtocol(
          new ManagementProtocolError("invalid_response"),
          "expected challenge",
        );
        return;
      }
      this.#state = "verifying_challenge";
      void this.#answerChallenge(frame);
      return;
    }

    if (this.#state === "awaiting_authentication") {
      if (frame.type !== "authenticated") {
        this.#failProtocol(
          new ManagementProtocolError("invalid_response"),
          "expected authentication",
        );
        return;
      }
      this.#completeAuthentication(frame);
      return;
    }

    if (this.#state !== "authenticated") {
      this.#failProtocol(
        new ManagementProtocolError("invalid_frame"),
        "unexpected frame",
      );
      return;
    }

    if (frame.type === "devices.changed") {
      this.#callbacks.onDevicesChanged?.(frame.revision);
      return;
    }
    if (frame.type === "session.control.changed") {
      this.#callbacks.onSessionControlChanged?.(frame);
      return;
    }
    if (frame.type === "result") {
      this.#handleResult(frame);
      return;
    }
    this.#failProtocol(
      new ManagementProtocolError("invalid_frame"),
      "unexpected frame",
    );
  }

  #handleBinary(wire: Uint8Array): void {
    if (this.#state !== "authenticated") {
      this.#failProtocol(
        new ManagementProtocolError("invalid_frame"),
        "binary before authentication",
      );
      return;
    }
    let frame: SessionServerBinary;
    try {
      frame = decodeSessionServerBinary(wire, "authenticated");
    } catch {
      this.#failProtocol(
        new ManagementProtocolError("invalid_frame"),
        "invalid binary frame",
      );
      return;
    }
    if (frame.kind === "view_data") {
      this.#callbacks.onViewData?.(frame.frame);
      return;
    }
    const status = this.#inputAckTracker.accept(frame.acknowledgement);
    if (status === "no_pending") {
      this.#failProtocol(
        new ManagementProtocolError("invalid_response"),
        "unexpected input acknowledgement",
      );
      return;
    }
    if (status === "mismatch") {
      this.#rejectInputAcknowledgement(
        new ManagementProtocolError("invalid_response"),
      );
      this.#failProtocol(
        new ManagementProtocolError("invalid_response"),
        "input acknowledgement mismatch",
      );
      return;
    }
  }

  async #answerChallenge(challenge: ChallengeFrame): Promise<void> {
    if (this.#hello === null) {
      this.#failProtocol(
        new ManagementProtocolError("invalid_response"),
        "missing hello",
      );
      return;
    }
    let binding: AuthBinding;
    let proof: string;
    try {
      binding = await verifyChallenge(
        challenge,
        this.#identity,
        this.#hello,
      );
      proof = await createClientProof(binding, this.#privateKey);
    } catch (error) {
      this.#failProtocol(
        error instanceof Error
          ? error
          : new ManagementProtocolError("host_proof_invalid"),
        "node authentication failed",
      );
      return;
    }
    if (this.#state !== "verifying_challenge") {
      return;
    }
    this.#challenge = challenge;
    this.#state = "awaiting_authentication";
    try {
      this.#send({ v: 1, type: "proof", proof });
    } catch {
      this.#failProtocol(
        new ManagementConnectionError("connection_closed"),
        "send failed",
      );
    }
  }

  #completeAuthentication(frame: AuthenticatedFrame): void {
    const challenge = this.#challenge;
    if (
      challenge === null ||
      frame.connectionId !== challenge.connectionId ||
      (this.#identity.clientId !== null &&
        frame.clientId !== this.#identity.clientId)
    ) {
      this.#failProtocol(
        new ManagementProtocolError("invalid_response"),
        "authentication mismatch",
      );
      return;
    }
    this.#clientId = frame.clientId;
    this.#state = "authenticated";
    this.#clearAuthenticationTimer();
    const resolve = this.#connectResolve;
    this.#connectResolve = null;
    this.#connectReject = null;
    resolve?.(frame);
  }

  async #command<T>(
    action: string,
    payload: Record<string, unknown>,
    parse: (value: JsonValue) => T | Promise<T>,
  ): Promise<T> {
    return await this.#commandWithId(action, payload, randomId128(), parse);
  }

  async #commandWithId<T>(
    action: string,
    payload: Record<string, unknown>,
    requestId: string,
    parse: (value: JsonValue) => T | Promise<T>,
  ): Promise<T> {
    return await this.#request({
      v: 1,
      type: "command",
      requestId,
      action,
      payload,
    }, parse);
  }

  async #request<T>(
    body: Record<string, unknown>,
    parse: (value: JsonValue) => T | Promise<T>,
  ): Promise<T> {
    if (this.#state !== "authenticated") {
      throw new ManagementConnectionError("not_connected");
    }
    const requestId = body.requestId;
    if (typeof requestId !== "string") {
      throw new ManagementProtocolError("invalid_response");
    }
    return await new Promise<T>((resolve, reject) => {
      const timeout = globalThis.setTimeout(() => {
        this.#pending.delete(requestId);
        reject(new ManagementConnectionError("request_timeout"));
        this.#failProtocol(
          new ManagementConnectionError("request_timeout"),
          "request timeout",
        );
      }, REQUEST_TIMEOUT_MS);
      this.#pending.set(requestId, {
        resolve: (value) => resolve(value as T),
        reject,
        parse,
        timeout,
      });
      try {
        this.#send(body);
      } catch {
        globalThis.clearTimeout(timeout);
        this.#pending.delete(requestId);
        reject(new ManagementConnectionError("connection_closed"));
        this.#failProtocol(
          new ManagementConnectionError("connection_closed"),
          "send failed",
        );
      }
    });
  }

  #handleResult(
    frame: Extract<ManagementFrame, { readonly type: "result" }>,
  ): void {
    const pending = this.#pending.get(frame.requestId);
    if (pending === undefined) {
      this.#failProtocol(
        new ManagementProtocolError("invalid_response"),
        "unexpected result",
      );
      return;
    }
    this.#pending.delete(frame.requestId);
    globalThis.clearTimeout(pending.timeout);
    if (!frame.ok) {
      if (frame.error === null) {
        this.#failProtocol(
          new ManagementProtocolError("invalid_response"),
          "missing error",
        );
        return;
      }
      pending.reject(new ManagementRequestError(frame.error.code));
      return;
    }
    void Promise.resolve(frame.result)
      .then((value) => pending.parse(value))
      .then((value) => pending.resolve(value))
      .catch(() => {
        pending.reject(
          new ManagementProtocolError("invalid_response"),
        );
        this.#failProtocol(
          new ManagementProtocolError("invalid_response"),
          "invalid result",
        );
      });
  }

  #onTransportError(): void {
    if (this.#state === "closed" || this.#state === "closing") {
      return;
    }
    const error = new ManagementConnectionError("connection_closed");
    this.#rejectConnection(error);
    this.#rejectPending(error);
    this.#rejectInputAcknowledgement(error);
  }

  #onClose(event: CloseEvent): void {
    const error =
      this.#failure === null
        ? new ManagementConnectionError(
            event.code === 4410 ? "revoked" : "connection_closed",
            event.code,
            this.#challenge !== null,
          )
        : this.#failure;
    const close: ManagementClose = {
      code: event.code,
      reason: event.reason,
      wasClean: event.wasClean,
      error,
    };
    this.#state = "closed";
    this.#clearAuthenticationTimer();
    this.#rejectConnection(error);
    this.#rejectPending(error);
    this.#rejectInputAcknowledgement(error);
    if (!this.#closeNotified) {
      this.#closeNotified = true;
      this.#callbacks.onClose?.(close);
    }
  }

  #send(value: object): void {
    const socket = this.#socket;
    if (socket === null || socket.readyState !== SOCKET_OPEN) {
      throw new ManagementConnectionError("connection_closed");
    }
    socket.send(JSON.stringify(value));
  }

  #sendBinary(value: Uint8Array): void {
    const socket = this.#socket;
    if (socket === null || socket.readyState !== SOCKET_OPEN) {
      throw new ManagementConnectionError("connection_closed");
    }
    const copy = new Uint8Array(value.length);
    copy.set(value);
    socket.send(copy);
  }

  #failProtocol(error: Error, reason: string): void {
    if (this.#state === "closed") {
      return;
    }
    this.#failure ??= error;
    this.#rejectConnection(error);
    this.#rejectPending(error);
    this.#rejectInputAcknowledgement(error);
    this.#state = "closing";
    this.#clearAuthenticationTimer();
    try {
      this.#socket?.close(PROTOCOL_CLOSURE, reason);
    } catch {
      // The close event still owns final resource cleanup.
    }
  }

  #rejectConnection(error: Error): void {
    const reject = this.#connectReject;
    this.#connectResolve = null;
    this.#connectReject = null;
    reject?.(error);
  }

  #rejectPending(error: Error): void {
    for (const [requestId, pending] of this.#pending) {
      this.#pending.delete(requestId);
      globalThis.clearTimeout(pending.timeout);
      pending.reject(error);
    }
  }

  #rejectInputAcknowledgement(error: Error): void {
    this.#inputAckTracker.reject(error);
  }

  #clearAuthenticationTimer(): void {
    if (this.#authenticationTimer !== null) {
      globalThis.clearTimeout(this.#authenticationTimer);
      this.#authenticationTimer = null;
    }
  }
}
