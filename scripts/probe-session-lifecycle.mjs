#!/usr/bin/env node
/**
 * 真实 Codex ACP 会话生命周期探针（WP0）。
 *
 * 目的：在**隔离**的数据目录里，用真实适配器验证四件交付前必须成立的事：
 *   1. recall    同一 Agent 会话内约定随机标记，close 后对同一 sessionId resume，
 *                仍能回答标记、未新建替代会话、未重发旧 prompt、未重复工具副作用；
 *   2. restart   终止适配器进程树后，用同一 codex-home 起新进程 resume 原 sessionId，
 *                仍能回答标记，且新进程没有 session/new；
 *   3. isolation 同一进程内 A、B 两个会话，关闭 A 后 B 仍可正常应答，共享进程未终止；
 *   4. release   会话建立后、A close 后、进程退出后的进程树进程数与 WorkingSet 采样。
 *
 * 安全边界：
 *   - data-dir 必须是独立目录（拒绝仓库根、用户家目录、已存在的 .codex 目录）；
 *   - codex-home 默认在 data-dir 下，只从真实 CODEX_HOME 复制 auth.json 与 config.toml，
 *     **从不读取/打印/记录** 这些文件的内容；
 *   - transcript 只记录 ACP 协议消息；权限应答整体脱敏，只留 requestId；
 *   - **入站消息写转录前脱敏**：认证类通知（如 `_auth/status_update`）的 `account`、以及
 *     任何 `token`/`authorization`/`apiKey`/`email` 等凭据/PII 字段一律替换为 "[redacted]"，
 *     字符串中的邮箱/JWT/Bearer 凭据同理；认证状态本身不进入报告。
 *   - 默认结束时删除隔离 codex-home（内含凭据副本与 Agent 会话状态）；`--keep` 保留。
 *
 * 退出码：0 = 无 FAIL（允许 SKIP）；1 = 至少一个 FAIL；2 = 参数/环境/协议致命错误；130 = 中断。
 *
 * 本文件只用 Node 内置模块，不依赖仓库 package.json。
 */

import { spawn, spawnSync, execFileSync } from "node:child_process";
import { createHash, randomBytes } from "node:crypto";
import {
  copyFileSync,
  createWriteStream,
  existsSync,
  mkdirSync,
  readFileSync,
  rmSync,
  statSync,
  unlinkSync,
  writeFileSync,
} from "node:fs";
import os from "node:os";
import { createInterface } from "node:readline";
import { basename, dirname, extname, isAbsolute, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const TOOL_NAME = "probe-session-lifecycle";
const TOOL_VERSION = "1.0.0";
const KNOWN_ADAPTER_PATH = "D:/tmp/wp7-agents/tools/node_modules/@agentclientprotocol/codex-acp/dist/index.js";
const PROTOCOL_VERSION = 1;
const DEFAULT_TIMEOUT_MS = 180_000;
// 副作用轮需要 Agent 选择编辑/写文件工具并落盘，实测明显慢于普通问答；
// 单独给一个更宽的超时，避免把“慢”误判成“recall 失败”，同时不影响其他轮次。
const DEFAULT_SIDE_EFFECT_TIMEOUT_MS = 300_000;
const WINDOWS = process.platform === "win32";
const SCENARIOS = ["recall", "restart", "isolation", "release"];

const HELP_TEXT = `用法：node scripts/probe-session-lifecycle.mjs [选项]

在隔离的 data-dir 与 codex-home 中启动真实 codex-acp 适配器，验证会话恢复、
关闭隔离与进程树释放。默认执行全部四个场景。

选项：
  --adapter <path>       codex-acp 的 dist/index.js（或 codex-acp 可执行文件）。
                         缺省依次取环境变量 ACPR_CODEX_ACP_ADAPTER、PATH 中的
                         codex-acp、已知路径 ${KNOWN_ADAPTER_PATH}。
  --data-dir <path>      隔离数据目录。缺省在系统临时目录下创建
                         acpr-session-lifecycle-<random>；拒绝仓库根、用户家目录、
                         已存在的 .codex 目录以及仓库内的路径。
  --codex-home <path>    隔离 Codex home。缺省 <data-dir>/codex-home。会从真实
                         CODEX_HOME（缺省 %USERPROFILE%/.codex 或 $HOME/.codex）
                         复制 auth.json 与 config.toml（若存在），只记录文件名，
                         不读取或输出内容。
  --scenario <name>      recall | restart | isolation | release | all（默认 all）。
                         restart 需要同一 --data-dir 中已完成的 recall（close+resume），
                         或与 recall 一起在 all 中运行；跨次运行 restart 须在 recall
                         时使用 --keep，否则隔离 codex-home 已被清理。
  --json-out <path>      报告 JSON 路径。缺省 <data-dir>/report.json。
  --timeout-ms <n>       单个 ACP 请求超时（毫秒）。缺省 ${DEFAULT_TIMEOUT_MS}。
  --side-effect-timeout-ms <n>
                         仅作用于 recall 的副作用轮 session/prompt（要求 Agent 用编辑/写文件
                         工具直接创建 side-effect.txt）。缺省 ${DEFAULT_SIDE_EFFECT_TIMEOUT_MS}；
                         其他轮次仍使用 --timeout-ms。
  --keep                 保留隔离 codex-home 与全部临时产物（用于跨次 restart 或排查）。
  --self-test            只对脱敏逻辑做内置断言（构造假 _auth/status_update 等消息），
                         不创建目录、不启动子进程；全部通过退出 0，否则退出 1。
  --help, -h             只打印本说明并退出 0；不创建目录、不启动子进程。

运行产物：
  <data-dir>/transcript.ndjson   入站/出站 ACP 消息（入站写盘前脱敏：权限应答、
                                _auth/status_update 的账号、token/authorization/apiKey/email 等）。
  <data-dir>/adapter-*.stderr.log 适配器 stderr（只落盘，不回显、不入报告）。
  <data-dir>/state.json          跨阶段/跨次运行所需的随机标记与会话身份（不含凭据）。
  <json-out>                     结构化报告（默认 <data-dir>/report.json）。

退出码：0 无 FAIL；1 存在 FAIL；2 参数/环境/协议致命错误；130 中断。
默认在结束时清理隔离 codex-home；报告、转录与 state.json 保留。`;

/**
 * 用法/参数错误：只影响参数解析，退出码 2。
 */
class ProbeUsageError extends Error {}

/**
 * 探针自身的环境/安全前置错误，退出码 2。
 */
class ProbeError extends Error {}

/**
 * 单个 ACP 请求超时。
 */
class RequestTimeoutError extends Error {
  constructor(method, requestId, timeoutMs) {
    super(`请求 ${method}（id=${requestId}）在 ${timeoutMs}ms 内没有响应`);
    this.name = "RequestTimeoutError";
    this.method = method;
    this.requestId = requestId;
    this.timeoutMs = timeoutMs;
  }
}

/**
 * Agent 返回的 JSON-RPC 错误。
 */
class RequestError extends Error {
  constructor(method, requestId, error) {
    const code = error && typeof error === "object" ? error.code : undefined;
    const message = error && typeof error === "object" ? error.message : String(error);
    super(`请求 ${method}（id=${requestId}）失败：${code ?? "?"} ${message ?? ""}`.trim());
    this.name = "RequestError";
    this.method = method;
    this.requestId = requestId;
    this.code = code;
    this.data = error && typeof error === "object" ? error.data : undefined;
  }
}

/**
 * 适配器进程在请求完成前退出。
 */
class ProcessExitError extends Error {
  constructor(method, requestId, code, signal, reason) {
    super(reason ?? `请求 ${method}（id=${requestId}）等待期间适配器退出（code=${code} signal=${signal}）`);
    this.name = "ProcessExitError";
    this.method = method;
    this.requestId = requestId;
    this.code = code;
    this.signal = signal;
  }
}

const nowIso = () => new Date().toISOString();
const delay = (ms) => new Promise((resolveDelay) => setTimeout(resolveDelay, ms));

function sha256(value) {
  return createHash("sha256").update(value).digest("hex");
}

function truncate(value, max) {
  const text = typeof value === "string" ? value : JSON.stringify(value);
  if (text === undefined) return "";
  return text.length > max ? `${text.slice(0, max)}…` : text;
}

const REDACTED = "[redacted]";

/**
 * 键名命中即整体替换的凭据/PII 字段。大小写不敏感，支持 token/apiKey/api_key 等写法。
 * `email` 既可能是字符串也可能是对象，整体替换即可，不保留任何原值片段。
 */
const SENSITIVE_KEY_RE =
  /^(token|access[_-]?token|refresh[_-]?token|id[_-]?token|session[_-]?token|authorization|proxy[_-]?authorization|auth|api[_-]?key|apikey|email|e[_-]?mail|password|passwd|secret|client[_-]?secret|credential|credentials|cookie|set[_-]?cookie)$/i;

/** 值层面的邮箱与会话凭据形状，用于键名不敏感的字段（例如 authStatus.message 里的提示语）。 */
const EMAIL_PATTERN = /[A-Za-z0-9._%+'-]+@[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)+/g;
const JWT_PATTERN = /\beyJ[A-Za-z0-9_-]{5,}\.[A-Za-z0-9_-]{5,}\.[A-Za-z0-9_-]{5,}\b/g;
const BEARER_PATTERN = /\bBearer\s+[A-Za-z0-9._~+/-]{12,}=*/gi;

/**
 * 认证类通知：适配器在 initialize 之后会推送 `_auth/status_update`，
 * params.authStatus.account 里含登录账号（邮箱）。这类消息的账号字段必须整块脱敏。
 */
function isAuthMethod(method) {
  return typeof method === "string" && /(^|[^a-z])_?auth([^a-z]|$)|auth[_-]?status|authenticat/i.test(method);
}

/**
 * 值层面的脱敏：邮箱、JWT、Bearer 凭据。用于 transcript 与报告摘要里的任意字符串。
 */
function redactSensitiveText(text, onHit = null) {
  let output = text.replace(EMAIL_PATTERN, () => {
    if (onHit) onHit();
    return REDACTED;
  });
  output = output.replace(JWT_PATTERN, () => {
    if (onHit) onHit();
    return REDACTED;
  });
  output = output.replace(BEARER_PATTERN, () => {
    if (onHit) onHit();
    return REDACTED;
  });
  return output;
}

/**
 * 对**入站** ACP 消息做脱敏后才写入 transcript。
 *
 * 规则：
 *  - 键名命中 SENSITIVE_KEY_RE（token/authorization/apiKey/email/...）→ 值整体替换为 "[redacted]"；
 *  - 认证类方法的 `account` 字段整体替换（保留 authStatus 的其余结构，便于判断是否发生了状态变更）；
 *  - 任何字符串里的邮箱、JWT、Bearer 凭据按值替换。
 * 返回新对象，不修改原消息（原消息继续用于协议处理与检查）。
 */
function redactForTranscript(message) {
  const stats = { fields: 0, values: 0, authAccounts: 0 };
  const authContext = isAuthMethod(message?.method);
  const countValueHit = () => {
    stats.values += 1;
  };

  const walk = (value, key) => {
    if (key !== null) {
      if (SENSITIVE_KEY_RE.test(key)) {
        stats.fields += 1;
        return REDACTED;
      }
      if (authContext && key === "account") {
        stats.authAccounts += 1;
        return REDACTED;
      }
    }
    if (typeof value === "string") return redactSensitiveText(value, countValueHit);
    if (Array.isArray(value)) return value.map((item) => walk(item, key));
    if (value && typeof value === "object") {
      const output = {};
      for (const [childKey, childValue] of Object.entries(value)) {
        output[childKey] = walk(childValue, childKey);
      }
      return output;
    }
    return value;
  };

  const value = walk(message, null);
  const applied = stats.fields + stats.values + stats.authAccounts > 0;
  return { value, applied, stats };
}

/**
 * 入站消息要写进 transcript 的那一条记录（已脱敏）。onLine 只负责落盘。
 */
function inboundTranscriptEntry(message) {
  const redaction = redactForTranscript(message);
  if (!redaction.applied) {
    return { redacted: false, entry: { direction: "in", message } };
  }
  return {
    redacted: true,
    entry: {
      direction: "in",
      redacted: true,
      redaction: redaction.stats,
      method: typeof message?.method === "string" ? message.method : null,
      message: redaction.value,
    },
  };
}

/**
 * 内置自检（--self-test）：只验证脱敏逻辑本身，不创建目录、不启动适配器。
 * 断言对象是“写进 transcript 的那一行 JSON”，因为那才是证据里真实会出现的字节。
 */
function runRedactionSelfTest() {
  const results = [];
  const assert = (name, condition, detail = null) => {
    results.push({ name, ok: Boolean(condition), detail });
  };
  const transcriptLine = (message) => JSON.stringify({ ts: "self-test", runId: "self-test", direction: "in", message });

  const secretEmail = "pv0.account@example.com";
  const secretToken = "sk-live-SELFTEST0000000000000000";

  // 1. 真实形状：initialize 之后的 _auth/status_update 带账号对象。
  const authStatusMessage = {
    jsonrpc: "2.0",
    method: "_auth/status_update",
    params: {
      authStatus: {
        type: "authenticated",
        methodId: "chatgpt",
        account: { type: "chatgpt", email: secretEmail, plan: "plus" },
        token: secretToken,
      },
    },
  };
  const authStatusRedacted = redactForTranscript(authStatusMessage);
  const authStatusLine = transcriptLine(authStatusRedacted.value);
  assert("auth_status: email 未进入 transcript 行", !authStatusLine.includes(secretEmail), authStatusLine);
  assert("auth_status: token 未进入 transcript 行", !authStatusLine.includes(secretToken), authStatusLine);
  assert(
    "auth_status: account 被替换为 [redacted]",
    authStatusRedacted.value.params.authStatus.account === REDACTED,
    authStatusRedacted.value.params.authStatus.account,
  );
  assert(
    "auth_status: 保留非敏感状态字段（methodId/type）",
    authStatusRedacted.value.params.authStatus.methodId === "chatgpt" &&
      authStatusRedacted.value.params.authStatus.type === "authenticated",
    authStatusRedacted.value.params.authStatus,
  );
  assert("auth_status: applied=true", authStatusRedacted.applied === true, authStatusRedacted.stats);
  assert("auth_status: 原消息未被就地修改", authStatusMessage.params.authStatus.account.email === secretEmail);

  // 写盘用的记录本身（inboundTranscriptEntry）也不得含账号。
  const authStatusEntry = inboundTranscriptEntry(authStatusMessage);
  const authStatusEntryLine = JSON.stringify(authStatusEntry.entry);
  assert(
    "auth_status: transcript 记录标记 redacted=true",
    authStatusEntry.redacted === true && authStatusEntry.entry.redacted === true,
    authStatusEntry.entry,
  );
  assert(
    "auth_status: transcript 记录不含 email/token",
    !authStatusEntryLine.includes(secretEmail) && !authStatusEntryLine.includes(secretToken),
    authStatusEntryLine,
  );

  // 2. account 直接是邮箱字符串。
  const bareAccount = redactForTranscript({
    jsonrpc: "2.0",
    method: "_auth/status_update",
    params: { authStatus: { type: "authenticated", account: secretEmail } },
  });
  assert(
    "auth_status: 字符串 account 也不出现在 transcript 行",
    !transcriptLine(bareAccount.value).includes(secretEmail),
    bareAccount.value.params.authStatus,
  );

  // 3. 任意消息里的凭据键（大小写/下划线变体）与自由文本邮箱。
  const credentialMessage = {
    jsonrpc: "2.0",
    id: 7,
    result: {
      apiKey: "api-key-SELFTEST-value",
      authorization: "Bearer SELFTEST-BEARER-000111222333",
      nested: { deeper: { TOKEN: "raw-token-SELFTEST" } },
      profile: { email: secretEmail },
      note: `登录账号 ${secretEmail} 已生效`,
      header: "Authorization: Bearer SELFTEST-BEARER-000111222333",
    },
  };
  const credentialRedacted = redactForTranscript(credentialMessage);
  const credentialLine = transcriptLine(credentialRedacted.value);
  for (const leak of ["api-key-SELFTEST-value", "raw-token-SELFTEST", "SELFTEST-BEARER-000111222333", secretEmail]) {
    assert(`generic: ${leak} 未进入 transcript 行`, !credentialLine.includes(leak), credentialLine);
  }
  assert(
    "generic: email 键替换为 [redacted] 且其余键保留",
    credentialRedacted.value.result.profile.email === REDACTED && credentialRedacted.value.id === 7,
    credentialRedacted.value.result,
  );

  // 4. 普通消息不被改写（脱敏不能吃掉复查所需的证据文本）。
  const plainMessage = {
    jsonrpc: "2.0",
    method: "session/update",
    params: { sessionId: "sess-1", update: { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "acpr-marker-abc123" } } },
  };
  const plainRedacted = redactForTranscript(plainMessage);
  const plainEntry = inboundTranscriptEntry(plainMessage);
  assert(
    "plain: 普通消息内容保持不变",
    JSON.stringify(plainRedacted.value) === JSON.stringify(plainMessage) && plainRedacted.applied === false,
    plainRedacted.stats,
  );
  assert(
    "plain: 未触发脱敏时不改写 transcript 记录",
    plainEntry.redacted === false &&
      plainEntry.entry.redacted === undefined &&
      JSON.stringify(plainEntry.entry.message) === JSON.stringify(plainMessage),
    plainEntry.entry,
  );

  // 5. 认证类 session/update 的 kind 检测（报告不得记录 auth 状态）。
  assert("auth kind: 识别 _auth/status_update", isAuthMethod("_auth/status_update") === true);
  assert("auth kind: 不误判 session/update", isAuthMethod("session/update") === false);
  assert("auth kind: 不误判 initialize", isAuthMethod("initialize") === false);

  const failed = results.filter((result) => !result.ok);
  for (const result of results) {
    process.stdout.write(`${result.ok ? "PASS" : "FAIL"} ${result.name}\n`);
    if (!result.ok && result.detail !== null) process.stdout.write(`     实际：${truncate(result.detail, 400)}\n`);
  }
  process.stdout.write(`自检：${results.length - failed.length}/${results.length} 通过\n`);
  return failed.length === 0 ? 0 : 1;
}

function normPath(value) {
  const resolved = resolve(value);
  return WINDOWS ? resolved.toLowerCase() : resolved;
}

function samePath(left, right) {
  return normPath(left) === normPath(right);
}

/** child 是否位于 parent 内（不含 parent 自身）。 */
function isInside(parent, child) {
  const rel = relative(normPath(parent), normPath(child));
  return rel !== "" && !rel.startsWith("..") && !isAbsolute(rel);
}

function isAlive(pid) {
  if (!pid) return false;
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    return error?.code === "EPERM";
  }
}

