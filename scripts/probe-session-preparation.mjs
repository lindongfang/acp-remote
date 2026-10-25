#!/usr/bin/env node
/**
 * 会话准备（首条消息提交之前）的**真实 Codex** 跨进程探针（`web-session-preparation` 任务 8.2；PV8）。
 *
 * 与 `scripts/probe-session-lifecycle.mjs`（直接驱动 ACP 适配器）的分工：本探针在**真实 Daemon** 的
 * 组合根上走完整链路——隔离 data_dir / 隔离 store / 隔离节点身份 / 真实 TLS / 真实 Sync 握手与
 * 配对、`session.preparation` 控制帧、`session.start` 与 `session.read`——而 Agent 一侧是**真实的**
 * `codex-acp` 适配器。它回答的是「准备阶段跨进程后仍是同一条准备」这一类问题：
 *
 *   1. allocate    Owner 分配最终 sessionId 并签发凭据；模式/配置是真实 Agent 回报的值；
 *                 准备期间 `session.read` 必须回 `session.not_found`（没有正式会话行）；
 *   2. configure   用户改过的模式/配置真的下发到了适配器（`session/set_mode`/`session/set_config_option`）；
 *   3. restart     强制终止 Daemon（连同 Agent 进程树）后，用同一 data_dir/codex-home 重启；同一
 *                 receipt 的 `restore` 回到同一 sessionId、新的运行期资源代，适配器是**新进程**；
 *   4. first       `session.start` 的 `initial` 才是第一条 prompt：适配器只收到**恰好一次** prompt，
 *                 内容与提交正文逐字相同（没有隐藏 prompt）；正文在 `session.read` 里可读；Agent 回复
 *                 带有本次随机标记；`command.status` 的首条投影落到 terminal。
 *
 * 硬判据（不满足即 FAIL，不能写成 PASS）：
 *   - 准备期间/重启之后、显式 `session.start` 之前出现任何 `session/prompt`：自动派发，FAIL；
 *   - 整次运行出现任何 `session/resume`：退回旧正式恢复，FAIL；
 *   - 重启后的适配器进程 pid 与终止前相同，或旧进程仍存活：进程树未真正消失，FAIL；
 *   - `restore` 返回的 sessionId 与 allocate 的不同、或返回 `stale`（有效凭据的 restore 永不 stale）：FAIL；
 *   - 适配器收到的 prompt 正文与提交正文不逐字相同（隐藏 prompt/改写）：FAIL；
 *   - 首个 prompt 早于 `session.start` 的 accepted 结果之外出现第二条 prompt：重复执行，FAIL。
 *
 * 安全与隔离：
 *   - `--data-dir` 必须是独立目录：拒绝仓库根、用户家目录、真实 `CODEX_HOME`、仓库工作树内路径、
 *     已存在的 `.codex` 目录；`--codex-home` 拒绝真实 `CODEX_HOME` 及其内部路径。
 *   - 只从真实 `CODEX_HOME` 复制 `auth.json` 与 `config.toml`（若存在）；脚本**从不读取、打印、
 *     写入报告或转录**这些文件的内容，只记录文件名。
 *   - 配对码 / 配对秘密 / 设备私钥 / 节点私钥都只在内存里；报告只记录配对标识与设备标识。
 *     `--self-test` 之后还有一条运行期断言：报告文本里不得出现配对秘密或设备私钥。
 *   - 适配器 stderr 只写入 `<data-dir>/agent-*.stderr.log`；stdout/stderr 是 ACP 与日志通道，
 *     脚本不会把它们的原文写进报告。
 *   - 默认在结束时删除隔离 codex-home（含凭据副本与 Codex 会话状态）；`--keep` 保留完整目录。
 *   - 探针不会重启、停止或读取用户正在运行的 Daemon；它用独立 data_dir 与独立端口启动自己的实例。
 *
 * 退出码：0 = 无 FAIL（允许 SKIP）；1 = 至少一个 FAIL；2 = 参数/环境/前置致命错误；130 = 中断。
 *
 * 本文件只用 Node 内置模块，不依赖仓库 `package.json`。
 */

import { spawn, spawnSync } from "node:child_process";
import { createHash, createHmac, randomBytes, randomUUID } from "node:crypto";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  openSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { createServer } from "node:net";
import os from "node:os";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import tls from "node:tls";
import https from "node:https";

const TOOL_NAME = "probe-session-preparation";
const TOOL_VERSION = "1.0.0";
const PROTOCOL_VERSION = 1;
const KNOWN_ADAPTER_PATH = "D:/tmp/wp7-agents/tools/node_modules/@agentclientprotocol/codex-acp/dist/index.js";
const DEFAULT_TIMEOUT_MS = 240_000;
const SHORT_TIMEOUT_MS = 60_000;
const WINDOWS = process.platform === "win32";
const WS_SUBPROTOCOL = "acp-remote.sync.v1.json";
const WS_PATH = "/sync/v1";

/** 业务连接请求的 feature 集合（服务端做交集；准备 feature 只在准备端口齐备时出现）。 */
const SUPPORTED_FEATURES = [
  "core.snapshot.v1",
  "core.command-status.v1",
  "core.event-ack.v1",
  "acp.raw-payload.v1",
  "resource.remote-origin.v1",
  "core.local-catalog.v1",
  "core.session-create.v1",
  "core.session-history.v1",
  "core.event-origin.v1",
  "core.session-activity.v1",
  "core.session-title.v1",
  "core.local-session-lifecycle.v1",
  "core.attachment-transfer.v1",
  "core.session-preparation.v1",
];

const PREPARATION_FEATURE = "core.session-preparation.v1";

const HELP_TEXT = `用法：node scripts/probe-session-preparation.mjs [选项]

在隔离的 data_dir / codex-home / 节点身份里启动**真实 acp-remote Daemon**（真实 TLS）与
**真实 codex-acp** 适配器，走真实 Sync 握手，验证「首条消息提交之前」的准备跨进程行为：
allocate → 真实模式/配置 → configure → 强制终止并重启 → 同一 ID restore →
session.start → session.read → command.status。默认不读取用户已配置的会话，也不操作
用户正在运行的 Daemon。

选项：
  --server <path>         acp-remote 可执行文件。缺省依次取环境变量 ACPR_SERVER、
                          <repo>/target/debug/acp-remote.exe（Windows）或 acp-remote（其它平台）。
  --adapter <path>        codex-acp 的 dist/index.js（或 codex-acp 可执行文件）。缺省依次取
                          ACPR_CODEX_ACP_ADAPTER、PATH 中的 codex-acp、已知路径
                          ${KNOWN_ADAPTER_PATH}。
  --data-dir <path>       隔离数据目录。缺省在系统临时目录下创建 acpr-session-preparation-<random>。
                          拒绝仓库根、用户家目录、真实 CODEX_HOME、仓库工作树内路径与 .codex 目录。
  --codex-home <path>     隔离 Codex home。缺省 <data-dir>/codex-home。只从真实 CODEX_HOME
                          复制 auth.json 与 config.toml（若存在）；只记录文件名，不读取或输出内容。
  --cert <path>           TLS 证书 PEM。缺省用 openssl 生成自签证书（需要 PATH 中的 openssl）。
  --key <path>            TLS 私钥 PEM。缺省同上。
  --workspace <path>      准备使用的工作目录。缺省 <data-dir>/workspace。
  --port <n>              TLS 监听端口。缺省为内核分配的空闲端口（仅 listen 于 127.0.0.1）。
  --json-out <path>       报告 JSON 路径。缺省 <data-dir>/report.json。
  --timeout-ms <n>        单个请求/等待（含首个真实 turn）超时（毫秒）。缺省 ${DEFAULT_TIMEOUT_MS}。
  --keep                  保留隔离 codex-home 与全部临时产物（默认删除 codex-home）。
  --self-test             只跑内置密码学/transcript 断言（读取仓库 fixtures，不建目录、不起进程）；
                          全部通过退出 0，否则退出 1。
  --help, -h              只打印本说明并退出 0；不创建目录、不启动子进程。

运行产物：
  <data-dir>/agent-evidence.ndjson  Agent 边界证据：进程 pid、ACP 方法计数、prompt 摘要
                                    （只记录长度/SHA-256/是否含本次标记/是否与提交正文逐字相同）。
  <data-dir>/agent-*.stderr.log     适配器 stderr（只落盘，不回显）。
  <data-dir>/daemon-*.stderr.log    Daemon stderr（只落盘）。
  <data-dir>/daemon-*.log           Daemon 结构化日志（logging.file）。
  <json-out>                        结构化报告（默认 <data-dir>/report.json）。

退出码：0 无 FAIL；1 存在 FAIL；2 参数/环境/前置致命错误；130 中断。`;

class ProbeUsageError extends Error {}
class ProbeError extends Error {}
class ProtocolError extends Error {}

const sleep = (ms) => new Promise((done) => setTimeout(done, ms));
const nowIso = () => new Date().toISOString();

