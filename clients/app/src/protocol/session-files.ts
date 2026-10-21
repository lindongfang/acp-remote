/**
 * `core.session-files.v1` 四条只读查询的**严格** TypeScript 镜像与运行期解码。
 *
 * ## 为什么这里既有类型又有解码
 *
 * `command.ts` 只描述既有 Sync 面的类型，运行期校验一直放在 `src/sync-client/wire.ts`。
 * 但四条新查询的**结果**形状直接决定客户端状态（文件条目、字节块、Git 差异），把
 * 「未知键/缺必填/非法枚举/非法十进制/非法路径」留到消费现场判断会让错误形状流进 view model。
 * 因此本模块按 `schemas/sync/v1/command.schema.json` 的 `$defs` 逐字段给出：
 *
 * - 严格类型（`CommandBody` 的四个分支与四个结果类型）；
 * - 严格解码（`decodeSessionFilesCommandBody` / `decodeSessionFilesResult`）：接受 `unknown`，
 *   返回结构化拒绝原因，**不抛异常、不静默降级**；
 * - 客户端与 Rust 共用的 feature ID 与命令词表。
 *
 * 形状的唯一权威仍是仓库根的 `schemas/sync/v1/command.schema.json` 与
 * `fixtures/sync/v1/manifest.json`；本文件不复制样例，契约测试直接读共享 fixture。
 */

import type { CommandEnvelope } from "./command";
import type { DecimalString, Uuid } from "./common";
import { isRecord } from "./runtime";

/** 协商 feature：未选中时客户端**不派发**四条查询（服务端也会在任何 I/O 前拒绝）。 */
export const SESSION_FILES_FEATURE = "core.session-files.v1";

/** 四条查询的封闭命令词表。 */
export type SessionFilesCommandName =
  | "session.files.list"
  | "session.files.read"
  | "session.git.status"
  | "session.git.diff";

export const SESSION_FILES_COMMANDS: readonly SessionFilesCommandName[] = [
  "session.files.list",
  "session.files.read",
  "session.git.status",
  "session.git.diff",
];

/** 根内相对路径：非空、单分隔符、无父引用/控制字符/盘符/ADS、任一分量不得是 `.git`。 */
export type SessionRelativePath = string;

/** `session.files.list` 的路径：空串表示会话根。 */
export type SessionFilesPath = string;

/** 只用于翻页的短期不透明位置标识；客户端 MUST NOT 解析其结构。 */
export type SessionPageToken = string;

/** 连接期内容版本：不透明快照标识，客户端 MUST NOT 解析或跨连接复用。 */
export type SessionContentVersion = string;

export type SessionFileEntryKind = "file" | "directory" | "link" | "other";
export type SessionFileRestrictedReason = "outside_root" | "unreadable" | "unsupported";
export type SessionFileContentKind = "text" | "image" | "binary";
export type SessionFilesReadPurpose = "preview" | "download";
export type GitState = "available" | "not_repository" | "unavailable";
export type GitFileStatus = "added" | "modified" | "deleted" | "renamed" | "untracked";
export type GitDiffBase = "head" | "empty";
export type GitDiffKind = "text" | "binary" | "conflict";
export type GitDiffUnavailableReason = "not_repository" | "git_unavailable" | "too_large";

/** `$defs.sessionFileEntry`。 */
export interface SessionFileEntry {
  readonly path: SessionRelativePath;
  readonly name: string;
  readonly kind: SessionFileEntryKind;
  readonly openable: boolean;
  readonly restricted?: SessionFileRestrictedReason;
}

/** `$defs.sessionFilesList` 的 payload。 */
export interface SessionFilesListPayload {
  readonly path: SessionFilesPath;
  readonly hideIgnored: boolean;
  readonly after?: SessionPageToken;
}

/** `$defs.sessionFilesList`。 */
export interface SessionFilesListCommand extends CommandEnvelope {
  readonly body: {
    readonly requestId: Uuid;
    readonly command: "session.files.list";
    readonly sessionId: Uuid;
    readonly payload: SessionFilesListPayload;
  };
}