function shortError(error) {
  if (error instanceof RequestError) {
    return `${error.method} JSON-RPC ${error.code ?? "?"}：${truncate(error.message, 200)}`;
  }
  if (error instanceof RequestTimeoutError) return error.message;
  if (error instanceof ProcessExitError) return error.message;
  return truncate(error?.message ?? String(error), 300);
}

function describeError(error) {
  const result = {
    name: error?.name ?? "Error",
    message: truncate(error?.message ?? String(error), 500),
  };
  if (error instanceof RequestError) {
    result.method = error.method;
    result.requestId = error.requestId;
    result.code = error.code;
    if (error.data !== undefined) result.data = truncate(JSON.stringify(error.data), 500);
  }
  if (error instanceof RequestTimeoutError) {
    result.method = error.method;
    result.requestId = error.requestId;
    result.timeoutMs = error.timeoutMs;
  }
  return result;
}

function parseArgs(argv) {
  const options = {
    adapter: null,
    dataDir: null,
    codexHome: null,
    scenario: "all",
    jsonOut: null,
    timeoutMs: DEFAULT_TIMEOUT_MS,
    sideEffectTimeoutMs: DEFAULT_SIDE_EFFECT_TIMEOUT_MS,
    selfTest: false,
    keep: false,
  };

  for (let index = 0; index < argv.length; index += 1) {
    const raw = argv[index];
    let name = raw;
    let inline = null;
    if (raw.startsWith("--")) {
      const eq = raw.indexOf("=");
      if (eq !== -1) {
        name = raw.slice(0, eq);
        inline = raw.slice(eq + 1);
      }
    }
    const takeValue = () => {
      if (inline !== null) return inline;
      index += 1;
      if (index >= argv.length) throw new ProbeUsageError(`参数 ${name} 缺少取值`);
      return argv[index];
    };

    switch (name) {
      case "--adapter":
        options.adapter = takeValue();
        break;
      case "--data-dir":
        options.dataDir = takeValue();
        break;
      case "--codex-home":
        options.codexHome = takeValue();
        break;
      case "--scenario":
        options.scenario = takeValue();
        break;
      case "--json-out":
        options.jsonOut = takeValue();
        break;
      case "--timeout-ms": {
        const rawValue = takeValue();
        const parsed = Number(rawValue);
        if (!Number.isInteger(parsed) || parsed <= 0) {
          throw new ProbeUsageError(`--timeout-ms 必须是正整数，收到 ${rawValue}`);
        }
        options.timeoutMs = parsed;
        break;
      }
      case "--side-effect-timeout-ms": {
        const rawValue = takeValue();
        const parsed = Number(rawValue);
        if (!Number.isInteger(parsed) || parsed <= 0) {
          throw new ProbeUsageError(`--side-effect-timeout-ms 必须是正整数，收到 ${rawValue}`);
        }
        options.sideEffectTimeoutMs = parsed;
        break;
      }
      case "--self-test":
        if (inline !== null) throw new ProbeUsageError("--self-test 不接受取值");
        options.selfTest = true;
        break;
      case "--keep":
        if (inline !== null) throw new ProbeUsageError("--keep 不接受取值");
        options.keep = true;
        break;
      case "--help":
      case "-h":
        // --help 在 main 的最前面处理；这里保留以防单独调用 parseArgs。
        options.help = true;
        break;
      default:
        throw new ProbeUsageError(`未知参数 ${raw}`);
    }
  }

  if (!["all", ...SCENARIOS].includes(options.scenario)) {
    throw new ProbeUsageError(`--scenario 只能是 ${["all", ...SCENARIOS].join(" | ")}，收到 ${options.scenario}`);
  }
  return options;
}

function resolveRequiredFile(candidate, source) {
  const absolute = resolve(candidate);
  if (!existsSync(absolute)) {
    throw new ProbeError(`${source} 指向的适配器不存在：${absolute}`);
  }
  if (!statSync(absolute).isFile()) {
    throw new ProbeError(`${source} 指向的不是文件：${absolute}`);
  }
  return { path: absolute, source };
}

function findOnPath(commandName) {
  const pathValue = process.env.PATH ?? "";
  const dirs = pathValue.split(WINDOWS ? ";" : ":").filter(Boolean);
  const extensions = WINDOWS
    ? ["", ...(process.env.PATHEXT ?? ".COM;.EXE;.BAT;.CMD").split(";").filter(Boolean)]
    : [""];
  for (const dir of dirs) {
    for (const extension of extensions) {
      const candidate = join(dir, `${commandName}${extension}`);
      try {
        if (existsSync(candidate) && statSync(candidate).isFile()) return candidate;
      } catch {
        // 忽略不可访问的 PATH 项。
      }
    }
  }
  return null;
}

function resolveAdapter(explicit) {
  if (explicit) return resolveRequiredFile(explicit, "--adapter");
  if (process.env.ACPR_CODEX_ACP_ADAPTER) {
    return resolveRequiredFile(process.env.ACPR_CODEX_ACP_ADAPTER, "ACPR_CODEX_ACP_ADAPTER");
  }
  const onPath = findOnPath("codex-acp");
  if (onPath) return { path: resolve(onPath), source: "PATH" };
  if (existsSync(KNOWN_ADAPTER_PATH)) return { path: resolve(KNOWN_ADAPTER_PATH), source: "known-path" };
  throw new ProbeError(
    "找不到 codex-acp 适配器。已尝试：--adapter、ACPR_CODEX_ACP_ADAPTER、PATH 中的 codex-acp、" +
      `已知路径 ${KNOWN_ADAPTER_PATH}。请显式传入 --adapter <dist/index.js>。`,
  );
}

function buildLaunch(adapterPath) {
  const extension = extname(adapterPath).toLowerCase();
  if ([".js", ".mjs", ".cjs"].includes(extension)) {
    return { command: process.execPath, args: [adapterPath], kind: "node" };
  }
  if (WINDOWS && [".cmd", ".bat"].includes(extension)) {
    return { command: process.env.ComSpec || "cmd.exe", args: ["/d", "/s", "/c", adapterPath], kind: "cmd" };
  }
  if (WINDOWS && extension === ".ps1") {
    return {
      command: "powershell.exe",
      args: ["-NoProfile", "-ExecutionPolicy", "Bypass", "-File", adapterPath],
      kind: "powershell",
    };
  }
  return { command: adapterPath, args: [], kind: "exec" };
}

function readJsonFile(path) {
  try {
    return JSON.parse(readFileSync(path, "utf8"));
  } catch {
    return null;
  }
}

function findAncestorPackageJson(startDir, predicate) {
  let dir = resolve(startDir);
  for (let level = 0; level < 8; level += 1) {
    const candidate = join(dir, "package.json");
    if (existsSync(candidate)) {
      const json = readJsonFile(candidate);
      if (json && predicate(json)) return { path: candidate, name: json.name, version: json.version ?? null };
    }
    const parent = dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }
  return null;
}

function detectAdapterVersion(adapterPath) {
  return findAncestorPackageJson(dirname(adapterPath), (json) =>
    String(json.name ?? "").startsWith("@agentclientprotocol/codex-acp"),
  );
}

function detectCodexPackageVersion(adapterPath) {
  let dir = resolve(dirname(adapterPath));
  for (let level = 0; level < 10; level += 1) {
    const candidate = join(dir, "node_modules", "@openai", "codex", "package.json");
    if (existsSync(candidate)) {
      const json = readJsonFile(candidate);
      if (json) return { path: candidate, version: json.version ?? null };
    }
    const parent = dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }
  return null;
}

function detectCodexCliVersion() {
  try {
    const result = spawnSync("codex", ["--version"], {
      encoding: "utf8",
      timeout: 10_000,
      shell: WINDOWS,
      windowsHide: true,
    });
    if (result.status === 0) {
      const line = String(result.stdout ?? "").trim().split(/\r?\n/)[0];
      return line ? truncate(line, 120) : null;
    }
  } catch {
    // codex CLI 不在 PATH 时忽略；适配器可能自带 codex。
  }
  return null;
}

function realCodexHome() {
  const configured = process.env.CODEX_HOME;
  return resolve(configured && configured.trim() ? configured : join(os.homedir(), ".codex"));
}

function assertSafeDataDir(dataDir, repoRoot, realHome) {
  if (samePath(dataDir, repoRoot)) {
    throw new ProbeError(`--data-dir 不能是仓库根：${dataDir}`);
  }
  if (samePath(dataDir, os.homedir())) {
    throw new ProbeError(`--data-dir 不能是用户家目录：${dataDir}`);
  }
  if (basename(dataDir).toLowerCase() === ".codex") {
    throw new ProbeError(`--data-dir 不能是已存在的 .codex 目录：${dataDir}`);
  }
  if (samePath(dataDir, realHome)) {
    throw new ProbeError(`--data-dir 不能是真实 CODEX_HOME：${dataDir}`);
  }
  if (isInside(repoRoot, dataDir)) {
    throw new ProbeError(`--data-dir 不能位于仓库工作树内：${dataDir}`);
  }
  if (existsSync(dataDir) && !statSync(dataDir).isDirectory()) {
    throw new ProbeError(`--data-dir 已存在且不是目录：${dataDir}`);
  }
}

function prepareDataDir(options, repoRoot, realHome) {
  const autoCreated = !options.dataDir;
  const dataDir = options.dataDir
    ? resolve(options.dataDir)
    : join(os.tmpdir(), `acpr-session-lifecycle-${randomBytes(6).toString("hex")}`);
  assertSafeDataDir(dataDir, repoRoot, realHome);
  mkdirSync(dataDir, { recursive: true });
  const workspace = join(dataDir, "workspace");
  mkdirSync(workspace, { recursive: true });
  const jsonOut = options.jsonOut ? resolve(options.jsonOut) : join(dataDir, "report.json");
  mkdirSync(dirname(jsonOut), { recursive: true });
  return {
    dataDir,
    workspace,
    transcriptPath: join(dataDir, "transcript.ndjson"),
    statePath: join(dataDir, "state.json"),
    jsonOut,
    autoCreated,
  };
}

function prepareCodexHome(options, dataDir, realHome, repoRoot) {
  const home = options.codexHome ? resolve(options.codexHome) : join(dataDir, "codex-home");
  if (samePath(home, realHome)) {
    throw new ProbeError(`--codex-home 不能是真实 CODEX_HOME：${home}`);
  }
  if (isInside(realHome, home)) {
    throw new ProbeError(`--codex-home 不能位于真实 CODEX_HOME 内：${home}`);
  }
  if (samePath(home, os.homedir())) {
    throw new ProbeError(`--codex-home 不能是用户家目录：${home}`);
  }
  if (samePath(home, dataDir)) {
    throw new ProbeError(`--codex-home 不能与 --data-dir 相同：${home}`);
  }
  if (samePath(home, join(dataDir, "workspace"))) {
    throw new ProbeError(`--codex-home 不能与会话工作目录相同：${home}`);
  }
  if (isInside(repoRoot, home)) {
    throw new ProbeError(`--codex-home 不能位于仓库工作树内：${home}`);
  }

  const existedBefore = existsSync(home);
  mkdirSync(home, { recursive: true });

  const copied = [];
  const present = [];
  const missing = [];
  for (const name of ["auth.json", "config.toml"]) {
    const source = join(realHome, name);
    const target = join(home, name);
    if (!existsSync(source)) {
      missing.push(name);
      continue;
    }
    if (existsSync(target)) {
      present.push(name);
      continue;
    }
    copyFileSync(source, target);
    copied.push(name);
  }

  return {
    home,
    realHome,
    copied,
    present,
    missing,
    created: !existedBefore,
    auto: !options.codexHome,
  };
}

