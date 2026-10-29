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

const AUTHENTICATION_TIMEOUT_MS = 20_000;
const REQUEST_TIMEOUT_MS = 15_000;
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

export interface ManagementClose {
  readonly code: number;
  readonly reason: string;
  readonly wasClean: boolean;
  readonly error: Error;
}

export interface ManagementConnectionCallbacks {
  readonly onDevicesChanged?: (revision: number) => void;
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
    this.#state = "opening";

    const connected = new Promise<AuthenticatedFrame>((resolve, reject) => {
      this.#connectResolve = resolve;
      this.#connectReject = reject;
    });
    socket.addEventListener("open", () => this.#onOpen());
    socket.addEventListener("message", (event) => this.#onMessage(event));
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

  #onMessage(event: MessageEvent<unknown>): void {
    if (typeof event.data !== "string") {
      this.#failProtocol(
        new ManagementProtocolError("invalid_frame"),
        "binary frame",
      );
      return;
    }
    let frame: ManagementFrame;
    try {
      frame = parseManagementFrame(event.data);
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
    if (frame.type === "result") {
      this.#handleResult(frame);
      return;
    }
    this.#failProtocol(
      new ManagementProtocolError("invalid_frame"),
      "unexpected frame",
    );
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
    return await this.#request({
      v: 1,
      type: "command",
      requestId: randomId128(),
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

  #failProtocol(error: Error, reason: string): void {
    if (this.#state === "closed") {
      return;
    }
    this.#failure ??= error;
    this.#rejectConnection(error);
    this.#rejectPending(error);
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

  #clearAuthenticationTimer(): void {
    if (this.#authenticationTimer !== null) {
      globalThis.clearTimeout(this.#authenticationTimer);
      this.#authenticationTimer = null;
    }
  }
}
