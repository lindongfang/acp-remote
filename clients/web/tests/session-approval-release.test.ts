import { describe, expect, it } from "vitest";

import {
  parseApprovalListResult,
  parseApprovalStatusResult,
  parseNativeReleaseRegisterResult,
  parseNativeReleaseStatusResult,
  SessionProtocolError,
} from "../src/management/session-protocol";
import type { JsonValue } from "../src/protocol/jws";

const APPROVAL = "AAECAwQFBgcICQoLDA0ODw";
const CHOICE = "AQEBAQEBAQEBAQEBAQEBAQ";
const TARGET = "AgICAgICAgICAgICAgICAg";
const OPERATION = "AwMDAwMDAwMDAwMDAwMDAw";

function rejects(run: () => unknown): void {
  let caught: unknown;
  try {
    run();
  } catch (error) {
    caught = error;
  }
  expect(caught).toBeInstanceOf(SessionProtocolError);
}

const status = {
  approvalId: APPROVAL,
  changeSequence: "9",
  validity: "current_pending",
  association: { kind: "registered_session", targetId: TARGET },
  decision: null,
  delivery: "not_attempted",
  sourceClose: "open",
  necessaryFactsMissing: false,
  reconciliationRunning: false,
} satisfies JsonValue;

const view = {
  approvalId: APPROVAL,
  source: {
    sourceId: TARGET,
    runtimeId: TARGET,
    instanceId: TARGET,
    connectionEpoch: TARGET,
    protocolVersion: "2",
  },
  context: { kind: "session_request" },
  choices: [{
    choiceId: CHOICE,
    ordinal: "0",
    originalOptionId: "allow-once",
    originalKind: "allow_once",
    selectable: true,
  }],
  content: {
    availability: "available",
    rawJson: '{"jsonrpc":"2.0","id":1,"method":"session/request_permission"}',
  },
  limitations: ["unknown_subject"],
  status,
} satisfies JsonValue;

describe("closed approval and release results", () => {
  it("accepts the exact minimal approval view and keeps raw content inert", () => {
    const parsed = parseApprovalListResult({ items: [view] });
    expect(parsed.items).toHaveLength(1);
    expect(parsed.items[0]?.content).toEqual(view.content);
    expect(Object.isFrozen(parsed.items[0])).toBe(true);
    expect(Object.isFrozen(parsed.items[0]?.choices[0])).toBe(true);
    expect(parseApprovalStatusResult(status).validity).toBe("current_pending");
  });

  it("rejects unknown fields and executable mismatches", () => {
    rejects(() => parseApprovalListResult({ items: [{ ...view, action: "approval.decide" }] }));
    rejects(() => parseApprovalListResult({
      items: [{ ...view, content: { ...view.content, rawJson: 3 } }],
    }));
    rejects(() => parseApprovalStatusResult({ ...status, approvalId: "not-an-id" }));
    rejects(() => parseApprovalStatusResult({ ...status, delivery: "completed" }));
  });

  it("accepts only the closed release registration and status shapes", () => {
    expect(parseNativeReleaseRegisterResult({
      releaseOperationId: OPERATION,
      targetId: TARGET,
      state: "registered",
    })).toEqual({
      releaseOperationId: OPERATION,
      targetId: TARGET,
      state: "registered",
    });
    expect(parseNativeReleaseStatusResult({
      releaseOperationId: OPERATION,
      targetId: TARGET,
      state: "released",
      error: null,
    }).state).toBe("released");
    rejects(() => parseNativeReleaseStatusResult({
      releaseOperationId: OPERATION,
      targetId: TARGET,
      state: "released",
      error: "not-null",
    }));
  });
});