function createEngine({ options, repoRoot, data, home, adapter, launch, adapterPackage, codexPackage, codexCliVersion, runId }) {
  const engine = {
    options,
    repoRoot,
    dataDir: data.dataDir,
    workspace: data.workspace,
    transcriptPath: data.transcriptPath,
    statePath: data.statePath,
    jsonOut: data.jsonOut,
    codexHome: home.home,
    codexHomeInfo: home,
    adapter,
    launch,
    adapterPackage,
    codexPackage,
    codexCliVersion,
    runId,
    startedAt: nowIso(),
    checks: [],
    ledger: [],
    resourceSamples: [],
    runs: [],
    sessionIds: {},
    timestamps: { stages: {} },
    priorPrompts: [],
    memoryMarkers: new Set(),
    sideEffectTokens: new Set(),
    capabilities: null,
    sideEffect: null,
    isolation: null,
    release: null,
    notes: [],
    connections: new Set(),
    transcriptStream: null,
    state: null,
    fatal: null,
    interrupted: false,
    checkIds: new Set(),
  };

  engine.marker = `acpr-marker-${randomBytes(12).toString("hex")}`;
  engine.sideEffectToken = `acpr-side-effect-${randomBytes(12).toString("hex")}`;
  engine.memoryMarkers.add(engine.marker);
  engine.sideEffectTokens.add(engine.sideEffectToken);

  engine.transcriptStream = createWriteStream(engine.transcriptPath, { flags: "a" });
  engine.transcriptStream.on("error", (error) => {
    engine.notes.push(`transcript 写入失败：${shortError(error)}`);
  });

  engine.progress = (stage, event, detail = {}) => {
    process.stderr.write(`${JSON.stringify({ ts: nowIso(), runId, stage, event, ...detail })}\n`);
  };

  engine.transcript = (entry) => {
    try {
      engine.transcriptStream.write(`${JSON.stringify({ ts: nowIso(), runId, ...entry })}\n`);
    } catch (error) {
      engine.notes.push(`transcript 记录失败：${shortError(error)}`);
    }
  };

  engine.transcript({
    type: "run-start",
    tool: TOOL_NAME,
    toolVersion: TOOL_VERSION,
    scenario: options.scenario,
    adapter: adapter.path,
    launchKind: launch.kind,
    dataDir: engine.dataDir,
    workspace: engine.workspace,
    codexHome: engine.codexHome,
    markerSha256: sha256(engine.marker),
    sideEffectTokenSha256: sha256(engine.sideEffectToken),
    clientCapabilities: clientCapabilities(),
  });

  return engine;
}

function clientCapabilities() {
  return {
    fs: { readTextFile: true, writeTextFile: true },
    terminal: true,
  };
}

const STAGE_CHECK_IDS = {
  environment: [
    "environment.adapter_resolved",
    "environment.adapter_version",
    "environment.isolated_data_dir",
    "environment.codex_home_isolated",
    "environment.credentials_copied",
  ],
  recall: [
    "recall.initialize_capabilities_recorded",
    "recall.resume_advertised",
    "recall.close_advertised",
    "recall.session_new",
    "recall.first_prompt_completed",
    "recall.side_effect_observed",
    "recall.marker_same_session",
    "recall.close_succeeded",
    "recall.resume_request_protocol_only",
    "recall.resume_request_succeeded",
    "recall.marker_after_resume",
    "recall.no_replacement_session",
    "recall.no_replayed_side_effect",
    "recall.no_old_prompt_resent",
    "recall.marker_sent_once",
    "recall.completed",
  ],
  restart: [
    "restart.previous_process_tree_terminated",
    "restart.initialize_capabilities_recorded",
    "restart.resume_request_protocol_only",
    "restart.resume_request_succeeded",
    "restart.no_session_new",
    "restart.marker_after_restart",
    "restart.no_replayed_side_effect",
    "restart.no_old_prompt_resent",
    "restart.completed",
  ],
  isolation: [
    "isolation.sessions_distinct",
    "isolation.b_ready_before_close",
    "isolation.a_close_succeeded",
    "isolation.b_works_after_a_close",
    "isolation.shared_process_survived",
    "isolation.resource_samples_recorded",
    "isolation.completed",
  ],
  release: [
    "release.session_established",
    "release.a_close_succeeded",
    "release.memory_after_close",
    "release.process_tree_terminated",
    "release.no_orphan_children",
    "release.completed",
  ],
};

function recordCheck(engine, stage, id, status, summary, details = {}) {
  if (!["PASS", "FAIL", "SKIP"].includes(status)) {
    throw new Error(`非法检查状态 ${status}（${id}）`);
  }
  if (engine.checkIds.has(id)) {
    throw new Error(`检查 id 重复：${id}`);
  }
  engine.checkIds.add(id);
  const check = { id, stage, status, summary, details, at: nowIso() };
  engine.checks.push(check);
  engine.progress(stage, "check", { id, status, summary });
  return check;
}

function ensureStageSkips(engine, stage, reason) {
  for (const id of STAGE_CHECK_IDS[stage] ?? []) {
    if (id === `${stage}.completed`) continue;
    if (engine.checkIds.has(id)) continue;
    recordCheck(engine, stage, id, "SKIP", reason, { reason });
  }
}

function stageChecks(engine, stage) {
  return engine.checks.filter((check) => check.stage === stage);
}

/**
 * 阶段在产生任何检查前就异常（例如适配器 spawn 失败）时，仍要留下 FAIL 结论与退出码。
 */
function recordStageFailure(engine, stage, error) {
  if (engine.checkIds.has(`${stage}.completed`)) return;
  ensureStageSkips(engine, stage, `阶段异常中断：${shortError(error)}`);
  recordCheck(engine, stage, `${stage}.completed`, "FAIL", `阶段异常：${shortError(error)}`, {
    error: describeError(error),
  });
}

function finalizeStage(engine, stage, skipSummaryIfUnverified = true) {
  const checks = stageChecks(engine, stage).filter((check) => check.id !== `${stage}.completed`);
  if (checks.some((check) => check.status === "FAIL")) {
    const failing = checks.filter((check) => check.status === "FAIL").map((check) => check.id);
    recordCheck(engine, stage, `${stage}.completed`, "FAIL", `阶段存在失败检查：${failing.join(", ")}`, {
      failing,
    });
    return "FAIL";
  }
  if (skipSummaryIfUnverified && checks.some((check) => check.status === "SKIP")) {
    const skipped = checks.filter((check) => check.status === "SKIP").map((check) => check.id);
    recordCheck(engine, stage, `${stage}.completed`, "SKIP", `阶段存在未验证检查：${skipped.join(", ")}`, {
      skipped,
    });
    return "SKIP";
  }
  recordCheck(engine, stage, `${stage}.completed`, "PASS", "阶段全部检查通过", {
    checks: checks.length,
  });
  return "PASS";
}

async function sampleProcessTree(engine, rootPid) {
  const base = {
    pid: rootPid ?? null,
    processCount: null,
    workingSetMb: null,
    rootPresent: false,
    method: "none",
    error: null,
  };
  if (!rootPid) {
    return { ...base, processCount: 0, workingSetMb: 0, method: "none" };
  }

  if (WINDOWS) {
    const script =
      "[Console]::OutputEncoding=[Text.Encoding]::UTF8; " +
      "Get-CimInstance Win32_Process | Select-Object ProcessId,ParentProcessId,WorkingSetSize,Name | ConvertTo-Json -Compress";
    let output;
    try {
      output = execFileSync("powershell.exe", ["-NoProfile", "-NonInteractive", "-Command", script], {
        encoding: "utf8",
        timeout: 20_000,
        maxBuffer: 32 * 1024 * 1024,
        windowsHide: true,
      });
    } catch (error) {
      return {
        ...base,
        rootPresent: isAlive(rootPid),
        method: "win32-cim",
        error: shortError(error),
      };
    }
    let rows = [];
    try {
      const parsed = JSON.parse(String(output).replace(/^\uFEFF/, "") || "[]");
      rows = Array.isArray(parsed) ? parsed : [parsed];
    } catch (error) {
      return {
        ...base,
        rootPresent: isAlive(rootPid),
        method: "win32-cim",
        error: `无法解析 PowerShell JSON：${shortError(error)}`,
      };
    }
    const byParent = new Map();
    const byPid = new Map();
    for (const row of rows) {
      const pid = Number(row.ProcessId);
      const ppid = Number(row.ParentProcessId);
      if (!Number.isFinite(pid)) continue;
      byPid.set(pid, row);
      if (!byParent.has(ppid)) byParent.set(ppid, []);
      byParent.get(ppid).push(pid);
    }
    const seen = new Set();
    const queue = [Number(rootPid)];
    let workingSetBytes = 0;
    const names = {};
    while (queue.length > 0) {
      const pid = queue.shift();
      if (seen.has(pid)) continue;
      seen.add(pid);
      const row = byPid.get(pid);
      if (row) {
        const bytes = Number(row.WorkingSetSize);
        if (Number.isFinite(bytes)) workingSetBytes += bytes;
        const name = String(row.Name ?? "unknown");
        names[name] = (names[name] ?? 0) + 1;
      }
      for (const child of byParent.get(pid) ?? []) queue.push(child);
    }
    return {
      ...base,
      processCount: [...seen].filter((pid) => byPid.has(pid)).length,
      workingSetMb: Math.round((workingSetBytes / 1024 / 1024) * 10) / 10,
      rootPresent: byPid.has(Number(rootPid)),
      method: "win32-cim",
      names,
    };
  }

  let output;
  try {
    output = execFileSync("ps", ["-eo", "pid=,ppid=,rss=,comm="], {
      encoding: "utf8",
      timeout: 20_000,
      maxBuffer: 16 * 1024 * 1024,
    });
  } catch (error) {
    return { ...base, rootPresent: isAlive(rootPid), method: "ps", error: shortError(error) };
  }
  const byParent = new Map();
  const byPid = new Map();
  for (const line of String(output).split(/\r?\n/)) {
    const match = line.trim().match(/^(\d+)\s+(\d+)\s+(\d+)\s+(.*)$/);
    if (!match) continue;
    const pid = Number(match[1]);
    const ppid = Number(match[2]);
    const rssKb = Number(match[3]);
    byPid.set(pid, { rssKb });
    if (!byParent.has(ppid)) byParent.set(ppid, []);
    byParent.get(ppid).push(pid);
  }
  const seen = new Set();
  const queue = [Number(rootPid)];
  let rssKb = 0;
  while (queue.length > 0) {
    const pid = queue.shift();
    if (seen.has(pid)) continue;
    seen.add(pid);
    const row = byPid.get(pid);
    if (row) rssKb += row.rssKb;
    for (const child of byParent.get(pid) ?? []) queue.push(child);
  }
  return {
    ...base,
    processCount: [...seen].filter((pid) => byPid.has(pid)).length,
    workingSetMb: Math.round((rssKb / 1024) * 10) / 10,
    rootPresent: byPid.has(Number(rootPid)),
    method: "ps",
  };
}

async function recordResourceSample(engine, stage, point, pid, runId) {
  const sample = await sampleProcessTree(engine, pid);
  const entry = {
    stage,
    point,
    runId: runId ?? null,
    sampledAt: nowIso(),
    pid: pid ?? null,
    processCount: sample.processCount,
    workingSetMb: sample.workingSetMb,
    rootPresent: sample.rootPresent,
    method: sample.method,
    names: sample.names ?? null,
    error: sample.error,
  };
  engine.resourceSamples.push(entry);
  engine.progress(stage, "resource", {
    point,
    processCount: entry.processCount,
    workingSetMb: entry.workingSetMb,
    rootPresent: entry.rootPresent,
  });
  return entry;
}

async function waitForTreeGone(engine, pid, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  let sample = await sampleProcessTree(engine, pid);
  while (Date.now() < deadline) {
    if (sample.processCount === 0 && !sample.rootPresent) return sample;
    await delay(150);
    sample = await sampleProcessTree(engine, pid);
  }
  return sample;
}

/**
 * 把进程树采样转成检查状态：采样失败时 SKIP（不伪造也没有证据的失败）。
 */
function treeTerminationStatus(tree) {
  if (!tree || tree.processCount === null) return "SKIP";
  return tree.processCount === 0 && !tree.rootPresent ? "PASS" : "FAIL";
}

function killProcessTreeSync(pid, timeoutMs = 15_000) {
  if (!pid) return;
  if (WINDOWS) {
    spawnSync("taskkill", ["/PID", String(pid), "/T", "/F"], {
      stdio: "ignore",
      timeout: timeoutMs,
      windowsHide: true,
    });
    return;
  }
  try {
    process.kill(-pid, "SIGKILL");
  } catch {
    try {
      process.kill(pid, "SIGKILL");
    } catch {
      // 进程已不存在。
    }
  }
}

async function killProcessTree(pid) {
  if (!pid) return;
  if (!WINDOWS) {
    try {
      process.kill(-pid, "SIGTERM");
    } catch {
      // 忽略：可能不是进程组或已退出。
    }
    await delay(400);
  }
  killProcessTreeSync(pid);
}

// 供同步清理（SIGINT/SIGTERM）使用的全局引擎引用。
let engineState = null;

/**
 * 一个 ACP stdio 连接：启动子进程、收发 JSON-RPC、应答 Agent 发往客户端的方法。
 */
class Connection {
  constructor(engine, label) {
    this.engine = engine;
    this.label = label;
    this.runId = randomBytes(4).toString("hex");
    this.child = null;
    this.pid = null;
    this.readline = null;
    this.pending = new Map();
    this.nextId = 1;
    this.updates = [];
    this.terminals = new Map();
    this.terminalSeq = 0;
    this.caps = null;
    this.counters = {
      toolCalls: 0,
      toolCallUpdates: 0,
      permissionRequests: 0,
      fsReads: 0,
      fsWrites: 0,
      fsDenied: 0,
      terminalCreates: 0,
      terminalDenied: 0,
      unknownClientMethods: {},
      protocolErrors: 0,
      inboundRedactions: 0,
      authNotifications: 0,
      authUpdatesSkipped: 0,
    };
    this.exitInfo = null;
    this.exited = null;
    this.expectedExit = false;
    this.terminatePromise = null;
    this.stderrBytes = 0;
    this.stderrLog = join(engine.dataDir, `adapter-${this.runId}.stderr.log`);
    this.stderrStream = createWriteStream(this.stderrLog, { flags: "a" });
    this.stderrStream.on("error", () => {});
    this.run = {
      runId: this.runId,
      label,
      pid: null,
      launch: { command: engine.launch.command, args: engine.launch.args, kind: engine.launch.kind },
      cwd: engine.workspace,
      codexHome: engine.codexHome,
      stderrLog: this.stderrLog,
      stderrBytes: 0,
      startedAt: nowIso(),
      endedAt: null,
      exitCode: null,
      signal: null,
      terminatedByProbe: false,
      terminationReason: null,
      capabilities: null,
      outboundMethods: {},
      inboundMethodCounts: {},
      counters: this.counters,
      treeAfter: null,
    };
    engine.runs.push(this.run);
  }

  async start() {
    await new Promise((resolveStart, rejectStart) => {
      const child = spawn(this.engine.launch.command, this.engine.launch.args, {
        cwd: this.engine.workspace,
        env: {
          ...process.env,
          CODEX_HOME: this.engine.codexHome,
          NO_COLOR: "1",
        },
        stdio: ["pipe", "pipe", "pipe"],
        windowsHide: true,
        detached: !WINDOWS,
      });
      this.child = child;

      const onError = (error) => {
        child.off("spawn", onSpawn);
        rejectStart(error);
      };
      const onSpawn = () => {
        child.off("error", onError);
        child.on("error", (error) => {
          this.engine.notes.push(`run ${this.runId} 子进程错误：${shortError(error)}`);
        });
        resolveStart();
      };
      child.once("error", onError);
      child.once("spawn", onSpawn);
    });

    this.pid = this.child.pid;
    this.run.pid = this.pid;
    this.engine.progress(this.label, "spawn", {
      runId: this.runId,
      pid: this.pid,
      command: this.engine.launch.command,
      args: this.engine.launch.args,
      codexHome: this.engine.codexHome,
    });

    this.child.stdout.setEncoding("utf8");
    this.readline = createInterface({ input: this.child.stdout, crlfDelay: Infinity });
    this.readline.on("line", (line) => this.onLine(line));

    this.child.stderr.on("data", (chunk) => {
      this.stderrBytes += chunk.length;
      try {
        this.stderrStream.write(chunk);
      } catch {
        // 日志写失败不应中断探针。
      }
    });

    this.exited = new Promise((resolveExit) => {
      this.child.once("exit", (code, signal) => {
        this.exitInfo = { code, signal, at: nowIso() };
        this.run.endedAt = nowIso();
        this.run.exitCode = code;
        this.run.signal = signal;
        if (!this.expectedExit) {
          this.engine.notes.push(`run ${this.runId} 在探针主动终止前退出（code=${code} signal=${signal}）`);
        }
        for (const [id, pending] of this.pending) {
          clearTimeout(pending.timer);
          pending.reject(new ProcessExitError(pending.method, id, code, signal));
        }
        this.pending.clear();
        resolveExit(this.exitInfo);
      });
    });

    this.child.stdin.on("error", (error) => {
      this.engine.notes.push(`run ${this.runId} stdin 错误：${shortError(error)}`);
    });
  }

