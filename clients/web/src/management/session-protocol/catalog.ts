import type { JsonValue } from "../../protocol/jws";
import {
  approvalQuantity,
  closed,
  object,
  oneOf,
  requireNullable,
  requireSessionId,
  requireSessionI64,
  requireSessionU64,
  requireText,
  requireTrue,
  responseResult,
  SessionProtocolError,
} from "./primitives";

export interface CatalogUploadBeginResult {
  readonly uploadId: string;
}

export type CatalogUploadStatus =
  | {
      readonly uploadId: string;
      readonly phase: "receiving" | "preparing" | "ready";
      readonly nextOffset: string;
    }
  | {
      readonly uploadId: string;
      readonly phase: "failed";
      readonly nextOffset: string;
      readonly error: string;
    };

export interface CatalogUploadFinishResult {
  readonly uploadId: string;
}

export interface CatalogUploadReleaseResult {
  readonly released: true;
}

export interface CatalogRegistration {
  readonly catalogRequestId: string;
  readonly state: "registered";
}

export type CatalogFactResult =
  | {
      readonly kind: "harness" | "profile";
      readonly harnessId: string;
      readonly sourceId: string;
      readonly profileId: string;
      readonly profileRevision: string;
    }
  | {
      readonly kind: "target";
      readonly targetId: string;
      readonly workspaceId: string;
      readonly harnessId: string;
      readonly sourceId: string;
      readonly profileId: string;
      readonly profileRevision: string;
    };

export interface CatalogFactMetadata {
  readonly catalogRequestId: string;
  readonly actorId: string;
  readonly action:
    | "install_harness"
    | "publish_profile"
    | "bind_target_initial";
  readonly state: "registered" | "committed" | "rejected";
  readonly registeredAtMs: string;
  readonly registerAuthGeneration: string;
  readonly admissionAuthGeneration: string | null;
  readonly resultAtMs: string | null;
  readonly result: CatalogFactResult | null;
}

export interface CatalogSqlSlotObservation {
  readonly role:
    | "identity_read"
    | "registered_snapshot"
    | "register_write"
    | "apply_write"
    | "query_read"
    | "configuration_read"
    | "recovery_read";
  readonly transaction:
    | "no_transaction"
    | "begun_read"
    | "begun_write"
    | "commit_polling"
    | "commit_confirmed"
    | "rollback_confirmed"
    | "effect_unknown";
  readonly cleanup:
    | "no_detached_connection"
    | "pending_original"
    | "confirmed_original"
    | "unconfirmed_original";
}

export interface CatalogSqlObservationSet {
  readonly operation:
    | "register"
    | "apply"
    | "query"
    | "resolve"
    | "resolve_read"
    | "reconcile";
  readonly slots: readonly [
    CatalogSqlSlotObservation | null,
    CatalogSqlSlotObservation | null,
  ];
}

export interface CatalogReceiptMetadata {
  readonly fact: CatalogFactMetadata;
  readonly auditWarning: "none" | "unavailable" | "full";
  readonly sqlObservation: CatalogSqlObservationSet;
}

export interface WorkspaceRecord {
  readonly workspaceId: string;
  readonly canonicalPath: string;
}

export interface ProductSourceSummary {
  readonly sourceId: string;
  readonly name: string;
}

export interface HarnessSummary {
  readonly harnessId: string;
  readonly name: string;
  readonly sourceId: string;
}

export interface ProfileSummary {
  readonly profileId: string;
  readonly revision: string;
  readonly harnessId: string;
  readonly sourceId: string;
}

export interface TargetBindingSummary {
  readonly profileId: string;
  readonly profileRevision: string;
  readonly workspaceId: string;
  readonly launchPath: string;
  readonly observedCanonicalPath: string;
}

export interface ProductTargetRecord {
  readonly targetId: string;
  readonly sourceId: string;
  readonly nativeSessionId: string;
  readonly workspaceId: string | null;
  readonly binding: TargetBindingSummary | null;
}

export interface CatalogInventory {
  readonly workspaces: readonly WorkspaceRecord[];
  readonly sources: readonly ProductSourceSummary[];
  readonly harnesses: readonly HarnessSummary[];
  readonly profiles: readonly ProfileSummary[];
  readonly targets: readonly ProductTargetRecord[];
}

export function parseCatalogUploadBeginResult(
  value: JsonValue,
): CatalogUploadBeginResult {
  return responseResult(() =>
    closed(value, {
      uploadId: requireSessionId,
    }),
  );
}

export function parseCatalogUploadStatus(
  value: JsonValue,
): CatalogUploadStatus {
  return responseResult(() => {
    const source = object(value);
    const phase = oneOf("receiving", "preparing", "ready", "failed")(
      source.phase,
    );
    switch (phase) {
      case "receiving":
      case "preparing":
      case "ready":
        return closed(value, {
          uploadId: requireSessionId,
          phase: oneOf(phase),
          nextOffset: requireSessionU64,
        });
      case "failed":
        return closed(value, {
          uploadId: requireSessionId,
          phase: oneOf("failed"),
          nextOffset: requireSessionU64,
          error: requireText,
        });
    }
  });
}

export function parseCatalogUploadFinishResult(
  value: JsonValue,
): CatalogUploadFinishResult {
  return responseResult(() =>
    closed(value, {
      uploadId: requireSessionId,
    }),
  );
}