function sha256Hex(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function b64url(bytes) {
  return Buffer.from(bytes).toString("base64url");
}

function fromB64url(text) {
  return Buffer.from(String(text), "base64url");
}

function shortError(error) {
  if (error instanceof Error) return error.message;
  return String(error);
}

function describeError(error) {
  return error instanceof Error ? error.stack ?? error.message : String(error);
}

// ---------------------------------------------------------------------------------------------
// transcript 编码（`docs/SYNC_PROTOCOL.md` §6.2/§6.3）
// ---------------------------------------------------------------------------------------------

const DOMAIN = {
  pairingProof: "acp-remote/pairing-proof/v1",
  pairingHostProof: "acp-remote/pairing-host-proof/v1",
  pairingStatus: "acp-remote/pairing-status/v1",
  hostChallenge: "acp-remote/host-challenge/v1",
  deviceProof: "acp-remote/device-proof/v1",
  preparation: "acp-remote/session-preparation/v1",
};

const TAG = {
  protocolVersion: 1,
  hostId: 2,
  deviceId: 3,
  pairingId: 4,
  pairingExpiresAt: 5,
  canonicalOrigin: 6,
  hostPublicKey: 7,
  devicePublicKey: 8,
  clientNonce: 9,
  serverNonce: 10,
  connectionId: 11,
  negotiatedFeatures: 12,
  pairingRequestId: 13,
  deviceName: 14,
  clientKind: 15,
  requestNonce: 16,
  ownerNodeId: 17,
  storeEpoch: 18,
  origin: 19,
  tabNonce: 20,
  sessionId: 21,
  agentId: 22,
  normalizedWorkspacePath: 23,
  workspaceSource: 24,
};

/** domain → 升序字段 tag 集合（§6.3 的「各用途字段集合」）。 */
const DOMAIN_FIELDS = {
  [DOMAIN.pairingProof]: [1, 2, 3, 4, 5, 6, 8, 9, 14, 15],
  [DOMAIN.pairingHostProof]: [1, 2, 3, 4, 6, 7, 8, 9, 10, 13],
  [DOMAIN.pairingStatus]: [1, 2, 3, 4, 13, 16],
  [DOMAIN.hostChallenge]: [1, 2, 3, 6, 9, 10, 11, 12],
  [DOMAIN.deviceProof]: [1, 2, 3, 6, 9, 10, 11, 12],
  [DOMAIN.preparation]: [1, 3, 17, 18, 19, 20, 21, 22, 23, 24],
};

function uuidBytes(text) {
  const compact = String(text).replaceAll("-", "");
  if (compact.length !== 32 || !/^[0-9a-fA-F]{32}$/.test(compact)) {
    throw new ProbeError(`不是规范 UUID：${text}`);
  }
  return Buffer.from(compact, "hex");
}

function u16be(value) {
  const bytes = Buffer.alloc(2);
  bytes.writeUInt16BE(value, 0);
  return bytes;
}

function u32be(value) {
  const bytes = Buffer.alloc(4);
  bytes.writeUInt32BE(value, 0);
  return bytes;
}

function u64be(value) {
  const bytes = Buffer.alloc(8);
  bytes.writeBigUInt64BE(BigInt(value), 0);
  return bytes;
}

/** 字段值 → 原始字节（按 tag 的约定编码；调用方负责给出正确形状）。 */
function fieldBytes(tag, input) {
  switch (tag) {
    case TAG.protocolVersion:
      return u16be(input);
    case TAG.hostId:
    case TAG.deviceId:
    case TAG.pairingId:
    case TAG.connectionId:
    case TAG.pairingRequestId:
    case TAG.ownerNodeId:
    case TAG.storeEpoch:
    case TAG.sessionId:
      return uuidBytes(input);
    case TAG.pairingExpiresAt:
      return u64be(input);
    case TAG.canonicalOrigin:
    case TAG.origin:
    case TAG.deviceName:
    case TAG.clientKind:
    case TAG.agentId:
    case TAG.normalizedWorkspacePath:
    case TAG.workspaceSource:
      return Buffer.from(String(input), "utf8");
    case TAG.hostPublicKey:
    case TAG.devicePublicKey:
      return typeof input === "string" ? fromB64url(input) : Buffer.from(input);
    case TAG.clientNonce:
    case TAG.serverNonce:
    case TAG.requestNonce:
    case TAG.tabNonce:
      return typeof input === "string" && input.length % 2 === 0 && /^[0-9a-f]+$/i.test(input)
        ? Buffer.from(input, "hex")
        : typeof input === "string"
          ? fromB64url(input)
          : Buffer.from(input);
    case TAG.negotiatedFeatures: {
      const sorted = [...input].sort();
      return Buffer.from(sorted.join("\u0000"), "utf8");
    }
    default:
      throw new ProbeError(`未登记的 transcript 字段 tag：${tag}`);
  }
}

/** 把 `{tag: value}` 编成 §6.2 的二进制 transcript（字段按 tag 升序）。 */
function encodeTranscript(domain, fields) {
  const tags = DOMAIN_FIELDS[domain];
  if (tags === undefined) throw new ProbeError(`未登记的 transcript domain：${domain}`);
  const given = Object.keys(fields).map(Number).sort((left, right) => left - right);
  if (JSON.stringify(given) !== JSON.stringify(tags)) {
    throw new ProbeError(
      `domain ${domain} 的字段集合必须是 ${tags.join(",")}，收到 ${given.join(",")}`,
    );
  }
  const domainText = Buffer.from(domain, "utf8");
  const parts = [Buffer.from("ACPR", "ascii"), Buffer.from([1]), u16be(domainText.length), domainText, u16be(tags.length)];
  for (const tag of tags) {
    const bytes = fieldBytes(tag, fields[tag]);
    parts.push(u16be(tag), u32be(bytes.length), bytes);
  }
  return Buffer.concat(parts);
}

/** transcript 输入对象（fixtures 与运行期共用）→ `{tag: value}`。 */
function preparationFields(input) {
  const tabNonce = input.tabNonceHex ?? input.tabNonce;
  return {
    [TAG.protocolVersion]: input.protocolVersion ?? PROTOCOL_VERSION,
    [TAG.deviceId]: input.deviceId,
    [TAG.ownerNodeId]: input.ownerNodeId,
    [TAG.storeEpoch]: input.storeEpoch,
    [TAG.origin]: input.origin,
    [TAG.tabNonce]: tabNonce,
    [TAG.sessionId]: input.sessionId,
    [TAG.agentId]: input.agentId,
    [TAG.normalizedWorkspacePath]: input.normalizedWorkspacePath,
    [TAG.workspaceSource]: input.workspaceSource,
  };
}

function pairingProofFields(input) {
  return {
    [TAG.protocolVersion]: input.protocolVersion,
    [TAG.hostId]: input.hostId,
    [TAG.deviceId]: input.deviceId,
    [TAG.pairingId]: input.pairingId,
    [TAG.pairingExpiresAt]: input.pairingExpiresAtUnixSeconds,
    [TAG.canonicalOrigin]: input.canonicalOrigin,
    [TAG.devicePublicKey]: input.devicePublicKey,
    [TAG.clientNonce]: input.clientNonceHex,
    [TAG.deviceName]: input.deviceName,
    [TAG.clientKind]: input.clientKind,
  };
}

function pairingHostProofFields(input) {
  return {
    [TAG.protocolVersion]: input.protocolVersion,
    [TAG.hostId]: input.hostId,
    [TAG.deviceId]: input.deviceId,
    [TAG.pairingId]: input.pairingId,
    [TAG.canonicalOrigin]: input.canonicalOrigin,
    [TAG.hostPublicKey]: input.hostPublicKey,
    [TAG.devicePublicKey]: input.devicePublicKey,
    [TAG.clientNonce]: input.clientNonceHex,
    [TAG.serverNonce]: input.serverNonceHex,
    [TAG.pairingRequestId]: input.pairingRequestId,
  };
}

function pairingStatusFields(input) {
  return {
    [TAG.protocolVersion]: input.protocolVersion,
    [TAG.hostId]: input.hostId,
    [TAG.deviceId]: input.deviceId,
    [TAG.pairingId]: input.pairingId,
    [TAG.pairingRequestId]: input.pairingRequestId,
    [TAG.requestNonce]: input.requestNonceHex,
  };
}

function challengeFields(input) {
  return {
    [TAG.protocolVersion]: input.protocolVersion,
    [TAG.hostId]: input.hostId,
    [TAG.deviceId]: input.deviceId,
    [TAG.canonicalOrigin]: input.canonicalOrigin,
    [TAG.clientNonce]: input.clientNonceHex,
    [TAG.serverNonce]: input.serverNonceHex,
    [TAG.connectionId]: input.connectionId,
    [TAG.negotiatedFeatures]: input.negotiatedFeatures,
  };
}

// ---------------------------------------------------------------------------------------------
// 密码学（只用 Node 内置 WebCrypto / crypto）
// ---------------------------------------------------------------------------------------------

async function generateDeviceKey() {
  const pair = await crypto.subtle.generateKey({ name: "ECDSA", namedCurve: "P-256" }, true, [
    "sign",
    "verify",
  ]);
  const raw = Buffer.from(await crypto.subtle.exportKey("raw", pair.publicKey));
  return {
    privateKey: pair.privateKey,
    publicKey: pair.publicKey,
    publicBytes: raw,
    publicB64url: b64url(raw),
    fingerprint: sha256Hex(raw),
  };
}

async function signP1363(privateKey, transcript) {
  const signature = await crypto.subtle.sign(
    { name: "ECDSA", hash: "SHA-256" },
    privateKey,
    transcript,
  );
  return Buffer.from(signature);
}

async function importRawPublicKey(publicBytes) {
  return crypto.subtle.importKey(
    "raw",
    Buffer.from(publicBytes),
    { name: "ECDSA", namedCurve: "P-256" },
    false,
    ["verify"],
  );
}

async function verifyP1363(publicKey, transcript, signature) {
  return crypto.subtle.verify({ name: "ECDSA", hash: "SHA-256" }, publicKey, signature, transcript);
}

function hmacSha256(keyBytes, transcript) {
  return createHmac("sha256", Buffer.from(keyBytes)).update(transcript).digest();
}

// ---------------------------------------------------------------------------------------------
// 内置自检（`--self-test`）：用仓库固定向量验证 transcript / HMAC / P-1363
// ---------------------------------------------------------------------------------------------

function fixturePath(repoRoot, name) {
  return join(repoRoot, "fixtures", "sync", "v1", "transcripts", name);
}

async function runSelfTest(repoRoot) {
  const checks = [];
  const record = (id, ok, detail) => {
    checks.push({ id, status: ok ? "PASS" : "FAIL", detail });
    process.stdout.write(`${ok ? "PASS" : "FAIL"} ${id}${ok ? "" : `: ${detail}`}\n`);
  };

  const cases = [
    { file: "device-proof.json", fields: challengeFields, hmac: false },
    { file: "host-challenge.json", fields: challengeFields, hmac: false },
    { file: "pairing-proof.json", fields: pairingProofFields, hmac: true },
    { file: "pairing-status.json", fields: pairingStatusFields, hmac: true },
    { file: "pairing-host-proof.json", fields: pairingHostProofFields, hmac: false },
  ];

  for (const entry of cases) {
    let fixture;
    try {
      fixture = JSON.parse(readFileSync(fixturePath(repoRoot, entry.file), "utf8"));
    } catch (error) {
      record(`${entry.file}:fixture`, false, shortError(error));
      continue;
    }
    let transcript;
    try {
      transcript = encodeTranscript(fixture.domain, entry.fields(fixture.input));
    } catch (error) {
      record(`${entry.file}:encode`, false, shortError(error));
      continue;
    }
    const encoded = b64url(transcript);
    record(
      `${entry.file}:transcript`,
      encoded === fixture.expected.transcriptBase64url,
      `transcript 与固定向量不一致（got ${encoded.slice(0, 24)}…）`,
    );
    record(
      `${entry.file}:sha256`,
      sha256Hex(transcript) === fixture.expected.transcriptSha256Hex,
      `transcriptSha256 不一致`,
    );
    if (entry.hmac) {
      const key = fromB64url(fixture.expected.hmacKeyBase64url);
      const mac = b64url(hmacSha256(key, transcript));
      record(
        `${entry.file}:hmac`,
        mac === fixture.expected.hmacSha256,
        `HMAC 与固定向量不一致（got ${mac.slice(0, 16)}…）`,
      );
      const tampered = Buffer.from(transcript);
      tampered[tampered.length - 1] ^= 0x01;
      record(
        `${entry.file}:hmac-tamper`,
        b64url(hmacSha256(key, tampered)) !== fixture.expected.hmacSha256,
        "篡改 transcript 之后 HMAC 仍匹配固定向量",
      );
    } else {
      const publicKey = await importRawPublicKey(fromB64url(fixture.expected.publicKey));
      const valid = await verifyP1363(
        publicKey,
        transcript,
        fromB64url(fixture.expected.p1363Signature),
      );
      record(`${entry.file}:signature`, valid, "固定签名无法用固定公钥验证");
      const tampered = Buffer.from(transcript);
      tampered[tampered.length - 1] ^= 0x01;
      const rejected = await verifyP1363(
        publicKey,
        tampered,
        fromB64url(fixture.expected.p1363Signature),
      );
      record(`${entry.file}:signature-tamper`, !rejected, "篡改 transcript 之后签名仍通过验证");
    }
  }

  // 准备凭据：固定向量给的是**签名**，用 WebCrypto 验证；再用固定私钥签一次自验（ECDSA 随机化，
  // 因此不能与固定签名逐字节比较，但必须能被同一把公钥验证）。
  try {
    const fixture = JSON.parse(readFileSync(fixturePath(repoRoot, "session-preparation.json"), "utf8"));
    const transcript = encodeTranscript(fixture.domain, preparationFields(fixture.input));
    record(
      "session-preparation:transcript",
      b64url(transcript) === fixture.expected.transcriptBase64url,
      "transcript 与固定向量不一致",
    );
    record(
      "session-preparation:sha256",
      sha256Hex(transcript) === fixture.expected.transcriptSha256Hex,
      "transcriptSha256 不一致",
    );
    const publicKey = await importRawPublicKey(fromB64url(fixture.expected.publicKey));
    record(
      "session-preparation:signature",
      await verifyP1363(publicKey, transcript, fromB64url(fixture.expected.p1363Signature)),
      "固定签名无法用固定公钥验证",
    );
    const jwk = fixture.expected.privateJwk;
    const privateKey = await crypto.subtle.importKey(
      "jwk",
      jwk,
      { name: "ECDSA", namedCurve: "P-256" },
      false,
      ["sign"],
    );
    const signed = await signP1363(privateKey, transcript);
    record(
      "session-preparation:roundtrip",
      (await verifyP1363(publicKey, transcript, signed)) && signed.length === 64,
      "用固定私钥签名后无法由同一把公钥验证（或签名不是 64 字节 P-1363）",
    );
    const tampered = Buffer.from(transcript);
    tampered[0] ^= 0x01;
    record(
      "session-preparation:tamper",
      !(await verifyP1363(publicKey, tampered, fromB64url(fixture.expected.p1363Signature))),
      "篡改 transcript 之后固定签名仍通过验证",
    );
  } catch (error) {
    record("session-preparation:fixture", false, shortError(error));
  }

  const failed = checks.filter((check) => check.status === "FAIL").length;
  process.stdout.write(`self-test: ${checks.length - failed} passed, ${failed} failed\n`);
  return failed === 0 ? 0 : 1;
}

// ---------------------------------------------------------------------------------------------
// 参数
// ---------------------------------------------------------------------------------------------

function parseArgs(argv) {
  const options = {
    server: null,
    adapter: null,
    dataDir: null,
    codexHome: null,
    cert: null,
    key: null,
    workspace: null,
    port: null,
    jsonOut: null,
    timeoutMs: DEFAULT_TIMEOUT_MS,
    keep: false,
    selfTest: false,
    help: false,
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
      case "--server":
        options.server = takeValue();
        break;
      case "--adapter":
        options.adapter = takeValue();
        break;
      case "--data-dir":
        options.dataDir = takeValue();
        break;
      case "--codex-home":
        options.codexHome = takeValue();
        break;
      case "--cert":
        options.cert = takeValue();
        break;
      case "--key":
        options.key = takeValue();
        break;
      case "--workspace":
        options.workspace = takeValue();
        break;
      case "--port": {
        const value = Number(takeValue());
        if (!Number.isInteger(value) || value <= 0 || value > 65535) {
          throw new ProbeUsageError("--port 必须是 1..65535 的整数");
        }
        options.port = value;
        break;
      }
      case "--json-out":
        options.jsonOut = takeValue();
        break;
      case "--timeout-ms": {
        const value = Number(takeValue());
        if (!Number.isInteger(value) || value <= 0) {
          throw new ProbeUsageError("--timeout-ms 必须是正整数");
        }
        options.timeoutMs = value;
        break;
      }
      case "--keep":
        if (inline !== null) throw new ProbeUsageError("--keep 不接受取值");
        options.keep = true;
        break;
      case "--self-test":
        if (inline !== null) throw new ProbeUsageError("--self-test 不接受取值");
        options.selfTest = true;
        break;
      case "--help":
      case "-h":
        options.help = true;
        break;
      default:
        throw new ProbeUsageError(`未知参数 ${raw}`);
    }
  }
  return options;
}