  transcript(entry) {
    this.engine.transcript({ runId: this.runId, ...entry });
  }

  sendRaw(message) {
    if (!this.child || this.child.stdin.destroyed) {
      throw new ProbeError(`run ${this.runId} 的 stdin 已关闭，无法发送 ${message.method ?? "响应"}`);
    }
    this.child.stdin.write(`${JSON.stringify(message)}\n`);
    const method = message.method ?? (message.result !== undefined ? "response" : "error-response");
    this.run.outboundMethods[method] = (this.run.outboundMethods[method] ?? 0) + 1;
  }

  respond(id, result, { redact = false, method = null } = {}) {
    if (redact) {
      this.transcript({ direction: "out", redacted: true, method, requestId: id });
    } else {
      this.transcript({ direction: "out", message: { jsonrpc: "2.0", id, result } });
    }
    this.sendRaw({ jsonrpc: "2.0", id, result });
  }

  respondError(id, code, message, data) {
    const error = { code, message };
    if (data !== undefined) error.data = data;
    this.transcript({ direction: "out", message: { jsonrpc: "2.0", id, error } });
    this.sendRaw({ jsonrpc: "2.0", id, error });
  }

  notify(method, params) {
    this.transcript({ direction: "out", message: { jsonrpc: "2.0", method, params } });
    this.sendRaw({ jsonrpc: "2.0", method, params });
  }

  request(method, params, options = {}) {
    const id = this.nextId;
    this.nextId += 1;
    const serialized = JSON.stringify(params ?? {});
    const containsSessionMarker = [...this.engine.memoryMarkers].some((token) => serialized.includes(token));
    const containsSideEffectToken = [...this.engine.sideEffectTokens].some((token) => serialized.includes(token));
    const containsOldPrompt = containsPriorPrompt(this.engine, params);
    const entry = {
      runId: this.runId,
      processLabel: this.label,
      method,
      requestId: id,
      paramsSha256: sha256(serialized),
      paramsBytes: Buffer.byteLength(serialized),
      paramsKeys: Object.keys(params ?? {}),
      containsSessionMarker,
      containsSideEffectToken,
      containsSensitiveToken: containsSessionMarker || containsSideEffectToken,
      containsOldPrompt,
      sentAt: nowIso(),
    };
    this.engine.ledger.push(entry);

    const timeoutMs = options.timeoutMs ?? this.engine.options.timeoutMs;
    return new Promise((resolveRequest, rejectRequest) => {
      const timer = setTimeout(() => {
        if (!this.pending.has(id)) return;
        this.pending.delete(id);
        rejectRequest(new RequestTimeoutError(method, id, timeoutMs));
      }, timeoutMs);
      this.pending.set(id, { method, id, timer, resolve: resolveRequest, reject: rejectRequest });
      try {
        this.transcript({ direction: "out", message: { jsonrpc: "2.0", id, method, params } });
        this.sendRaw({ jsonrpc: "2.0", id, method, params });
      } catch (error) {
        clearTimeout(timer);
        this.pending.delete(id);
        rejectRequest(error);
      }
    });
  }

  onLine(line) {
    if (!line.trim()) return;
    let message;
    try {
      message = JSON.parse(line);
    } catch {
      this.counters.protocolErrors += 1;
      this.engine.notes.push(`run ${this.runId} 收到无法解析的入站行（${line.length} bytes）`);
      return;
    }
    // 入站消息可能携带登录账号（_auth/status_update）或凭据；写转录前先脱敏，
    // 协议处理仍使用未脱敏的原消息。
    const inbound = inboundTranscriptEntry(message);
    if (inbound.redacted) this.counters.inboundRedactions += 1;
    this.transcript(inbound.entry);

    if (message && typeof message === "object" && typeof message.method === "string") {
      if (message.id !== undefined && message.id !== null) {
        this.run.inboundMethodCounts[message.method] = (this.run.inboundMethodCounts[message.method] ?? 0) + 1;
        this.handleServerRequest(message).catch((error) => {
          this.engine.notes.push(`run ${this.runId} 处理 ${message.method} 失败：${shortError(error)}`);
          try {
            this.respondError(message.id, -32603, `probe internal error: ${shortError(error)}`);
          } catch {
            // stdin 可能已关闭。
          }
        });
      } else {
        this.handleNotification(message);
      }
      return;
    }

    if (message && typeof message === "object" && message.id !== undefined && message.id !== null) {
      const pending = this.pending.get(message.id);
      if (!pending) {
        this.engine.notes.push(`run ${this.runId} 收到未知 id=${message.id} 的响应`);
        return;
      }
      this.pending.delete(message.id);
      clearTimeout(pending.timer);
      if (message.error) pending.reject(new RequestError(pending.method, message.id, message.error));
      else pending.resolve(message.result);
    }
  }

  handleNotification(message) {
    if (message.method !== "session/update") {
      if (isAuthMethod(message.method)) {
        // 认证状态通知只计数：params 里的账号/凭据不进入报告，转录中的副本已在 onLine 脱敏。
        this.counters.authNotifications += 1;
        return;
      }
      this.engine.notes.push(`run ${this.runId} 收到未处理的通知 ${message.method}`);
      return;
    }
    const update = message.params?.update;
    if (!update || typeof update !== "object") {
      this.counters.protocolErrors += 1;
      return;
    }
    const kind = update.sessionUpdate;
    if (typeof kind === "string" && /auth/i.test(kind)) {
      // 认证类 session/update 同样只计数，摘要不落报告，避免把登录状态写进证据。
      this.counters.authUpdatesSkipped += 1;
      return;
    }
    if (kind === "tool_call") this.counters.toolCalls += 1;
    if (kind === "tool_call_update") this.counters.toolCallUpdates += 1;
    const summary = {
      seq: this.updates.length,
      at: nowIso(),
      sessionId: message.params?.sessionId ?? null,
      kind,
      toolCallId: kind === "tool_call" || kind === "tool_call_update" ? update.toolCallId ?? null : null,
      title:
        typeof update.title === "string" && update.title.length <= 200
          ? redactSensitiveText(update.title)
          : null,
    };
    if (kind === "agent_message_chunk" && update.content?.type === "text") {
      // 摘要是报告的输入之一，同样做值层脱敏，避免 Agent 文本把邮箱/凭据带进证据。
      summary.text = redactSensitiveText(update.content.text ?? "");
    }
    this.updates.push(summary);
  }

  async handleServerRequest(message) {
    const params = message.params ?? {};
    switch (message.method) {
      case "session/request_permission": {
        this.counters.permissionRequests += 1;
        const options = Array.isArray(params.options) ? params.options : [];
        const chosen =
          options.find((option) => option?.kind === "allow_once") ??
          options.find((option) => String(option?.kind ?? "").startsWith("allow")) ??
          options[0];
        const result = chosen
          ? { outcome: { outcome: "selected", optionId: chosen.optionId } }
          : { outcome: { outcome: "cancelled" } };
        // 权限应答内容不进入证据：只记录 requestId 与脱敏标记。
        this.respond(message.id, result, { redact: true, method: message.method });
        return;
      }
      case "fs/read_text_file": {
        this.counters.fsReads += 1;
        const path = params.path;
        if (typeof path !== "string" || !this.isAllowedPath(path)) {
          this.counters.fsDenied += 1;
          this.respondError(message.id, -32602, "path outside isolated probe workspace");
          return;
        }
        let content;
        try {
          content = readFileSync(path, "utf8");
        } catch (error) {
          this.respondError(message.id, -32000, `read failed: ${shortError(error)}`);
          return;
        }
        const line = Number.isInteger(params.line) ? params.line : null;
        const limit = Number.isInteger(params.limit) ? params.limit : null;
        if (line !== null || limit !== null) {
          const lines = content.split(/\r?\n/);
          const start = line !== null ? Math.max(0, line - 1) : 0;
          const end = limit !== null ? start + limit : lines.length;
          content = lines.slice(start, end).join("\n");
        }
        this.respond(message.id, { content });
        return;
      }
      case "fs/write_text_file": {
        this.counters.fsWrites += 1;
        const path = params.path;
        if (typeof path !== "string" || !this.isAllowedPath(path) || typeof params.content !== "string") {
          this.counters.fsDenied += 1;
          this.respondError(message.id, -32602, "write outside isolated probe workspace or missing content");
          return;
        }
        try {
          mkdirSync(dirname(path), { recursive: true });
          writeFileSync(path, params.content);
        } catch (error) {
          this.respondError(message.id, -32000, `write failed: ${shortError(error)}`);
          return;
        }
        this.respond(message.id, {});
        return;
      }
      case "terminal/create":
        await this.createTerminal(message);
        return;
      case "terminal/output": {
        const terminal = this.terminals.get(params.terminalId);
        if (!terminal) {
          this.respondError(message.id, -32602, "unknown terminalId");
          return;
        }
        this.respond(message.id, {
          output: terminal.output.toString("utf8"),
          truncated: terminal.truncated,
          exitStatus: terminal.exited ? { exitCode: terminal.exitCode, signal: terminal.exitSignal } : null,
        });
        return;
      }
      case "terminal/wait_for_exit": {
        const terminal = this.terminals.get(params.terminalId);
        if (!terminal) {
          this.respondError(message.id, -32602, "unknown terminalId");
          return;
        }
        await Promise.race([terminal.done, delay(this.engine.options.timeoutMs)]);
        this.respond(message.id, { exitCode: terminal.exitCode, signal: terminal.exitSignal });
        return;
      }
      case "terminal/kill": {
        const terminal = this.terminals.get(params.terminalId);
        if (!terminal) {
          this.respondError(message.id, -32602, "unknown terminalId");
          return;
        }
        await killProcessTree(terminal.child.pid);
        this.respond(message.id, {});
        return;
      }
      case "terminal/release": {
        const terminal = this.terminals.get(params.terminalId);
        if (!terminal) {
          this.respondError(message.id, -32602, "unknown terminalId");
          return;
        }
        await killProcessTree(terminal.child.pid);
        this.terminals.delete(params.terminalId);
        this.respond(message.id, {});
        return;
      }
      default: {
        this.counters.unknownClientMethods[message.method] =
          (this.counters.unknownClientMethods[message.method] ?? 0) + 1;
        this.respondError(message.id, -32601, `probe: unsupported client method ${message.method}`);
      }
    }
  }

  isAllowedPath(path) {
    const resolved = resolve(path);
    // 只开放隔离 workspace：隔离 codex-home 里的凭据副本绝不通过客户端 fs 服务暴露。
    return samePath(resolved, this.engine.workspace) || isInside(this.engine.workspace, resolved);
  }

  async createTerminal(message) {
    this.counters.terminalCreates += 1;
    const params = message.params ?? {};
    const cwd = typeof params.cwd === "string" ? resolve(params.cwd) : this.engine.workspace;
    if (!samePath(cwd, this.engine.workspace) && !isInside(this.engine.workspace, cwd)) {
      this.counters.terminalDenied += 1;
      this.respondError(message.id, -32602, "cwd outside isolated probe workspace");
      return;
    }
    if (typeof params.command !== "string" || params.command.length === 0) {
      this.respondError(message.id, -32602, "missing command");
      return;
    }
    const args = Array.isArray(params.args) ? params.args.filter((arg) => typeof arg === "string") : [];
    const env = { ...process.env, CODEX_HOME: this.engine.codexHome, NO_COLOR: "1" };
    for (const item of Array.isArray(params.env) ? params.env : []) {
      if (typeof item?.name === "string" && typeof item?.value === "string") env[item.name] = item.value;
    }
    const limit = Number.isInteger(params.outputByteLimit) && params.outputByteLimit > 0 ? params.outputByteLimit : 1024 * 1024;

    let child;
    try {
      child = spawn(params.command, args, {
        cwd,
        env,
        stdio: ["ignore", "pipe", "pipe"],
        windowsHide: true,
        detached: !WINDOWS,
      });
      await new Promise((resolveSpawn, rejectSpawn) => {
        child.once("spawn", resolveSpawn);
        child.once("error", rejectSpawn);
      });
    } catch (error) {
      this.respondError(message.id, -32000, `terminal spawn failed: ${shortError(error)}`);
      return;
    }

    this.terminalSeq += 1;
    const terminalId = `probe-terminal-${this.terminalSeq}`;
    const terminal = {
      id: terminalId,
      child,
      output: Buffer.alloc(0),
      truncated: false,
      limit,
      exited: false,
      exitCode: null,
      exitSignal: null,
      done: null,
    };
    terminal.done = new Promise((resolveDone) => {
      const append = (chunk) => {
        terminal.output = Buffer.concat([terminal.output, chunk]);
        if (terminal.output.length > limit) {
          terminal.output = terminal.output.subarray(terminal.output.length - limit);
          terminal.truncated = true;
        }
      };
      child.stdout.on("data", append);
      child.stderr.on("data", append);
      child.once("close", (code, signal) => {
        terminal.exited = true;
        terminal.exitCode = code;
        terminal.exitSignal = signal;
        resolveDone();
      });
    });
    this.terminals.set(terminalId, terminal);
    this.respond(message.id, { terminalId });
  }

  async terminate(reason = "cleanup") {
    if (this.terminatePromise) return this.terminatePromise;
    this.terminatePromise = (async () => {
      this.expectedExit = true;
      for (const [id, pending] of this.pending) {
        clearTimeout(pending.timer);
        pending.reject(new ProcessExitError(pending.method, id, null, null, "探针主动关闭连接"));
      }
      this.pending.clear();

      for (const terminal of this.terminals.values()) {
        await killProcessTree(terminal.child.pid);
      }
      this.terminals.clear();

      if (this.pid) {
        if (!this.exitInfo) {
          this.engine.progress(this.label, "terminate", { runId: this.runId, pid: this.pid, reason });
          await killProcessTree(this.pid);
          const exited = await Promise.race([this.exited, delay(5000).then(() => null)]);
          if (!exited) {
            try {
              this.child.kill("SIGKILL");
            } catch {
              // 已退出。
            }
            await Promise.race([this.exited, delay(2000)]);
          }
        }
        // 即使根进程已退出，也要确认没有遗留后代进程。
        const tree = await waitForTreeGone(this.engine, this.pid, 8000);
        this.run.treeAfter = {
          processCount: tree.processCount,
          rootPresent: tree.rootPresent,
          workingSetMb: tree.workingSetMb,
          method: tree.method,
          error: tree.error,
        };
      }

      this.run.terminatedByProbe = true;
      this.run.terminationReason = reason;
      this.run.stderrBytes = this.stderrBytes;
      if (!this.run.endedAt) this.run.endedAt = nowIso();
      if (this.readline) this.readline.close();
      if (this.child?.stdin && !this.child.stdin.destroyed) {
        try {
          this.child.stdin.end();
        } catch {
          // 已关闭。
        }
      }
      this.stderrStream.end();
      this.engine.connections.delete(this);
      return this.exitInfo ?? { code: null, signal: null, at: nowIso() };
    })();
    return this.terminatePromise;
  }
}

function hasCapability(sessionCapabilities, name) {
  return Boolean(
    sessionCapabilities &&
      typeof sessionCapabilities === "object" &&
      sessionCapabilities[name] !== undefined &&
      sessionCapabilities[name] !== null,
  );
}

async function spawnConnection(engine, label) {
  const connection = new Connection(engine, label);
  engine.connections.add(connection);
  try {
    await connection.start();
    return connection;
  } catch (error) {
    engine.connections.delete(connection);
    connection.run.error = shortError(error);
    connection.run.endedAt = nowIso();
    connection.stderrStream.end();
    throw error;
  }
}

