/**
 * 严格解码的判别力：客户端逻辑依赖的每个判别点都必须被挡住。
 *
 * 反例对应：
 * - 放宽版本判定 → 「非 v1 被拒」变红；
 * - 不校验认证前后信封 → 「认证前消息带 connectionId 被拒」变红；
 * - 把 `rejected` 的 `acceptedAt` 当可选 → 「rejected 的 acceptedAt 必须为 null」变红；
 * - 允许未登记 type → 「未登记 type 被拒」变红。
 */

import { describe, expect, it } from "vitest";

import { cursorsEqual, decodeWireMessage, isCommandStatusRecord, isCursor, isErrorCode } from "./wire";
import { Base64UrlError, bytesEqual, decodeBase64Url, encodeBase64Url } from "./base64url";

const CONNECTION_ID = "2de54db7-74ae-4c21-88e3-05df0dc2e637";
const CURSOR = { serverEpoch: "00384a03-bc90-4095-b65d-82fb8cc47e13", globalSequence: "10" };

/** 一条合法的 `sync.ack`，可覆写任意字段。 */
function ack(overrides: Record<string, unknown> = {}): string {
  return JSON.stringify({
    protocolVersion: 1,
    type: "sync.ack",
    messageId: "aaaaaaaa-1111-4111-8111-000000000001",
    connectionId: CONNECTION_ID,
    connectionSequence: "1",
    body: { cursor: CURSOR },
    ...overrides,
  });
}

describe("解码：版本与信封", () => {
  it("合法消息解码成功", () => {
    const decoded = decodeWireMessage(ack());
    expect(decoded.ok).toBe(true);
  });

  it("非 v1 版本被拒", () => {
    // 反例：若只判 `protocolVersion` 存在，这里会通过 → 断言变红。
    expect(decodeWireMessage(ack({ protocolVersion: 2 }))).toMatchObject({ ok: false, reason: "protocol_version" });
  });

  it("非法 JSON 与非对象顶层被拒", () => {
    expect(decodeWireMessage("{")).toMatchObject({ ok: false, reason: "invalid_json" });
    expect(decodeWireMessage("[]")).toMatchObject({ ok: false, reason: "not_an_object" });
  });

  it("认证后消息缺 connectionId 被拒", () => {
    const without = JSON.parse(ack()) as Record<string, unknown>;
    delete without["connectionId"];
    expect(decodeWireMessage(JSON.stringify(without))).toMatchObject({ ok: false, reason: "envelope_violation" });
  });

  it("认证前消息携带 connectionId 被拒", () => {
    const decoded = decodeWireMessage(
      JSON.stringify({
        protocolVersion: 1,
        type: "auth.client_hello",
        messageId: "ed93263a-3628-4668-82aa-c0f551589fec",
        connectionId: CONNECTION_ID,
        body: {
          minProtocolVersion: 1,
          maxProtocolVersion: 1,
          hostId: "bdb2ec20-f98c-4d87-b789-e540d527ef87",
          deviceId: "2ae1c07c-9242-46e9-a9d2-4ec58c130f49",
          clientKind: "pwa",
          clientNonce: "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8",
          supportedFeatures: [],
          requiredFeatures: [],
        },
      }),
    );
    // 反例：若不区分认证前后信封，这条会被接受 → 断言变红。
    expect(decoded).toMatchObject({ ok: false, reason: "envelope_violation" });
  });

  it("messageId 非 UUID 被拒", () => {
    expect(decodeWireMessage(ack({ messageId: "not-a-uuid" }))).toMatchObject({ ok: false, reason: "field_type" });
  });

  it("未登记的 type 被拒", () => {
    expect(decodeWireMessage(ack({ type: "sync.unknown_thing" }))).toMatchObject({ ok: false, reason: "unknown_type" });
  });
});