/** `$defs.sessionFilesListResult`。 */
export interface SessionFilesListResult {
  readonly path: SessionFilesPath;
  readonly gitState: GitState;
  readonly entries: readonly SessionFileEntry[];
  readonly next?: SessionPageToken;
}

/** `$defs.sessionFilesRead` 的 payload。 */
export interface SessionFilesReadPayload {
  readonly path: SessionRelativePath;
  readonly purpose: SessionFilesReadPurpose;
  readonly offset: DecimalString;
  readonly version?: SessionContentVersion;
}

/** `$defs.sessionFilesRead`。 */
export interface SessionFilesReadCommand extends CommandEnvelope {
  readonly body: {
    readonly requestId: Uuid;
    readonly command: "session.files.read";
    readonly sessionId: Uuid;
    readonly payload: SessionFilesReadPayload;
  };
}

/** `$defs.sessionFilesReadResult`。 */
export interface SessionFilesReadResult {
  readonly path: SessionRelativePath;
  readonly kind: SessionFileContentKind;
  readonly sizeBytes: DecimalString;
  readonly version: SessionContentVersion;
  readonly offset: DecimalString;
  /** 无填充 base64url；单块原始字节最多 32 KiB（零字节文件允许空串）。 */
  readonly dataBase64: string;
  readonly eof: boolean;
  readonly previewTruncated: boolean;
}

/** `$defs.sessionGitStatus` 的 payload。 */
export interface SessionGitStatusPayload {
  readonly after?: SessionPageToken;
}

/** `$defs.sessionGitStatus`。 */
export interface SessionGitStatusCommand extends CommandEnvelope {
  readonly body: {
    readonly requestId: Uuid;
    readonly command: "session.git.status";
    readonly sessionId: Uuid;
    readonly payload: SessionGitStatusPayload;
  };
}

/** `$defs.gitStatusEntry`。 */
export interface GitStatusEntry {
  readonly path: SessionRelativePath;
  readonly status: GitFileStatus;
  readonly previousPath?: SessionRelativePath;
  readonly conflicted: boolean;
  readonly binary: boolean;
  readonly addedLines?: number;
  readonly deletedLines?: number;
}

/** `$defs.sessionGitStatusResult`。 */
export interface SessionGitStatusResult {
  readonly gitState: GitState;
  readonly entries: readonly GitStatusEntry[];
  readonly next?: SessionPageToken;
}

/** `$defs.sessionGitDiff` 的 payload。 */
export interface SessionGitDiffPayload {
  readonly path: SessionRelativePath;
}

/** `$defs.sessionGitDiff`。 */
export interface SessionGitDiffCommand extends CommandEnvelope {
  readonly body: {
    readonly requestId: Uuid;
    readonly command: "session.git.diff";
    readonly sessionId: Uuid;
    readonly payload: SessionGitDiffPayload;
  };
}

/**
 * `$defs.sessionGitDiffResult`。
 *
 * `diffLines` 与 `unavailable` **互斥且都可缺席**：schema 没有 `not`，但描述明确要求「不可用时
 * 只带 unavailable，两者不得同时出现」，解码器据此显式拒绝同时出现的形状。
 */
export interface SessionGitDiffResult {
  readonly path: SessionRelativePath;
  readonly base: GitDiffBase;
  readonly kind: GitDiffKind;
  readonly diffLines?: readonly string[];
  readonly unavailable?: GitDiffUnavailableReason;
  readonly truncated: boolean;
  readonly comparison?: {readonly before: string; readonly after: string};
}

export type SessionFilesCommand =
  | SessionFilesListCommand
  | SessionFilesReadCommand
  | SessionGitStatusCommand
  | SessionGitDiffCommand;

export type SessionFilesCommandBody = SessionFilesCommand["body"];