async function initializeConnection(connection, engine) {
  const result = await connection.request("initialize", {
    protocolVersion: PROTOCOL_VERSION,
    clientCapabilities: clientCapabilities(),
    clientInfo: { name: "acpr-session-lifecycle-probe", version: TOOL_VERSION },
  });
  const sessionCapabilities =
    result?.agentCapabilities?.sessionCapabilities ?? result?.sessionCapabilities ?? null;
  const capabilities = {
    protocolVersion: result?.protocolVersion ?? null,
    agentInfo: result?.agentInfo ?? null,
    sessionCapabilities,
    supportsResume: hasCapability(sessionCapabilities, "resume"),
    supportsClose: hasCapability(sessionCapabilities, "close"),
    rawInitializeResult: result ?? null,
  };
  connection.caps = capabilities;
  connection.run.capabilities = capabilities;
  // 顶层报告保留首次 initialize 的能力原文；后续进程的能力在 runs[].capabilities 中。
  if (!engine.capabilities) engine.capabilities = capabilities;
  engine.progress(connection.label, "initialize", {
    runId: connection.runId,
    protocolVersion: capabilities.protocolVersion,
    agentInfo: capabilities.agentInfo,
    supportsResume: capabilities.supportsResume,
    supportsClose: capabilities.supportsClose,
  });
  return capabilities;
}

function pickResumeResponseId(result) {
  return result && typeof result === "object" ? result.sessionId ?? null : null;
}

async function promptTurn(connection, engine, sessionId, text, label, turnOptions = {}) {
  // 只有唯一一轮记忆 prompt 允许携带标记；其余轮次携带会把“标记只出现一次”的判据污染成恒 FAIL。
  if (label !== "memory" && text.includes(engine.marker)) {
    throw new ProbeError(`探针构造错误：${label} 轮 prompt 不能包含记忆标记`);
  }
  const seqStart = connection.updates.length;
  const countersBefore = {
    toolCalls: connection.counters.toolCalls,
    toolCallUpdates: connection.counters.toolCallUpdates,
  };
  const timeoutMs = turnOptions.timeoutMs ?? engine.options.timeoutMs;
  const requestPromise = connection.request(
    "session/prompt",
    {
      sessionId,
      prompt: [{ type: "text", text }],
    },
    { timeoutMs },
  );
  engine.priorPrompts.push(text);

  let result;
  try {
    result = await requestPromise;
  } catch (error) {
    if (error instanceof RequestTimeoutError) {
      try {
        connection.notify("session/cancel", { sessionId });
      } catch {
        // 连接可能已关闭。
      }
    }
    throw error;
  }
  // 给流式 session/update 一个落定窗口，避免响应先于最后一条 chunk 到达。
  await delay(200);

  let updates = [];
  let assistantText = "";
  for (let attempt = 0; attempt < 10; attempt += 1) {
    updates = connection.updates.slice(seqStart);
    assistantText = updates
      .filter((update) => update.sessionId === sessionId && update.kind === "agent_message_chunk")
      .map((update) => update.text ?? "")
      .join("")
      .trim();
    if (assistantText) break;
    await delay(100);
  }

  return {
    label,
    result,
    stopReason: result?.stopReason ?? null,
    assistantText,
    updateCount: updates.length,
    toolCalls: connection.counters.toolCalls - countersBefore.toolCalls,
    toolCallUpdates: connection.counters.toolCallUpdates - countersBefore.toolCallUpdates,
    timeoutMs,
  };
}

function inspectSideEffect(workspace, token) {
  const path = join(workspace, "side-effect.txt");
  if (!existsSync(path)) {
    return {
      path,
      exists: false,
      observed: false,
      lineCount: 0,
      bytes: 0,
      expectedTokenCount: 0,
      contentSha256: null,
      tokenSha256: sha256(token),
    };
  }
  const content = readFileSync(path, "utf8");
  const lines = content.split(/\r?\n/).filter((line) => line.length > 0);
  const expectedTokenCount = content.split(token).length - 1;
  return {
    path,
    exists: true,
    observed: expectedTokenCount > 0,
    lineCount: lines.length,
    bytes: Buffer.byteLength(content),
    expectedTokenCount,
    contentSha256: sha256(content),
    tokenSha256: sha256(token),
  };
}

function sameSideEffect(left, right) {
  if (!left || !right) return null;
  return (
    left.exists === right.exists &&
    left.lineCount === right.lineCount &&
    left.bytes === right.bytes &&
    left.expectedTokenCount === right.expectedTokenCount &&
    left.contentSha256 === right.contentSha256
  );
}

function replyDetails(turn) {
  return {
    replySha256: sha256(turn.assistantText),
    replyLength: turn.assistantText.length,
    replyExcerpt: truncate(turn.assistantText.replace(/\s+/g, " "), 200),
    stopReason: turn.stopReason,
    updateCount: turn.updateCount,
    toolCalls: turn.toolCalls,
    toolCallUpdates: turn.toolCallUpdates,
  };
}

// 旧 prompt 判定用整串相等：各轮追问天然共享措辞（如“只回复标记本身”），
// 子串匹配会把正常追问误判成重发旧 prompt，使 no_old_prompt_resent 恒 FAIL。
function promptTexts(params) {
  if (!Array.isArray(params?.prompt)) return [];
  return params.prompt
    .map((part) => (part && typeof part.text === "string" ? part.text : null))
    .filter((text) => text !== null);
}

function containsPriorPrompt(engine, params) {
  return promptTexts(params).some((text) => engine.priorPrompts.includes(text));
}

function evaluateResumeParams(engine, entry, params, expectedSessionId, expectedCwd) {
  const allowed = new Set(["sessionId", "cwd", "additionalDirectories", "mcpServers", "_meta"]);
  if (!entry) {
    return { ok: false, details: { reason: "未在请求账本中找到 session/resume" } };
  }
  const serialized = JSON.stringify(params ?? {});
  const unknownKeys = entry.paramsKeys.filter((key) => !allowed.has(key));
  const containsMarker = [...engine.memoryMarkers].some((token) => serialized.includes(token));
  const containsSideEffectToken = [...engine.sideEffectTokens].some((token) => serialized.includes(token));
  const containsOldPrompt = containsPriorPrompt(engine, params);
  const sessionIdMatches = params?.sessionId === expectedSessionId;
  const cwdMatches = typeof params?.cwd === "string" && samePath(params.cwd, expectedCwd);
  const ok =
    unknownKeys.length === 0 &&
    !containsMarker &&
    !containsSideEffectToken &&
    !containsOldPrompt &&
    sessionIdMatches &&
    cwdMatches;
  return {
    ok,
    details: {
      requestId: entry.requestId,
      paramsSha256: entry.paramsSha256,
      paramsKeys: entry.paramsKeys,
      unknownKeys,
      containsSessionMarker: containsMarker,
      containsSideEffectToken,
      containsOldPrompt,
      sessionIdMatches,
      cwdMatches,
      expectedSessionId,
      expectedCwd,
    },
  };
}

function ledgerForRun(engine, runId) {
  return engine.ledger.filter((entry) => entry.runId === runId);
}

function markerEntries(engine, runId) {
  return ledgerForRun(engine, runId).filter((entry) => entry.containsSessionMarker);
}

function sideEffectTokenEntries(engine, runId) {
  return ledgerForRun(engine, runId).filter((entry) => entry.containsSideEffectToken);
}

function writeState(engine, state) {
  state.updatedAt = nowIso();
  engine.state = state;
  try {
    writeFileSync(engine.statePath, `${JSON.stringify(state, null, 2)}\n`);
  } catch (error) {
    engine.notes.push(`state.json 写入失败：${shortError(error)}`);
  }
}

function readState(engine) {
  if (!existsSync(engine.statePath)) return null;
  const state = readJsonFile(engine.statePath);
  if (!state) {
    engine.notes.push(`state.json 存在但无法解析：${engine.statePath}`);
    return null;
  }
  return state;
}

function registerStateSecrets(engine, state) {
  if (!state) return;
  if (state.marker) engine.memoryMarkers.add(state.marker);
  if (state.sideEffectToken) engine.sideEffectTokens.add(state.sideEffectToken);
  for (const prompt of state.priorPrompts ?? []) {
    if (!engine.priorPrompts.includes(prompt)) engine.priorPrompts.push(prompt);
  }
}

/**
 * recall 场景：记忆 → 同会话追问 → 工具副作用 → close → resume → 再追问。
 */