function repoRootOf() {
  return resolve(dirname(fileURLToPath(import.meta.url)), "..");
}

function samePath(left, right) {
  const normalize = (value) => {
    const resolved = resolve(value);
    return WINDOWS ? resolved.toLowerCase() : resolved;
  };
  return normalize(left) === normalize(right);
}

function isInside(parent, candidate) {
  const normalize = (value) => (WINDOWS ? resolve(value).toLowerCase() : resolve(value));
  const base = normalize(parent);
  const target = normalize(candidate);
  if (base === target) return true;
  const prefix = base.endsWith(WINDOWS ? "\\" : "/") ? base : base + (WINDOWS ? "\\" : "/");
  return target.startsWith(prefix);
}

function realCodexHome() {
  const configured = process.env.CODEX_HOME;
  return resolve(configured && configured.trim() ? configured : join(os.homedir(), ".codex"));
}

function resolveExecutable(explicit, source, fallbacks) {
  const candidate = explicit ?? fallbacks.find((path) => path && existsSync(path)) ?? null;
  if (candidate === null) {
    throw new ProbeError(
      `找不到 ${source}：${fallbacks.filter(Boolean).join("、") || "（无候选）"}；请显式传入路径`,
    );
  }
  const absolute = resolve(candidate);
  if (!existsSync(absolute)) throw new ProbeError(`${source} 不存在：${absolute}`);
  if (!statSync(absolute).isFile()) throw new ProbeError(`${source} 不是文件：${absolute}`);
  return absolute;
}

function resolveAdapter(explicit) {
  const fallbacks = [process.env.ACPR_CODEX_ACP_ADAPTER, KNOWN_ADAPTER_PATH];
  return resolveExecutable(explicit, "codex-acp 适配器", fallbacks);
}

function resolveServer(explicit) {
  const repoRoot = repoRootOf();
  const fallbacks = [
    process.env.ACPR_SERVER,
    WINDOWS ? join(repoRoot, "target", "debug", "acp-remote.exe") : join(repoRoot, "target", "debug", "acp-remote"),
  ];
  return resolveExecutable(explicit, "acp-remote 可执行文件", fallbacks);
}

function assertSafePaths(options, repoRoot, realHome) {
  const dataDir = resolve(
    options.dataDir ?? join(os.tmpdir(), `acpr-session-preparation-${randomBytes(6).toString("hex")}`),
  );
  if (samePath(dataDir, repoRoot)) throw new ProbeError(`--data-dir 不能是仓库根：${dataDir}`);
  if (samePath(dataDir, os.homedir())) throw new ProbeError(`--data-dir 不能是用户家目录：${dataDir}`);
  if (basename(dataDir).toLowerCase() === ".codex") {
    throw new ProbeError(`--data-dir 不能是 .codex 目录：${dataDir}`);
  }
  if (samePath(dataDir, realHome) || isInside(realHome, dataDir)) {
    throw new ProbeError(`--data-dir 不能位于真实 CODEX_HOME 内：${dataDir}`);
  }
  if (isInside(repoRoot, dataDir)) throw new ProbeError(`--data-dir 不能位于仓库工作树内：${dataDir}`);
  if (existsSync(dataDir) && !statSync(dataDir).isDirectory()) {
    throw new ProbeError(`--data-dir 已存在且不是目录：${dataDir}`);
  }
  const codexHome = resolve(options.codexHome ?? join(dataDir, "codex-home"));
  if (samePath(codexHome, realHome) || isInside(realHome, codexHome)) {
    throw new ProbeError(`--codex-home 不能是/位于真实 CODEX_HOME 内：${codexHome}`);
  }
  if (samePath(codexHome, dataDir) || !isInside(dataDir, codexHome)) {
    throw new ProbeError(`--codex-home 必须在隔离 data-dir 内：${codexHome}`);
  }
  const workspace = resolve(options.workspace ?? join(dataDir, "workspace"));
  if (samePath(workspace, realHome) || isInside(realHome, workspace)) {
    throw new ProbeError(`--workspace 不能是/位于真实 CODEX_HOME 内：${workspace}`);
  }
  return { dataDir, codexHome, workspace };
}

// ---------------------------------------------------------------------------------------------
// 准备目录、证书、配置、Agent 包装器
// ---------------------------------------------------------------------------------------------

function copyCredentials(realHome, codexHome) {
  const copied = [];
  const missing = [];
  for (const name of ["auth.json", "config.toml"]) {
    const source = join(realHome, name);
    if (!existsSync(source)) {
      missing.push(name);
      continue;
    }
    // 只复制字节；不读入内存、不解析、不打日志。
    copyFileSync(source, join(codexHome, name));
    copied.push(name);
  }
  return { copied, missing, realHome };
}

function prepareDirs(options, paths) {
  const createdDataDir = !existsSync(paths.dataDir);
  mkdirSync(paths.dataDir, { recursive: true });
  mkdirSync(paths.codexHome, { recursive: true });
  mkdirSync(paths.workspace, { recursive: true });
  return { createdDataDir };
}

function freePort() {
  return new Promise((resolvePort, reject) => {
    const server = createServer();
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      server.close(() => resolvePort(address.port));
    });
  });
}

function generateCertificate(root) {
  const certPath = join(root, "tls-cert.pem");
  const keyPath = join(root, "tls-key.pem");
  const result = spawnSync(
    "openssl",
    [
      "req",
      "-x509",
      "-newkey",
      "rsa:2048",
      "-nodes",
      "-keyout",
      keyPath,
      "-out",
      certPath,
      "-days",
      "2",
      "-subj",
      "/CN=localhost",
      "-addext",
      "subjectAltName=DNS:localhost,IP:127.0.0.1",
    ],
    { stdio: "ignore", windowsHide: true },
  );
  if (result.error || result.status !== 0) {
    throw new ProbeError(
      "生成 TLS 自签证书失败（需要 PATH 中的 openssl）；请用 --cert/--key 提供证书与私钥",
    );
  }
  return { certPath, keyPath };
}

function tomlPath(path) {
  return resolve(path).replaceAll("\\", "/");
}

function writeConfig({ dataDir, port, publicOrigin, certPath, keyPath, logPath, agentCommand }) {
  const command = agentCommand.map((part) => JSON.stringify(part)).join(", ");
  const configPath = join(dataDir, "acp-remote.probe.toml");
  const text = `[daemon]
data_dir = "${tomlPath(dataDir)}"
public_origin = "${publicOrigin}"
bind = "127.0.0.1"
port = ${port}

[daemon.tls]
mode = "direct"
cert_path = "${tomlPath(certPath)}"
key_path = "${tomlPath(keyPath)}"

[identity]
keystore = "platform"

[agent]
default = "codex"
codex = [${command}]

[logging]
level = "info"
format = "json"
file = "${tomlPath(logPath)}"
`;
  writeFileSync(configPath, text, "utf8");
  return configPath;
}

/**
 * Agent 包装器：把真实 `codex-acp` 放在 stdio 后面并记录 **Agent 边界** 的事实。
 *
 * 只记录方法名、参数键、prompt 的摘要/长度/是否与提交正文逐字相同——不记录 prompt 正文、
 * 不记录任何凭据、不改写任何消息字节。
 */
function writeAgentWrapper(root) {
  const wrapperPath = join(root, "agent-wrapper.cjs");
  const source = `#!/usr/bin/env node
'use strict';
const fs = require('fs');
const readline = require('readline');
const { spawn } = require('child_process');

const [adapterPath, evidencePath] = process.argv.slice(2);
if (!adapterPath || !evidencePath) {
  process.stderr.write('agent-wrapper: missing adapter/evidence path\\n');
  process.exit(2);
}
const expected = process.env.PROBE_FIRST_MESSAGE ?? '';
const append = (record) => {
  try { fs.appendFileSync(evidencePath, JSON.stringify(record) + '\\n'); } catch {}
};
const sha256 = (text) => require('crypto').createHash('sha256').update(text).digest('hex');
append({ type: 'wrapper_start', pid: process.pid, ppid: process.ppid, cwd: process.cwd(), at: new Date().toISOString() });

const child = spawn(process.execPath, [adapterPath], { stdio: ['pipe', 'pipe', 'pipe'], env: process.env });
append({ type: 'adapter_start', pid: child.pid, at: new Date().toISOString() });

child.stderr.on('data', (chunk) => { try { process.stderr.write(chunk); } catch {} });
child.on('exit', (code, signal) => {
  append({ type: 'adapter_exit', code, signal, at: new Date().toISOString() });
});

const inbound = readline.createInterface({ input: child.stdout });
inbound.on('line', (line) => {
  let message = null;
  try { message = JSON.parse(line); } catch {}
  if (message && typeof message.method === 'string') {
    const params = message.params && typeof message.params === 'object' ? message.params : {};
    if (message.method === 'session/prompt') {
      const blocks = Array.isArray(params.prompt) ? params.prompt : [];
      const text = blocks.map((block) => (block && typeof block.text === 'string' ? block.text : '')).join('');
      append({
        type: 'prompt',
        pid: child.pid,
        length: text.length,
        textSha256: sha256(text),
        matchesExpected: expected.length > 0 && text === expected,
        expectedSha256: expected.length > 0 ? sha256(expected) : null,
        at: new Date().toISOString(),
      });
    } else {
      append({
        type: 'request',
        pid: child.pid,
        method: message.method,
        paramsKeys: Object.keys(params).sort(),
        sessionId: typeof params.sessionId === 'string' ? params.sessionId : null,
        modeId: typeof params.modeId === 'string' ? params.modeId : null,
        configId: typeof params.configId === 'string' ? params.configId : null,
        cwd: typeof params.cwd === 'string' ? params.cwd : null,
        at: new Date().toISOString(),
      });
    }
  }
  try { process.stdout.write(line + '\\n'); } catch {}
});

const outbound = readline.createInterface({ input: process.stdin });
outbound.on('line', (line) => {
  let message = null;
  try { message = JSON.parse(line); } catch {}
  if (message && typeof message.method === 'string') {
    const params = message.params && typeof message.params === 'object' ? message.params : {};
    if (message.method === 'session/prompt') {
      const blocks = Array.isArray(params.prompt) ? params.prompt : [];
      const text = blocks.map((block) => (block && typeof block.text === 'string' ? block.text : '')).join('');
      append({
        type: 'prompt',
        pid: child.pid,
        length: text.length,
        textSha256: sha256(text),
        matchesExpected: expected.length > 0 && text === expected,
        expectedSha256: expected.length > 0 ? sha256(expected) : null,
        at: new Date().toISOString(),
      });
    } else {
      append({
        type: 'request',
        pid: child.pid,
        method: message.method,
        paramsKeys: Object.keys(params).sort(),
        sessionId: typeof params.sessionId === 'string' ? params.sessionId : null,
        modeId: typeof params.modeId === 'string' ? params.modeId : null,
        configId: typeof params.configId === 'string' ? params.configId : null,
        cwd: typeof params.cwd === 'string' ? params.cwd : null,
        at: new Date().toISOString(),
      });
    }
  }
  try { child.stdin.write(line + '\\n'); } catch {}
});
outbound.on('close', () => { try { child.stdin.end(); } catch {} });
child.on('close', () => { process.exit(0); });
`;
  writeFileSync(wrapperPath, source, "utf8");
  return wrapperPath;
}

