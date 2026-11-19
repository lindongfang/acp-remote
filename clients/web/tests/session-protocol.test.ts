import * as SessionProtocolBarrel from "../src/management/session-protocol";
import { requireSessionId as primRequireSessionId } from "../src/management/session-protocol/primitives";
import { parseSessionQueue as qiParseSessionQueue } from "../src/management/session-protocol/queue-and-input";
import { parseCatalogInventory as catParseCatalogInventory } from "../src/management/session-protocol/catalog";
import { parseSessionViewOpened as vaParseSessionViewOpened } from "../src/management/session-protocol/view-and-approval";
import { parseSessionCapacity as scParseSessionCapacity } from "../src/management/session-protocol/source-and-capacity";
import { parseSessionCommand as wcParseSessionCommand } from "../src/management/session-protocol/wire-codec";
import { describe, expect, it } from "vitest";
import { encodeInputData } from "../src/management/input-transfer";
import {
  decodeSessionServerBinary, encodeSessionCommand, parseCatalogInventory,
  parseCatalogReceiptMetadata, parseCatalogUploadStatus, parseSessionCommand,
  parseSessionConsumeResult, parseSessionQueue, parseSessionRegistration, parseSessionResumeResult,
  requireSessionI64, requireSessionId, requireSessionU64, SessionProtocolError,
} from "../src/management/session-protocol";
import { encodeViewCredit, encodeViewDetach } from "../src/views/binary";

const ID = "AAECAwQFBgcICQoLDA0ODw";
const ID2 = "AAAAAAAAAAAAAAAAAAAAAA";
const BIG = "9007199254740993";
const U64_MAX = "18446744073709551615";
const envelope = (action: string, payload: unknown): string => JSON.stringify({
  v: 1, type: "command", requestId: ID, action, payload,
});
function failure(run: () => unknown, code: SessionProtocolError["code"]): void {
  let caught: unknown;
  try { run(); } catch (error) { caught = error; }
  expect(caught).toBeInstanceOf(SessionProtocolError);
  expect((caught as SessionProtocolError).code).toBe(code);
  expect(String(caught)).toBe(`SessionProtocolError: ${code}`);
  expect(Object.hasOwn(caught as object, "cause")).toBe(false);
}

const COMMANDS: readonly (readonly [string, unknown])[] = [
  ["product.status", {}],
  ["input.upload.begin", { targetId: ID, purpose: "submit" }],
  ["input.upload.finish", { uploadId: ID, totalBytes: U64_MAX }],
  ["input.upload.status", { uploadId: ID }],
  ["input.upload.release", { uploadId: ID }],
  ["input.register", { targetId: ID, operation: { kind: "submit", bodyId: ID2 } }],
  ["input.consume", { targetId: ID, registrationId: ID2, bodyId: null }],
  ["input.request", { targetId: ID, registrationId: ID2 }],
  ["input.queue", { targetId: ID }],
  ["input.body.open", { targetId: ID, itemId: ID2, expectedRevision: BIG }],
  ["catalog.upload.begin", {}],
  ["catalog.upload.finish", { uploadId: ID, totalBytes: BIG }],
  ["catalog.upload.status", { uploadId: ID }],
  ["catalog.upload.release", { uploadId: ID }],
  ["catalog.register", { documentUploadId: ID }],
  ["catalog.apply", { catalogRequestId: ID }],
  ["catalog.request", { catalogRequestId: ID }],
  ["workspace.register", { path: "D:\\Project\\demo" }],
  ["target.register", { sourceId: ID, nativeSessionId: "native-1", workspaceId: ID2 }],
  ["catalog.inventory", {}],
  ["approval.list", {}],
  ["approval.read", { approvalId: ID }],
  ["approval.register", { approvalId: ID, choiceId: ID2 }],
  ["approval.decide", { approvalRegistrationId: ID }],
  ["approval.status", { approvalId: ID }],
  ["native.release.register", { targetId: ID }],
  ["native.release", { releaseOperationId: ID }],
  ["native.release.status", { releaseOperationId: ID }],
  ["view.open", { targetId: ID, reason: "first_open" }],
  ["view.close", { streamId: ID }],
  ["view.original", { streamId: ID }],
  ["view.rebind", { streamId: ID }],
];