async function runRecall(engine) {
  const stage = "recall";
  const startedAt = nowIso();
  engine.timestamps.stages.recall = { startedAt, endedAt: null };
  const connection = await spawnConnection(engine, stage);
  const state = {
    schemaVersion: 1,
    runId: engine.runId,
    createdAt: engine.startedAt,
    dataDir: engine.dataDir,
    workspace: engine.workspace,
    codexHome: engine.codexHome,
    marker: engine.marker,
    sideEffectToken: engine.sideEffectToken,
    sessionId: null,
    sessionCwd: engine.workspace,
    capabilities: null,
    closeSucceeded: false,
    resumeSucceeded: false,
    markerAfterResume: false,
    sideEffect: null,
    priorPrompts: [],
    ready: false,
    lastRun: null,
  };
  writeState(engine, state);

  try {
    const capabilities = await initializeConnection(connection, engine);
    state.capabilities = {
      agentInfo: capabilities.agentInfo,
      sessionCapabilities: capabilities.sessionCapabilities,
      supportsResume: capabilities.supportsResume,
      supportsClose: capabilities.supportsClose,
    };

    recordCheck(
      engine,
      stage,
      "recall.initialize_capabilities_recorded",
      capabilities.sessionCapabilities ? "PASS" : "FAIL",
      capabilities.sessionCapabilities
        ? "已记录 initialize 的 agentInfo 与 sessionCapabilities 原文"
        : "initialize 未返回 sessionCapabilities",
      { agentInfo: capabilities.agentInfo, sessionCapabilities: capabilities.sessionCapabilities },
    );

    recordCheck(
      engine,
      stage,
      "recall.resume_advertised",
      capabilities.supportsResume ? "PASS" : "FAIL",
      capabilities.supportsResume
        ? "Agent 宣告 sessionCapabilities.resume"
        : "Agent 未宣告 sessionCapabilities.resume；交付门槛不满足",
      { sessionCapabilities: capabilities.sessionCapabilities },
    );

    recordCheck(
      engine,
      stage,
      "recall.close_advertised",
      capabilities.supportsClose ? "PASS" : "FAIL",
      capabilities.supportsClose
        ? "Agent 宣告 sessionCapabilities.close"
        : "Agent 未宣告 sessionCapabilities.close；交付门槛要求 close 后可恢复",
      { sessionCapabilities: capabilities.sessionCapabilities },
    );

    if (!capabilities.supportsResume) {
      ensureStageSkips(engine, stage, "Agent 未宣告 sessionCapabilities.resume；恢复路径检查未执行");
      recordCheck(engine, stage, "recall.completed", "FAIL", "Agent 不支持 session/resume，WP0 门槛不满足", {
        supportsResume: false,
      });
      return { connection, state, ok: false };
    }

    const created = await connection.request("session/new", {
      cwd: engine.workspace,
      mcpServers: [],
    });
    const sessionId = created?.sessionId;
    state.sessionId = typeof sessionId === "string" && sessionId.length > 0 ? sessionId : null;
    if (state.sessionId) engine.sessionIds.recall = state.sessionId;
    recordCheck(
      engine,
      stage,
      "recall.session_new",
      state.sessionId ? "PASS" : "FAIL",
      state.sessionId ? "在隔离 cwd 下创建会话并取得 sessionId" : "session/new 未返回 sessionId",
      { sessionId: state.sessionId, cwd: engine.workspace, responseKeys: Object.keys(created ?? {}) },
    );
    if (!state.sessionId) {
      ensureStageSkips(engine, stage, "session/new 未返回 sessionId，后续检查未执行");
      recordCheck(engine, stage, "recall.completed", "FAIL", "无法创建会话，recall 门槛不满足", {});
      return { connection, state, ok: false };
    }
    writeState(engine, state);

    const memoryTurn = await promptTurn(
      connection,
      engine,
      state.sessionId,
      `请仅在本轮记住这个标记，不要写入任何文件，也不要复述：${engine.marker}`,
      "memory",
    );
    recordCheck(
      engine,
      stage,
      "recall.first_prompt_completed",
      memoryTurn.stopReason === "end_turn" ? "PASS" : "FAIL",
      memoryTurn.stopReason === "end_turn"
        ? "第一轮 prompt 正常结束"
        : `第一轮 prompt 未正常结束：stopReason=${memoryTurn.stopReason}`,
      { stopReason: memoryTurn.stopReason, updateCount: memoryTurn.updateCount },
    );

    const sameSessionTurn = await promptTurn(
      connection,
      engine,
      state.sessionId,
      "我刚才让你记住的标记是什么？只回复标记本身。",
      "recall-before-close",
    );
    const markerSeen = sameSessionTurn.assistantText.includes(engine.marker);
    recordCheck(
      engine,
      stage,
      "recall.marker_same_session",
      markerSeen ? "PASS" : "FAIL",
      markerSeen ? "同一会话内回答包含随机标记" : "同一会话内回答不包含随机标记",
      replyDetails(sameSessionTurn),
    );

    // 副作用请求插在记忆验证之后、close 之前：既证明上下文承接，又便于 resume 后比对。
    // 真实运行观察：若 prompt 只说“使用你的工具”，Codex 会先尝试 shell/exec（本机 ShellExecuteExW 失败）
    // 再退回编辑工具，耗时远超普通问答。这里显式只允许编辑/写文件工具，并给该轮独立超时。
    const sideEffectTurn = await promptTurn(
      connection,
      engine,
      state.sessionId,
      "请使用你的文件编辑/写文件工具（例如 apply_patch、write_file 等编辑类工具），" +
        "在当前工作目录（cwd）直接创建或覆盖文件 side-effect.txt，" +
        `只写入唯一一行：${engine.sideEffectToken}。` +
        "不要运行任何 shell 命令，也不要调用终端/exec 类工具；完成后只回复 done。",
      "side-effect",
      { timeoutMs: engine.options.sideEffectTimeoutMs },
    );
    const effect = inspectSideEffect(engine.workspace, engine.sideEffectToken);
    state.sideEffect = effect;
    engine.sideEffect = effect;
    if (effect.observed) {
      recordCheck(
        engine,
        stage,
        "recall.side_effect_observed",
        "PASS",
        "观察到工具副作用：side-effect.txt 含预期随机标记",
        {
          path: effect.path,
          lineCount: effect.lineCount,
          bytes: effect.bytes,
          expectedTokenCount: effect.expectedTokenCount,
          contentSha256: effect.contentSha256,
          toolCalls: sideEffectTurn.toolCalls,
          toolCallUpdates: sideEffectTurn.toolCallUpdates,
          stopReason: sideEffectTurn.stopReason,
          timeoutMs: sideEffectTurn.timeoutMs,
        },
      );
    } else if (!effect.exists) {
      recordCheck(
        engine,
        stage,
        "recall.side_effect_observed",
        "SKIP",
        "未观察到工具副作用：side-effect.txt 不存在；不据此伪造工具未重放结论",
        {
          path: effect.path,
          toolCalls: sideEffectTurn.toolCalls,
          toolCallUpdates: sideEffectTurn.toolCallUpdates,
          stopReason: sideEffectTurn.stopReason,
          timeoutMs: sideEffectTurn.timeoutMs,
        },
      );
    } else {
      recordCheck(
        engine,
        stage,
        "recall.side_effect_observed",
        "FAIL",
        "side-effect.txt 存在但不含预期随机标记",
        {
          path: effect.path,
          lineCount: effect.lineCount,
          expectedTokenCount: effect.expectedTokenCount,
          contentSha256: effect.contentSha256,
          toolCalls: sideEffectTurn.toolCalls,
          stopReason: sideEffectTurn.stopReason,
          timeoutMs: sideEffectTurn.timeoutMs,
        },
      );
    }

    if (capabilities.supportsClose) {
      try {
        await connection.request("session/close", { sessionId: state.sessionId });
        state.closeSucceeded = true;
        recordCheck(engine, stage, "recall.close_succeeded", "PASS", "session/close 返回成功", {
          sessionId: state.sessionId,
        });
      } catch (error) {
        recordCheck(engine, stage, "recall.close_succeeded", "FAIL", `session/close 失败：${shortError(error)}`, {
          error: describeError(error),
        });
      }
    } else {
      recordCheck(
        engine,
        stage,
        "recall.close_succeeded",
        "SKIP",
        "Agent 未宣告 sessionCapabilities.close；未执行 close",
        {},
      );
    }

    if (!state.closeSucceeded) {
      const oldPromptEntries = ledgerForRun(engine, connection.runId).filter((entry) => entry.containsOldPrompt);
      recordCheck(
        engine,
        stage,
        "recall.no_old_prompt_resent",
        oldPromptEntries.length === 0 ? "PASS" : "FAIL",
        oldPromptEntries.length === 0 ? "没有请求重发旧 prompt 文本" : "检测到旧 prompt 文本被重发",
        { oldPromptRequestIds: oldPromptEntries.map((entry) => entry.requestId) },
      );
      const markerLedger = markerEntries(engine, connection.runId);
      recordCheck(
        engine,
        stage,
        "recall.marker_sent_once",
        markerLedger.length === 1 ? "PASS" : "FAIL",
        markerLedger.length === 1 ? "记忆标记只在第一轮 prompt 中出现一次" : "记忆标记出现次数不为 1",
        { count: markerLedger.length, requestIds: markerLedger.map((entry) => entry.requestId) },
      );
      ensureStageSkips(engine, stage, "close 未成功，未执行 close 后的 resume 路径");
      recordCheck(engine, stage, "recall.completed", "FAIL", "close 未成功，无法验证 close 后可恢复", {});
      return { connection, state, ok: false };
    }

    const resumeParams = { sessionId: state.sessionId, cwd: engine.workspace, mcpServers: [] };
    // 工具调用统计从 resume 之前开始，覆盖“恢复本身 + 恢复后的追问”，避免漏掉 resume 期间的副作用。
    const toolCallsBeforeResume = connection.counters.toolCalls;
    const toolCallUpdatesBeforeResume = connection.counters.toolCallUpdates;
    const resumePromise = connection.request("session/resume", resumeParams);
    let resumeResult = null;
    let resumeError = null;
    try {
      resumeResult = await resumePromise;
    } catch (error) {
      resumeError = error;
    }
    const resumeEntry = ledgerForRun(engine, connection.runId).find((entry) => entry.method === "session/resume");
    const paramsEvaluation = evaluateResumeParams(
      engine,
      resumeEntry,
      resumeParams,
      state.sessionId,
      engine.workspace,
    );
    recordCheck(
      engine,
      stage,
      "recall.resume_request_protocol_only",
      paramsEvaluation.ok ? "PASS" : "FAIL",
      paramsEvaluation.ok
        ? "resume 请求只含协议字段，未携带标记、旧 prompt 或非协议字段"
        : "resume 请求含非协议字段或标记/旧 prompt 文本",
      paramsEvaluation.details,
    );

    if (!resumeResult) {
      recordCheck(
        engine,
        stage,
        "recall.resume_request_succeeded",
        "FAIL",
        `session/resume 失败：${shortError(resumeError)}`,
        { error: describeError(resumeError) },
      );
      ensureStageSkips(engine, stage, "resume 失败，未执行恢复后的追问");
      recordCheck(engine, stage, "recall.completed", "FAIL", "session/resume 失败，无法验证上下文恢复", {});
      return { connection, state, ok: false };
    }

    const returnedSessionId = pickResumeResponseId(resumeResult);
    const identityPreserved = returnedSessionId === null || returnedSessionId === state.sessionId;
    state.resumeSucceeded = identityPreserved;
    recordCheck(
      engine,
      stage,
      "recall.resume_request_succeeded",
      identityPreserved ? "PASS" : "FAIL",
      identityPreserved
        ? "session/resume 返回成功且未返回不同的 sessionId"
        : `session/resume 返回了不同的 sessionId：${returnedSessionId}`,
      {
        sessionId: state.sessionId,
        responseSessionId: returnedSessionId,
        responseKeys: Object.keys(resumeResult ?? {}),
      },
    );

    const afterResumeTurn = await promptTurn(
      connection,
      engine,
      state.sessionId,
      "请再回答一次：刚才那个标记是什么？只回复标记本身。",
      "recall-after-resume",
    );
    const markerAfterResume = afterResumeTurn.assistantText.includes(engine.marker);
    state.markerAfterResume = markerAfterResume;
    recordCheck(
      engine,
      stage,
      "recall.marker_after_resume",
      markerAfterResume ? "PASS" : "FAIL",
      markerAfterResume ? "close 后 resume 同一 sessionId，回答仍包含随机标记" : "resume 后回答不包含随机标记",
      replyDetails(afterResumeTurn),
    );

    const sessionNewCount = ledgerForRun(engine, connection.runId).filter(
      (entry) => entry.method === "session/new",
    ).length;
    recordCheck(
      engine,
      stage,
      "recall.no_replacement_session",
      sessionNewCount === 1 ? "PASS" : "FAIL",
      sessionNewCount === 1
        ? "recall 全程只创建过一个会话，恢复未新建替代会话"
        : `recall 全程出现 ${sessionNewCount} 次 session/new`,
      { sessionNewCount, expected: 1 },
    );

    const effectAfter = inspectSideEffect(engine.workspace, engine.sideEffectToken);
    const toolCallsInResumeWindow = connection.counters.toolCalls - toolCallsBeforeResume;
    const toolCallUpdatesInResumeWindow = connection.counters.toolCallUpdates - toolCallUpdatesBeforeResume;
    if (!state.sideEffect?.observed) {
      recordCheck(
        engine,
        stage,
        "recall.no_replayed_side_effect",
        "SKIP",
        "未观察到工具副作用，无法断言 resume 未重复执行工具",
        {
          before: state.sideEffect,
          after: effectAfter,
          toolCallsInResumeWindow,
          toolCallUpdatesInResumeWindow,
        },
      );
    } else {
      const unchanged = sameSideEffect(state.sideEffect, effectAfter);
      const ok = toolCallsInResumeWindow === 0 && unchanged === true;
      recordCheck(
        engine,
        stage,
        "recall.no_replayed_side_effect",
        ok ? "PASS" : "FAIL",
        ok
          ? "resume 窗口内没有新的工具调用，副作用文件未变化"
          : "resume 窗口内出现工具调用或副作用文件变化",
        {
          toolCallsInResumeWindow,
          toolCallUpdatesInResumeWindow,
          before: state.sideEffect,
          after: effectAfter,
          unchanged,
        },
      );
    }

    const oldPromptEntries = ledgerForRun(engine, connection.runId).filter((entry) => entry.containsOldPrompt);
    const markerLedger = markerEntries(engine, connection.runId);
    const sideEffectLedger = sideEffectTokenEntries(engine, connection.runId);
    const noReplay = oldPromptEntries.length === 0 && markerLedger.length === 1 && sideEffectLedger.length === 1;
    recordCheck(
      engine,
      stage,
      "recall.no_old_prompt_resent",
      noReplay ? "PASS" : "FAIL",
      noReplay
        ? "没有请求重发旧 prompt 文本；随机标记与副作用标记各只出现一次"
        : "检测到旧 prompt 或随机标记被重复发送",
      {
        oldPromptRequestIds: oldPromptEntries.map((entry) => entry.requestId),
        markerRequestIds: markerLedger.map((entry) => entry.requestId),
        sideEffectRequestIds: sideEffectLedger.map((entry) => entry.requestId),
      },
    );

    recordCheck(
      engine,
      stage,
      "recall.marker_sent_once",
      markerLedger.length === 1 ? "PASS" : "FAIL",
      markerLedger.length === 1 ? "记忆标记只在第一轮 prompt 中出现一次" : "记忆标记出现次数不为 1",
      { count: markerLedger.length, requestIds: markerLedger.map((entry) => entry.requestId) },
    );

    state.priorPrompts = [...engine.priorPrompts];
    state.ready = state.closeSucceeded && state.resumeSucceeded && state.markerAfterResume;
    writeState(engine, state);
    finalizeStage(engine, stage);
    return { connection, state, ok: state.ready };
  } catch (error) {
    ensureStageSkips(engine, stage, `阶段异常中断：${shortError(error)}`);
    if (!engine.checkIds.has("recall.completed")) {
      recordCheck(engine, stage, "recall.completed", "FAIL", `recall 阶段异常：${shortError(error)}`, {
        error: describeError(error),
      });
    }
    await connection.terminate("recall-error").catch(() => {});
    throw error;
  } finally {
    engine.timestamps.stages.recall.endedAt = nowIso();
  }
}

/**
 * restart 场景：终止旧进程树，用同一 codex-home/data-dir 起新进程，resume 原 sessionId。
 */
async function runRestart(engine, { liveConnection, state }) {
  const stage = "restart";
  const startedAt = nowIso();
  engine.timestamps.stages.restart = { startedAt, endedAt: null };
  try {
    if (state) registerStateSecrets(engine, state);

    if (liveConnection) {
      const before = await recordResourceSample(
        engine,
        stage,
        "before_terminate",
        liveConnection.pid,
        liveConnection.runId,
      );
      const exit = await liveConnection.terminate("restart");
      const tree = await waitForTreeGone(engine, liveConnection.pid, 10_000);
      if (state) {
        state.lastRun = {
          pid: liveConnection.pid,
          exitCode: exit.code ?? liveConnection.run.exitCode,
          signal: exit.signal ?? liveConnection.run.signal,
          terminatedAt: nowIso(),
          treeAfter: { processCount: tree.processCount, rootPresent: tree.rootPresent },
        };
        writeState(engine, state);
      }
      const terminationStatus = treeTerminationStatus(tree);
      recordCheck(
        engine,
        stage,
        "restart.previous_process_tree_terminated",
        terminationStatus,
        terminationStatus === "PASS"
          ? "旧适配器进程树已终止且无残留"
          : terminationStatus === "SKIP"
            ? "进程树采样失败，无法确认旧适配器进程树是否终止"
            : "旧适配器进程树仍有残留",
        {
          pid: liveConnection.pid,
          exitCode: exit.code,
          signal: exit.signal,
          before,
          after: { processCount: tree.processCount, rootPresent: tree.rootPresent },
        },
      );
      liveConnection = null;
    } else if (state?.lastRun) {
      const tree = await recordResourceSample(engine, stage, "previous_pid", state.lastRun.pid, null);
      const terminationStatus = treeTerminationStatus(tree);
      recordCheck(
        engine,
        stage,
        "restart.previous_process_tree_terminated",
        terminationStatus,
        terminationStatus === "PASS"
          ? "上次运行记录的适配器进程树已不存在"
          : terminationStatus === "SKIP"
            ? "进程树采样失败，无法确认上次运行的进程是否终止"
            : "上次运行记录的适配器进程仍有残留",
        { lastRun: state.lastRun, tree },
      );
    } else {
      recordCheck(
        engine,
        stage,
        "restart.previous_process_tree_terminated",
        "SKIP",
        "独立运行且没有上次进程记录，无法验证旧进程树终止",
        {},
      );
    }

    if (!state?.ready) {
      ensureStageSkips(engine, stage, "前置 recall 未完成 close+resume，restart 检查未执行");
      // 用汇总规则保留前一步进程终止检查的 FAIL，而不是无条件写成 SKIP。
      finalizeStage(engine, stage);
      return;
    }

    const connection = await spawnConnection(engine, stage);
    try {
      const capabilities = await initializeConnection(connection, engine);
      recordCheck(
        engine,
        stage,
        "restart.initialize_capabilities_recorded",
        capabilities.sessionCapabilities ? "PASS" : "FAIL",
        capabilities.sessionCapabilities
          ? "新进程已记录 initialize 能力"
          : "新进程 initialize 未返回 sessionCapabilities",
        { agentInfo: capabilities.agentInfo, sessionCapabilities: capabilities.sessionCapabilities },
      );

      const resumeParams = { sessionId: state.sessionId, cwd: state.workspace, mcpServers: [] };
      const toolCallsBeforeRestartResume = connection.counters.toolCalls;
      const toolCallUpdatesBeforeRestartResume = connection.counters.toolCallUpdates;
      let resumeResult = null;
      let resumeError = null;
      try {
        resumeResult = await connection.request("session/resume", resumeParams);
      } catch (error) {
        resumeError = error;
      }
      const resumeEntry = ledgerForRun(engine, connection.runId).find((entry) => entry.method === "session/resume");
      const evaluation = evaluateResumeParams(engine, resumeEntry, resumeParams, state.sessionId, state.workspace);
      recordCheck(
        engine,
        stage,
        "restart.resume_request_protocol_only",
        evaluation.ok ? "PASS" : "FAIL",
        evaluation.ok
          ? "重启后的 resume 请求只含协议字段，未携带标记或旧 prompt"
          : "重启后的 resume 请求含非协议字段或标记/旧 prompt 文本",
        evaluation.details,
      );

      const restartReturnedSessionId = pickResumeResponseId(resumeResult);
      const restartIdentityPreserved =
        Boolean(resumeResult) &&
        (restartReturnedSessionId === null || restartReturnedSessionId === state.sessionId);
      recordCheck(
        engine,
        stage,
        "restart.resume_request_succeeded",
        restartIdentityPreserved ? "PASS" : "FAIL",
        !resumeResult
          ? `新进程 resume 失败：${shortError(resumeError)}`
          : restartIdentityPreserved
            ? "新进程 resume 原 sessionId 成功且未返回不同身份"
            : `新进程 resume 返回了不同的 sessionId：${restartReturnedSessionId}`,
        {
          sessionId: state.sessionId,
          responseSessionId: restartReturnedSessionId,
          error: resumeError ? describeError(resumeError) : null,
        },
      );

      const sessionNewCount = ledgerForRun(engine, connection.runId).filter(
        (entry) => entry.method === "session/new",
      ).length;
      recordCheck(
        engine,
        stage,
        "restart.no_session_new",
        sessionNewCount === 0 ? "PASS" : "FAIL",
        sessionNewCount === 0 ? "重启后的进程没有发送 session/new" : `重启后的进程发送了 ${sessionNewCount} 次 session/new`,
        { sessionNewCount, expected: 0 },
      );

      if (!resumeResult) {
        ensureStageSkips(engine, stage, "resume 失败，未执行重启后的追问");
      } else {
        const turn = await promptTurn(
          connection,
          engine,
          state.sessionId,
          "进程已重启。请回答：之前让你记住的标记是什么？只回复标记本身。",
          "restart-marker",
        );
        const markerSeen = turn.assistantText.includes(state.marker);
        recordCheck(
          engine,
          stage,
          "restart.marker_after_restart",
          markerSeen ? "PASS" : "FAIL",
          markerSeen
            ? "进程重启后 resume 原 sessionId，回答仍包含随机标记"
            : "进程重启后回答不包含随机标记",
          { ...replyDetails(turn), expectedMarkerSha256: sha256(state.marker) },
        );

        const effectAfter = inspectSideEffect(state.workspace, state.sideEffectToken);
        const toolCallsInRestartResumeWindow = connection.counters.toolCalls - toolCallsBeforeRestartResume;
        const toolCallUpdatesInRestartResumeWindow =
          connection.counters.toolCallUpdates - toolCallUpdatesBeforeRestartResume;
        if (!state.sideEffect?.observed) {
          recordCheck(
            engine,
            stage,
            "restart.no_replayed_side_effect",
            "SKIP",
            "recall 阶段未观察到工具副作用，无法断言重启后未重复执行工具",
            {
              before: state.sideEffect,
              after: effectAfter,
              toolCallsInRestartResumeWindow,
              toolCallUpdatesInRestartResumeWindow,
            },
          );
        } else {
          const unchanged = sameSideEffect(state.sideEffect, effectAfter);
          const ok = toolCallsInRestartResumeWindow === 0 && unchanged === true;
          recordCheck(
            engine,
            stage,
            "restart.no_replayed_side_effect",
            ok ? "PASS" : "FAIL",
            ok ? "重启 resume 窗口内没有新的工具调用，副作用文件未变化" : "重启 resume 窗口内出现工具调用或副作用文件变化",
            {
              toolCallsInRestartResumeWindow,
              toolCallUpdatesInRestartResumeWindow,
              before: state.sideEffect,
              after: effectAfter,
              unchanged,
            },
          );
        }

        const oldPromptEntries = ledgerForRun(engine, connection.runId).filter(
          (entry) => entry.containsOldPrompt,
        );
        const markerLedger = markerEntries(engine, connection.runId);
        const sideEffectLedger = sideEffectTokenEntries(engine, connection.runId);
        const clean = oldPromptEntries.length === 0 && markerLedger.length === 0 && sideEffectLedger.length === 0;
        recordCheck(
          engine,
          stage,
          "restart.no_old_prompt_resent",
          clean ? "PASS" : "FAIL",
          clean
            ? "重启后的进程没有重发旧 prompt、记忆标记或副作用标记"
            : "重启后的进程重发了旧 prompt 或随机标记",
          {
            oldPromptRequestIds: oldPromptEntries.map((entry) => entry.requestId),
            markerRequestIds: markerLedger.map((entry) => entry.requestId),
            sideEffectRequestIds: sideEffectLedger.map((entry) => entry.requestId),
          },
        );
      }

      finalizeStage(engine, stage);
    } finally {
      const exit = await connection.terminate("restart-complete");
      if (state) {
        const tree = await sampleProcessTree(engine, connection.pid);
        state.lastRun = {
          pid: connection.pid,
          exitCode: exit.code ?? connection.run.exitCode,
          signal: exit.signal ?? connection.run.signal,
          terminatedAt: nowIso(),
          treeAfter: { processCount: tree.processCount, rootPresent: tree.rootPresent },
        };
        writeState(engine, state);
      }
    }
  } catch (error) {
    ensureStageSkips(engine, stage, `阶段异常中断：${shortError(error)}`);
    if (!engine.checkIds.has("restart.completed")) {
      recordCheck(engine, stage, "restart.completed", "FAIL", `restart 阶段异常：${shortError(error)}`, {
        error: describeError(error),
      });
    }
    throw error;
  } finally {
    engine.timestamps.stages.restart.endedAt = nowIso();
  }
}