describe("解码：cursor 与 chunkCount", () => {
  it("cursor 两键都必须合法", () => {
    expect(isCursor(CURSOR)).toBe(true);
    expect(isCursor({ serverEpoch: CURSOR.serverEpoch })).toBe(false);
    expect(isCursor({ serverEpoch: "not-uuid", globalSequence: "10" })).toBe(false);
    expect(isCursor({ serverEpoch: CURSOR.serverEpoch, globalSequence: "010" })).toBe(false);
  });

  it("cursor 比较逐字段", () => {
    expect(cursorsEqual(CURSOR, { ...CURSOR })).toBe(true);
    expect(cursorsEqual(CURSOR, { ...CURSOR, globalSequence: "11" })).toBe(false);
  });

  it("ack 的 cursor 形状非法被拒", () => {
    expect(decodeWireMessage(ack({ body: { cursor: { serverEpoch: CURSOR.serverEpoch } } }))).toMatchObject({
      ok: false,
    });
  });
});

describe("解码：command.result 的 acceptedAt 规则", () => {
  const base = {
    protocolVersion: 1,
    type: "command.result",
    messageId: "bbbbbbbb-1111-4111-8111-000000000001",
    connectionId: CONNECTION_ID,
    connectionSequence: "1",
  };

  it("rejected 的 acceptedAt 必须为 null", () => {
    const rejected = decodeWireMessage(
      JSON.stringify({
        ...base,
        body: {
          requestId: "4c4dafda-dd98-442e-8d55-252b75bac72d",
          command: "session.prompt",
          status: "rejected",
          acceptedAt: "2026-10-01T00:00:00.000Z",
          terminalEventId: null,
          result: null,
          error: { code: "session.busy", message: "忙", retryable: true, details: {} },
        },
      }),
    );
    // 反例：若不校验这条，命令会被当作已接受 → 断言变红。
    expect(rejected).toMatchObject({ ok: false, reason: "field_type" });
  });

  it("rejected 缺结构化 error 被拒", () => {
    const rejected = decodeWireMessage(
      JSON.stringify({
        ...base,
        body: {
          requestId: "4c4dafda-dd98-442e-8d55-252b75bac72d",
          command: "session.prompt",
          status: "rejected",
          acceptedAt: null,
          terminalEventId: null,
          result: null,
          error: null,
        },
      }),
    );
    expect(rejected).toMatchObject({ ok: false, reason: "field_type" });
  });

  it("accepted 缺 acceptedAt 被拒（未确认接受不得显示为已接受）", () => {
    const accepted = decodeWireMessage(
      JSON.stringify({
        ...base,
        body: {
          requestId: "4c4dafda-dd98-442e-8d55-252b75bac72d",
          command: "session.prompt",
          status: "accepted",
          acceptedAt: null,
          terminalEventId: null,
          result: null,
          error: null,
        },
      }),
    );
    expect(accepted).toMatchObject({ ok: false, reason: "field_type" });
  });

  it("status 不在封闭词表内被拒", () => {
    const bad = decodeWireMessage(
      JSON.stringify({
        ...base,
        body: {
          requestId: "4c4dafda-dd98-442e-8d55-252b75bac72d",
          command: "session.prompt",
          status: "pending",
          acceptedAt: "2026-10-01T00:00:00.000Z",
          terminalEventId: null,
          result: null,
          error: null,
        },
      }),
    );
    expect(bad).toMatchObject({ ok: false, reason: "field_type" });
  });
});

describe("解码：事件必填字段", () => {
  const event = (body: Record<string, unknown>): string =>
    JSON.stringify({
      protocolVersion: 1,
      type: "event",
      messageId: "aaaaaaaa-1111-4111-8111-000000000001",
      connectionId: CONNECTION_ID,
      connectionSequence: "1",
      body: {
        globalSequence: "1",
        sessionSequence: null,
        eventId: "aaaaaaaa-0000-4000-8000-000000000001",
        sessionId: null,
        eventType: "session.updated",
        causationRequestId: null,
        origin: { kind: "daemon", deviceId: null },
        remoteOrigin: null,
        createdAt: "2026-10-01T00:00:00.000Z",
        payload: { view: {} },
        ...body,
      },
    });

  it("合法事件解码成功", () => {
    expect(decodeWireMessage(event({})).ok).toBe(true);
  });

  it("globalSequence 非 decimalString 被拒", () => {
    expect(decodeWireMessage(event({ globalSequence: "007" }))).toMatchObject({ ok: false });
  });

  it("eventType 非 family.reason 形状被拒", () => {
    expect(decodeWireMessage(event({ eventType: "" }))).toMatchObject({ ok: false, reason: "field_type" });
  });

  it("createdAt 非 timestamp 形状被拒", () => {
    expect(decodeWireMessage(event({ createdAt: "2026-10-01" }))).toMatchObject({ ok: false, reason: "field_type" });
  });

  it("origin.kind 不在封闭词表内被拒", () => {
    expect(decodeWireMessage(event({ origin: { kind: "unknown", deviceId: null } }))).toMatchObject({ ok: false });
  });

  it("缺 payload.view 被拒", () => {
    expect(decodeWireMessage(event({ payload: {} }))).toMatchObject({ ok: false, reason: "field_type" });
  });
});