describe("canonical session scalars", () => {
  it("requires canonical 128-bit base64url without assigning authority", () => {
    expect(requireSessionId(ID)).toBe(ID);
    expect(requireSessionId(ID2)).toBe(ID2);
    for (const invalid of [null, 1, "", ID + "=", ID.slice(0, -1) + "x", "🦀".repeat(11), "A".repeat(23)]) {
      failure(() => requireSessionId(invalid), "invalid_id");
    }
  });
  it("keeps unsigned full precision and enforces both range endpoints", () => {
    for (const value of ["0", "1", BIG, U64_MAX]) expect(requireSessionU64(value)).toBe(value);
    for (const invalid of [null, 0, 1n, "", "00", "01", "+1", "-0", "-1", "1.0", "1e3", " 1", "1\n", "18446744073709551616", "9".repeat(1000)]) {
      failure(() => requireSessionU64(invalid), "invalid_decimal");
    }
  });
  it("keeps signed full precision including the asymmetric minimum", () => {
    for (const value of ["0", "1", "-1", BIG, "-" + BIG, "9223372036854775807", "-9223372036854775808"]) {
      expect(requireSessionI64(value)).toBe(value);
    }
    for (const invalid of [null, 0, "-0", "-01", "+1", "00", "9223372036854775808", "-9223372036854775809", "-", "1\n", "-1\n"]) {
      failure(() => requireSessionI64(invalid), "invalid_decimal");
    }
  });
});

describe("closed session command syntax", () => {
  it.each(COMMANDS)("round trips %s without changing scalar precision", (action, payload) => {
    const wire = envelope(action, payload);
    const parsed = parseSessionCommand(wire);
    expect(parsed).toEqual(JSON.parse(wire));
    expect(encodeSessionCommand(parsed)).toBe(wire);
    expect(Object.isFrozen(parsed)).toBe(true);
    expect(Object.isFrozen(parsed.payload)).toBe(true);
  });
  it.each(COMMANDS)("rejects extra payload fields for %s", (action, payload) => {
    failure(() => parseSessionCommand(envelope(action, { ...(payload as object), source: ID })), "invalid_command");
  });
  it.each(COMMANDS.filter(([, value]) => Object.keys(value as object).length > 0))(
    "rejects missing required fields for %s", (action, payload) => {
      const missing = { ...(payload as object) } as Record<string, unknown>;
      delete missing[Object.keys(missing)[0]!];
      expect(() => parseSessionCommand(envelope(action, missing))).toThrowError(SessionProtocolError);
    },
  );
  it("closes each mutation branch and retains full revision text", () => {
    for (const operation of [
      { kind: "submit", bodyId: ID },
      { kind: "edit", itemId: ID, expectedRevision: BIG, bodyId: ID2 },
      { kind: "cancel", itemId: ID, expectedRevision: U64_MAX },
      { kind: "prioritize", itemId: ID, expectedRevision: "1" },
      { kind: "resume" },
    ]) {
      const parsed = parseSessionCommand(envelope("input.register", { targetId: ID, operation }));
      expect(parsed.payload).toEqual({ targetId: ID, operation });
      if (parsed.action === "input.register") expect(Object.isFrozen(parsed.payload.operation)).toBe(true);
      failure(() => parseSessionCommand(envelope("input.register", { targetId: ID, operation: { ...operation, actor: ID } })), "invalid_command");
    }
    failure(() => parseSessionCommand(envelope("input.register", { targetId: ID, operation: { kind: "dispatch" } })), "invalid_command");
  });
  it("requires nullable bodyId explicitly and does not bind a body to an operation locally", () => {
    const payload = { targetId: ID, registrationId: ID2, bodyId: ID };
    expect(parseSessionCommand(envelope("input.consume", payload)).payload).toEqual(payload);
    failure(() => parseSessionCommand(envelope("input.consume", { targetId: ID, registrationId: ID2 })), "invalid_command");
  });
  it("rejects duplicate decoded keys, wrong envelopes and prototype names", () => {
    const wire = envelope("input.queue", { targetId: ID });
    for (const invalid of [
      wire.replace('"v":1', '"v":1,"v":1'),
      wire.replace('"targetId":', '"targetId":"' + ID + '","target\\u0049d":'),
      wire.replace('"v":1', '"v":2'),
      wire.replace('"type":"command"', '"type":"result"'),
      wire.replace('"action":"input.queue"', '"action":"__proto__"'),
      wire.replace('"action":"input.queue"', '"action":"toString"'),
      wire.replace('"payload":{', '"actor":"' + ID + '","payload":{'),
      envelope("input.queue", null), envelope("input.queue", []), envelope("product.status", { x: 0 }),
    ]) failure(() => parseSessionCommand(invalid), "invalid_command");
  });
  it("rejects wrong scalar types and unsupported purpose/reason instead of coercing", () => {
    failure(() => parseSessionCommand(envelope("input.upload.finish", { uploadId: ID, totalBytes: 42 })), "invalid_decimal");
    failure(() => parseSessionCommand(envelope("input.queue", { targetId: 42 })), "invalid_id");
    failure(() => parseSessionCommand(envelope("input.upload.begin", { targetId: ID, purpose: "catalog" })), "invalid_command");
    failure(() => parseSessionCommand(envelope("view.open", { targetId: ID, reason: "auto" })), "invalid_command");
    for (const reason of ["explicit_refresh", "reopen", "reconnect", "reacquire"]) {
      expect(parseSessionCommand(envelope("view.open", { targetId: ID, reason })).action).toBe("view.open");
    }
  });
  it("retains the original 32KiB control frame and depth limits", () => {
    failure(() => parseSessionCommand(" ".repeat(32769)), "invalid_command");
    failure(() => parseSessionCommand(' '.repeat(32768) + envelope("product.status", {})), "invalid_command");
    const nested = JSON.stringify({ a: { a: { a: { a: { a: { a: { a: { a: { a: 1 } } } } } } } } });
    failure(() => parseSessionCommand(nested), "invalid_command");
  });
});