// ---------------------------------------------------------------------------------------------
// 报告与检查
// ---------------------------------------------------------------------------------------------

function createEngine() {
  return {
    checks: [],
    checkIds: new Set(),
    timings: {},
    notes: [],
    progress: () => {},
  };
}

function recordCheck(engine, stage, id, status, summary, details = {}) {
  if (!["PASS", "FAIL", "SKIP"].includes(status)) throw new Error(`非法检查状态 ${status}`);
  if (engine.checkIds.has(id)) throw new Error(`检查 id 重复：${id}`);
  engine.checkIds.add(id);
  const check = { id, stage, status, summary, details, at: nowIso() };
  engine.checks.push(check);
  if (process.env.ACPR_PROBE_PROGRESS === "1") {
    process.stderr.write(`${status} ${id}: ${summary}\n`);
  }
  return check;
}

const STAGE_CHECKS = {
  environment: [
    "environment.server_binary",
    "environment.adapter_resolved",
    "environment.isolated_data_dir",
    "environment.codex_home_isolated",
    "environment.credentials_copied",
    "environment.tls_certificate",
    "environment.daemon_ready",
    "environment.feature_negotiated",
  ],
  allocate: [
    "allocate.ready",
    "allocate.real_modes",
    "allocate.real_config",
    "allocate.configure_applied",
    "allocate.no_formal_session",
    "allocate.no_prompt_before_first_message",
    "allocate.no_formal_resume",
  ],
  restart: [
    "restart.previous_process_tree_terminated",
    "restart.same_id_restored",
    "restart.new_runtime_epoch",
    "restart.new_endpoint_process",
    "restart.no_prompt_before_first_message",
    "restart.no_formal_resume",
  ],
  first_message: [
    "first_message.accepted_then_completed",
    "first_message.user_text_readback",
    "first_message.marker_delivered",
    "first_message.exactly_one_prompt",
    "first_message.no_hidden_prompt",
    "first_message.no_formal_resume",
    "first_message.terminal_projection",
  ],
};

function skipRemaining(engine, stage, reason) {
  for (const id of STAGE_CHECKS[stage] ?? []) {
    if (id === `${stage}.completed`) continue;
    if (engine.checkIds.has(id)) continue;
    recordCheck(engine, stage, id, "SKIP", reason, { reason });
  }
}

function summarizeStage(engine, stage) {
  const checks = engine.checks.filter((check) => check.stage === stage && check.id !== `${stage}.completed`);
  let status = "PASS";
  if (checks.some((check) => check.status === "FAIL")) status = "FAIL";
  else if (checks.some((check) => check.status === "SKIP")) status = "SKIP";
  recordCheck(engine, stage, `${stage}.completed`, status, `${stage} 阶段汇总：${status}`, {
    pass: checks.filter((check) => check.status === "PASS").length,
    fail: checks.filter((check) => check.status === "FAIL").length,
    skip: checks.filter((check) => check.status === "SKIP").length,
  });
}

function readEvidence(evidencePath) {
  if (!existsSync(evidencePath)) return [];
  const text = readFileSync(evidencePath, "utf8");
  const records = [];
  for (const line of text.split(/\r?\n/)) {
    if (!line.trim()) continue;
    try {
      records.push(JSON.parse(line));
    } catch {
      // 进程被强制终止时最后一行可能只写了一半：忽略它，保留已有证据。
    }
  }
  return records;
}

function waitForEvidence(evidencePath, predicate, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  return (async () => {
    while (Date.now() < deadline) {
      const records = readEvidence(evidencePath);
      if (predicate(records)) return records;
      await sleep(100);
    }
    return readEvidence(evidencePath);
  })();
}

function processAlive(pid) {
  if (!pid) return false;
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

// ---------------------------------------------------------------------------------------------
// 子进程：Daemon 与 CLI
// ---------------------------------------------------------------------------------------------

function daemonSpawn(root, serverPath, configPath, codexHome, firstMessage) {
  const suffix = randomBytes(3).toString("hex");
  return spawn(serverPath, ["daemon", "start", "--config", configPath], {
    cwd: root,
    env: {
      ...process.env,
      CODEX_HOME: codexHome,
      PROBE_FIRST_MESSAGE: firstMessage,
      ACP_REMOTE_CONFIG: configPath,
    },
    stdio: [
      "ignore",
      (() => {
        const out = join(root, `daemon-${suffix}.stdout.log`);
        writeFileSync(out, "", "utf8");
        return openSync(out, "a");
      })(),
      (() => {
        const err = join(root, `daemon-${suffix}.stderr.log`);
        writeFileSync(err, "", "utf8");
        return openSync(err, "a");
      })(),
    ],
    windowsHide: true,
  });
}

async function waitForExit(child, timeoutMs) {
  if (child.exitCode !== null || child.signalCode !== null) return true;
  return new Promise((done) => {
    const timer = setTimeout(() => done(false), timeoutMs);
    child.once("exit", () => {
      clearTimeout(timer);
      done(true);
    });
  });
}

function killTree(child) {
  if (!child || child.pid === undefined) return;
  if (WINDOWS) {
    spawnSync("taskkill", ["/PID", String(child.pid), "/T", "/F"], {
      stdio: "ignore",
      windowsHide: true,
    });
  } else {
    try {
      process.kill(-child.pid, "SIGKILL");
    } catch {
      try {
        child.kill("SIGKILL");
      } catch {}
    }
  }
}

function httpsJson(port, hostHeader, origin, path, body) {
  return new Promise((resolveReply, reject) => {
    const payload = Buffer.from(JSON.stringify(body), "utf8");
    const request = https.request(
      {
        host: "127.0.0.1",
        port,
        servername: "localhost",
        rejectUnauthorized: false,
        method: "POST",
        path,
        headers: {
          Host: hostHeader,
          Origin: origin,
          "Content-Type": "application/json",
          "Content-Length": payload.length,
        },
      },
      (response) => {
        const chunks = [];
        response.on("data", (chunk) => chunks.push(chunk));
        response.on("end", () => {
          const text = Buffer.concat(chunks).toString("utf8");
          let parsed = null;
          try {
            parsed = JSON.parse(text);
          } catch {}
          resolveReply({ status: response.statusCode ?? 0, body: parsed, text });
        });
      },
    );
    request.once("error", reject);
    request.end(payload);
  });
}

function httpsGet(port, hostHeader, origin, path) {
  return new Promise((resolveReply, reject) => {
    const request = https.request(
      {
        host: "127.0.0.1",
        port,
        servername: "localhost",
        rejectUnauthorized: false,
        method: "GET",
        path,
        headers: { Host: hostHeader, Origin: origin },
      },
      (response) => {
        const chunks = [];
        response.on("data", (chunk) => chunks.push(chunk));
        response.on("end", () =>
          resolveReply({
            status: response.statusCode ?? 0,
            text: Buffer.concat(chunks).toString("utf8"),
          }),
        );
      },
    );
    request.once("error", reject);
    request.end();
  });
}

async function waitDaemonReady(port, hostHeader, origin, child, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  let last = "没有响应";
  while (Date.now() < deadline) {
    if (child.exitCode !== null) {
      throw new ProbeError(`Daemon 在就绪前退出（exit=${child.exitCode}）`);
    }
    try {
      const reply = await httpsGet(port, hostHeader, origin, "/pair");
      if (reply.status === 200) return;
      last = `GET /pair -> ${reply.status}`;
    } catch (error) {
      last = shortError(error);
    }
    await sleep(100);
  }
  throw new ProbeError(`等待 Daemon 就绪超时：${last}`);
}

/**
 * 用真实 CLI 完成设备配对的一半：`device pair` 负责 `begin` 与**本机确认**。
 *
 * 配对码只在本进程内存里用于 `pairing/lookup`；CLI 的终端输出（含配对码与二维码）不落报告。
 */
async function startPairCli({ serverPath, configPath, fingerprint, dataDir, timeoutMs }) {
  const child = spawn(
    serverPath,
    ["device", "pair", "--config", configPath, "--fingerprint", fingerprint],
    {
      cwd: dataDir,
      env: { ...process.env, ACP_REMOTE_CONFIG: configPath },
      stdio: ["ignore", "pipe", "pipe"],
      windowsHide: true,
    },
  );
  let stdout = "";
  let stderr = "";
  child.stdout.on("data", (chunk) => {
    stdout += chunk.toString("utf8");
  });
  child.stderr.on("data", (chunk) => {
    stderr += chunk.toString("utf8");
    if (stderr.length > 200_000) stderr = stderr.slice(-100_000);
  });
  const deadline = Date.now() + timeoutMs;
  let code = null;
  while (Date.now() < deadline && code === null) {
    const match = /配对码[：:]\s*([0-9A-Za-z]{4}-[0-9A-Za-z]{4})/.exec(stdout);
    if (match) code = match[1];
    else if (child.exitCode !== null) break;
    else await sleep(50);
  }
  if (code === null) {
    killTree(child);
    throw new ProbeError(
      `CLI 没有给出配对码（exit=${child.exitCode}）：${stderr.split(/\r?\n/).filter((line) => !/二维码|█|▀|▄/.test(line)).slice(-3).join(" | ")}`,
    );
  }
  return { child, pairingCode: code };
}

// ---------------------------------------------------------------------------------------------
// 原始 WebSocket（真实 TLS）：控制帧与断线都能被探针直接观察
// ---------------------------------------------------------------------------------------------

class RawWebSocket {
  constructor(socket) {
    this.socket = socket;
    this.buffer = Buffer.alloc(0);
    this.inbox = [];
    this.waiters = [];
    this.closed = false;
    this.closeInfo = null;
    socket.on("data", (chunk) => {
      this.buffer = Buffer.concat([this.buffer, chunk]);
      this.#drain();
    });
    socket.on("error", (error) => {
      this.closeInfo = shortError(error);
      this.#close();
    });
    socket.on("close", () => this.#close());
  }

  static async connect({ port, hostHeader, origin, path, timeoutMs }) {
    const socket = tls.connect({
      host: "127.0.0.1",
      port,
      servername: "localhost",
      rejectUnauthorized: false,
    });
    socket.setNoDelay(true);
    await new Promise((done, fail) => {
      socket.once("secureConnect", done);
      socket.once("error", fail);
    });
    const key = b64url(randomBytes(16));
    socket.write(
      `GET ${path} HTTP/1.1\r\n` +
        `Host: ${hostHeader}\r\n` +
        "Upgrade: websocket\r\n" +
        "Connection: Upgrade\r\n" +
        `Sec-WebSocket-Key: ${key}\r\n` +
        "Sec-WebSocket-Version: 13\r\n" +
        `Sec-WebSocket-Protocol: ${WS_SUBPROTOCOL}\r\n` +
        `Origin: ${origin}\r\n\r\n`,
    );
    let head = Buffer.alloc(0);
    const deadline = Date.now() + timeoutMs;
    while (true) {
      if (Date.now() > deadline) throw new ProbeError("WebSocket 升级响应超时");
      const chunk = await new Promise((done, fail) => {
        const timer = setTimeout(() => fail(new ProbeError("WebSocket 升级响应超时")), 500);
        socket.once("data", (data) => {
          clearTimeout(timer);
          done(data);
        });
        socket.once("error", (error) => {
          clearTimeout(timer);
          fail(error);
        });
      }).catch((error) => {
        throw error;
      });
      head = Buffer.concat([head, chunk]);
      const index = head.indexOf("\r\n\r\n");
      if (index === -1) continue;
      const headText = head.slice(0, index).toString("utf8");
      if (!headText.startsWith("HTTP/1.1 101")) {
        throw new ProbeError(`WebSocket 升级被拒绝：${headText.split("\r\n")[0]}`);
      }
      const socketWrapper = new RawWebSocket(socket);
      socketWrapper.buffer = head.slice(index + 4);
      socketWrapper.#drain();
      return socketWrapper;
    }
  }

  #close() {
    if (this.closed) return;
    this.closed = true;
    for (const waiter of this.waiters.splice(0)) waiter.reject(new ProbeError("WebSocket 已关闭"));
  }

  #drain() {
    while (true) {
      if (this.buffer.length < 2) return;
      const first = this.buffer[0];
      const opcode = first & 0x0f;
      const masked = (this.buffer[1] & 0x80) !== 0;
      let length = this.buffer[1] & 0x7f;
      let offset = 2;
      if (length === 126) {
        if (this.buffer.length < 4) return;
        length = this.buffer.readUInt16BE(2);
        offset = 4;
      } else if (length === 127) {
        if (this.buffer.length < 10) return;
        length = Number(this.buffer.readBigUInt64BE(2));
        offset = 10;
      }
      const maskOffset = offset;
      if (masked) offset += 4;
      if (this.buffer.length < offset + length) return;
      let payload = Buffer.from(this.buffer.slice(offset, offset + length));
      if (masked) {
        const mask = this.buffer.slice(maskOffset, maskOffset + 4);
        for (let index = 0; index < payload.length; index += 1) {
          payload[index] ^= mask[index % 4];
        }
      }
      this.buffer = this.buffer.slice(offset + length);
      if (opcode === 0x8) {
        this.closeInfo = "closed by peer";
        this.#close();
        try {
          this.socket.end();
        } catch {}
        return;
      }
      if (opcode === 0x9) {
        this.#writeFrame(0xa, payload);
        continue;
      }
      if (opcode === 0xa) continue;
      if (opcode !== 0x1 && opcode !== 0x0) continue;
      const text = payload.toString("utf8");
      const waiter = this.waiters.shift();
      if (waiter) waiter.resolve(text);
      else this.inbox.push(text);
    }
  }

  #writeFrame(opcode, payload) {
    const mask = randomBytes(4);
    let header;
    if (payload.length < 126) {
      header = Buffer.from([0x80 | opcode, 0x80 | payload.length]);
    } else if (payload.length <= 0xffff) {
      header = Buffer.alloc(4);
      header[0] = 0x80 | opcode;
      header[1] = 0x80 | 126;
      header.writeUInt16BE(payload.length, 2);
    } else {
      header = Buffer.alloc(10);
      header[0] = 0x80 | opcode;
      header[1] = 0x80 | 127;
      header.writeBigUInt64BE(BigInt(payload.length), 2);
    }
    const masked = Buffer.alloc(payload.length);
    for (let index = 0; index < payload.length; index += 1) {
      masked[index] = payload[index] ^ mask[index % 4];
    }
    this.socket.write(Buffer.concat([header, mask, masked]));
  }

  sendText(text) {
    this.#writeFrame(0x1, Buffer.from(text, "utf8"));
  }

  async nextText(timeoutMs) {
    if (this.inbox.length > 0) return this.inbox.shift();
    if (this.closed) throw new ProbeError(`WebSocket 已关闭：${this.closeInfo ?? "unknown"}`);
    return new Promise((done, fail) => {
      const waiter = { resolve: done, reject: fail };
      this.waiters.push(waiter);
      const timer = setTimeout(() => {
        const index = this.waiters.indexOf(waiter);
        if (index !== -1) this.waiters.splice(index, 1);
        fail(new ProbeError(`等待 WebSocket 消息超时（${timeoutMs}ms）`));
      }, timeoutMs);
      const originalResolve = waiter.resolve;
      waiter.resolve = (value) => {
        clearTimeout(timer);
        originalResolve(value);
      };
      const originalReject = waiter.reject;
      waiter.reject = (error) => {
        clearTimeout(timer);
        originalReject(error);
      };
    });
  }

  close() {
    try {
      this.#writeFrame(0x8, Buffer.alloc(0));
    } catch {}
    try {
      this.socket.end();
    } catch {}
    this.#close();
  }
}

