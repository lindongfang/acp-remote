import {
  parseStrictJsonObject,
  type JsonValue,
} from "../../protocol/jws";
import { decodeViewFrame, type ViewDataFrame } from "../../views/binary";
import { ViewDecodeError } from "../../views/errors";
import {
  decodeInputAcknowledgement,
  InputTransferError,
  type InputAcknowledgement,
} from "../input-transfer";
import { MAX_MANAGEMENT_FRAME_BYTES } from "../protocol";
import {
  closed,
  closedWithOptional,
  object,
  oneOf,
  requireBoundedSourceText,
  requireSessionId,
  requireSessionU64,
  SessionProtocolError,
  type Shape,
  type ShapeValue,
} from "./primitives";
import { mutation } from "./queue-and-input";
import { sourceOperation, sourceSelection } from "./source-and-capacity";

const ENCODER = new TextEncoder();

const SHAPES = {
  "product.status": {},
  "input.upload.begin": {
    targetId: requireSessionId,
    purpose: oneOf("submit", "edit"),
  },
  "input.upload.finish": {
    uploadId: requireSessionId,
    totalBytes: requireSessionU64,
  },
  "input.upload.status": { uploadId: requireSessionId },
  "input.upload.release": { uploadId: requireSessionId },
  "input.register": {
    required: { targetId: requireSessionId, operation: mutation },
    optional: { expectedControlEpoch: requireSessionU64 },
  },
  "input.consume": {
    required: {
      targetId: requireSessionId,
      registrationId: requireSessionId,
      bodyId: (value: JsonValue | undefined): string | null =>
        value === null ? null : requireSessionId(value),
    },
    optional: { expectedControlEpoch: requireSessionU64 },
  },
  "input.request": {
    targetId: requireSessionId,
    registrationId: requireSessionId,
  },
  "input.queue": { targetId: requireSessionId },
  "input.body.open": {
    targetId: requireSessionId,
    itemId: requireSessionId,
    expectedRevision: requireSessionU64,
  },
  "catalog.upload.begin": {},
  "catalog.upload.finish": {
    uploadId: requireSessionId,
    totalBytes: requireSessionU64,
  },
  "catalog.upload.status": { uploadId: requireSessionId },
  "catalog.upload.release": { uploadId: requireSessionId },
  "catalog.register": { documentUploadId: requireSessionId },
  "catalog.apply": { catalogRequestId: requireSessionId },
  "catalog.request": { catalogRequestId: requireSessionId },
  "workspace.register": { path: requireBoundedSourceText(true) },
  "target.register": {
    sourceId: requireSessionId,
    nativeSessionId: requireBoundedSourceText(true),
    workspaceId: requireSessionId,
  },
  "catalog.inventory": {},
  "session.capacity.inspect": {},
  "approval.list": {},
  "approval.read": { approvalId: requireSessionId },
  "approval.register": {
    required: {
      approvalId: requireSessionId,
      choiceId: requireSessionId,
    },
    optional: { expectedControlEpoch: requireSessionU64 },
  },
  "approval.decide": {
    required: { approvalRegistrationId: requireSessionId },
    optional: { expectedControlEpoch: requireSessionU64 },
  },
  "approval.status": { approvalId: requireSessionId },
  "session.control.inspect": { targetId: requireSessionId },
  "session.control.takeover": { targetId: requireSessionId },
  "session.control.release": {
    required: { targetId: requireSessionId },
    optional: { expectedControlEpoch: requireSessionU64 },
  },
  "native.release.register": { targetId: requireSessionId },
  "native.release": { releaseOperationId: requireSessionId },
  "native.release.status": { releaseOperationId: requireSessionId },
  "source.session.register": {
    selection: sourceSelection,
    operation: sourceOperation,
  },
  "source.session.execute": { sourceOperationId: requireSessionId },
  "source.session.status": { sourceOperationId: requireSessionId },
  "source.session.reconcile": { sourceOperationId: requireSessionId },
  "source.session.result.open": { sourceOperationId: requireSessionId },
  "source.session.result.release": { sourceOperationId: requireSessionId },
  "view.open": {
    targetId: requireSessionId,
    reason: oneOf(
      "first_open",
      "explicit_refresh",
      "reopen",
      "reconnect",
      "reacquire",
    ),
  },
  "view.open_source": {
    selection: sourceSelection,
    nativeSessionId: requireBoundedSourceText(true),
    reason: oneOf(
      "first_open",
      "explicit_refresh",
      "reopen",
      "reconnect",
      "reacquire",
    ),
  },
  "view.close": { streamId: requireSessionId },
  "view.original": { streamId: requireSessionId },
  "view.rebind": { streamId: requireSessionId },
} as const;

type CommandShapeSpec =
  | Shape
  | { readonly required: Shape; readonly optional: Shape };