/**
 * isolation 场景：同一进程内 A、B 两会话，关闭 A 后 B 仍能应答。
 */
async function runIsolation(engine) {
  const stage = "isolation";
  const startedAt = nowIso();
  engine.timestamps.stages.isolation = { startedAt, endedAt: null };
  const connection = await spawnConnection(engine, stage);
  try {
    const capabilities = await initializeConnection(connection, engine);
    const sessionA = await connection.request("session/new", { cwd: engine.workspace, mcpServers: [] });
    const sessionB = await connection.request("session/new", { cwd: engine.workspace, mcpServers: [] });
    const sessionAId = sessionA?.sessionId ?? null;
    const sessionBId = sessionB?.sessionId ?? null;
    engine.sessionIds.isolationA = sessionAId;
    engine.sessionIds.isolationB = sessionBId;
    recordCheck(
      engine,
      stage,
      "isolation.sessions_distinct",
      sessionAId && sessionBId && sessionAId !== sessionBId ? "PASS" : "FAIL",
      sessionAId && sessionBId && sessionAId !== sessionBId
        ? "同一进程内创建了 A、B 两个不同会话"
        : "未能创建两个不同的会话",
      { sessionAId, sessionBId },
    );
    if (!sessionAId || !sessionBId) {
      ensureStageSkips(engine, stage, "会话创建失败，隔离检查未执行");
      recordCheck(engine, stage, "isolation.completed", "FAIL", "无法创建 A/B 会话", {});
      return;
    }

    await recordResourceSample(engine, stage, "after_establish", connection.pid, connection.runId);

    const readyTurn = await promptTurn(connection, engine, sessionBId, "请只回复 ready。", "isolation-b-ready");
    const ready = readyTurn.assistantText.toLowerCase().includes("ready");
    recordCheck(
      engine,
      stage,
      "isolation.b_ready_before_close",
      ready ? "PASS" : "FAIL",
      ready ? "A close 前 B 能正常应答" : "A close 前 B 未能正常应答",
      replyDetails(readyTurn),
    );

    let closeSucceeded = false;
    if (capabilities.supportsClose) {
      try {
        await connection.request("session/close", { sessionId: sessionAId });
        closeSucceeded = true;
        recordCheck(engine, stage, "isolation.a_close_succeeded", "PASS", "session/close(A) 返回成功", {
          sessionAId,
        });
      } catch (error) {
        recordCheck(
          engine,
          stage,
          "isolation.a_close_succeeded",
          "FAIL",
          `session/close(A) 失败：${shortError(error)}`,
          { sessionAId, error: describeError(error) },
        );
      }
    } else {
      recordCheck(
        engine,
        stage,
        "isolation.a_close_succeeded",
        "SKIP",
        "Agent 未宣告 sessionCapabilities.close；未执行 A close",
        { sessionAId },
      );
    }

    await recordResourceSample(engine, stage, "after_a_close", connection.pid, connection.runId);

    if (!closeSucceeded) {
      recordCheck(
        engine,
        stage,
        "isolation.b_works_after_a_close",
        "SKIP",
        "未成功执行 A close，无法验证 A 关闭后 B 是否仍工作",
        { sessionBId },
      );
      recordCheck(
        engine,
        stage,
        "isolation.shared_process_survived",
        "SKIP",
        "未成功执行 A close，无法验证共享进程是否存活",
        { pid: connection.pid },
      );
      recordCheck(
        engine,
        stage,
        "isolation.resource_samples_recorded",
        engine.resourceSamples.some((sample) => sample.stage === stage) ? "PASS" : "SKIP",
        "已记录隔离阶段进程树资源采样",
        { samples: engine.resourceSamples.filter((sample) => sample.stage === stage) },
      );
      finalizeStage(engine, stage);
      return;
    }

    const bTurn = await promptTurn(connection, engine, sessionBId, "请计算 17*23，只回复数字。", "isolation-b-after-close");
    const bWorks = /\b391\b/.test(bTurn.assistantText);
    recordCheck(
      engine,
      stage,
      "isolation.b_works_after_a_close",
      bWorks ? "PASS" : "FAIL",
      bWorks ? "A close 后 B 仍正常返回正确答案" : "A close 后 B 未能正常返回正确答案",
      { ...replyDetails(bTurn), sessionBId, expected: "391" },
    );

    const afterB = await recordResourceSample(engine, stage, "after_b_turn", connection.pid, connection.runId);
    // 采样失败时 processCount 为 null，但 rootPresent 仍由存活探测给出真实结果。
    const alive = afterB.rootPresent === true && (afterB.processCount === null || afterB.processCount >= 1);
    recordCheck(
      engine,
      stage,
      "isolation.shared_process_survived",
      alive ? "PASS" : "FAIL",
      alive ? "A close 后共享适配器进程仍存活" : "A close 后共享适配器进程已消失",
      { pid: connection.pid, afterB },
    );

    recordCheck(
      engine,
      stage,
      "isolation.resource_samples_recorded",
      engine.resourceSamples.some((sample) => sample.stage === stage) ? "PASS" : "SKIP",
      "已记录 A close 前后及 B 交互后的进程树资源采样",
      { samples: engine.resourceSamples.filter((sample) => sample.stage === stage) },
    );

    engine.isolation = {
      sessionAId,
      sessionBId,
      aClosed: closeSucceeded,
      bWorksAfterClose: bWorks,
      sharedProcessAlive: alive,
      processCountAfterB: afterB.processCount,
      conclusion:
        closeSucceeded && bWorks && alive
          ? "A 的关闭没有终止共享进程：A close 后 B 正常返回且进程仍存活"
          : "未能同时满足 A close、B 正常返回与共享进程存活；隔离未验证或失败",
    };
    finalizeStage(engine, stage);
  } catch (error) {
    ensureStageSkips(engine, stage, `阶段异常中断：${shortError(error)}`);
    if (!engine.checkIds.has("isolation.completed")) {
      recordCheck(engine, stage, "isolation.completed", "FAIL", `isolation 阶段异常：${shortError(error)}`, {
        error: describeError(error),
      });
    }
    throw error;
  } finally {
    engine.timestamps.stages.isolation.endedAt = nowIso();
    await connection.terminate("isolation-complete").catch(() => {});
  }
}

/**
 * release 场景：会话建立后、A close 后、进程退出后的进程树资源采样。
 */
async function runRelease(engine) {
  const stage = "release";
  const startedAt = nowIso();
  engine.timestamps.stages.release = { startedAt, endedAt: null };
  const connection = await spawnConnection(engine, stage);
  let sessionId = null;
  try {
    const capabilities = await initializeConnection(connection, engine);
    const created = await connection.request("session/new", { cwd: engine.workspace, mcpServers: [] });
    sessionId = created?.sessionId ?? null;
    engine.sessionIds.release = sessionId;
    recordCheck(
      engine,
      stage,
      "release.session_established",
      sessionId ? "PASS" : "FAIL",
      sessionId ? "已建立用于释放采样的会话" : "session/new 未返回 sessionId",
      { sessionId },
    );
    if (!sessionId) {
      ensureStageSkips(engine, stage, "会话建立失败，释放采样未执行");
      recordCheck(engine, stage, "release.completed", "FAIL", "无法建立会话", {});
      return;
    }

    await promptTurn(connection, engine, sessionId, "请只回复 ok。", "release-baseline");
    const before = await recordResourceSample(engine, stage, "after_session", connection.pid, connection.runId);

    let closeSucceeded = false;
    if (capabilities.supportsClose) {
      try {
        await connection.request("session/close", { sessionId });
        closeSucceeded = true;
        recordCheck(engine, stage, "release.a_close_succeeded", "PASS", "释放采样会话的 session/close 返回成功", {
          sessionId,
        });
      } catch (error) {
        recordCheck(
          engine,
          stage,
          "release.a_close_succeeded",
          "FAIL",
          `session/close 失败：${shortError(error)}`,
          { sessionId, error: describeError(error) },
        );
      }
    } else {
      recordCheck(
        engine,
        stage,
        "release.a_close_succeeded",
        "SKIP",
        "Agent 未宣告 sessionCapabilities.close；未执行 close",
        { sessionId },
      );
    }

    if (closeSucceeded) await delay(1000);
    const afterClose = await recordResourceSample(engine, stage, "after_a_close", connection.pid, connection.runId);

    const beforeMb = before.workingSetMb;
    const afterMb = afterClose.workingSetMb;
    const dropMb =
      typeof beforeMb === "number" && typeof afterMb === "number" ? Math.round((beforeMb - afterMb) * 10) / 10 : null;
    const meaningfulDrop =
      closeSucceeded &&
      typeof beforeMb === "number" &&
      typeof afterMb === "number" &&
      dropMb >= Math.max(16, beforeMb * 0.05);
    if (meaningfulDrop) {
      recordCheck(
        engine,
        stage,
        "release.memory_after_close",
        "PASS",
        "close 成功且进程树 WorkingSet 出现可观测下降；仍不据此推断全部堆已归还",
        { beforeMb, afterMb, dropMb, note: "WorkingSet 下降与 close 请求证据一致，但分配器可能保留堆" },
      );
    } else if (!closeSucceeded) {
      recordCheck(
        engine,
        stage,
        "release.memory_after_close",
        "SKIP",
        "close 未成功，不能判断内存是否释放",
        { beforeMb, afterMb, dropMb },
      );
    } else {
      recordCheck(
        engine,
        stage,
        "release.memory_after_close",
        "SKIP",
        "单次 WorkingSet 采样未显著下降；分配器可能保留堆，不能据此判断泄漏或释放成功",
        { beforeMb, afterMb, dropMb, note: "RSS/WorkingSet 受分配器保留堆影响，单次采样不是泄漏判据" },
      );
    }

    const exit = await connection.terminate("release");
    const afterExit = await recordResourceSample(engine, stage, "after_exit", connection.pid, connection.runId);
    const terminationStatus = treeTerminationStatus(afterExit);
    recordCheck(
      engine,
      stage,
      "release.process_tree_terminated",
      terminationStatus,
      terminationStatus === "PASS"
        ? "进程退出后进程树已清空"
        : terminationStatus === "SKIP"
          ? "进程树采样失败，无法确认是否清空"
          : "进程退出后仍有进程树残留",
      {
        pid: connection.pid,
        exitCode: exit.code,
        signal: exit.signal,
        afterExit,
      },
    );
    recordCheck(
      engine,
      stage,
      "release.no_orphan_children",
      terminationStatus,
      terminationStatus === "PASS"
        ? "没有遗留后台子进程"
        : terminationStatus === "SKIP"
          ? "进程树采样失败，无法确认是否遗留子进程"
          : "存在遗留子进程",
      { afterExit },
    );

    engine.release = {
      beforeClose: before,
      afterClose,
      afterExit,
      closeSucceeded,
      memoryNote: "WorkingSet 为进程树各进程 WorkingSet 之和，含共享内存；分配器可能保留堆，单次采样不作泄漏判据",
    };

    finalizeStage(engine, stage);
  } catch (error) {
    ensureStageSkips(engine, stage, `阶段异常中断：${shortError(error)}`);
    if (!engine.checkIds.has("release.completed")) {
      recordCheck(engine, stage, "release.completed", "FAIL", `release 阶段异常：${shortError(error)}`, {
        error: describeError(error),
      });
    }
    throw error;
  } finally {
    engine.timestamps.stages.release.endedAt = nowIso();
    await connection.terminate("release-cleanup").catch(() => {});
  }
}

function buildReport(engine, exitCode) {
  const finishedAt = nowIso();
  const checks = engine.checks;
  const failed = checks.filter((check) => check.status === "FAIL");
  const skipped = checks.filter((check) => check.status === "SKIP");
  const passed = checks.filter((check) => check.status === "PASS");
  return {
    schemaVersion: 1,
    tool: { name: TOOL_NAME, version: TOOL_VERSION },
    runId: engine.runId,
    scenario: engine.options.scenario,
    startedAt: engine.startedAt,
    finishedAt,
    durationMs: Date.parse(finishedAt) - Date.parse(engine.startedAt),
    exitCode,
    summary: {
      total: checks.length,
      pass: passed.length,
      fail: failed.length,
      skip: skipped.length,
      failedChecks: failed.map((check) => check.id),
      skippedChecks: skipped.map((check) => check.id),
    },
    environment: {
      platform: process.platform,
      release: os.release(),
      arch: process.arch,
      nodeVersion: process.version,
      cwd: process.cwd(),
      adapter: {
        path: engine.adapter.path,
        source: engine.adapter.source,
        launch: engine.launch,
        package: engine.adapterPackage
          ? { name: engine.adapterPackage.name, version: engine.adapterPackage.version, packageJson: engine.adapterPackage.path }
          : null,
        entrySha256: safeFileSha256(engine.adapter.path),
        entryBytes: safeFileSize(engine.adapter.path),
      },
      codex: {
        packageVersion: engine.codexPackage?.version ?? null,
        packageJson: engine.codexPackage?.path ?? null,
        cliVersion: engine.codexCliVersion,
      },
      dataDir: engine.dataDir,
      dataDirAutoCreated: engine.options.dataDir ? false : true,
      workspace: engine.workspace,
      codexHome: engine.codexHome,
      realCodexHome: engine.codexHomeInfo.realHome,
      codexHomeCopied: engine.codexHomeInfo.copied,
      codexHomePresent: engine.codexHomeInfo.present,
      codexHomeMissing: engine.codexHomeInfo.missing,
      transcriptPath: engine.transcriptPath,
      statePath: engine.statePath,
      jsonOut: engine.jsonOut,
      timeoutMs: engine.options.timeoutMs,
      sideEffectTimeoutMs: engine.options.sideEffectTimeoutMs,
      keep: engine.options.keep,
    },
    capabilities: engine.capabilities,
    checks,
    requestLedger: engine.ledger,
    resourceSamples: engine.resourceSamples,
    sessionIds: engine.sessionIds,
    timestamps: engine.timestamps,
    runs: engine.runs,
    sideEffect: engine.sideEffect,
    isolation: engine.isolation,
    release: engine.release,
    server: serverSummary(engine),
    notes: engine.notes,
  };
}