// ---------------------------------------------------------------------------------------------
// Sync 客户端（真实握手 + 命令/控制帧）
// ---------------------------------------------------------------------------------------------

class SyncClient {
  constructor({ socket, engine, timeoutMs, negotiated }) {
    this.socket = socket;
    this.engine = engine;
    this.timeoutMs = timeoutMs;
    this.negotiated = negotiated;
    this.connectionId = null;
    this.sequence = 0;
    this.pending = [];
    this.frames = [];
  }

  static async connect({ port, hostHeader, origin, timeoutMs }) {
    const socket = await RawWebSocket.connect({
      port,
      hostHeader,
      origin,
      path: WS_PATH,
      timeoutMs,
    });
    return new SyncClient({ socket, engine: null, timeoutMs, negotiated: [] });
  }

  async authenticate({ hostId, deviceKey, origin, clientNonce, serverPublicKey }) {
    this.hostId = hostId;
    this.origin = origin;
    this.clientNonce = clientNonce;
    this.#sendPreAuth("auth.client_hello", {
      minProtocolVersion: 1,
      maxProtocolVersion: 1,
      hostId,
      deviceId: deviceKey.deviceId,
      clientKind: "desktop",
      clientNonce: b64url(clientNonce),
      supportedFeatures: [...SUPPORTED_FEATURES].sort(),
      requiredFeatures: [],
    });
    const challenge = await this.expectType("auth.server_challenge", this.timeoutMs);
    const body = challenge.body;
    this.connectionId = body.connectionId;
    const selected = body.selectedFeatures;
    this.negotiated = selected;
    const serverNonce = fromB64url(body.serverNonce);
    const transcript = encodeTranscript(DOMAIN.deviceProof, {
      [TAG.protocolVersion]: 1,
      [TAG.hostId]: hostId,
      [TAG.deviceId]: deviceKey.deviceId,
      [TAG.canonicalOrigin]: origin,
      [TAG.clientNonce]: clientNonce,
      [TAG.serverNonce]: serverNonce,
      [TAG.connectionId]: body.connectionId,
      [TAG.negotiatedFeatures]: selected,
    });
    if (serverPublicKey) {
      const verified = await verifyP1363(
        serverPublicKey,
        encodeTranscript(DOMAIN.hostChallenge, {
          [TAG.protocolVersion]: 1,
          [TAG.hostId]: hostId,
          [TAG.deviceId]: deviceKey.deviceId,
          [TAG.canonicalOrigin]: origin,
          [TAG.clientNonce]: clientNonce,
          [TAG.serverNonce]: serverNonce,
          [TAG.connectionId]: body.connectionId,
          [TAG.negotiatedFeatures]: selected,
        }),
        fromB64url(body.hostProof),
      );
      this.hostProofVerified = verified;
    }
    const proof = await signP1363(deviceKey.privateKey, transcript);
    this.#sendPreAuth("auth.client_proof", {
      connectionId: body.connectionId,
      deviceId: deviceKey.deviceId,
      deviceProof: b64url(proof),
    });
    const authenticated = await this.expectAny(this.timeoutMs);
    if (authenticated.type !== "auth.authenticated") {
      throw new ProtocolError(`认证失败：${JSON.stringify(authenticated)}`);
    }
    return authenticated.body;
  }

  #sendPreAuth(type, body) {
    this.socket.sendText(
      JSON.stringify({
        protocolVersion: 1,
        type,
        messageId: randomUUID(),
        body,
      }),
    );
  }

  send(type, body) {
    this.sequence += 1;
    this.socket.sendText(
      JSON.stringify({
        protocolVersion: 1,
        type,
        messageId: randomUUID(),
        connectionId: this.connectionId,
        connectionSequence: String(this.sequence),
        body,
      }),
    );
    this.frames.push({ type, body });
  }

  async command(requestId, command, sessionId, payload) {
    const body = { requestId, command, payload };
    if (sessionId !== null && sessionId !== undefined) body.sessionId = sessionId;
    this.send("command", body);
    return body;
  }

  async preparation(body) {
    this.send("session.preparation", body);
    return this.expectType("session.preparation.result", this.timeoutMs);
  }

  /** 跳过其它消息（事件/心跳/迟到结果），只返回类型匹配的那一条。 */
  async expectType(type, timeoutMs) {
    const deadline = Date.now() + timeoutMs;
    while (true) {
      if (this.pending.length > 0) {
        const index = this.pending.findIndex((message) => message.type === type);
        if (index !== -1) return this.pending.splice(index, 1)[0];
      }
      const remaining = deadline - Date.now();
      if (remaining <= 0) throw new ProbeError(`等待 ${type} 超时`);
      const message = await this.#next(remaining);
      if (message.type === type) return message;
      if (message.type === "error") {
        throw new ProtocolError(`服务端错误：${JSON.stringify(message.body)}`);
      }
      this.pending.push(message);
    }
  }

  async expectResult(requestId, timeoutMs) {
    const deadline = Date.now() + timeoutMs;
    while (true) {
      const index = this.pending.findIndex(
        (message) =>
          message.type === "command.result" && message.body && message.body.requestId === requestId,
      );
      if (index !== -1) return this.pending.splice(index, 1)[0];
      const remaining = deadline - Date.now();
      if (remaining <= 0) throw new ProbeError(`等待 requestId=${requestId} 的 command.result 超时`);
      const message = await this.#next(remaining);
      if (message.type === "command.result" && message.body.requestId === requestId) return message;
      if (message.type === "error") {
        throw new ProtocolError(
          `请求 ${requestId} 被服务端拒绝：${JSON.stringify(message.body)}`,
        );
      }
      this.pending.push(message);
    }
  }

  async expectAny(timeoutMs) {
    if (this.pending.length > 0) return this.pending.shift();
    return this.#next(timeoutMs);
  }

  async #next(timeoutMs) {
    while (true) {
      const text = await this.socket.nextText(timeoutMs);
      let message;
      try {
        message = JSON.parse(text);
      } catch {
        throw new ProtocolError("收到无法解析的 Sync 消息");
      }
      if (message.type === "control.ping") {
        this.send("control.pong", message.body);
        continue;
      }
      return message;
    }
  }

  close() {
    this.socket.close();
  }
}

// ---------------------------------------------------------------------------------------------
// 主流程
// ---------------------------------------------------------------------------------------------