export type SessionFilesResult =
  | SessionFilesListResult
  | SessionFilesReadResult
  | SessionGitStatusResult
  | SessionGitDiffResult;

// ── 解码 ────────────────────────────────────────────────────────────────────

/** 结构拒绝原因：只描述形状，不含 wire 原文（原文可能含用户内容）。 */
export type SessionFilesRejectReason =
  | "not_an_object"
  | "unknown_key"
  | "missing_key"
  | "field_type"
  | "invalid_path"
  | "invalid_enum"
  | "invalid_decimal"
  | "invalid_token"
  | "invalid_base64"
  | "mutually_exclusive";

export interface SessionFilesDecoded<T> {
  readonly ok: true;
  readonly value: T;
}

export interface SessionFilesRejected {
  readonly ok: false;
  readonly reason: SessionFilesRejectReason;
  readonly detail: string;
}

export type SessionFilesDecodeResult<T> = SessionFilesDecoded<T> | SessionFilesRejected;

function decoded<T>(value: T): SessionFilesDecoded<T> {
  return { ok: true, value };
}

function rejected<T>(reason: SessionFilesRejectReason, detail: string): SessionFilesDecodeResult<T> {
  return { ok: false, reason, detail };
}

/** 键集合问题：区分「缺必填」与「未知键」，两者都是显式拒绝。 */
interface KeyProblem {
  readonly reason: "missing_key" | "unknown_key";
  readonly detail: string;
}

/** 逐键确认：必填键全在、且没有未声明的键。 */
function exactKeys(
  record: Readonly<Record<string, unknown>>,
  required: readonly string[],
  optional: readonly string[] = [],
): KeyProblem | null {
  for (const key of required) {
    if (!Object.hasOwn(record, key)) return { reason: "missing_key", detail: `缺必填键 ${key}` };
  }
  for (const key of Object.keys(record)) {
    if (!required.includes(key) && !optional.includes(key)) return { reason: "unknown_key", detail: `未知键 ${key}` };
  }
  return null;
}

/** 把键集合问题转成结构化拒绝。 */
function keyRejected<T>(problem: KeyProblem): SessionFilesDecodeResult<T> {
  return rejected(problem.reason, problem.detail);
}

/** `$defs.uuid`（小写 canonical UUID；schema 的 pattern 不带 `i`）。 */
const UUID_PATTERN = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;

/** `$defs.decimalString`。 */
const DECIMAL_PATTERN = /^(0|[1-9][0-9]*)$/;

/** `$defs.sessionPageToken` / `$defs.sessionContentVersion` 共用的字符集。 */
const TOKEN_PATTERN = /^[A-Za-z0-9_-]+$/;

/** `$defs.sessionFileData`（无填充 base64url，允许空串）。 */
const BASE64URL_PATTERN = /^[A-Za-z0-9_-]*$/;

/**
 * `$defs.sessionRelativePath` 的 pattern 原文。
 *
 * 与 schema 逐字一致：拒绝控制字符、任意 `:`（盘符/ADS）、`.`/`..` 分量与任一分量为 `.git`
 * （大小写不敏感）；分隔符 `/` 与 `\` 等价；长度上限在下面单独检查。
 */
const RELATIVE_PATH_PATTERN =
  /^(?!.*[\u0000-\u001F\u007F-\u009F:])(?!.*(?:^|[/\\])(?:\.{1,2}|\.[gG][iI][tT])(?=$|[/\\]))[^\u0000-\u001F\u007F-\u009F:/\\]+(?:[\\/][^\u0000-\u001F\u007F-\u009F:/\\]+)*$/;

const MAX_RELATIVE_PATH_LENGTH = 4096;
const MAX_TOKEN_LENGTH = 256;
const MAX_VERSION_LENGTH = 128;
const MAX_FILE_DATA_LENGTH = 43691;
const MAX_PAGE_ENTRIES = 128;
const MAX_DIFF_LINES = 400;
const MAX_DIFF_LINE_LENGTH = 4096;

