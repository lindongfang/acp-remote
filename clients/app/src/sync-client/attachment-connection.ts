/**
 * 附件传输的**独立已认证 WSS 连接**（`docs/SYNC_PROTOCOL.md` §3.4）。
 *
 * ## 为什么是第二条连接
 *
 * 附件正文不进业务消息连接：单块 32 KiB、一次一块未确认的传输会与事件交付争同一条通道，
 * 而 §3.4 要求「附件传输 MUST NOT 阻塞会话事件交付」。因此业务连接与附件连接各自一条
 * socket，各自持有握手与认证状态（§8.4：认证状态不跨连接共享）。
 *
 * ## 认证完全复用
 *
 * 子协议核对、challenge-response、Host 验签、feature 必需项与阻断判定全部由
 * `handshake.ts` 完成；本对象只多做三件事：把 socket 文本分派给握手或附件帧、维护本连接的
 * 出站序号（`control.pong` 与附件帧共用同一个计数器）、在认证完成前拒绝任何附件帧
 * （`attachment.negotiation_required` 并 `4400` 关闭，§3.4）。
 *
 * ## 不负责什么
 *
 * 它不做重连与传输编排队列——那是 `AttachmentTransport` 的职责；它也不决定什么时候开一条
 * 连接（组合根按业务连接的认证结果决定）。这样「协议连接」与「生命周期策略」可以分开测。
 */

import type { AttachmentFrame } from "../protocol";
import { ATTACHMENT_FEATURE, ATTACHMENT_WS_PATH, decodeAttachmentFrame } from "../protocol";
import type { ClientBlockCause, IncompatibleReason } from "./blocking";
import { CLOSE, blockCauseForCloseCode, isRetryableCloseCode } from "./blocking";
import { AuthHandshake, PROTOCOL_VERSION } from "./handshake";
import type {
  DeviceIdentityLike,
  HostIdentityPort,
  RandomPort,
  SocketFactory,
  SocketPort,
  TranscriptCodec,
} from "./ports";
import { stringifyJson } from "./json";
import { decodeWireMessage } from "./wire";

/** 认证完成后交给调用方的事实：标识、协商结果与出站入口。 */
export interface AttachmentLink {
  readonly connectionId: string;
  readonly selectedFeatures: readonly string[];
  readonly negotiated: boolean;
  /** 发送一帧（序号由本连接统一分配；调用方给的序号会被覆盖）。 */
  send(frame: AttachmentFrame): void;
}

export interface AttachmentConnectionOptions {
  readonly url: string;
  readonly subprotocols: readonly string[];
  readonly hostId: string;
  readonly identity: DeviceIdentityLike;
  readonly host: HostIdentityPort;
  readonly random: RandomPort;
  readonly transcript: TranscriptCodec;
  readonly openSocket: SocketFactory;
  readonly supportedFeatures: readonly string[];
  readonly requiredFeatures: readonly string[];
  readonly clientKind?: "pwa" | "android" | "ios" | "desktop";
  /** 认证完成且本连接协商到附件能力时回调一次。 */
  readonly onReady: (link: AttachmentLink) => void;
  /** 一条形状合法的入站附件帧。 */
  readonly onFrame: (frame: AttachmentFrame) => void;
  /** socket 关闭；`retryable` 与业务连接的判定同一份表。 */
  readonly onClosed: (code: number, retryable: boolean, detail: string) => void;
  /** 进入阻断态：调用方 MUST NOT 重连。 */
  readonly onBlocked: (cause: ClientBlockCause, code: number, reason?: IncompatibleReason) => void;
  readonly onRejected: (reason: string, detail: string) => void;
}

export class AttachmentConnection {
  readonly #options: AttachmentConnectionOptions;
  readonly #socket: SocketPort;
  readonly #handshake: AuthHandshake;
  readonly #random: RandomPort;
  #outboundSequence = 0;
  #link: AttachmentLink | null = null;
  #closed = false;