async function main() {
  let options;
  try {
    options = parseArgs(process.argv.slice(2));
  } catch (error) {
    process.stderr.write(`${TOOL_NAME}: ${shortError(error)}\n`);
    process.stderr.write("用 --help 查看用法。\n");
    return 2;
  }
  if (options.help) {
    process.stdout.write(`${HELP_TEXT}\n`);
    return 0;
  }
  if (options.selfTest) {
    try {
      return await runSelfTest(repoRootOf());
    } catch (error) {
      process.stderr.write(`${TOOL_NAME}: self-test 失败：${shortError(error)}\n`);
      return 1;
    }
  }

  const engine = createEngine();
  const report = {
    schemaVersion: 1,
    tool: TOOL_NAME,
    version: TOOL_VERSION,
    protocolVersion: PROTOCOL_VERSION,
    runId: randomBytes(6).toString("hex"),
    startedAt: nowIso(),
    finishedAt: null,
    environment: {},
    checks: [],
    summary: null,
    timings: {},
    notes: [],
    agent: null,
    exitCode: null,
  };
  const state = {
    daemon: null,
    dataDir: null,
    createdDataDir: false,
    codexHome: null,
    secret: null,
    deviceKey: null,
    receipt: null,
    sessionId: null,
    runtimeEpoch: null,
    generation: null,
    marker: null,
    firstMessage: null,
    requestId: null,
    keep: options.keep,
  };

  const cleanup = async () => {
    if (state.daemon && state.daemon.exitCode === null) {
      killTree(state.daemon);
      await waitForExit(state.daemon, 10_000);
    }
    if (state.codexHome && !state.keep) {
      try {
        rmSync(state.codexHome, { recursive: true, force: true });
      } catch {}
    }
  };

  try {
    const repoRoot = repoRootOf();
    const realHome = realCodexHome();
    const paths = assertSafePaths(options, repoRoot, realHome);
    state.dataDir = paths.dataDir;
    const serverPath = resolveServer(options.server);
    const adapterPath = resolveAdapter(options.adapter);
    const adapterPackage = findAdapterPackage(adapterPath);
    const prepared = prepareDirs(options, paths);
    state.createdDataDir = prepared.createdDataDir;
    state.codexHome = paths.codexHome;
    const credentials = copyCredentials(realHome, paths.codexHome);
    const workspace = paths.workspace;

    report.environment = {
      platform: process.platform,
      arch: process.arch,
      node: process.version,
      serverPath,
      serverSha256: sha256Hex(readFileSync(serverPath)),
      adapterPath,
      adapterSha256: sha256Hex(readFileSync(adapterPath)),
      adapterPackage,
      dataDir: paths.dataDir,
      codexHome: paths.codexHome,
      realCodexHome: realHome,
      workspace,
      credentialsCopied: credentials.copied,
      credentialsMissing: credentials.missing,
      timeoutMs: options.timeoutMs,
    };
    recordCheck(engine, "environment", "environment.server_binary", "PASS", "acp-remote 可执行文件已解析", {
      path: serverPath,
    });
    recordCheck(engine, "environment", "environment.adapter_resolved", "PASS", "codex-acp 适配器已解析", {
      path: adapterPath,
      package: adapterPackage,
    });
    recordCheck(engine, "environment", "environment.isolated_data_dir", "PASS", "data-dir 是独立目录", {
      dataDir: paths.dataDir,
    });
    recordCheck(engine, "environment", "environment.codex_home_isolated", "PASS", "codex-home 与真实 CODEX_HOME 分离", {
      codexHome: paths.codexHome,
    });
    if (credentials.copied.includes("auth.json")) {
      recordCheck(engine, "environment", "environment.credentials_copied", "PASS", "已复制 Codex 凭据（只记录文件名）", {
        copied: credentials.copied,
        missing: credentials.missing,
      });
    } else {
      recordCheck(
        engine,
        "environment",
        "environment.credentials_copied",
        "SKIP",
        "真实 CODEX_HOME 中没有 auth.json：不伪造登录，后续真实 turn 可能失败",
        { copied: credentials.copied, missing: credentials.missing },
      );
    }

    const cert =
      options.cert && options.key
        ? { certPath: resolve(options.cert), keyPath: resolve(options.key) }
        : generateCertificate(paths.dataDir);
    for (const path of [cert.certPath, cert.keyPath]) {
      if (!existsSync(path)) throw new ProbeError(`TLS 文件不存在：${path}`);
    }
    recordCheck(engine, "environment", "environment.tls_certificate", "PASS", "TLS 证书与私钥已就绪", {
      certPath: cert.certPath,
      keyPath: cert.keyPath,
      provided: Boolean(options.cert && options.key),
    });

    const port = options.port ?? (await freePort());
    const publicOrigin = `https://localhost:${port}`;
    const hostHeader = `localhost:${port}`;
    report.environment.listen = { bind: "127.0.0.1", port, publicOrigin };
    const logPath = join(paths.dataDir, "daemon.log");
    const wrapperPath = writeAgentWrapper(paths.dataDir);

    state.marker = `prep-marker-${randomBytes(8).toString("hex")}`;
    state.firstMessage = `Reply with exactly this token and nothing else: ${state.marker}`;
    report.marker = state.marker;

    const configPath = writeConfig({
      dataDir: paths.dataDir,
      port,
      publicOrigin,
      certPath: cert.certPath,
      keyPath: cert.keyPath,
      logPath,
      agentCommand: [process.execPath, wrapperPath, adapterPath, join(paths.dataDir, "agent-evidence.ndjson")],
    });
    report.environment.configPath = configPath;

    // ── 第一次运行：启动 / 配对 ─────────────────────────────────────────────────────────────
    state.daemon = daemonSpawn(paths.dataDir, serverPath, configPath, paths.codexHome, state.firstMessage);
    const readinessStart = Date.now();
    await waitDaemonReady(port, hostHeader, publicOrigin, state.daemon, options.timeoutMs);
    engine.timings.daemon_ready_ms = Date.now() - readinessStart;
    recordCheck(engine, "environment", "environment.daemon_ready", "PASS", "Daemon 在独立端口上就绪", {
      port,
      elapsedMs: engine.timings.daemon_ready_ms,
    });

    state.deviceKey = await generateDeviceKey();
    state.deviceKey.deviceId = randomUUID();
    const pairing = await pairDevice({
      serverPath,
      configPath,
      dataDir: paths.dataDir,
      port,
      hostHeader,
      publicOrigin,
      deviceKey: state.deviceKey,
      timeoutMs: options.timeoutMs,
      state,
    });
    report.pairing = pairing.report;

    let client = await SyncClient.connect({ port, hostHeader, origin: publicOrigin, timeoutMs: options.timeoutMs });
    const authenticated = await client.authenticate({
      hostId: pairing.hostId,
      deviceKey: state.deviceKey,
      origin: publicOrigin,
      clientNonce: randomBytes(32),
      serverPublicKey: await importRawPublicKey(fromB64url(pairing.hostPublicKey)),
    });
    if (!client.negotiated.includes(PREPARATION_FEATURE)) {
      throw new ProbeError(
        `服务端没有协商 ${PREPARATION_FEATURE}（会话准备端口未装配）：${JSON.stringify(client.negotiated)}`,
      );
    }
    recordCheck(
      engine,
      "environment",
      "environment.feature_negotiated",
      "PASS",
      "业务连接协商到会话准备 feature",
      {
        selectedFeatures: client.negotiated,
        serverEpoch: authenticated.serverEpoch,
        hostProofVerified: client.hostProofVerified === true,
      },
    );
    if (client.hostProofVerified !== true) {
      engine.notes.push("业务连接的 hostProof 没有通过验证（只记录，不影响准备判据）");
    }

    // ── allocate + configure ──────────────────────────────────────────────────────────────
    const tabNonce = randomBytes(32);
    const allocateStart = Date.now();
    const allocated = await preparation(client, {
      operationId: randomUUID(),
      action: "allocate",
      agentId: "codex",
      workspace: { workspacePath: workspace.replaceAll("\\", "/") },
      tabNonce: b64url(tabNonce),
    });
    engine.timings.allocate_ms = Date.now() - allocateStart;
    if (allocated.body.status !== "ready") {
      throw new ProbeError(`allocate 不是 ready：${JSON.stringify(allocated.body)}`);
    }
    state.sessionId = allocated.body.sessionId;
    state.receipt = allocated.body.receipt;
    state.runtimeEpoch = allocated.body.runtimeEpoch;
    state.generation = allocated.body.generation;
    recordCheck(engine, "allocate", "allocate.ready", "PASS", "Owner 分配了准备会话并签发凭据", {
      sessionId: state.sessionId,
      runtimeEpoch: state.runtimeEpoch,
      generation: state.generation,
      leaseExpiresAt: allocated.body.leaseExpiresAt,
      elapsedMs: engine.timings.allocate_ms,
    });
    const modes = Array.isArray(allocated.body.modes) ? allocated.body.modes : [];
    const configOptions = Array.isArray(allocated.body.configOptions) ? allocated.body.configOptions : [];
    recordCheck(
      engine,
      "allocate",
      "allocate.real_modes",
      modes.length > 0 ? "PASS" : "FAIL",
      modes.length > 0 ? "模式来自真实 Agent（session/new 的 modes）" : "Agent 没有回报任何可用模式",
      { modes },
    );
    recordCheck(
      engine,
      "allocate",
      "allocate.real_config",
      configOptions.length > 0 ? "PASS" : "SKIP",
      configOptions.length > 0 ? "配置项来自真实 Agent" : "真实 Agent 没有宣告可调整配置项（不伪造）",
      { configOptions },
    );

    // 选一个非当前模式（若可用），并选一个非当前配置值；两者都必须真的下发到适配器。
    const selectedMode = modes.length > 1 ? modes[modes.length - 1].modeId : null;
    const selectedConfig = pickConfigSelection(configOptions);
    if (selectedMode !== null || selectedConfig !== null) {
      const configureBody = {
        operationId: randomUUID(),
        action: "configure",
        sessionId: state.sessionId,
        receipt: state.receipt,
        runtimeEpoch: state.runtimeEpoch,
        generation: state.generation,
      };
      if (selectedMode !== null) configureBody.mode = selectedMode;
      if (selectedConfig !== null) configureBody.configOptions = { [selectedConfig.id]: selectedConfig.value };
      const configureStart = Date.now();
      const configured = await preparation(client, configureBody);
      engine.timings.configure_ms = Date.now() - configureStart;
      const applied =
        configured.body.status === "ready" &&
        (selectedMode === null || !(configured.body.unavailable ?? []).includes(selectedMode)) &&
        (selectedConfig === null || !(configured.body.unavailable ?? []).includes(selectedConfig.id));
      recordCheck(
        engine,
        "allocate",
        "allocate.configure_applied",
        applied ? "PASS" : "FAIL",
        applied ? "模式/配置经 configure 成功应用到临时端点" : `configure 未成功：${JSON.stringify(configured.body)}`,
        {
          selectedMode,
          selectedConfig,
          status: configured.body.status,
          unavailable: configured.body.unavailable ?? null,
          elapsedMs: engine.timings.configure_ms,
          sent: {
            mode: configureBody.mode ?? null,
            configOptions: configureBody.configOptions ?? null,
            runtimeEpochType: typeof configureBody.runtimeEpoch,
            generation: configureBody.generation ?? null,
            receiptKeys: Object.keys(configureBody.receipt ?? {}),
          },
        },
      );
      state.selectedMode = selectedMode;
      state.selectedConfig = selectedConfig;
    } else {
      recordCheck(
        engine,
        "allocate",
        "allocate.configure_applied",
        "SKIP",
        "真实 Agent 既没有第二模式也没有可选配置：没有可下发的改动（不伪造成功）",
        { modes: modes.length, configOptions: configOptions.length },
      );
    }

    // 准备期间没有正式会话：`session.read` 必须是权威 not_found。
    const beforeRead = randomUUID();
    await client.command(beforeRead, "session.read", state.sessionId, { include: ["messages"] });
    const beforeResult = (await client.expectResult(beforeRead, SHORT_TIMEOUT_MS)).body;
    const notFound =
      beforeResult.status === "rejected" && beforeResult.error?.code === "session.not_found";
    recordCheck(
      engine,
      "allocate",
      "allocate.no_formal_session",
      notFound ? "PASS" : "FAIL",
      notFound ? "准备期间没有正式会话行（session.read 回 session.not_found）" : "准备期间已存在正式会话",
      { result: beforeResult },
    );

    const evidenceBefore = readEvidence(join(paths.dataDir, "agent-evidence.ndjson"));
    const promptBeforeKill = evidenceBefore.filter((record) => record.type === "prompt");
    const resumeBeforeKill = evidenceBefore.filter(
      (record) => record.type === "request" && record.method === "session/resume",
    );
    recordCheck(
      engine,
      "allocate",
      "allocate.no_prompt_before_first_message",
      promptBeforeKill.length === 0 ? "PASS" : "FAIL",
      promptBeforeKill.length === 0
        ? "显式 session.start 之前适配器没有收到任何 prompt"
        : "适配器在首条提交前收到 prompt（自动派发）",
      { prompts: promptBeforeKill.length },
    );
    recordCheck(
      engine,
      "allocate",
      "allocate.no_formal_resume",
      resumeBeforeKill.length === 0 ? "PASS" : "FAIL",
      resumeBeforeKill.length === 0 ? "没有退回旧正式恢复（无 session/resume）" : "出现了 session/resume",
      { resumes: resumeBeforeKill.length },
    );
    report.allocate = {
      sessionId: state.sessionId,
      runtimeEpoch: state.runtimeEpoch,
      generation: state.generation,
      modes,
      configOptions,
      selectedMode: state.selectedMode ?? null,
      selectedConfig: state.selectedConfig ?? null,
      readBeforeCommit: beforeResult,
    };

    // ── 强制终止并重启 ───────────────────────────────────────────────────────────────────
    const oldPids = {
      daemon: state.daemon.pid,
      agent: evidenceBefore.filter((record) => record.type === "adapter_start").at(-1)?.pid ?? null,
    };
    const killStart = Date.now();
    killTree(state.daemon);
    const exited = await waitForExit(state.daemon, 15_000);
    if (!exited) throw new ProbeError("Daemon 未在强制终止后退出");
    // 等 Agent 进程树真正消失（Windows 上 taskkill /T 是同步枚举，但句柄释放有延迟）。
    const treeDeadline = Date.now() + 15_000;
    let treeDead = false;
    while (Date.now() < treeDeadline) {
      if (!processAlive(oldPids.agent) && !processAlive(oldPids.daemon)) {
        treeDead = true;
        break;
      }
      await sleep(100);
    }
    recordCheck(
      engine,
      "restart",
      "restart.previous_process_tree_terminated",
      treeDead ? "PASS" : "FAIL",
      treeDead ? "强制终止后 Daemon 与 Agent 进程树都已消失" : "强制终止后仍有进程存活（进程树泄漏）",
      { daemonPid: oldPids.daemon, agentPid: oldPids.agent, killElapsedMs: Date.now() - killStart },
    );

    const restartStart = Date.now();
    state.daemon = daemonSpawn(paths.dataDir, serverPath, configPath, paths.codexHome, state.firstMessage);
    await waitDaemonReady(port, hostHeader, publicOrigin, state.daemon, options.timeoutMs);
    engine.timings.restart_ready_ms = Date.now() - restartStart;
    client = await SyncClient.connect({ port, hostHeader, origin: publicOrigin, timeoutMs: options.timeoutMs });
    await client.authenticate({
      hostId: pairing.hostId,
      deviceKey: state.deviceKey,
      origin: publicOrigin,
      clientNonce: randomBytes(32),
      serverPublicKey: await importRawPublicKey(fromB64url(pairing.hostPublicKey)),
    });

    const restoreStart = Date.now();
    const restored = await preparation(client, {
      operationId: randomUUID(),
      action: "restore",
      sessionId: state.sessionId,
      receipt: state.receipt,
      knownRuntimeEpoch: state.runtimeEpoch,
      knownGeneration: state.generation,
    });
    engine.timings.restore_ms = Date.now() - restoreStart;
    const restoredBody = restored.body;
    const sameId = restoredBody.status === "ready" && restoredBody.sessionId === state.sessionId;
    recordCheck(
      engine,
      "restart",
      "restart.same_id_restored",
      sameId ? "PASS" : "FAIL",
      sameId
        ? "重启后同一 receipt restore 回到同一 sessionId 且 ready"
        : `restore 未回到同一 ID：${JSON.stringify(restoredBody)}`,
      {
        status: restoredBody.status,
        sessionId: restoredBody.sessionId,
        runtimeEpoch: restoredBody.runtimeEpoch,
        generation: restoredBody.generation,
        elapsedMs: engine.timings.restore_ms,
      },
    );
    if (!sameId) throw new ProbeError("restore 未回到同一准备会话，后续判据不再执行");
    recordCheck(
      engine,
      "restart",
      "restart.new_runtime_epoch",
      restoredBody.runtimeEpoch !== state.runtimeEpoch ? "PASS" : "FAIL",
      restoredBody.runtimeEpoch !== state.runtimeEpoch
        ? "运行期资源代在重启后改变"
        : "运行期资源代没有改变（可能是旧进程仍在服务）",
      { before: state.runtimeEpoch, after: restoredBody.runtimeEpoch },
    );
    state.runtimeEpoch = restoredBody.runtimeEpoch;
    state.generation = restoredBody.generation;
    state.receipt = restoredBody.receipt ?? state.receipt;

    const evidenceAfterRestart = await waitForEvidence(
      join(paths.dataDir, "agent-evidence.ndjson"),
      (records) => records.some((record) => record.type === "adapter_start" && record.pid !== oldPids.agent),
      options.timeoutMs,
    );
    const newAgent = evidenceAfterRestart.filter((record) => record.type === "adapter_start").at(-1);
    recordCheck(
      engine,
      "restart",
      "restart.new_endpoint_process",
      newAgent !== undefined && newAgent.pid !== oldPids.agent ? "PASS" : "FAIL",
      newAgent !== undefined && newAgent.pid !== oldPids.agent
        ? "临时端点由新的适配器进程重建"
        : "重启后的适配器进程与终止前相同（旧进程未消失）",
      { before: oldPids.agent, after: newAgent?.pid ?? null },
    );

    // 重启后重新应用非默认设置（恢复后模式/配置以 Agent 确认为准）。
    if (state.selectedMode !== null || state.selectedConfig !== null) {
      const body = {
        operationId: randomUUID(),
        action: "configure",
        sessionId: state.sessionId,
        receipt: state.receipt,
        runtimeEpoch: state.runtimeEpoch,
        generation: state.generation,
      };
      if (state.selectedMode !== null) body.mode = state.selectedMode;
      if (state.selectedConfig !== null) body.configOptions = { [state.selectedConfig.id]: state.selectedConfig.value };
      await preparation(client, body);
    }

    const afterRestartEvidence = readEvidence(join(paths.dataDir, "agent-evidence.ndjson"));
    const promptsAfterRestart = afterRestartEvidence.filter((record) => record.type === "prompt");
    const resumesAfterRestart = afterRestartEvidence.filter(
      (record) => record.type === "request" && record.method === "session/resume",
    );
    recordCheck(
      engine,
      "restart",
      "restart.no_prompt_before_first_message",
      promptsAfterRestart.length === 0 ? "PASS" : "FAIL",
      promptsAfterRestart.length === 0
        ? "重启+restore 之后、显式 session.start 之前没有自动 prompt"
        : "重启之后出现了自动 prompt（未等待用户提交）",
      { prompts: promptsAfterRestart.length },
    );
    recordCheck(
      engine,
      "restart",
      "restart.no_formal_resume",
      resumesAfterRestart.length === 0 ? "PASS" : "FAIL",
      resumesAfterRestart.length === 0 ? "重启后没有退回旧正式恢复" : "重启后出现 session/resume",
      { resumes: resumesAfterRestart.length },
    );
    report.restart = {
      killElapsedMs: engine.timings.restart_ready_ms,
      restoreElapsedMs: engine.timings.restore_ms,
      beforeRuntimeEpoch: report.allocate.runtimeEpoch,
      afterRuntimeEpoch: state.runtimeEpoch,
      oldAgentPid: oldPids.agent,
      newAgentPid: newAgent?.pid ?? null,
    };

    // ── 首条消息 ─────────────────────────────────────────────────────────────────────────
    state.requestId = randomUUID();
    const startBody = {
      kind: "initial",
      receipt: state.receipt,
      runtimeEpoch: state.runtimeEpoch,
      generation: state.generation,
      content: [{ type: "text", text: state.firstMessage }],
    };
    if (state.selectedMode !== null) startBody.mode = state.selectedMode;
    if (state.selectedConfig !== null) startBody.configOptions = { [state.selectedConfig.id]: state.selectedConfig.value };
    const startAt = Date.now();
    await client.command(state.requestId, "session.start", state.sessionId, startBody);
    const accepted = (await client.expectResult(state.requestId, options.timeoutMs)).body;
    const acceptedMs = Date.now() - startAt;
    state.acceptedAt = Date.now();
    engine.timings.session_start_accepted_ms = acceptedMs;

    const completed = (await client.expectResult(state.requestId, options.timeoutMs)).body;
    engine.timings.session_start_completed_ms = Date.now() - startAt;
    const acceptedThenCompleted = accepted.status === "accepted" && completed.status === "completed";
    recordCheck(
      engine,
      "first_message",
      "first_message.accepted_then_completed",
      acceptedThenCompleted ? "PASS" : "FAIL",
      acceptedThenCompleted
        ? "session.start 先 accepted、后 completed"
        : `session.start 终态异常：accepted=${JSON.stringify(accepted)} completed=${JSON.stringify(completed)}`,
      {
        acceptedStatus: accepted.status,
        completedStatus: completed.status,
        acceptedMs,
        completedMs: engine.timings.session_start_completed_ms,
      },
    );

    const readRequest = randomUUID();
    await client.command(readRequest, "session.read", state.sessionId, { include: ["messages"] });
    const readResult = (await client.expectResult(readRequest, options.timeoutMs)).body;
    const messages = readResult.result?.resources?.messages ?? [];
    const userMessages = messages.filter((message) => message.role === "user");
    const userText = messageText(userMessages[0]);
    const readback = userMessages.length === 1 && userText === state.firstMessage;
    recordCheck(
      engine,
      "first_message",
      "first_message.user_text_readback",
      readback ? "PASS" : "FAIL",
      readback
        ? "首条原文在 session.read 里逐字可读且只有一条"
        : `用户正文读取不一致：${JSON.stringify(userMessages)}`,
      { userMessages: userMessages.map((message) => ({ messageId: message.messageId, text: messageText(message) })) },
    );

    const assistantText = messages
      .filter((message) => message.role === "assistant")
      .map((message) => messageText(message))
      .join("\n");
    const markerDelivered = assistantText.includes(state.marker);
    recordCheck(
      engine,
      "first_message",
      "first_message.marker_delivered",
      markerDelivered ? "PASS" : "FAIL",
      markerDelivered ? "真实 Codex 回复带有本次随机标记" : "真实 Codex 回复里没有本次随机标记",
      { assistantTextLength: assistantText.length, marker: state.marker },
    );

    const finalEvidence = await waitForEvidence(
      join(paths.dataDir, "agent-evidence.ndjson"),
      (records) => records.some((record) => record.type === "prompt"),
      options.timeoutMs,
    );
    const prompts = finalEvidence.filter((record) => record.type === "prompt");
    const resumes = finalEvidence.filter(
      (record) => record.type === "request" && record.method === "session/resume",
    );
    recordCheck(
      engine,
      "first_message",
      "first_message.exactly_one_prompt",
      prompts.length === 1 ? "PASS" : "FAIL",
      prompts.length === 1 ? "适配器恰好收到一次 prompt" : `适配器收到 ${prompts.length} 次 prompt`,
      { prompts: prompts.map((record) => ({ length: record.length, matchesExpected: record.matchesExpected })) },
    );
    recordCheck(
      engine,
      "first_message",
      "first_message.no_hidden_prompt",
      prompts.length === 1 && prompts[0].matchesExpected === true ? "PASS" : "FAIL",
      prompts.length === 1 && prompts[0].matchesExpected === true
        ? "唯一的 prompt 与提交正文逐字相同（没有隐藏/改写 prompt）"
        : "prompt 正文与提交正文不一致，或数量不是 1",
      {
        expectedSha256: prompts[0]?.expectedSha256 ?? null,
        actualSha256: prompts[0]?.textSha256 ?? null,
        length: prompts[0]?.length ?? null,
      },
    );
    recordCheck(
      engine,
      "first_message",
      "first_message.no_formal_resume",
      resumes.length === 0 ? "PASS" : "FAIL",
      resumes.length === 0 ? "整次运行没有 session/resume" : `出现 ${resumes.length} 次 session/resume`,
      { resumes: resumes.length },
    );

    const statusRequest = randomUUID();
    await client.command(statusRequest, "command.status", null, { targetRequestId: state.requestId });
    const statusResult = (await client.expectResult(statusRequest, options.timeoutMs)).body;
    const projection = statusResult.result?.preparation ?? null;
    const terminal =
      statusResult.status === "completed" &&
      projection !== null &&
      projection.stage === "terminal" &&
      projection.firstRequestId === state.requestId &&
      projection.everDispatched === true;
    recordCheck(
      engine,
      "first_message",
      "first_message.terminal_projection",
      terminal ? "PASS" : "FAIL",
      terminal
        ? "command.status 的首条投影落到 terminal 且请求身份一致"
        : `首条投影不符合预期：${JSON.stringify(projection)}`,
      { status: statusResult.status, preparation: projection },
    );

    report.firstMessage = {
      sessionId: state.sessionId,
      requestId: state.requestId,
      acceptedStatus: accepted.status,
      completedStatus: completed.status,
      userMessages: userMessages.map((message) => message.messageId),
      assistantLength: assistantText.length,
      timings: {
        acceptedMs,
        completedMs: engine.timings.session_start_completed_ms,
      },
      projection,
    };
    report.agent = {
      wrapperStarts: finalEvidence.filter((record) => record.type === "wrapper_start").length,
      adapterStarts: finalEvidence.filter((record) => record.type === "adapter_start").map((record) => record.pid),
      sessionNew: finalEvidence.filter((record) => record.type === "request" && record.method === "session/new").length,
      sessionResume: resumes.length,
      prompts: prompts.length,
      setModes: finalEvidence
        .filter((record) => record.type === "request" && record.method === "session/set_mode")
        .map((record) => record.modeId),
      setConfigs: finalEvidence
        .filter((record) => record.type === "request" && record.method === "session/set_config_option")
        .map((record) => ({ configId: record.configId })),
    };

    report.notes.push(
      "准备附件（attachment begin/manifest/chunk 的 target 分支）不在本探针内驱动：真实 daemon 的附件上传/封存/认领分段由真实浏览器验收（session-preparation-live.mjs）记录；本层不伪造 SKIP 为 PASS。",
    );
    report.timings = engine.timings;
    client.close();
  } catch (error) {
    for (const stage of Object.keys(STAGE_CHECKS)) {
      skipRemaining(engine, stage, `阶段因错误提前结束：${shortError(error)}`);
      if (!engine.checkIds.has(`${stage}.completed`)) {
        const failed = engine.checks.some((check) => check.stage === stage && check.status === "FAIL");
        recordCheck(engine, stage, `${stage}.completed`, failed ? "FAIL" : "SKIP", `阶段未完成：${shortError(error)}`, {
          error: describeError(error),
        });
      }
    }
    recordCheck(engine, "probe", "probe.fatal", "FAIL", `致命错误：${shortError(error)}`, {
      error: describeError(error),
    });
    report.exitCode = 2;
  } finally {
    await cleanup();
  }

  // 阶段汇总（只补未写过的）。
  for (const stage of Object.keys(STAGE_CHECKS)) {
    if (engine.checkIds.has(`${stage}.completed`)) continue;
    const hadFailure = engine.checks.some((check) => check.stage === stage && check.status === "FAIL");
    if (!hadFailure) skipRemaining(engine, stage, "阶段未完成");
    summarizeStage(engine, stage);
  }

  report.checks = engine.checks;
  report.notes = [...engine.notes, ...report.notes];
  report.summary = {
    total: report.checks.length,
    pass: report.checks.filter((check) => check.status === "PASS").length,
    fail: report.checks.filter((check) => check.status === "FAIL").length,
    skip: report.checks.filter((check) => check.status === "SKIP").length,
    failed: report.checks.filter((check) => check.status === "FAIL").map((check) => check.id),
    skipped: report.checks.filter((check) => check.status === "SKIP").map((check) => check.id),
  };
  report.finishedAt = nowIso();
  if (report.exitCode !== 2) {
    report.exitCode = report.summary.fail > 0 ? 1 : 0;
  }

  // 报告的脱敏自检：任何秘密（配对秘密/设备私钥）都不得出现在证据文本里。
  let serialized = JSON.stringify(report, null, 2) + "\n";
  const leaks = [];
  if (state.secret !== null && serialized.includes(b64url(state.secret))) leaks.push("pairingSecret");
  if (state.deviceKey?.privateJwk?.d && serialized.includes(state.deviceKey.privateJwk.d)) {
    leaks.push("devicePrivateKey");
  }
  if (leaks.length > 0) {
    recordCheck(engine, "probe", "probe.no_secrets_in_report", "FAIL", `报告里出现了秘密字段：${leaks.join(",")}`, {
      leaks,
    });
    report.checks = engine.checks;
    report.summary = {
      ...report.summary,
      total: report.checks.length,
      fail: report.checks.filter((check) => check.status === "FAIL").length,
      failed: report.checks.filter((check) => check.status === "FAIL").map((check) => check.id),
    };
    report.exitCode = 1;
    serialized = JSON.stringify(report, null, 2) + "\n";
  } else {
    recordCheck(engine, "probe", "probe.no_secrets_in_report", "PASS", "报告文本不含配对秘密或设备私钥", {});
    report.checks = engine.checks;
    report.summary = {
      ...report.summary,
      total: report.checks.length,
      pass: report.checks.filter((check) => check.status === "PASS").length,
      fail: report.checks.filter((check) => check.status === "FAIL").length,
      failed: report.checks.filter((check) => check.status === "FAIL").map((check) => check.id),
    };
    serialized = JSON.stringify(report, null, 2) + "\n";
  }

  const jsonOut = options.jsonOut ? resolve(options.jsonOut) : join(state.dataDir, "report.json");
  try {
    mkdirSync(dirname(jsonOut), { recursive: true });
    writeFileSync(jsonOut, serialized, "utf8");
  } catch (error) {
    process.stderr.write(`${TOOL_NAME}: 报告写入失败：${shortError(error)}\n`);
    return 2;
  }
  process.stderr.write(
    `${TOOL_NAME}: ${report.summary.pass} passed, ${report.summary.fail} failed, ` +
      `${report.summary.skip} skipped → exit ${report.exitCode}\n` +
      `report: ${jsonOut}\n`,
  );
  return report.exitCode ?? 1;
}