const FILE_ENTRY_KINDS: Readonly<Record<string, SessionFileEntryKind>> = {
  file: "file",
  directory: "directory",
  link: "link",
  other: "other",
};

const RESTRICTED_REASONS: Readonly<Record<string, SessionFileRestrictedReason>> = {
  outside_root: "outside_root",
  unreadable: "unreadable",
  unsupported: "unsupported",
};

const GIT_STATES: Readonly<Record<string, GitState>> = {
  available: "available",
  not_repository: "not_repository",
  unavailable: "unavailable",
};

const READ_PURPOSES: Readonly<Record<string, SessionFilesReadPurpose>> = {
  preview: "preview",
  download: "download",
};

const CONTENT_KINDS: Readonly<Record<string, SessionFileContentKind>> = {
  text: "text",
  image: "image",
  binary: "binary",
};

const GIT_FILE_STATUSES: Readonly<Record<string, GitFileStatus>> = {
  added: "added",
  modified: "modified",
  deleted: "deleted",
  renamed: "renamed",
  untracked: "untracked",
};

const GIT_DIFF_BASES: Readonly<Record<string, GitDiffBase>> = { head: "head", empty: "empty" };

const GIT_DIFF_KINDS: Readonly<Record<string, GitDiffKind>> = {
  text: "text",
  binary: "binary",
  conflict: "conflict",
};

const GIT_DIFF_UNAVAILABLE: Readonly<Record<string, GitDiffUnavailableReason>> = {
  not_repository: "not_repository",
  git_unavailable: "git_unavailable",
  too_large: "too_large",
};

function isUuid(value: unknown): value is Uuid {
  return typeof value === "string" && UUID_PATTERN.test(value);
}

function isDecimal(value: unknown): value is DecimalString {
  return typeof value === "string" && DECIMAL_PATTERN.test(value);
}

function isToken(value: unknown): value is SessionPageToken | SessionContentVersion {
  return typeof value === "string" && value.length >= 1 && value.length <= MAX_TOKEN_LENGTH && TOKEN_PATTERN.test(value);
}

function isVersion(value: unknown): value is SessionContentVersion {
  return (
    typeof value === "string" &&
    value.length >= 1 &&
    value.length <= MAX_VERSION_LENGTH &&
    TOKEN_PATTERN.test(value)
  );
}

/**
 * 值是否是根内相对路径（`$defs.sessionRelativePath`）。
 *
 * 导出给领域层归并会话改动时判断「`displayPath` 是否可证明是根内完整相对路径」——只有可靠
 * 的完整路径才能作为归并键，文件名或越界路径不得与别的同名文件合并。
 */
export function isSessionRelativePath(value: unknown): value is SessionRelativePath {
  return (
    typeof value === "string" &&
    value.length >= 1 &&
    value.length <= MAX_RELATIVE_PATH_LENGTH &&
    RELATIVE_PATH_PATTERN.test(value)
  );
}

function isFilesPath(value: unknown): value is SessionFilesPath {
  return value === "" || isSessionRelativePath(value);
}

function enumValue<T>(table: Readonly<Record<string, T>>, value: unknown): T | null {
  return typeof value === "string" ? (table[value] ?? null) : null;
}

