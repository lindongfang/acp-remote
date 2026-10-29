import { describe, expect, it, vi } from "vitest";

import {
  MAX_RESPONSE_BYTES,
  PairingApi,
  PairingApiError,
} from "../src/pairing/api";

const ORIGIN = "https://node.example.test";

function apiReturning(createResponse: () => Response): PairingApi {
  const fetchImpl: typeof fetch = async () => createResponse();
  return new PairingApi(ORIGIN, fetchImpl);
}

function jsonResponse(
  status: number,
  value: unknown,
  headers: Record<string, string> = {},
): Response {
  return new Response(JSON.stringify(value), {
    status,
    headers: {
      "Content-Type": "application/json",
      ...headers,
    },
  });
}

function rawResponse(
  status: number,
  body: string,
  headers: Record<string, string> = {},
): Response {
  return new Response(body, {
    status,
    headers: {
      "Content-Type": "application/json",
      ...headers,
    },
  });
}

const RATE_LIMITED = {
  v: 1,
  error: { code: "rate_limited", retryAfterSeconds: 3 },
} as const;

describe("pairing API closed responses", () => {
  it("invokes browser fetch with its required global receiver", async () => {
    const receiverSensitiveFetch = function (
      this: typeof globalThis,
      _input: RequestInfo | URL,
      _init?: RequestInit,
    ): Promise<Response> {
      if (this !== globalThis) {
        throw new TypeError("Illegal invocation");
      }
      return Promise.resolve(rawResponse(200, '{"v":1,"hostProof":"proof"}'));
    };
    vi.stubGlobal("fetch", receiverSensitiveFetch);
    try {
      const api = new PairingApi(ORIGIN);
      await expect(api.claim("proof")).resolves.toEqual({
        v: 1,
        hostProof: "proof",
      });
    } finally {
      vi.unstubAllGlobals();
    }
  });

  it("accepts a correlated 429 and exposes its retry delay", async () => {
    const api = apiReturning(() =>
      jsonResponse(429, RATE_LIMITED, { "Retry-After": "3" }),
    );
    try {
      await api.claim("proof");
      throw new Error("expected PairingApiError");
    } catch (error) {
      expect(error).toBeInstanceOf(PairingApiError);
      expect(error).toMatchObject({
        code: "rate_limited",
        httpStatus: 429,
        retryAfterSeconds: 3,
      });
    }
  });

  it("rejects uncorrelated or misplaced retry metadata", async () => {
    const cases = [
      apiReturning(() => jsonResponse(429, RATE_LIMITED)),
      apiReturning(() =>
        jsonResponse(429, RATE_LIMITED, { "Retry-After": "4" }),
      ),
      apiReturning(() =>
        jsonResponse(
          503,
          {
            v: 1,
            error: {
              code: "storage_unavailable",
              retryAfterSeconds: 3,
            },
          },
          { "Retry-After": "3" },
        ),
      ),
      apiReturning(() =>
        jsonResponse(
          503,
          { v: 1, error: { code: "storage_unavailable", retryAfterSeconds: null } },
          { "Retry-After": "3" },
        ),
      ),
    ];

    for (const api of cases) {
      await expect(api.claim("proof")).rejects.toMatchObject({
        name: "PairingProtocolError",
        code: "invalid_response",
      });
    }
  });

  it("rejects status and code combinations outside the contract", async () => {
    const cases = [
      apiReturning(() => rawResponse(201, '{"v":1,"hostProof":"proof"}')),
      apiReturning(() => jsonResponse(503, RATE_LIMITED)),
      apiReturning(() =>
        jsonResponse(400, {
          v: 1,
          error: { code: "pairing_invalid", retryAfterSeconds: null },
        }),
      ),
      apiReturning(() =>
        jsonResponse(500, {
          v: 1,
          error: { code: "storage_unavailable", retryAfterSeconds: null },
        }),
      ),
    ];

    for (const api of cases) {
      await expect(api.claim("proof")).rejects.toMatchObject({
        name: "PairingProtocolError",
        code: "invalid_response",
      });
    }
  });

  it("accepts a closed non-retryable storage failure", async () => {
    const api = apiReturning(() =>
      jsonResponse(503, {
        v: 1,
        error: { code: "storage_unavailable", retryAfterSeconds: null },
      }),
    );
    await expect(api.claim("proof")).rejects.toMatchObject({
      name: "PairingApiError",
      code: "storage_unavailable",
      httpStatus: 503,
      retryAfterSeconds: null,
    });
  });

  it("rejects oversized, duplicate-key, and unknown-field bodies", async () => {
    const cases = [
      apiReturning(() => rawResponse(200, " ".repeat(MAX_RESPONSE_BYTES + 1))),
      apiReturning(() =>
        rawResponse(200, '{"v":1,"v":1,"hostProof":"proof"}'),
      ),
      apiReturning(() =>
        rawResponse(200, '{"v":1,"hostProof":"proof","extra":true}'),
      ),
      apiReturning(() =>
        rawResponse(
          503,
          '{"v":1,"error":{"code":"storage_unavailable","code":"node_stopping","retryAfterSeconds":null}}',
        ),
      ),
      apiReturning(() =>
        rawResponse(
          503,
          '{"v":1,"error":{"code":"storage_unavailable","retryAfterSeconds":null,"extra":true}}',
        ),
      ),
    ];

    for (const api of cases) {
      await expect(api.claim("proof")).rejects.toMatchObject({
        name: "PairingProtocolError",
        code: "invalid_response",
      });
    }
  });
});