describe("closed session queue results", () => {
  const entry = {
    itemId: ID2,
    revision: U64_MAX,
    submitterId: ID,
    editorId: ID2,
    suspended: true,
  };
  const queue = {
    targetId: ID,
    failureSuspended: true,
    stopReason: "max_turn_requests",
    priorityFailure: null,
    waiting: [entry],
    active: null,
    priority: ID2,
    processing: {
      phase: "await_settle",
      applied: false,
      registrationId: ID2,
    },
  };

  it("preserves stop status, full precision revisions and bounded metadata", () => {
    const parsed = parseSessionQueue(queue);
    expect(parsed.stopReason).toBe("max_turn_requests");
    expect(parsed.priorityFailure).toBeNull();
    expect(parsed.waiting[0]?.revision).toBe(U64_MAX);
    expect(Object.isFrozen(parsed.waiting)).toBe(true);
    expect(parsed.waiting[0]?.suspended).toBe(true);
  });

  it("rejects unknown stop reasons, hidden fields and oversized queues", () => {
    failure(() => parseSessionQueue({ ...queue, stopReason: "completed" }), "invalid_frame");
    failure(() => parseSessionQueue({ ...queue, priorityFailure: "completed" }), "invalid_frame");
    failure(() => parseSessionQueue({ ...queue, body: "private" }), "invalid_frame");
    failure(() => parseSessionQueue({
      ...queue,
      waiting: [entry, entry, entry, entry],
    }), "invalid_frame");
  });

  it("parses explicit immediate-send selection failures", () => {
    for (const reason of ["selection_changed", "interrupt_unconfirmed"] as const) {
      const parsed = parseSessionQueue({
        ...queue,
        stopReason: null,
        priorityFailure: reason,
      });
      expect(parsed.stopReason).toBeNull();
      expect(parsed.priorityFailure).toBe(reason);
      expect(parsed.failureSuspended).toBe(true);
      expect(parsed.waiting[0]?.suspended).toBe(true);
    }
  });

  it("parses stable registration, all consume variants, and explicit-resume facts", () => {
    for (const action of [
      "input.submit",
      "input.edit",
      "input.cancel",
      "input.prioritize",
      "input.resume",
    ] as const) {
      expect(parseSessionRegistration({
        registrationId: ID,
        state: "registered",
        action,
        targetId: ID2,
        registerAuthGeneration: "7",
        registeredAtMs: "-1",
      })).toEqual({
        registrationId: ID,
        state: "registered",
        action,
        targetId: ID2,
        registerAuthGeneration: "7",
        registeredAtMs: "-1",
      });
    }
    failure(() => parseSessionRegistration({
      registrationId: ID,
      state: "registered",
      action: "submit",
      targetId: ID2,
      registerAuthGeneration: "7",
      registeredAtMs: "-1",
    }), "invalid_frame");
    expect(parseSessionConsumeResult({
      state: "settled",
      returned: true,
      request: {
        registrationId: ID,
        state: "settled",
        outcome: { kind: "direct_selected", itemId: ID, revision: "1" },
      },
    })).toEqual({
      kind: "resolved",
      returned: true,
      request: {
        registrationId: ID,
        state: "settled",
        outcome: { kind: "direct_selected", itemId: ID, revision: "1" },
      },
    });
    expect(parseSessionConsumeResult({
      registrationId: ID,
      state: "settled",
      outcome: { kind: "queued", itemId: ID, revision: "1" },
    })).toEqual({
      kind: "observed",
      fact: {
        registrationId: ID,
        state: "settled",
        outcome: { kind: "queued", itemId: ID, revision: "1" },
      },
    });
    expect(parseSessionConsumeResult({
      state: "pending",
      registrationId: ID,
    })).toEqual({
      kind: "pending",
      registrationId: ID,
    });
    expect(parseSessionConsumeResult({
      state: "refused",
      reason: "Conflict",
      returned: true,
    })).toEqual({
      kind: "refused",
      reason: "Conflict",
      returned: true,
    });
    failure(() => parseSessionConsumeResult({
      state: "settled",
      returned: true,
    }), "invalid_frame");
    failure(() => parseSessionConsumeResult({
      state: "refused",
      reason: "Conflict",
    }), "invalid_frame");
    expect(parseSessionResumeResult({
      state: "settled",
      returned: false,
      request: {
        registrationId: ID,
        state: "settled",
        outcome: { kind: "resumed", restored: "3" },
      },
    })).toEqual({
      registrationId: ID,
      restored: "3",
      returned: false,
    });
    failure(() => parseSessionResumeResult({
      state: "settled",
      returned: false,
      request: {
        registrationId: ID,
        state: "settled",
        outcome: { kind: "queued", restored: "3" },
      },
    }), "invalid_frame");
  });
});