describe("解码：control 与错误码", () => {
  it("ping 原样接受（由连接层回 pong）", () => {
    const decoded = decodeWireMessage(
      JSON.stringify({
        protocolVersion: 1,
        type: "control.ping",
        messageId: "eeeeeeee-1111-4111-8111-000000000001",
        connectionId: CONNECTION_ID,
        connectionSequence: "1",
        body: { nonce: "AAAAAAAAAAAAAAAA" },
      }),
    );
    expect(decoded.ok).toBe(true);
  });

  it("错误码形状判定", () => {
    expect(isErrorCode("auth.device_revoked")).toBe(true);
    expect(isErrorCode("not_a_code")).toBe(false);
    expect(isErrorCode(42)).toBe(false);
  });

  it("error 缺 details 被拒", () => {
    const decoded = decodeWireMessage(
      JSON.stringify({
        protocolVersion: 1,
        type: "error",
        messageId: "dddddddd-1111-4111-8111-000000000001",
        connectionId: CONNECTION_ID,
        connectionSequence: "1",
        body: { code: "auth.required", message: "需要认证", retryable: false, correlationId: null },
      }),
    );
    expect(decoded).toMatchObject({ ok: false, reason: "missing_field" });
  });
});

describe("command.status 记录收窄", () => {
  it("state 在封闭词表内且键齐全时收窄成功", () => {
    expect(
      isCommandStatusRecord({
        targetRequestId: "4c4dafda-dd98-442e-8d55-252b75bac72d",
        state: "uncertain",
        acceptedAt: "2026-10-01T00:00:00.000Z",
        terminalAt: "2026-10-01T00:00:05.000Z",
        terminalEventId: "aaaaaaaa-0000-4000-8000-000000000009",
        result: null,
        error: null,
      }),
    ).toBe(true);
  });

  it("state 不在词表内时不收窄", () => {
    expect(
      isCommandStatusRecord({
        targetRequestId: "4c4dafda-dd98-442e-8d55-252b75bac72d",
        state: "submitted",
        acceptedAt: null,
        terminalAt: null,
        terminalEventId: null,
        result: null,
        error: null,
      }),
    ).toBe(false);
  });
});

describe("base64url 编解码", () => {
  it("往返一致", () => {
    const bytes = new Uint8Array([0, 1, 2, 250, 251, 252, 253, 254, 255]);
    expect(decodeBase64Url(encodeBase64Url(bytes))).toEqual(bytes);
  });

  it("32 字节 nonce 的长度约束生效", () => {
    expect(decodeBase64Url(encodeBase64Url(new Uint8Array(32)), 32)).toHaveLength(32);
    expect(() => decodeBase64Url(encodeBase64Url(new Uint8Array(31)), 32)).toThrow(Base64UrlError);
  });

  it("非字母表字符与非法长度被拒", () => {
    expect(() => decodeBase64Url("abc=")).toThrow(Base64UrlError);
    expect(() => decodeBase64Url("a+/b")).toThrow(Base64UrlError);
    expect(() => decodeBase64Url("abcde")).toThrow(Base64UrlError);
  });

  it("字节比较对长度敏感", () => {
    expect(bytesEqual(new Uint8Array([1, 2]), new Uint8Array([1, 2]))).toBe(true);
    expect(bytesEqual(new Uint8Array([1, 2]), new Uint8Array([1, 2, 3]))).toBe(false);
  });
});