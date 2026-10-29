import {
  createPairClaim,
  createPairStatus,
  normalizePairingCode,
  randomId128,
  validateDisplayName,
  verifyClaimResult,
  verifyStatusResult,
  type PairStatusResult,
  type PairStatusValue,
} from "../protocol/pairing";
import { PairingApi, PairingApiError } from "./api";
import {
  IdentityStoreError,
  type IdentityRecord,
  type IdentityVault,
  type PendingPairClaim,
  type StoredPairingStatus,
} from "./identity";

export type PairingSessionErrorCode =
  | "already_bound"
  | "node_mismatch"
  | "client_mismatch"
  | "no_pending_claim"
  | "pairing_not_pending"
  | "crypto_unavailable";

export class PairingSessionError extends Error {
  readonly code: PairingSessionErrorCode;

  constructor(code: PairingSessionErrorCode) {
    super(code);
    this.name = "PairingSessionError";
    this.code = code;
  }
}

export class PairingSession {
  readonly #vault: IdentityVault;
  readonly #api: PairingApi;
  #record: IdentityRecord;
  #pendingClaim: PendingPairClaim | null;

  constructor(
    vault: IdentityVault,
    api: PairingApi,
    record: IdentityRecord,
  ) {
    this.#vault = vault;
    this.#api = api;
    this.#record = record;
    this.#pendingClaim = record.pendingClaim;
  }

  get identity(): IdentityRecord {
    return this.#record;
  }

  get canRetryClaim(): boolean {
    return this.#pendingClaim !== null;
  }

  async begin(
    code: string,
    displayName: string,
    signal?: AbortSignal,
  ): Promise<IdentityRecord> {
    if (
      this.#record.clientId !== null ||
      this.#record.status === "approved" ||
      this.#record.status === "consumed"
    ) {
      throw new PairingSessionError("already_bound");
    }
    const normalizedCode = normalizePairingCode(code);
    const validDisplayName = validateDisplayName(displayName);
    const requestId = randomId128();
    const lookup = await this.#api.lookup(normalizedCode, requestId, signal);
    if (
      (this.#record.nodeId !== null &&
        this.#record.nodeId !== lookup.nodeId) ||
      (this.#record.nodePublicKey !== null &&
        this.#record.nodePublicKey !== lookup.nodePublicKey)
    ) {
      throw new PairingSessionError("node_mismatch");
    }

    const proof = await createPairClaim(
      lookup,
      this.#record.publicKey,
      validDisplayName,
      this.#record.privateKey,
    );
    const pendingClaim = { lookup, proof };
    this.#record = await this.#vault.save({
      ...this.#record,
      nodeId: lookup.nodeId,
      nodePublicKey: lookup.nodePublicKey,
      pairingId: lookup.pairingId,
      displayName: validDisplayName,
      status: "pending_claim",
      pendingClaim,
      updatedAt: Date.now(),
    });
    this.#pendingClaim = pendingClaim;
    return await this.#submitPendingClaim(signal);
  }

  async retryClaim(signal?: AbortSignal): Promise<IdentityRecord> {
    if (this.#pendingClaim === null) {
      throw new PairingSessionError("no_pending_claim");
    }
    return await this.#submitPendingClaim(signal);
  }

  async acceptAuthenticated(clientId: string): Promise<IdentityRecord> {
    if (
      this.#record.clientId !== null &&
      this.#record.clientId !== clientId
    ) {
      throw new PairingSessionError("client_mismatch");
    }
    this.#record = await this.#vault.save({
      ...this.#record,
      clientId,
      status: "consumed",
      updatedAt: Date.now(),
    });
    return this.#record;
  }

  async pollOnce(signal?: AbortSignal): Promise<PairStatusResult> {
    const pairingId = this.#record.pairingId;
    const nodeId = this.#record.nodeId;
    const nodePublicKey = this.#record.nodePublicKey;
    if (
      pairingId === null ||
      nodeId === null ||
      nodePublicKey === null ||
      this.#record.status !== "pending_confirmation"
    ) {
      throw new PairingSessionError("pairing_not_pending");
    }

    const requestId = randomId128();
    const challenge = await this.#api.challenge(
      pairingId,
      this.#record.publicKey,
      requestId,
      nodeId,
      nodePublicKey,
      signal,
    );
    const proof = await createPairStatus(
      challenge,
      pairingId,
      this.#record.publicKey,
      this.#record.privateKey,
    );
    const hostProof = await this.#api.status(proof, signal);
    const result = await verifyStatusResult(
      hostProof.hostProof,
      challenge,
      pairingId,
      this.#record.publicKey,
    );
    const nextStatus = storedStatus(result.status);
    if (
      nextStatus !== this.#record.status ||
      result.clientId !== this.#record.clientId
    ) {
      this.#record = await this.#vault.save({
        ...this.#record,
        status: nextStatus,
        clientId: result.clientId,
        updatedAt: Date.now(),
      });
    }
    return result;
  }

  async #submitPendingClaim(signal?: AbortSignal): Promise<IdentityRecord> {
    const pending = this.#pendingClaim;
    if (pending === null) {
      throw new PairingSessionError("no_pending_claim");
    }
    const response = await this.#api.claim(pending.proof, signal);
    await verifyClaimResult(
      response.hostProof,
      pending.lookup,
      this.#record.publicKey,
    );
    this.#record = await this.#vault.save({
      ...this.#record,
      status: "pending_confirmation",
      pendingClaim: null,
      updatedAt: Date.now(),
    });
    this.#pendingClaim = null;
    return this.#record;
  }
}

function storedStatus(value: PairStatusValue): StoredPairingStatus {
  switch (value) {
    case "pending_confirmation":
    case "approved":
    case "rejected":
    case "cancelled":
    case "expired":
    case "consumed":
      return value;
  }
}

export function isFailedPairingStatus(
  status: StoredPairingStatus,
): status is "rejected" | "cancelled" | "expired" {
  return (
    status === "rejected" ||
    status === "cancelled" ||
    status === "expired"
  );
}

export function pairingFailureIsRecoverable(error: unknown): boolean {
  return (
    error instanceof PairingSessionError ||
    error instanceof IdentityStoreError ||
    (error instanceof Error && error.name === "PairingApiError") ||
    (error instanceof Error && error.name === "PairingProtocolError")
  );
}

export function shouldRecoverPendingManagement(
  error: unknown,
  record: Pick<
    IdentityRecord,
    "status" | "clientId" | "nodeId" | "nodePublicKey"
  >,
  attempted: boolean,
): boolean {
  return (
    !attempted &&
    error instanceof PairingApiError &&
    error.code === "pairing_invalid" &&
    record.status === "pending_confirmation" &&
    record.clientId === null &&
    record.nodeId !== null &&
    record.nodePublicKey !== null
  );
}