function viewData(): Uint8Array {
  const bytes = encodeViewCredit(ID, 1n);
  const wire = new Uint8Array(75);
  wire.set(bytes);
  wire[5] = 1;
  wire[7] = 1;
  const header = new DataView(wire.buffer);
  header.setBigUint64(32, 1n);
  header.setBigUint64(48, 3n);
  wire[56] = 1;
  header.setUint32(64, 3);
  wire.set([0, 255, 128], 72);
  return wire;
}
const ack = (): Uint8Array => {
  const wire = encodeInputData(ID, 9007199254740993n, Uint8Array.of(1)).slice(0, 32);
  wire[5] = 2;
  return wire;
};

describe("authenticated server binary dispatch", () => {
  it("delegates exact ACPI ack and ACPV data including selected byte views", () => {
    expect(decodeSessionServerBinary(ack(), "authenticated")).toEqual({
      kind: "input_ack", acknowledgement: { uploadId: ID, nextOffset: 9007199254740993n },
    });
    const wire = viewData();
    const framed = new Uint8Array(wire.length + 2);
    framed.set(wire, 1);
    const decoded = decodeSessionServerBinary(framed.subarray(1, -1), "authenticated");
    expect(decoded.kind).toBe("view_data");
    if (decoded.kind === "view_data") {
      expect(decoded.frame.payload).toEqual(Uint8Array.of(0, 255, 128));
      framed.fill(0);
      expect(decoded.frame.payload).toEqual(Uint8Array.of(0, 255, 128));
    }
  });
  it("rejects server credit, detach and upload data directions", () => {
    for (const wire of [encodeViewCredit(ID, 1n), encodeViewDetach(ID), encodeInputData(ID, 0n, Uint8Array.of(1))]) {
      failure(() => decodeSessionServerBinary(wire, "authenticated"), "invalid_direction");
    }
  });
  it("rejects all binary during handshake before any decoding", () => {
    for (const wire of [ack(), viewData(), new Uint8Array(0)]) {
      failure(() => decodeSessionServerBinary(wire, "handshake"), "binary_before_auth");
    }
  });
  it("does not accept unknown magic, reserved bits or truncated frames", () => {
    const reserved = ack(); reserved[6] = 1;
    const wrongMagic = ack(); wrongMagic[3] = 0;
    const malformedView = viewData(); malformedView[57] = 1;
    for (const wire of [reserved, wrongMagic, malformedView, ack().slice(0, 31), viewData().slice(0, -1), Uint8Array.of(65, 67, 80)]) {
      failure(() => decodeSessionServerBinary(wire, "authenticated"), "invalid_frame");
    }
  });
});