/** 解码 `command` 消息的 `body`（四条查询之一）。 */
export function decodeSessionFilesCommandBody(value: unknown): SessionFilesDecodeResult<SessionFilesCommandBody> {
  if (!isRecord(value)) return rejected("not_an_object", "command body 不是对象");
  const keyProblem = exactKeys(value, ["requestId", "command", "sessionId", "payload"]);
  if (keyProblem !== null) return keyRejected(keyProblem);
  if (!isUuid(value["requestId"])) return rejected("field_type", "requestId 不是 canonical UUID");
  if (!isUuid(value["sessionId"])) return rejected("field_type", "sessionId 不是 canonical UUID");
  const command = value["command"];
  if (typeof command !== "string" || !SESSION_FILES_COMMANDS.includes(command as SessionFilesCommandName)) {
    return rejected("invalid_enum", `command 不在四条会话文件查询内：${String(command)}`);
  }
  const payload = value["payload"];
  switch (command as SessionFilesCommandName) {
    case "session.files.list": {
      if (!isRecord(payload)) return rejected("not_an_object", "payload 不是对象");
      const payloadProblem = exactKeys(payload, ["path", "hideIgnored"], ["after"]);
      if (payloadProblem !== null) return keyRejected(payloadProblem);
      if (!isFilesPath(payload["path"])) return rejected("invalid_path", "path 既不是空串也不是根内相对路径");
      if (typeof payload["hideIgnored"] !== "boolean") return rejected("field_type", "hideIgnored 不是布尔");
      const after = payload["after"];
      if (after !== undefined && !isToken(after)) return rejected("invalid_token", "after 不是合法页标识");
      return decoded({
        requestId: value["requestId"],
        command: "session.files.list",
        sessionId: value["sessionId"],
        payload: after === undefined
          ? { path: payload["path"], hideIgnored: payload["hideIgnored"] }
          : { path: payload["path"], hideIgnored: payload["hideIgnored"], after },
      });
    }
    case "session.files.read": {
      if (!isRecord(payload)) return rejected("not_an_object", "payload 不是对象");
      const payloadProblem = exactKeys(payload, ["path", "purpose", "offset"], ["version"]);
      if (payloadProblem !== null) return keyRejected(payloadProblem);
      if (!isSessionRelativePath(payload["path"])) return rejected("invalid_path", "path 不是根内相对路径");
      const purpose = enumValue(READ_PURPOSES, payload["purpose"]);
      if (purpose === null) return rejected("invalid_enum", "purpose 不是 preview/download");
      if (!isDecimal(payload["offset"])) return rejected("invalid_decimal", "offset 不是规范十进制字符串");
      const version = payload["version"];
      if (version !== undefined && !isVersion(version)) return rejected("invalid_token", "version 不是合法内容版本");
      return decoded({
        requestId: value["requestId"],
        command: "session.files.read",
        sessionId: value["sessionId"],
        payload: version === undefined
          ? { path: payload["path"], purpose, offset: payload["offset"] }
          : { path: payload["path"], purpose, offset: payload["offset"], version },
      });
    }
    case "session.git.status": {
      if (!isRecord(payload)) return rejected("not_an_object", "payload 不是对象");
      const payloadProblem = exactKeys(payload, [], ["after"]);
      if (payloadProblem !== null) return keyRejected(payloadProblem);
      const after = payload["after"];
      if (after !== undefined && !isToken(after)) return rejected("invalid_token", "after 不是合法页标识");
      return decoded({
        requestId: value["requestId"],
        command: "session.git.status",
        sessionId: value["sessionId"],
        payload: after === undefined ? {} : { after },
      });
    }
    case "session.git.diff": {
      if (!isRecord(payload)) return rejected("not_an_object", "payload 不是对象");
      const payloadProblem = exactKeys(payload, ["path"]);
      if (payloadProblem !== null) return keyRejected(payloadProblem);
      if (!isSessionRelativePath(payload["path"])) return rejected("invalid_path", "path 不是根内相对路径");
      return decoded({
        requestId: value["requestId"],
        command: "session.git.diff",
        sessionId: value["sessionId"],
        payload: { path: payload["path"] },
      });
    }
  }
}

