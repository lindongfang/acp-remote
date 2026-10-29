import {
  type HostProofResponse,
  type PairChallenge,
  type PairLookup,
  type PairingApiErrorBody,
  PairingProtocolError,
  parseApiErrorBody,
  parseChallengeResponse,
  parseHostProofResponse,
  parseLookupResponse,
} from "../protocol/pairing";

export const MAX_RESPONSE_BYTES = 16 * 1024;
const API_PREFIX = "/api/access/v1/pair";

export type PairingApiErrorCode =
  | "invalid_request"
  | "unsupported_version"
  | "pairing_invalid"
  | "request_conflict"
  | "rate_limited"
  | "storage_unavailable"
  | "node_stopping";

export class PairingApiError extends Error {
  readonly code: PairingApiErrorCode;
  readonly httpStatus: number;
  readonly retryAfterSeconds: number | null;

  constructor(
    code: PairingApiErrorCode,
    httpStatus: number,
    retryAfterSeconds: number | null,
  ) {
    super(code);
    this.name = "PairingApiError";
    this.code = code;
    this.httpStatus = httpStatus;
    this.retryAfterSeconds = retryAfterSeconds;
  }
}

export class PairingApi {
  readonly #origin: string;
  readonly #fetch: typeof fetch;

  constructor(
    origin = globalThis.location.origin,
    fetchImpl = (...args: Parameters<typeof fetch>) =>
      globalThis.fetch(...args),
  ) {
    this.#origin = origin;
    this.#fetch = (...args) => fetchImpl(...args);
  }

  async lookup(
    code: string,
    requestId: string,
    signal?: AbortSignal,
  ): Promise<PairLookup> {
    const bytes = await this.#post(
      "/lookup",
      { v: 1, code, requestId },
      signal,
    );
    return parseLookupResponse(bytes, this.#origin, requestId);
  }

  async claim(proof: string, signal?: AbortSignal): Promise<HostProofResponse> {
    const bytes = await this.#post("/claim", { v: 1, proof }, signal);
    return parseHostProofResponse(bytes);
  }

  async challenge(
    pairingId: string,
    clientPublicKey: string,
    requestId: string,
    nodeId: string,
    nodePublicKey: string,
    signal?: AbortSignal,
  ): Promise<PairChallenge> {
    const bytes = await this.#post(
      "/challenge",
      { v: 1, pairingId, clientPublicKey, requestId },
      signal,
    );
    return parseChallengeResponse(
      bytes,
      this.#origin,
      nodeId,
      nodePublicKey,
      requestId,
    );
  }

  async status(
    proof: string,
    signal?: AbortSignal,
  ): Promise<HostProofResponse> {
    const bytes = await this.#post("/status", { v: 1, proof }, signal);
    return parseHostProofResponse(bytes);
  }

  async #post(
    path: string,
    body: Record<string, unknown>,
    signal?: AbortSignal,
  ): Promise<Uint8Array> {
    let response: Response;
    try {
      response = await this.#fetch(`${API_PREFIX}${path}`, {
        method: "POST",
        headers: {
          Accept: "application/json",
          "Content-Type": "application/json",
        },
        body: JSON.stringify(body),
        cache: "no-store",
        credentials: "same-origin",
        redirect: "error",
        referrerPolicy: "no-referrer",
        signal,
      });
    } catch {
      throw new PairingProtocolError("invalid_response");
    }

    const bytes = await readLimitedBody(response);
    if (response.status !== 200) {
      let error;
      try {
        error = parseApiErrorBody(bytes);
      } catch {
        throw new PairingProtocolError("invalid_response");
      }
      const retryHeader = parseRetryAfter(response.headers.get("Retry-After"));
      throw validatedApiError(response.status, error, retryHeader);
    }
    return bytes;
  }
}

async function readLimitedBody(response: Response): Promise<Uint8Array> {
  const contentLength = response.headers.get("Content-Length");
  if (
    contentLength !== null &&
    Number.isFinite(Number(contentLength)) &&
    Number(contentLength) > MAX_RESPONSE_BYTES
  ) {
    await response.body?.cancel();
    throw new PairingProtocolError("invalid_response");
  }
  if (response.body === null) {
    return new Uint8Array();
  }
  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let total = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) {
      break;
    }
    total += value.length;
    if (total > MAX_RESPONSE_BYTES) {
      await reader.cancel();
      throw new PairingProtocolError("invalid_response");
    }
    chunks.push(value);
  }
  const result = new Uint8Array(total);
  let offset = 0;
  for (const chunk of chunks) {
    result.set(chunk, offset);
    offset += chunk.length;
  }
  return result;
}

function parseRetryAfter(value: string | null): number | null {
  if (value === null || !/^[0-9]+$/u.test(value)) {
    return null;
  }
  const parsed = Number(value);
  return Number.isSafeInteger(parsed) && parsed >= 1 ? parsed : null;
}

function validatedApiError(
  status: number,
  error: PairingApiErrorBody,
  retryHeader: number | null,
): PairingApiError {
  const code = knownErrorCode(error.code);
  const statusAllowsCode =
    (status === 400 &&
      (code === "invalid_request" || code === "unsupported_version")) ||
    (status === 401 && code === "pairing_invalid") ||
    (status === 409 && code === "request_conflict") ||
    (status === 429 && code === "rate_limited") ||
    (status === 503 &&
      (code === "storage_unavailable" || code === "node_stopping"));
  if (!statusAllowsCode) {
    throw new PairingProtocolError("invalid_response");
  }
  if (status === 429) {
    if (
      error.retryAfterSeconds === null ||
      retryHeader === null ||
      error.retryAfterSeconds !== retryHeader
    ) {
      throw new PairingProtocolError("invalid_response");
    }
  } else if (error.retryAfterSeconds !== null || retryHeader !== null) {
    throw new PairingProtocolError("invalid_response");
  }
  return new PairingApiError(code, status, error.retryAfterSeconds);
}

function knownErrorCode(value: string): PairingApiErrorCode {
  switch (value) {
    case "invalid_request":
    case "unsupported_version":
    case "pairing_invalid":
    case "request_conflict":
    case "rate_limited":
    case "storage_unavailable":
    case "node_stopping":
      return value;
    default:
      throw new PairingProtocolError("invalid_response");
  }
}
