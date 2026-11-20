export {
  closedWithOptional, requireSessionI64, requireSessionId,
  requireSessionU64, SessionProtocolError,
} from "./session-protocol/primitives";

export type {
  InputUploadBeginResult, InputUploadFinishResult,
  InputUploadReleaseResult, InputUploadStatus,
  QueueMutationDto, SessionConsumeResult, SessionInputFact,
  SessionInputOutcome, SessionPriorityFailure, SessionQueueAction,
  SessionQueueEntry, SessionQueueProcessing,
  SessionQueueProcessingPhase, SessionQueueSnapshot,
  SessionQueuedBodyDescriptor, SessionRegistration,
  SessionResumeResult, SessionStopReason,
} from "./session-protocol/queue-and-input";
export {
  parseInputUploadBeginResult, parseInputUploadFinishResult,
  parseInputUploadReleaseResult, parseInputUploadStatus,
  parseSessionConsumeResult, parseSessionInputFact,
  parseSessionQueue, parseSessionQueuedBodyDescriptor,
  parseSessionRegistration, parseSessionResumeResult,
} from "./session-protocol/queue-and-input";

export type {
  CatalogFactMetadata, CatalogFactResult, CatalogInventory,
  CatalogReceiptMetadata, CatalogRegistration,
  CatalogSqlObservationSet, CatalogSqlSlotObservation,
  CatalogUploadBeginResult, CatalogUploadFinishResult,
  CatalogUploadReleaseResult, CatalogUploadStatus,
  HarnessSummary, ProductSourceSummary, ProductTargetRecord,
  ProfileSummary, TargetBindingSummary, WorkspaceRecord,
} from "./session-protocol/catalog";
export {
  parseCatalogFactMetadata, parseCatalogInventory,
  parseCatalogReceiptMetadata, parseCatalogRegistration,
  parseCatalogUploadBeginResult, parseCatalogUploadFinishResult,
  parseCatalogUploadReleaseResult, parseCatalogUploadStatus,
  parseProductTargetRecord, parseWorkspaceRecord,
} from "./session-protocol/catalog";

export type {
  ApprovalAssociation, ApprovalDecision, ApprovalDecisionResult,
  ApprovalListResult, ApprovalRegistration, ApprovalStatus,
  ApprovalValidity, ApprovalView,
  SessionViewClosed, SessionViewFreshness, SessionViewJoin,
  SessionViewMode, SessionViewOpened, SessionViewProjection,
  SessionViewStatus, TranscriptCacheState,
  TranscriptCapabilityBoundary, TranscriptSource, TranscriptSyncState,
} from "./session-protocol/view-and-approval";
export {
  parseApprovalDecisionResult, parseApprovalListResult,
  parseApprovalRegistration, parseApprovalResult,
  parseApprovalStatusResult, parseSessionViewClosed,
  parseSessionViewOpened,
} from "./session-protocol/view-and-approval";

export type {
  NativeReleaseRegistered, NativeReleaseStatus, SessionCapacity,
  SourceLaunchSelectionInput, SourceOperationInput,
  SourceSessionFact, SourceSessionFailureClass,
  SourceSessionRegistration, SourceSessionRegistrationMetadata,
  SourceSessionResponseFact, SourceSessionResultDescriptor,
  SourceSessionResultReleaseResult, SourceSessionRpcId,
  SourceSessionTransportFact, SourceSessionTransportReason,
} from "./session-protocol/source-and-capacity";
export {
  parseNativeReleaseRegisterResult, parseNativeReleaseStatusResult,
  parseSessionCapacity, parseSourceSessionFact,
  parseSourceSessionRegistration, parseSourceSessionResultDescriptor,
  parseSourceSessionResultReleaseResult,
} from "./session-protocol/source-and-capacity";

export type {
  SessionAction, SessionCommand, SessionServerBinary,
} from "./session-protocol/wire-codec";
export {
  decodeSessionServerBinary, encodeSessionCommand, parseSessionCommand,
} from "./session-protocol/wire-codec";

export type { SessionControlSnapshot } from "./session-protocol/control";
export { parseSessionControlSnapshot } from "./session-protocol/control";
export type { HistoryPage, HistoryRequest } from "./session-protocol/history";