type CommandPayloadValue<Spec extends CommandShapeSpec> = Spec extends {
  readonly required: infer R extends Shape;
  readonly optional: infer O extends Shape;
}
  ? ShapeValue<R> & { readonly [K in keyof O]?: ReturnType<O[K]> }
  : Spec extends Shape
    ? ShapeValue<Spec>
    : never;

function isOptionalShapeSpec(
  spec: CommandShapeSpec,
): spec is { readonly required: Shape; readonly optional: Shape } {
  return (
    Object.hasOwn(spec, "required") &&
    Object.hasOwn(spec, "optional") &&
    typeof spec.required === "object" &&
    typeof spec.optional === "object"
  );
}

const VALID_SHAPES = SHAPES satisfies Readonly<Record<string, CommandShapeSpec>>;
void VALID_SHAPES;

export type SessionAction = keyof typeof SHAPES;
export type SessionCommand = {
  [A in SessionAction]: {
    readonly v: 1;
    readonly type: "command";
    readonly requestId: string;
    readonly action: A;
    readonly payload: CommandPayloadValue<(typeof SHAPES)[A]>;
  };
}[SessionAction];

/** Syntax only: neither service availability, admission nor delivery is implied. */
export function parseSessionCommand(input: string): SessionCommand {
  try {
    if (
      typeof input !== "string" ||
      input.length > MAX_MANAGEMENT_FRAME_BYTES
    ) {
      throw new SessionProtocolError("invalid_command");
    }
    const bytes = ENCODER.encode(input);
    if (bytes.length > MAX_MANAGEMENT_FRAME_BYTES) {
      throw new SessionProtocolError("invalid_command");
    }
    const source = parseStrictJsonObject(bytes);
    const envelope = closed(source, {
      v: (value) => {
        if (value !== 1) throw new SessionProtocolError("invalid_command");
        return 1 as const;
      },
      type: oneOf("command"),
      requestId: requireSessionId,
      action: (value): SessionAction => {
        if (typeof value !== "string" || !Object.hasOwn(SHAPES, value)) {
          throw new SessionProtocolError("invalid_command");
        }
        return value as SessionAction;
      },
      payload: object,
    });
    const shapeSpec: CommandShapeSpec = SHAPES[envelope.action];
    const payload = isOptionalShapeSpec(shapeSpec)
      ? closedWithOptional(envelope.payload, shapeSpec.required, shapeSpec.optional)
      : closed(envelope.payload, shapeSpec);
    // The action selects the exact closed shape before this discriminated union is built.
    return Object.freeze({ ...envelope, payload }) as SessionCommand;
  } catch (error) {
    if (error instanceof SessionProtocolError) throw error;
    if (error instanceof RangeError)
      throw new SessionProtocolError("allocation_failed");
    throw new SessionProtocolError("invalid_command");
  }
}

export function encodeSessionCommand(command: SessionCommand): string {
  try {
    return JSON.stringify(parseSessionCommand(JSON.stringify(command)));
  } catch (error) {
    if (error instanceof SessionProtocolError) throw error;
    if (error instanceof RangeError)
      throw new SessionProtocolError("allocation_failed");
    throw new SessionProtocolError("invalid_command");
  }
}

export type SessionServerBinary =
  | {
      readonly kind: "input_ack";
      readonly acknowledgement: InputAcknowledgement;
    }
  | { readonly kind: "view_data"; readonly frame: ViewDataFrame };

/** The caller supplies its connection phase; this function never authenticates a socket. */
export function decodeSessionServerBinary(
  wire: Uint8Array,
  phase: "handshake" | "authenticated",
): SessionServerBinary {
  if (phase !== "authenticated")
    throw new SessionProtocolError("binary_before_auth");
  if (!(wire instanceof Uint8Array) || wire.length < 4) {
    throw new SessionProtocolError("invalid_frame");
  }
  try {
    if (wire[0] !== 65 || wire[1] !== 67 || wire[2] !== 80) {
      throw new SessionProtocolError("invalid_frame");
    }
    if (wire[3] === 73) {
      return Object.freeze({
        kind: "input_ack",
        acknowledgement: decodeInputAcknowledgement(wire),
      });
    }
    if (wire[3] === 86) {
      const frame = decodeViewFrame(wire);
      if (frame.type !== "data")
        throw new SessionProtocolError("invalid_direction");
      return Object.freeze({ kind: "view_data", frame });
    }
    throw new SessionProtocolError("invalid_frame");
  } catch (error) {
    if (error instanceof SessionProtocolError) throw error;
    if (
      error instanceof RangeError ||
      ((error instanceof ViewDecodeError ||
        error instanceof InputTransferError) &&
        error.code === "allocation_failed")
    ) {
      throw new SessionProtocolError("allocation_failed");
    }
    if (
      error instanceof InputTransferError &&
      error.code === "invalid_direction"
    ) {
      throw new SessionProtocolError("invalid_direction");
    }
    throw new SessionProtocolError("invalid_frame");
  }
}