function decodeFileEntry(value: unknown): SessionFilesDecodeResult<SessionFileEntry> {
  if (!isRecord(value)) return rejected("not_an_object", "file entry 不是对象");
  const keyProblem = exactKeys(value, ["path", "name", "kind", "openable"], ["restricted"]);
  if (keyProblem !== null) return keyRejected(keyProblem);
  if (!isSessionRelativePath(value["path"])) return rejected("invalid_path", "条目 path 不是根内相对路径");
  const name = value["name"];
  if (typeof name !== "string" || name.length < 1 || name.length > 256) {
    return rejected("field_type", "条目 name 不是 1..256 字符的字符串");
  }
  const kind = enumValue(FILE_ENTRY_KINDS, value["kind"]);
  if (kind === null) return rejected("invalid_enum", "条目 kind 不在封闭词表内");
  if (typeof value["openable"] !== "boolean") return rejected("field_type", "条目 openable 不是布尔");
  const restricted = value["restricted"];
  if (restricted === undefined) {
    return decoded({ path: value["path"], name, kind, openable: value["openable"] });
  }
  const reason = enumValue(RESTRICTED_REASONS, restricted);
  if (reason === null) return rejected("invalid_enum", "条目 restricted 不在封闭词表内");
  return decoded({ path: value["path"], name, kind, openable: value["openable"], restricted: reason });
}

function decodeGitStatusEntry(value: unknown): SessionFilesDecodeResult<GitStatusEntry> {
  if (!isRecord(value)) return rejected("not_an_object", "git entry 不是对象");
  const keyProblem = exactKeys(value, ["path", "status", "conflicted", "binary"], ["previousPath", "addedLines", "deletedLines"]);
  if (keyProblem !== null) return keyRejected(keyProblem);
  if (!isSessionRelativePath(value["path"])) return rejected("invalid_path", "git 条目 path 不是根内相对路径");
  const status = enumValue(GIT_FILE_STATUSES, value["status"]);
  if (status === null) return rejected("invalid_enum", "git 条目 status 不在封闭词表内");
  if (typeof value["conflicted"] !== "boolean") return rejected("field_type", "conflicted 不是布尔");
  if (typeof value["binary"] !== "boolean") return rejected("field_type", "binary 不是布尔");
  const addedLines = value["addedLines"];
  const deletedLines = value["deletedLines"];
  const validCount = (n: unknown): n is number => typeof n === "number" && Number.isInteger(n) && n >= 0 && n <= 4294967295;
  if ((addedLines !== undefined || deletedLines !== undefined) &&
      (!validCount(addedLines) || !validCount(deletedLines) || value["binary"] || value["conflicted"])) {
    return rejected("field_type", "invalid Git line counts");
  }
  const counts = addedLines === undefined ? {} : { addedLines: addedLines as number, deletedLines: deletedLines as number };
  const previousPath = value["previousPath"];
  if (previousPath === undefined) {
    return decoded({ ...counts, path: value["path"], status, conflicted: value["conflicted"], binary: value["binary"] });
  }
  if (!isSessionRelativePath(previousPath)) return rejected("invalid_path", "previousPath 不是根内相对路径");
  return decoded({
    path: value["path"],
    status,
    previousPath,
    ...counts,
    conflicted: value["conflicted"],
    binary: value["binary"],
  });
}

function decodeListResult(value: unknown): SessionFilesDecodeResult<SessionFilesListResult> {
  if (!isRecord(value)) return rejected("not_an_object", "list result 不是对象");
  const keyProblem = exactKeys(value, ["path", "gitState", "entries"], ["next"]);
  if (keyProblem !== null) return keyRejected(keyProblem);
  if (!isFilesPath(value["path"])) return rejected("invalid_path", "结果 path 非法");
  const gitState = enumValue(GIT_STATES, value["gitState"]);
  if (gitState === null) return rejected("invalid_enum", "gitState 不在封闭词表内");
  const rawEntries = value["entries"];
  if (!Array.isArray(rawEntries)) return rejected("field_type", "entries 不是数组");
  if (rawEntries.length > MAX_PAGE_ENTRIES) return rejected("field_type", `entries 超过 ${MAX_PAGE_ENTRIES} 项上限`);
  const entries: SessionFileEntry[] = [];
  for (const item of rawEntries) {
    const entry = decodeFileEntry(item);
    if (!entry.ok) return entry;
    entries.push(entry.value);
  }
  const next = value["next"];
  if (next !== undefined && !isToken(next)) return rejected("invalid_token", "next 不是合法页标识");
  return decoded(next === undefined
    ? { path: value["path"], gitState, entries }
    : { path: value["path"], gitState, entries, next });
}