  constructor(options: AttachmentConnectionOptions) {
    this.#options = options;
    this.#random = options.random;
    this.#handshake = new AuthHandshake({
      hostId: options.hostId,
      identity: options.identity,
      host: options.host,
      random: options.random,
      transcript: options.transcript,
      subprotocols: options.subprotocols,
      supportedFeatures: [...options.supportedFeatures],
      requiredFeatures: [...options.requiredFeatures],
      ...(options.clientKind === undefined ? {} : { clientKind: options.clientKind }),
      send: (message) => {
        this.#socket.send(stringifyJson(message));
      },
      close: (code, reason) => {
        this.close(code, reason);
      },
      onRejected: options.onRejected,
      onBlocked: (cause, code, reason) => {
        this.#block(cause, code, reason);
      },
    });
    this.#socket = options.openSocket({
      url: options.url,
      protocols: options.subprotocols,
      handlers: {
        onOpen: () => {
          void this.#handshake.begin(this.#socket.negotiatedSubprotocol());
        },
        onMessage: (text) => {
          this.#onMessage(text);
        },
        onClose: (code, reason) => {
          this.#onClose(code, reason);
        },
        onError: (reason) => {
          this.#options.onRejected("socket_error", reason);
        },
      },
    });
  }

  /** 认证完成后的出站入口；未认证为 `null`（调用方据此拒绝发起传输）。 */
  get link(): AttachmentLink | null {
    return this.#link;
  }

  get closed(): boolean {
    return this.#closed;
  }

  /** 主动关闭（页面卸载 / 业务连接断开 / 设备撤销）。 */
  close(code: number = CLOSE.normal, reason = "client_closed"): void {
    if (this.#closed) return;
    this.#closed = true;
    this.#link = null;
    this.#socket.close(code, reason);
  }

  /**
   * 进入阻断态：**先**关闭本连接的 socket，**再**通知调用方。
   *
   * 顺序是刻意的（与业务连接的 `#block()` 同构）：调用方的 `onBlocked` 会丢弃对这条连接的引用，
   * 若 socket 仍开着，日后它的 `onClose` 会带着同一批回调再触发一次收尾——那时调用方可能已经
   * 建好了新连接，旧回调就会把新连接与在途传输一起置空。先关闭让 `#closed` 生效，旧 socket 的
   * 任何后续回调都被本对象挡住。
   */
  #block(cause: ClientBlockCause, code: number, reason?: IncompatibleReason): void {
    this.close(code, reason ?? cause);
    this.#options.onBlocked(cause, code, reason);
  }

  #onMessage(text: string): void {
    if (this.#closed) return;
    let value: unknown;
    try {
      value = JSON.parse(text);
    } catch {
      this.#options.onRejected("json", "附件连接收到非 JSON 文本");
      this.close(CLOSE.protocolError, "protocol.schema_invalid");
      return;
    }
    const type = typeof value === "object" && value !== null ? Reflect.get(value, "type") : null;
    if (typeof type === "string" && type.startsWith("attachment.")) {
      this.#onAttachmentFrame(value);
      return;
    }
    const decoded = decodeWireMessage(text);
    if (!decoded.ok) {
      this.#options.onRejected(decoded.reason, decoded.detail);
      return;
    }
    switch (decoded.message.type) {
      case "auth.server_challenge":
        void this.#handshake.acceptChallenge(decoded.message);
        return;
      case "auth.authenticated": {
        if (!this.#handshake.acceptAuthenticated(decoded.message)) return;
        const state = this.#handshake.state;
        if (state === null) return;
        const negotiated = state.selectedFeatures.includes(ATTACHMENT_FEATURE);
        const link: AttachmentLink = {
          connectionId: state.connectionId,
          selectedFeatures: state.selectedFeatures,
          negotiated,
          send: (frame) => {
            this.#sendFrame(frame);
          },
        };
        this.#link = link;
        if (negotiated) this.#options.onReady(link);
        else this.#options.onRejected("attachment_not_negotiated", "服务端未选中 core.attachment-transfer.v1");
        return;
      }
      case "control.ping":
        this.#socket.send(
          stringifyJson({
            protocolVersion: PROTOCOL_VERSION,
            type: "control.pong",
            messageId: this.#random.uuid(),
            connectionId: this.#handshake.connectionId,
            connectionSequence: this.#nextSequence(),
            body: { nonce: decoded.message.body.nonce },
          }),
        );
        return;
      case "control.pong":
        return;
      case "error":
        this.#onProtocolError(decoded.message.body.code, decoded.message.body.message);
        return;
      default:
        this.#options.onRejected("unexpected_type", decoded.message.type);
    }
  }

  #onAttachmentFrame(value: unknown): void {
    if (this.#link === null || !this.#link.negotiated) {
      this.#options.onRejected("attachment_before_auth", "认证完成前收到附件帧");
      this.close(CLOSE.protocolError, "attachment.negotiation_required");
      return;
    }
    const decoded = decodeAttachmentFrame(value);
    if (!decoded.ok) {
      this.#options.onRejected(decoded.reason, decoded.detail);
      this.close(CLOSE.protocolError, "protocol.schema_invalid");
      return;
    }
    this.#options.onFrame(decoded.frame);
  }

  /** 附件连接的协议错误：撤销/身份/版本落到阻断，其余只上抛（连接由服务端关闭）。 */
  #onProtocolError(code: string, message: string): void {
    switch (code) {
      case "auth.device_revoked":
        this.#block("device_revoked", CLOSE.deviceRevoked);
        return;
      case "protocol.version_unsupported":
        this.#block("version_incompatible", CLOSE.versionIncompatible, "protocol_version_unsupported");
        return;
      case "protocol.feature_required":
        this.#block("version_incompatible", CLOSE.versionIncompatible, "required_features_missing");
        return;
      case "auth.origin_mismatch":
      case "auth.host_mismatch":
        this.#block("host_identity_changed", CLOSE.identityConflict);
        return;
      default:
        this.#options.onRejected(code, message);
    }
  }

  #onClose(code: number, reason: string): void {
    if (this.#closed) return;
    this.#closed = true;
    this.#link = null;
    const blocked = blockCauseForCloseCode(code);
    if (blocked !== null) {
      // socket 已经关闭：`#block` 的 `close()` 在这里是幂等的空操作，通知顺序不变。
      if (blocked === "version_incompatible") {
        this.#block(blocked, code, "protocol_version_unsupported");
      } else {
        this.#block(blocked, code);
      }
      return;
    }
    this.#options.onClosed(code, isRetryableCloseCode(code), reason);
  }

  /**
   * 发送一帧：`connectionId` 与 `connectionSequence` 由本连接统一分配。
   *
   * 覆盖调用方给的序号是刻意的：`AttachmentTransferClient` 与 `control.pong` 是同一连接上的
   * 两个出站源，各自计数会撞号（§4.1 要求严格加一）。这里只有一个计数器。
   */
  #sendFrame(frame: AttachmentFrame): void {
    const link = this.#link;
    if (link === null || !link.negotiated) {
      throw new Error("附件连接尚未认证或未协商：不能发送附件帧");
    }
    this.#socket.send(
      stringifyJson({ ...frame, connectionId: link.connectionId, connectionSequence: this.#nextSequence() }),
    );
  }

  #nextSequence(): string {
    this.#outboundSequence += 1;
    return String(this.#outboundSequence);
  }
}

/**
 * 附件连接的出站地址。
 *
 * 与 `syncSocketUrl` 同构：只替换 scheme、路径取协议常量。单独成函数是为了让「第二条连接连到
 * 哪里」与「业务连接连到哪里」各自有唯一构造点，同时都不在组合根里拼字面量。
 */
export function attachmentSocketUrl(canonicalOrigin: string): string {
  return `${canonicalOrigin.replace(/^https:\/\//, "wss://")}${ATTACHMENT_WS_PATH}`;
}