describe("catalog upload, receipt, and inventory parsers", () => {
  it("parses all catalog.upload.status phases and rejects malformed failure payloads", () => {
    for (const phase of ["receiving", "preparing", "ready"] as const) {
      expect(
        parseCatalogUploadStatus({
          uploadId: ID,
          phase,
          nextOffset: BIG,
        }),
      ).toEqual({
        uploadId: ID,
        phase,
        nextOffset: BIG,
      });
    }
    expect(
      parseCatalogUploadStatus({
        uploadId: ID,
        phase: "failed",
        nextOffset: "0",
        error: "invalid_document",
      }),
    ).toEqual({
      uploadId: ID,
      phase: "failed",
      nextOffset: "0",
      error: "invalid_document",
    });
    failure(
      () =>
        parseCatalogUploadStatus({
          uploadId: ID,
          phase: "ready",
          nextOffset: "0",
          error: "unexpected",
        }),
      "invalid_frame",
    );
    failure(
      () =>
        parseCatalogUploadStatus({
          uploadId: ID,
          phase: "failed",
          nextOffset: "0",
        }),
      "invalid_frame",
    );
  });

  it("parses catalog receipt metadata with exact 2-slot SQL observation", () => {
    const slot = {
      role: "apply_write",
      transaction: "commit_confirmed",
      cleanup: "confirmed_original",
    };
    const valid = {
      fact: {
        catalogRequestId: ID,
        actorId: ID2,
        action: "install_harness",
        state: "committed",
        registeredAtMs: "100",
        registerAuthGeneration: "1",
        admissionAuthGeneration: "1",
        resultAtMs: "200",
        result: {
          kind: "harness",
          harnessId: ID,
          sourceId: ID2,
          profileId: ID,
          profileRevision: "1",
        },
      },
      auditWarning: "none",
      sqlObservation: {
        operation: "apply",
        slots: [slot, null],
      },
    };
    const parsed = parseCatalogReceiptMetadata(valid);
    expect(parsed.fact.state).toBe("committed");
    expect(parsed.sqlObservation.slots).toEqual([slot, null]);
    failure(
      () =>
        parseCatalogReceiptMetadata({
          ...valid,
          sqlObservation: { operation: "apply", slots: [slot] },
        }),
      "invalid_frame",
    );
    failure(
      () =>
        parseCatalogReceiptMetadata({
          ...valid,
          sqlObservation: { operation: "apply", slots: [slot, null, null] },
        }),
      "invalid_frame",
    );
  });

  it("parses catalog.inventory with more than 3 items via approvalQuantity", () => {
    const workspaces = [1, 2, 3, 4].map((i) => ({
      workspaceId: ID,
      canonicalPath: "D:\\ws-" + i,
    }));
    const targets = [1, 2, 3, 4].map((i) => ({
      targetId: ID,
      sourceId: ID2,
      nativeSessionId: "codex-" + i,
      workspaceId: ID,
      binding:
        i % 2 === 0
          ? {
              profileId: ID,
              profileRevision: String(i),
              workspaceId: ID,
              launchPath: "D:\\ws-" + i,
              observedCanonicalPath: "D:\\ws-" + i,
            }
          : null,
    }));
    const parsed = parseCatalogInventory({
      workspaces,
      sources: [{ sourceId: ID2, name: "Codex Source" }],
      harnesses: [{ harnessId: ID, name: "Codex ACP", sourceId: ID2 }],
      profiles: [{ profileId: ID, revision: "1", harnessId: ID, sourceId: ID2 }],
      targets,
    });
    expect(parsed.workspaces).toHaveLength(4);
    expect(parsed.targets).toHaveLength(4);
    expect(parsed.targets[1]?.binding?.profileRevision).toBe("2");
    expect(parsed.targets[0]?.binding).toBeNull();
  });
});

describe("session-protocol barrel boundaries and submodules", () => {
  it("does not export internal validation primitives from the barrel", () => {
    const internalPrimitives = [
      "closed",
      "object",
      "oneOf",
      "literal",
      "responseResult",
      "requireNullable",
      "requireNull",
      "requireBoolean",
      "requireArray",
      "approvalQuantity",
      "requireText",
      "requireTrue",
      "requireBoundedSourceText",
      "mutation",
      "sourceSelection",
      "sourceOperation",
    ];
    for (const name of internalPrimitives) {
      expect(Object.hasOwn(SessionProtocolBarrel, name)).toBe(false);
    }
  });

  it("exports identical functions from submodules and barrel", () => {
    expect(primRequireSessionId).toBe(SessionProtocolBarrel.requireSessionId);
    expect(qiParseSessionQueue).toBe(SessionProtocolBarrel.parseSessionQueue);
    expect(catParseCatalogInventory).toBe(SessionProtocolBarrel.parseCatalogInventory);
    expect(vaParseSessionViewOpened).toBe(SessionProtocolBarrel.parseSessionViewOpened);
    expect(scParseSessionCapacity).toBe(SessionProtocolBarrel.parseSessionCapacity);
    expect(wcParseSessionCommand).toBe(SessionProtocolBarrel.parseSessionCommand);
  });
});