function safeFileSha256(path) {
  try {
    return sha256(readFileSync(path));
  } catch {
    return null;
  }
}

function safeFileSize(path) {
  try {
    return statSync(path).size;
  } catch {
    return null;
  }
}

function serverSummary(engine) {
  const summary = {
    permissionRequests: 0,
    fsReads: 0,
    fsWrites: 0,
    fsDenied: 0,
    terminalCreates: 0,
    terminalDenied: 0,
    unknownClientMethods: {},
    protocolErrors: 0,
    inboundRedactions: 0,
    authNotifications: 0,
    authUpdatesSkipped: 0,
  };
  for (const run of engine.runs) {
    const counters = run.counters;
    if (!counters) continue;
    summary.permissionRequests += counters.permissionRequests ?? 0;
    summary.fsReads += counters.fsReads ?? 0;
    summary.fsWrites += counters.fsWrites ?? 0;
    summary.fsDenied += counters.fsDenied ?? 0;
    summary.terminalCreates += counters.terminalCreates ?? 0;
    summary.terminalDenied += counters.terminalDenied ?? 0;
    summary.protocolErrors += counters.protocolErrors ?? 0;
    summary.inboundRedactions += counters.inboundRedactions ?? 0;
    summary.authNotifications += counters.authNotifications ?? 0;
    summary.authUpdatesSkipped += counters.authUpdatesSkipped ?? 0;
    for (const [method, count] of Object.entries(counters.unknownClientMethods ?? {})) {
      summary.unknownClientMethods[method] = (summary.unknownClientMethods[method] ?? 0) + count;
    }
  }
  return summary;
}

function printSummary(engine, exitCode) {
  const lines = [];
  lines.push("");
  lines.push(`探针结果：runId=${engine.runId} scenario=${engine.options.scenario} exitCode=${exitCode}`);
  for (const check of engine.checks) {
    lines.push(`  ${check.status.padEnd(4)} ${check.id} — ${check.summary}`);
  }
  const failed = engine.checks.filter((check) => check.status === "FAIL");
  const skipped = engine.checks.filter((check) => check.status === "SKIP");
  lines.push(
    `汇总：PASS ${engine.checks.filter((check) => check.status === "PASS").length} / ` +
      `FAIL ${failed.length} / SKIP ${skipped.length}`,
  );
  lines.push(`报告：${engine.jsonOut}`);
  lines.push(`转录：${engine.transcriptPath}`);
  lines.push(`state：${engine.statePath}`);
  process.stderr.write(`${lines.join("\n")}\n`);
  for (const check of failed) {
    process.stderr.write(`FAIL ${check.id}: ${check.summary}\n`);
  }
}

function scrubIsolatedCodexHome(engine) {
  if (engine.options.keep) {
    engine.progress("finalize", "codex-home-kept", { path: engine.codexHome });
    return;
  }
  const info = engine.codexHomeInfo;
  for (const name of info.copied) {
    try {
      unlinkSync(join(info.home, name));
    } catch {
      // 已不存在。
    }
  }
  if (info.created && isInside(engine.dataDir, info.home)) {
    try {
      rmSync(info.home, { recursive: true, force: true });
      engine.progress("finalize", "codex-home-scrubbed", { path: info.home });
    } catch (error) {
      engine.notes.push(`清理隔离 codex-home 失败：${shortError(error)}`);
    }
  }
}

async function writeReport(engine, exitCode) {
  const report = buildReport(engine, exitCode);
  mkdirSync(dirname(engine.jsonOut), { recursive: true });
  writeFileSync(engine.jsonOut, `${JSON.stringify(report, null, 2)}\n`);
  engine.transcript({ type: "run-end", exitCode, report: engine.jsonOut, summary: report.summary });
  return report;
}

async function closeTranscript(engine) {
  await new Promise((resolveClose) => {
    if (!engine.transcriptStream) {
      resolveClose();
      return;
    }
    engine.transcriptStream.end(() => resolveClose());
  });
}

function emergencyCleanup() {
  if (!engineState) return;
  for (const connection of engineState.connections) {
    for (const terminal of connection.terminals.values()) {
      killProcessTreeSync(terminal.child?.pid);
    }
    killProcessTreeSync(connection.pid);
  }
}

async function main() {
  const argv = process.argv.slice(2);
  if (argv.includes("--help") || argv.includes("-h")) {
    process.stdout.write(`${HELP_TEXT}\n`);
    return 0;
  }

  const options = parseArgs(argv);
  if (options.help) {
    process.stdout.write(`${HELP_TEXT}\n`);
    return 0;
  }
  if (options.selfTest) {
    // 只做脱敏断言：不读凭据、不建目录、不启动子进程。
    return runRedactionSelfTest();
  }

  const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
  const adapter = resolveAdapter(options.adapter);
  const realHome = realCodexHome();
  const data = prepareDataDir(options, repoRoot, realHome);
  const home = prepareCodexHome(options, data.dataDir, realHome, repoRoot);
  const adapterPackage = detectAdapterVersion(adapter.path);
  const codexPackage = detectCodexPackageVersion(adapter.path);
  const codexCliVersion = detectCodexCliVersion();
  const runId = `${new Date().toISOString().replace(/[-:.TZ]/g, "").slice(0, 14)}-${randomBytes(4).toString("hex")}`;
  const engine = createEngine({
    options,
    repoRoot,
    data,
    home,
    adapter,
    launch: buildLaunch(adapter.path),
    adapterPackage,
    codexPackage,
    codexCliVersion,
    runId,
  });
  engineState = engine;

  process.on("SIGINT", () => {
    engine.interrupted = true;
    emergencyCleanup();
    process.exit(130);
  });
  process.on("SIGTERM", () => {
    engine.interrupted = true;
    emergencyCleanup();
    process.exit(130);
  });

  engine.progress("setup", "ready", {
    dataDir: engine.dataDir,
    codexHome: engine.codexHome,
    adapter: engine.adapter,
    adapterPackage: engine.adapterPackage?.version ?? null,
    codexPackage: engine.codexPackage?.version ?? null,
    launch: engine.launch,
  });

  let exitCode = 0;
  let liveConnection = null;
  let recallOutcome = null;
  let persistedState = null;
  const scenarios = options.scenario === "all" ? [...SCENARIOS] : [options.scenario];

  try {
    recordCheck(engine, "environment", "environment.adapter_resolved", "PASS", "已解析适配器入口", {
      path: engine.adapter.path,
      source: engine.adapter.source,
      launchKind: engine.launch.kind,
      entrySha256: safeFileSha256(engine.adapter.path),
    });
    recordCheck(
      engine,
      "environment",
      "environment.adapter_version",
      engine.adapterPackage?.version ? "PASS" : "SKIP",
      engine.adapterPackage?.version
        ? `适配器包版本 ${engine.adapterPackage.name}@${engine.adapterPackage.version}`
        : "未能从适配器包 package.json 读取版本",
      {
        package: engine.adapterPackage,
        codexPackageVersion: engine.codexPackage?.version ?? null,
        codexCliVersion: engine.codexCliVersion,
      },
    );
    recordCheck(
      engine,
      "environment",
      "environment.isolated_data_dir",
      "PASS",
      "data-dir 不在仓库、用户家目录或真实 CODEX_HOME 内",
      { dataDir: engine.dataDir, repoRoot: engine.repoRoot },
    );
    recordCheck(
      engine,
      "environment",
      "environment.codex_home_isolated",
      "PASS",
      "codex-home 与真实 CODEX_HOME 分离",
      { codexHome: engine.codexHome, realCodexHome: engine.codexHomeInfo.realHome },
    );
    const hasAuth =
      engine.codexHomeInfo.copied.includes("auth.json") || engine.codexHomeInfo.present.includes("auth.json");
    recordCheck(
      engine,
      "environment",
      "environment.credentials_copied",
      hasAuth ? "PASS" : "SKIP",
      hasAuth
        ? "已复制 auth.json/config.toml（只记录文件名，未读取内容）"
        : "真实 CODEX_HOME 中未找到 auth.json；若未通过其他方式登录，prompt 会失败",
      {
        copied: engine.codexHomeInfo.copied,
        present: engine.codexHomeInfo.present,
        missing: engine.codexHomeInfo.missing,
      },
    );

    // 只有单独 restart 才加载历史 state：all 场景中的 recall 会生成新标记，
    // 预载旧 prompt 文本会把“同一句追问”误判成旧输入重发。
    if (options.scenario === "restart") {
      persistedState = readState(engine);
      registerStateSecrets(engine, persistedState);
    }

    for (const scenario of scenarios) {
      if (!engine.timestamps.stages[scenario]) {
        engine.timestamps.stages[scenario] = { startedAt: nowIso(), endedAt: null };
      }
      if (scenario === "recall") {
        try {
          recallOutcome = await runRecall(engine);
          liveConnection = recallOutcome.connection;
          if (!recallOutcome.ok) engine.notes.push("recall 未满足门槛，后续依赖 restart 将跳过");
        } catch (error) {
          engine.progress("recall", "stage-error", { error: shortError(error) });
          recordStageFailure(engine, "recall", error);
        }
        continue;
      }

      if (scenario === "restart") {
        const state = recallOutcome?.state ?? persistedState;
        if (!state) {
          if (options.scenario === "restart") {
            const preconditionError = new ProbeError(
              `restart 需要同一 --data-dir（${engine.dataDir}）中已完成的 recall（close+resume）。` +
                "请先运行 --scenario all，或在 recall 时使用 --keep 后再跨次运行 restart。",
            );
            recordStageFailure(engine, "restart", preconditionError);
            throw preconditionError;
          }
          ensureStageSkips(engine, "restart", "缺少 recall 会话状态；restart 检查未执行");
          finalizeStage(engine, "restart");
          continue;
        }
        if (!state.ready && options.scenario === "restart") {
          const preconditionError = new ProbeError(
            "上次 recall 未完成 close+resume（state.ready=false），不能单独执行 restart。" +
              "请重新运行 --scenario all，并在需要跨次运行 restart 时使用 --keep。",
          );
          recordStageFailure(engine, "restart", preconditionError);
          throw preconditionError;
        }
        if (options.scenario === "restart" && (!state.codexHome || !existsSync(state.codexHome))) {
          const preconditionError = new ProbeError(
            `上次 recall 的隔离 codex-home 已被清理（${state.codexHome ?? "未知路径"}）。` +
              "跨次运行 restart 必须在 recall 时使用 --keep。",
          );
          recordStageFailure(engine, "restart", preconditionError);
          throw preconditionError;
        }
        try {
          await runRestart(engine, { liveConnection, state });
        } catch (error) {
          engine.progress("restart", "stage-error", { error: shortError(error) });
          recordStageFailure(engine, "restart", error);
        } finally {
          liveConnection = null;
        }
        continue;
      }

      if (scenario === "isolation") {
        try {
          await runIsolation(engine);
        } catch (error) {
          engine.progress("isolation", "stage-error", { error: shortError(error) });
          recordStageFailure(engine, "isolation", error);
        }
        continue;
      }

      if (scenario === "release") {
        try {
          await runRelease(engine);
        } catch (error) {
          engine.progress("release", "stage-error", { error: shortError(error) });
          recordStageFailure(engine, "release", error);
        }
      }
    }
  } catch (error) {
    engine.fatal = error;
    engine.notes.push(`未预期错误：${shortError(error)}`);
    process.stderr.write(`探针未预期错误：${shortError(error)}\n`);
    if (!engine.checkIds.has("probe.fatal")) {
      recordCheck(engine, "fatal", "probe.fatal", "FAIL", `探针致命错误：${shortError(error)}`, {
        error: describeError(error),
      });
    }
  } finally {
    if (liveConnection) {
      const exit = await liveConnection.terminate("orchestrator-shutdown").catch(() => null);
      if (recallOutcome?.state) {
        recallOutcome.state.lastRun = {
          pid: liveConnection.pid,
          exitCode: exit?.code ?? liveConnection.run.exitCode,
          signal: exit?.signal ?? liveConnection.run.signal,
          terminatedAt: nowIso(),
          treeAfter: liveConnection.run.treeAfter ?? null,
        };
        writeState(engine, recallOutcome.state);
      }
    }
    for (const connection of [...engine.connections]) {
      await connection.terminate("final-cleanup").catch(() => {});
    }

    // 每个阶段都必须有 completed 结论，避免报告缺少结论。
    for (const scenario of SCENARIOS) {
      if (!scenariosRun(options.scenario).includes(scenario)) continue;
      if (!engine.checkIds.has(`${scenario}.completed`)) {
        ensureStageSkips(engine, scenario, "阶段未产生结论");
        recordCheck(engine, scenario, `${scenario}.completed`, "SKIP", "阶段未执行或未产生结论", {});
      }
      if (!engine.timestamps.stages[scenario]) {
        engine.timestamps.stages[scenario] = { startedAt: null, endedAt: nowIso() };
      } else if (!engine.timestamps.stages[scenario].endedAt) {
        engine.timestamps.stages[scenario].endedAt = nowIso();
      }
    }

    const protocolErrorCount = engine.runs.reduce(
      (sum, run) => sum + (run.counters?.protocolErrors ?? 0),
      0,
    );
    recordCheck(
      engine,
      "protocol",
      "protocol.no_invalid_messages",
      protocolErrorCount > 0 ? "FAIL" : "PASS",
      protocolErrorCount > 0
        ? `收到 ${protocolErrorCount} 条无法解析的 ACP 入站消息`
        : "未收到无法解析的 ACP 入站消息",
      { protocolErrorCount },
    );

    const failed = engine.checks.some((check) => check.status === "FAIL");
    exitCode = engine.interrupted ? 130 : engine.fatal ? 2 : failed ? 1 : 0;
    try {
      await writeReport(engine, exitCode);
    } catch (error) {
      process.stderr.write(`写报告失败：${shortError(error)}\n`);
      exitCode = 2;
    }
    await closeTranscript(engine);
    scrubIsolatedCodexHome(engine);
    printSummary(engine, exitCode);
  }

  return exitCode;
}

function scenariosRun(scenario) {
  return scenario === "all" ? [...SCENARIOS] : [scenario];
}

main()
  .then((code) => {
    process.exitCode = code;
    // 给 stderr/报告文件一点排空时间；所有子进程已在上面的 finally 中终止。
    setImmediate(() => process.exit(code));
  })
  .catch((error) => {
    const message = error instanceof ProbeUsageError || error instanceof ProbeError ? error.message : shortError(error);
    process.stderr.write(`probe: ${message}\n`);
    if (error instanceof ProbeUsageError) {
      process.stderr.write("使用 --help 查看用法。\n");
    }
    process.exit(2);
  });