function decodeReadResult(value: unknown): SessionFilesDecodeResult<SessionFilesReadResult> {
  if (!isRecord(value)) return rejected("not_an_object", "read result 不是对象");
  const keyProblem = exactKeys(value, [
    "path",
    "kind",
    "sizeBytes",
    "version",
    "offset",
    "dataBase64",
    "eof",
    "previewTruncated",
  ]);
  if (keyProblem !== null) return keyRejected(keyProblem);
  if (!isSessionRelativePath(value["path"])) return rejected("invalid_path", "结果 path 非法");
  const kind = enumValue(CONTENT_KINDS, value["kind"]);
  if (kind === null) return rejected("invalid_enum", "kind 不在 text/image/binary 内");
  if (!isDecimal(value["sizeBytes"])) return rejected("invalid_decimal", "sizeBytes 不是规范十进制字符串");
  if (!isVersion(value["version"])) return rejected("invalid_token", "version 不是合法内容版本");
  if (!isDecimal(value["offset"])) return rejected("invalid_decimal", "offset 不是规范十进制字符串");
  const data = value["dataBase64"];
  if (typeof data !== "string" || data.length > MAX_FILE_DATA_LENGTH || !BASE64URL_PATTERN.test(data)) {
    return rejected("invalid_base64", "dataBase64 不是有界的无填充 base64url");
  }
  if (typeof value["eof"] !== "boolean") return rejected("field_type", "eof 不是布尔");
  if (typeof value["previewTruncated"] !== "boolean") return rejected("field_type", "previewTruncated 不是布尔");
  return decoded({
    path: value["path"],
    kind,
    sizeBytes: value["sizeBytes"],
    version: value["version"],
    offset: value["offset"],
    dataBase64: data,
    eof: value["eof"],
    previewTruncated: value["previewTruncated"],
  });
}

function decodeGitStatusResult(value: unknown): SessionFilesDecodeResult<SessionGitStatusResult> {
  if (!isRecord(value)) return rejected("not_an_object", "git status result 不是对象");
  const keyProblem = exactKeys(value, ["gitState", "entries"], ["next"]);
  if (keyProblem !== null) return keyRejected(keyProblem);
  const gitState = enumValue(GIT_STATES, value["gitState"]);
  if (gitState === null) return rejected("invalid_enum", "gitState 不在封闭词表内");
  const rawEntries = value["entries"];
  if (!Array.isArray(rawEntries)) return rejected("field_type", "entries 不是数组");
  if (rawEntries.length > MAX_PAGE_ENTRIES) return rejected("field_type", `entries 超过 ${MAX_PAGE_ENTRIES} 项上限`);
  const entries: GitStatusEntry[] = [];
  for (const item of rawEntries) {
    const entry = decodeGitStatusEntry(item);
    if (!entry.ok) return entry;
    entries.push(entry.value);
  }
  const next = value["next"];
  if (next !== undefined && !isToken(next)) return rejected("invalid_token", "next 不是合法页标识");
  return decoded(next === undefined ? { gitState, entries } : { gitState, entries, next });
}