/**
 * 发一条准备控制帧并返回它的 `session.preparation.result` 消息（调用方按 status 断言）。
 *
 * `operationId` 必须逐字回显：迟到的旧操作结果不能冒充本次操作。
 */
async function preparation(client, body) {
  client.send("session.preparation", body);
  const message = await client.expectType("session.preparation.result", client.timeoutMs);
  if (message.body.operationId !== body.operationId) {
    throw new ProtocolError(
      `准备结果的 operationId 与请求不一致：${JSON.stringify(message.body.operationId)}`,
    );
  }
  return message;
}

function messageText(message) {
  if (!message || !Array.isArray(message.content)) return "";
  return message.content
    .filter((block) => block && block.type === "text" && typeof block.text === "string")
    .map((block) => block.text)
    .join("");
}

function pickConfigSelection(configOptions) {
  // `category: "mode"` 的选项与 modes 是同一维度；改模式走 `mode`/`set_mode`，
  // 不要通过 `configOptions` 再设一次（真实 codex-acp 会拒绝）。
  for (const option of configOptions) {
    if (option.category === "mode") continue;
    if (option.type === "select" && Array.isArray(option.options)) {
      const choice = [...option.options].reverse().find((entry) => entry.value !== option.currentValue);
      if (choice) return { id: option.id, value: choice.value };
    }
  }
  for (const option of configOptions) {
    if (option.type === "boolean") {
      return { id: option.id, value: option.currentValue !== true };
    }
  }
  return null;
}