export function parseCatalogUploadReleaseResult(
  value: JsonValue,
): CatalogUploadReleaseResult {
  return responseResult(() =>
    closed(value, {
      released: requireTrue,
    }),
  );
}

export function parseCatalogRegistration(
  value: JsonValue,
): CatalogRegistration {
  return responseResult(() =>
    closed(value, {
      catalogRequestId: requireSessionId,
      state: oneOf("registered"),
    }),
  );
}

function catalogFactResult(
  value: JsonValue | undefined,
): CatalogFactResult | null {
  if (value === null) return null;
  const source = object(value);
  switch (oneOf("harness", "profile", "target")(source.kind)) {
    case "harness":
    case "profile":
      return closed(value, {
        kind: oneOf("harness", "profile"),
        harnessId: requireSessionId,
        sourceId: requireSessionId,
        profileId: requireSessionId,
        profileRevision: requireSessionU64,
      });
    case "target":
      return closed(value, {
        kind: oneOf("target"),
        targetId: requireSessionId,
        workspaceId: requireSessionId,
        harnessId: requireSessionId,
        sourceId: requireSessionId,
        profileId: requireSessionId,
        profileRevision: requireSessionU64,
      });
  }
}

function catalogFactMetadataValue(
  value: JsonValue | undefined,
): CatalogFactMetadata {
  return closed(value, {
    catalogRequestId: requireSessionId,
    actorId: requireSessionId,
    action: oneOf("install_harness", "publish_profile", "bind_target_initial"),
    state: oneOf("registered", "committed", "rejected"),
    registeredAtMs: requireSessionI64,
    registerAuthGeneration: requireSessionI64,
    admissionAuthGeneration: requireNullable(requireSessionI64),
    resultAtMs: requireNullable(requireSessionI64),
    result: catalogFactResult,
  });
}

export function parseCatalogFactMetadata(
  value: JsonValue,
): CatalogFactMetadata {
  return responseResult(() => catalogFactMetadataValue(value));
}

function catalogSqlSlot(
  value: JsonValue | undefined,
): CatalogSqlSlotObservation | null {
  if (value === null) return null;
  return closed(value, {
    role: oneOf(
      "identity_read",
      "registered_snapshot",
      "register_write",
      "apply_write",
      "query_read",
      "configuration_read",
      "recovery_read",
    ),
    transaction: oneOf(
      "no_transaction",
      "begun_read",
      "begun_write",
      "commit_polling",
      "commit_confirmed",
      "rollback_confirmed",
      "effect_unknown",
    ),
    cleanup: oneOf(
      "no_detached_connection",
      "pending_original",
      "confirmed_original",
      "unconfirmed_original",
    ),
  });
}

function catalogSqlObservation(
  value: JsonValue | undefined,
): CatalogSqlObservationSet {
  return closed(value, {
    operation: oneOf(
      "register",
      "apply",
      "query",
      "resolve",
      "resolve_read",
      "reconcile",
    ),
    slots: (slots) => {
      if (!Array.isArray(slots) || slots.length !== 2) {
        throw new SessionProtocolError("invalid_frame");
      }
      return Object.freeze([
        catalogSqlSlot(slots[0]),
        catalogSqlSlot(slots[1]),
      ] as const);
    },
  });
}

export function parseCatalogReceiptMetadata(
  value: JsonValue,
): CatalogReceiptMetadata {
  return responseResult(() =>
    closed(value, {
      fact: catalogFactMetadataValue,
      auditWarning: oneOf("none", "unavailable", "full"),
      sqlObservation: catalogSqlObservation,
    }),
  );
}

function workspaceRecordValue(
  value: JsonValue | undefined,
): WorkspaceRecord {
  return closed(value, {
    workspaceId: requireSessionId,
    canonicalPath: requireText,
  });
}

export function parseWorkspaceRecord(value: JsonValue): WorkspaceRecord {
  return responseResult(() => workspaceRecordValue(value));
}

function productTargetRecordValue(
  value: JsonValue | undefined,
): ProductTargetRecord {
  return closed(value, {
    targetId: requireSessionId,
    sourceId: requireSessionId,
    nativeSessionId: requireText,
    workspaceId: requireNullable(requireSessionId),
    binding: requireNullable((binding) =>
      closed(binding, {
        profileId: requireSessionId,
        profileRevision: requireSessionU64,
        workspaceId: requireSessionId,
        launchPath: requireText,
        observedCanonicalPath: requireText,
      }),
    ),
  });
}

export function parseProductTargetRecord(
  value: JsonValue,
): ProductTargetRecord {
  return responseResult(() => productTargetRecordValue(value));
}

export function parseCatalogInventory(value: JsonValue): CatalogInventory {
  return responseResult(() =>
    closed(value, {
      workspaces: (items) => approvalQuantity(items, workspaceRecordValue),
      sources: (items) =>
        approvalQuantity(items, (item) =>
          closed(item, {
            sourceId: requireSessionId,
            name: requireText,
          }),
        ),
      harnesses: (items) =>
        approvalQuantity(items, (item) =>
          closed(item, {
            harnessId: requireSessionId,
            name: requireText,
            sourceId: requireSessionId,
          }),
        ),
      profiles: (items) =>
        approvalQuantity(items, (item) =>
          closed(item, {
            profileId: requireSessionId,
            revision: requireSessionU64,
            harnessId: requireSessionId,
            sourceId: requireSessionId,
          }),
        ),
      targets: (items) => approvalQuantity(items, productTargetRecordValue),
    }),
  );
}