function decodeGitDiffResult(value: unknown): SessionFilesDecodeResult<SessionGitDiffResult> {
  if (!isRecord(value)) return rejected("not_an_object", "git diff result 不是对象");
  const keyProblem = exactKeys(value, ["path", "base", "kind", "truncated"], ["diffLines", "unavailable", "comparison"]);
  if (keyProblem !== null) return keyRejected(keyProblem);
  if (!isSessionRelativePath(value["path"])) return rejected("invalid_path", "结果 path 非法");
  const base = enumValue(GIT_DIFF_BASES, value["base"]);
  if (base === null) return rejected("invalid_enum", "base 不在 head/empty 内");
  const kind = enumValue(GIT_DIFF_KINDS, value["kind"]);
  if (kind === null) return rejected("invalid_enum", "kind 不在 text/binary/conflict 内");
  if (typeof value["truncated"] !== "boolean") return rejected("field_type", "truncated 不是布尔");
  const rawLines = value["diffLines"];
  const rawUnavailable = value["unavailable"];
  if (rawLines !== undefined && rawUnavailable !== undefined) {
    return rejected("mutually_exclusive", "diffLines 与 unavailable 不得同时出现");
  }
  let diffLines: readonly string[] | undefined;
  if (rawLines !== undefined) {
    if (!Array.isArray(rawLines)) return rejected("field_type", "diffLines 不是数组");
    if (rawLines.length > MAX_DIFF_LINES) return rejected("field_type", `diffLines 超过 ${MAX_DIFF_LINES} 行上限`);
    const lines: string[] = [];
    for (const line of rawLines) {
      if (typeof line !== "string") return rejected("field_type", "diffLines 元素不是字符串");
      if (line.length > MAX_DIFF_LINE_LENGTH) return rejected("field_type", "diffLines 单行超过 4096 字符");
      lines.push(line);
    }
    diffLines = lines;
  }
  let unavailable: GitDiffUnavailableReason | undefined;
  if (rawUnavailable !== undefined) {
    const reason = enumValue(GIT_DIFF_UNAVAILABLE, rawUnavailable);
    if (reason === null) return rejected("invalid_enum", "unavailable 不在封闭词表内");
    unavailable = reason;
  }
  const comparison = value["comparison"];
  if (comparison !== undefined && (!isRecord(comparison) || exactKeys(comparison,["before","after"],[]) !== null ||
      typeof comparison["before"] !== "string" || typeof comparison["after"] !== "string" ||
      new TextEncoder().encode(comparison["before"]).length > 65536 || new TextEncoder().encode(comparison["after"]).length > 65536 || kind !== "text" || unavailable !== undefined)) {
    return rejected("field_type", "invalid complete Git comparison");
  }
  return decoded({
    path: value["path"],
    base,
    kind,
    truncated: value["truncated"],
    ...(comparison === undefined ? {} : {comparison: comparison as {before:string;after:string}}),
    ...(diffLines === undefined ? {} : { diffLines }),
    ...(unavailable === undefined ? {} : { unavailable }),
  });
}

/** 按命令名解码 `command.result.body.result`（重载让调用方拿到具体结果类型）。 */
export function decodeSessionFilesResult(
  command: "session.files.list",
  value: unknown,
): SessionFilesDecodeResult<SessionFilesListResult>;
export function decodeSessionFilesResult(
  command: "session.files.read",
  value: unknown,
): SessionFilesDecodeResult<SessionFilesReadResult>;
export function decodeSessionFilesResult(
  command: "session.git.status",
  value: unknown,
): SessionFilesDecodeResult<SessionGitStatusResult>;
export function decodeSessionFilesResult(
  command: "session.git.diff",
  value: unknown,
): SessionFilesDecodeResult<SessionGitDiffResult>;
export function decodeSessionFilesResult(
  command: SessionFilesCommandName,
  value: unknown,
): SessionFilesDecodeResult<SessionFilesResult>;
export function decodeSessionFilesResult(
  command: SessionFilesCommandName,
  value: unknown,
): SessionFilesDecodeResult<SessionFilesResult> {
  switch (command) {
    case "session.files.list":
      return decodeListResult(value);
    case "session.files.read":
      return decodeReadResult(value);
    case "session.git.status":
      return decodeGitStatusResult(value);
    case "session.git.diff":
      return decodeGitDiffResult(value);
  }
}

/** 命令名是否是四条会话文件查询之一（用于从 `CommandResultMessage` 收窄）。 */
export function isSessionFilesCommandName(value: string): value is SessionFilesCommandName {
  return SESSION_FILES_COMMANDS.includes(value as SessionFilesCommandName);
}