function findAdapterPackage(adapterPath) {
  let dir = dirname(adapterPath);
  for (let level = 0; level < 6; level += 1) {
    const candidate = join(dir, "package.json");
    if (existsSync(candidate)) {
      try {
        const json = JSON.parse(readFileSync(candidate, "utf8"));
        return { name: json.name ?? null, version: json.version ?? null };
      } catch {
        return null;
      }
    }
    const parent = dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }
  return null;
}

/**
 * 配对：本地管理通道 `device pair` 负责 begin + 本机确认；探针自己完成 lookup/claim/status。
 *
 * 不打印、不写盘任何配对码或 secret；配对码只在本次调用内用于 lookup。
 */
async function pairDevice({ serverPath, configPath, dataDir, port, hostHeader, publicOrigin, deviceKey, timeoutMs, state }) {
  const pairingCli = await startPairCli({
    serverPath,
    configPath,
    fingerprint: deviceKey.fingerprint,
    dataDir,
    timeoutMs,
  });
  try {
    const lookup = await httpsJson(port, hostHeader, publicOrigin, "/sync/v1/pairing/lookup", {
      protocolVersion: 1,
      pairingCode: pairingCli.pairingCode,
    });
    if (lookup.status !== 200) {
      throw new ProbeError(`pairing/lookup 失败：HTTP ${lookup.status}`);
    }
    const ticket = lookup.body;
    const secret = fromB64url(ticket.pairingSecret);
    state.secret = secret;
    if (ticket.canonicalOrigin !== publicOrigin) {
      throw new ProbeError(`lookup 返回的 canonicalOrigin 不是本机 origin：${ticket.canonicalOrigin}`);
    }
    const deviceId = deviceKey.deviceId;
    const clientNonce = randomBytes(32);
    const claimStart = Date.now();
    const claim = await httpsJson(port, hostHeader, publicOrigin, "/sync/v1/pairing/claim", {
      protocolVersion: 1,
      pairingId: ticket.pairingId,
      hostId: ticket.hostId,
      deviceId,
      deviceName: "Preparation Probe",
      clientKind: "desktop",
      canonicalOrigin: ticket.canonicalOrigin,
      devicePublicKey: deviceKey.publicB64url,
      clientNonce: b64url(clientNonce),
      proof: b64url(
        hmacSha256(
          secret,
          encodeTranscript(
            DOMAIN.pairingProof,
            pairingProofFields({
              protocolVersion: 1,
              hostId: ticket.hostId,
              deviceId,
              pairingId: ticket.pairingId,
              pairingExpiresAtUnixSeconds: Math.floor(Date.parse(ticket.expiresAt) / 1000),
              canonicalOrigin: ticket.canonicalOrigin,
              devicePublicKey: deviceKey.publicB64url,
              clientNonceHex: clientNonce.toString("hex"),
              deviceName: "Preparation Probe",
              clientKind: "desktop",
            }),
          ),
        ),
      ),
    });
    if (claim.status !== 201) {
      throw new ProbeError(`pairing/claim 失败：HTTP ${claim.status} ${JSON.stringify(claim.body)}`);
    }
    // 验证 Host proof：节点身份必须与扫码取回的主机公钥一致。
    const hostPublicKey = await importRawPublicKey(fromB64url(ticket.hostPublicKey));
    const serverNonce = fromB64url(claim.body.serverNonce);
    const hostProofOk = await verifyP1363(
      hostPublicKey,
      encodeTranscript(
        DOMAIN.pairingHostProof,
        pairingHostProofFields({
          protocolVersion: 1,
          hostId: ticket.hostId,
          deviceId,
          pairingId: ticket.pairingId,
          canonicalOrigin: ticket.canonicalOrigin,
          hostPublicKey: ticket.hostPublicKey,
          devicePublicKey: deviceKey.publicB64url,
          clientNonceHex: clientNonce.toString("hex"),
          serverNonceHex: serverNonce.toString("hex"),
          pairingRequestId: claim.body.pairingRequestId,
        }),
      ),
      fromB64url(claim.body.hostProof),
    );
    if (!hostProofOk) throw new ProbeError("配对 Host proof 验证失败（hostPublicKey 不可信）");
    const claimMs = Date.now() - claimStart;

    // 等 CLI 完成本机确认（它轮询到 claimed 后确认并退出）。
    const cliExit = await waitForExit(pairingCli.child, timeoutMs);
    if (!cliExit || pairingCli.child.exitCode !== 0) {
      throw new ProbeError(`device pair CLI 未成功完成（exit=${pairingCli.child.exitCode}）`);
    }

    const requestNonce = randomBytes(32);
    const statusStart = Date.now();
    const status = await httpsJson(port, hostHeader, publicOrigin, "/sync/v1/pairing/status", {
      protocolVersion: 1,
      hostId: ticket.hostId,
      deviceId,
      pairingId: ticket.pairingId,
      pairingRequestId: claim.body.pairingRequestId,
      requestNonce: b64url(requestNonce),
      proof: b64url(
        hmacSha256(
          secret,
          encodeTranscript(
            DOMAIN.pairingStatus,
            pairingStatusFields({
              protocolVersion: 1,
              hostId: ticket.hostId,
              deviceId,
              pairingId: ticket.pairingId,
              pairingRequestId: claim.body.pairingRequestId,
              requestNonceHex: requestNonce.toString("hex"),
            }),
          ),
        ),
      ),
    });
    if (status.status !== 200 || status.body.status !== "approved") {
      throw new ProbeError(`pairing/status 未经批准：HTTP ${status.status} ${JSON.stringify(status.body)}`);
    }
    return {
      hostId: ticket.hostId,
      hostPublicKey: ticket.hostPublicKey,
      report: {
        pairingId: ticket.pairingId,
        hostId: ticket.hostId,
        deviceId,
        deviceName: "Preparation Probe",
        hostProofVerified: hostProofOk,
        claimMs,
        statusMs: Date.now() - statusStart,
        approved: true,
      },
    };
  } finally {
    if (pairingCli.child.exitCode === null) killTree(pairingCli.child);
  }
}

process.on("SIGINT", () => {
  process.stderr.write(`${TOOL_NAME}: interrupted\n`);
  process.exit(130);
});

const exitCode = await main();
process.exitCode = exitCode;
