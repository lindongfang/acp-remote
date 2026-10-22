#!/usr/bin/env node
/**
 * 本机会话生命周期的容量基准（WP6 / PV6 的 10/25/50 档证据）。
 *
 * 在**隔离**的 data-dir / codex-home 里启动真实 `codex-acp` 适配器，在一个共享 Agent 进程内建立
 * N 条会话，测量：
 *
 *   1. 建立耗时（每会话的 `session/new` 与首轮 `session/prompt`）；
 *   2. 建立后进程树的进程数与 WorkingSet（区分「保留资源」与「执行任务负载」：prompt 结束后再采一次）；
 *   3. 切页/恢复耗时分布：逐会话 `session/close` 再 `session/resume`（每轮一个 cycle）；
 *   4. 全量 close 后与适配器退出后的进程树（卸载效果）。
 *
 * 报告**不预填阈值、不给结论**：只写测量值与 `status`（`measured` / `failed`）。10/25/50 是否可接受
 * 由主检出的验收人根据报告判断；50 档失败时报告保留该档数据并写入 `unverifiedTiers`，不写成通过。
 *
 * 用法与安全边界见 `--help`。退出码：0 全部档位测量完成；1 至少一档失败（数据保留）；2 参数/环境错误。
 * 本文件只用 Node 内置模块，不依赖仓库 package.json。
 */

import { execFileSync, spawn, spawnSync } from 'node:child_process';
import { createHash, randomBytes } from 'node:crypto';
import {
  copyFileSync,
  createWriteStream,
  existsSync,
  mkdirSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import os from 'node:os';
import { createInterface } from 'node:readline';
import { basename, dirname, isAbsolute, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const TOOL_NAME = 'benchmark-session-lifecycle';
const TOOL_VERSION = '1.0.0';
const PROTOCOL_VERSION = 1;
const WINDOWS = process.platform === 'win32';
const KNOWN_ADAPTER_PATH =
  'D:/tmp/wp7-agents/tools/node_modules/@agentclientprotocol/codex-acp/dist/index.js';
const DEFAULT_TIERS = [10, 25, 50];

const HELP_TEXT = `用法：node scripts/benchmark-session-lifecycle.mjs [选项]

在隔离的 data-dir / codex-home 中用真实 codex-acp 建立 N 条会话，测量进程树内存、切页/恢复耗时
与回收效果。报告不预填阈值或结论。

选项：
  --sessions <n>         要测的会话数，可重复（例如 --sessions 10 --sessions 25 --sessions 50）。
                         缺省 ${DEFAULT_TIERS.join(' / ')}；每个取值按一个独立档位测量（独立适配器进程）。
  --adapter <path>       codex-acp 的 dist/index.js（或可执行文件）。缺省依次取
                         ACPR_CODEX_ACP_ADAPTER、PATH 中的 codex-acp、已知路径
                         ${KNOWN_ADAPTER_PATH}。
  --data-dir <path>      隔离数据目录。缺省在系统临时目录创建 acpr-bench-<random>；拒绝仓库内、
                         用户家目录、真实 CODEX_HOME 与已存在的 .codex 目录。
  --codex-home <path>    隔离 Codex home。缺省 <data-dir>/codex-home；从真实 CODEX_HOME 只复制
                         auth.json 与 config.toml（若存在），只记录文件名，不读取内容。
  --json-out <path>      报告 JSON 路径。缺省 <data-dir>/report.json。
  --context-prompts <n>  每会话首轮 prompt 的次数（0 = 不建立上下文）。缺省 1。
  --cycles <n>           建立后执行几轮「全部 close → 全部 resume」。缺省 1。
  --timeout-ms <n>       单个 ACP 请求超时（毫秒）。缺省 180000。
  --keep                 保留隔离 codex-home 与全部临时产物（排障用）。
  --help, -h             只打印本说明并退出 0；不创建目录、不启动子进程、不写报告。

产物：
  <json-out>                          结构化报告（硬件、版本、每档的耗时分布、进程树采样）。
  <data-dir>/adapter-tier-<n>.stderr.log  适配器 stderr（只落盘，不回显、不入报告）。

退出码：0 全部档位测量完成；1 至少一档失败（该档数据保留）；2 参数/环境/适配器解析错误。`;

class UsageError extends Error {}
class BenchError extends Error {}
class RequestTimeoutError extends Error {
  constructor(method, timeoutMs) {
    super(`请求 ${method} 在 ${timeoutMs}ms 内没有响应`);
  }
}

const repoRoot = dirname(dirname(fileURLToPath(import.meta.url)));

async function main() {
  const argv = process.argv.slice(2);
  if (argv.includes('--help') || argv.includes('-h')) {
    // --help 在解析与任何副作用之前处理。
    process.stdout.write(HELP_TEXT + '\n');
    return 0;
  }
  const options = parseArgs(argv);
  const adapter = resolveAdapter(options.adapter);
  const realHome = realCodexHome();
  const dataDir = options.dataDir
    ? resolve(options.dataDir)
    : join(os.tmpdir(), `acpr-bench-${randomBytes(6).toString('hex')}`);
  assertSafeDataDir(dataDir, realHome);
  mkdirSync(dataDir, { recursive: true });
  const jsonOut = options.jsonOut ? resolve(options.jsonOut) : join(dataDir, 'report.json');
  mkdirSync(dirname(jsonOut), { recursive: true });

  const codexHome = options.codexHome ? resolve(options.codexHome) : join(dataDir, 'codex-home');
  assertSafeCodexHome(codexHome, realHome, dataDir);
  mkdirSync(codexHome, { recursive: true });
  const credentials = copyCredentials(realHome, codexHome);

  const report = {
    tool: TOOL_NAME,
    toolVersion: TOOL_VERSION,
    schemaVersion: 1,
    startedAt: new Date().toISOString(),
    finishedAt: null,
    argv,
    environment: {
      platform: os.platform(),
      release: os.release(),
      arch: os.arch(),
      cpuModel: os.cpus()?.[0]?.model ?? null,
      cpuCount: os.cpus()?.length ?? null,
      totalMemoryMb: Math.round(os.totalmem() / 1024 / 1024),
      freeMemoryMb: Math.round(os.freemem() / 1024 / 1024),
      node: process.version,
      adapter: {
        path: adapter.path,
        packageName: adapter.packageName,
        packageVersion: adapter.packageVersion,
        codexPackageVersion: adapter.codexPackageVersion,
        sha256: sha256File(adapter.path),
      },
      codexCli: detectCodexCliVersion(),
    },
    settings: {
      tiers: options.tiers,
      contextPrompts: options.contextPrompts,
      cycles: options.cycles,
      timeoutMs: options.timeoutMs,
      dataDir,
      codexHome,
      credentials,
    },
    // 测量值只有原始数据：不预填阈值，也不写结论。
    thresholds: null,
    conclusions: [],
    tiers: [],
    unverifiedTiers: [],
  };

  let failed = false;
  for (const sessions of options.tiers) {
    const tier = await measureTier({
      sessions,
      options,
      dataDir,
      codexHome,
      adapter,
    });
    report.tiers.push(tier);
    if (tier.status !== 'measured' || tier.unverified === true) {
      failed = true;
      report.unverifiedTiers.push(sessions);
    }
  }
  report.finishedAt = new Date().toISOString();

  writeFileSync(jsonOut, JSON.stringify(report, null, 2) + '\n', 'utf8');
  process.stderr.write(
    `${TOOL_NAME}: ${report.tiers.length} tier(s), unverified=[${report.unverifiedTiers.join(', ')}] → ${jsonOut}\n`,
  );
  if (!options.keep) {
    try {
      rmSync(codexHome, { recursive: true, force: true });
    } catch (error) {
      process.stderr.write(`清理隔离 codex-home 失败：${shortError(error)}\n`);
    }
  }
  return failed ? 1 : 0;
}

function parseArgs(argv) {
  const options = {
    tiers: [],
    adapter: null,
    dataDir: null,
    codexHome: null,
    jsonOut: null,
    contextPrompts: 1,
    cycles: 1,
    timeoutMs: 180_000,
    keep: false,
  };
  for (let index = 0; index < argv.length; index += 1) {
    const raw = argv[index];
    const [flag, inline] = raw.includes('=') ? [raw.slice(0, raw.indexOf('=')), raw.slice(raw.indexOf('=') + 1)] : [raw, null];
    const value = (name) => {
      if (inline !== null) return inline;
      index += 1;
      if (index >= argv.length) throw new UsageError(`${name} 需要一个取值`);
      return argv[index];
    };
    switch (flag) {
      case '--sessions': {
        const text = value('--sessions');
        const count = Number(text);
        if (!Number.isInteger(count) || count <= 0) {
          throw new UsageError(`--sessions 必须是正整数，收到 ${text}`);
        }
        options.tiers.push(count);
        break;
      }
      case '--adapter':
        options.adapter = value('--adapter');
        break;
      case '--data-dir':
        options.dataDir = value('--data-dir');
        break;
      case '--codex-home':
        options.codexHome = value('--codex-home');
        break;
      case '--json-out':
        options.jsonOut = value('--json-out');
        break;
      case '--context-prompts': {
        const text = value('--context-prompts');
        const count = Number(text);
        if (!Number.isInteger(count) || count < 0) {
          throw new UsageError(`--context-prompts 必须是非负整数，收到 ${text}`);
        }
        options.contextPrompts = count;
        break;
      }
      case '--cycles': {
        const text = value('--cycles');
        const count = Number(text);
        if (!Number.isInteger(count) || count <= 0) {
          throw new UsageError(`--cycles 必须是正整数，收到 ${text}`);
        }
        options.cycles = count;
        break;
      }
      case '--timeout-ms': {
        const text = value('--timeout-ms');
        const count = Number(text);
        if (!Number.isInteger(count) || count <= 0) {
          throw new UsageError(`--timeout-ms 必须是正整数，收到 ${text}`);
        }
        options.timeoutMs = count;
        break;
      }
      case '--keep':
        if (inline !== null) throw new UsageError('--keep 不接受取值');
        options.keep = true;
        break;
      default:
        throw new UsageError(`未知参数：${flag}`);
    }
  }
  if (options.tiers.length === 0) options.tiers = [...DEFAULT_TIERS];
  // 同一档位重复给出只测一次（顺序保留首次出现的次序）。
  options.tiers = [...new Set(options.tiers)];
  return options;
}

function resolveAdapter(explicit) {
  const candidate =
    explicit ??
    (process.env.ACPR_CODEX_ACP_ADAPTER ? process.env.ACPR_CODEX_ACP_ADAPTER : null) ??
    findOnPath(WINDOWS ? 'codex-acp.cmd' : 'codex-acp') ??
    findOnPath('codex-acp') ??
    (existsSync(KNOWN_ADAPTER_PATH) ? KNOWN_ADAPTER_PATH : null);
  if (!candidate) {
    throw new BenchError(
      '找不到 codex-acp 适配器。请显式传入 --adapter <dist/index.js>（已尝试 ACPR_CODEX_ACP_ADAPTER、PATH 与已知安装路径）。',
    );
  }
  const path = resolve(candidate);
  if (!existsSync(path)) throw new BenchError(`适配器不存在：${path}`);
  if (!statSync(path).isFile()) throw new BenchError(`适配器不是文件：${path}`);
  const pkg = detectAdapterPackage(path);
  return {
    path,
    packageName: pkg?.name ?? null,
    packageVersion: pkg?.version ?? null,
    codexPackageVersion: detectCodexPackageVersion(path),
  };
}

function findOnPath(command) {
  const finder = WINDOWS ? 'where' : 'which';
  const result = spawnSync(finder, [command], { encoding: 'utf8', windowsHide: true });
  if (result.status !== 0) return null;
  const first = String(result.stdout ?? '')
    .split(/\r?\n/)
    .map((line) => line.trim())
    .find((line) => line.length > 0);
  return first ?? null;
}

function detectAdapterPackage(adapterPath) {
  let directory = dirname(adapterPath);
  for (let depth = 0; depth < 12; depth += 1) {
    const candidate = join(directory, 'package.json');
    if (existsSync(candidate)) {
      try {
        const parsed = JSON.parse(readFileSync(candidate, 'utf8'));
        if (typeof parsed?.name === 'string') return parsed;
      } catch {
        return null;
      }
    }
    const parent = dirname(directory);
    if (parent === directory) return null;
    directory = parent;
  }
  return null;
}

/** 适配器依赖树里内置的 `@openai/codex` 版本（读不到就为 null，不猜）。 */
function detectCodexPackageVersion(adapterPath) {
  let directory = dirname(adapterPath);
  for (let depth = 0; depth < 8; depth += 1) {
    const candidate = join(directory, 'node_modules', '@openai', 'codex', 'package.json');
    if (existsSync(candidate)) {
      try {
        return JSON.parse(readFileSync(candidate, 'utf8'))?.version ?? null;
      } catch {
        return null;
      }
    }
    const parent = dirname(directory);
    if (parent === directory) return null;
    directory = parent;
  }
  return null;
}

function detectCodexCliVersion() {
  const result = spawnSync('codex', ['--version'], {
    encoding: 'utf8',
    timeout: 15_000,
    windowsHide: true,
  });
  if (result.status !== 0 || typeof result.stdout !== 'string') return null;
  const text = result.stdout.trim();
  return text.length > 0 && text.length <= 200 ? text : null;
}

function realCodexHome() {
  const configured = process.env.CODEX_HOME;
  return resolve(configured && configured.trim() ? configured : join(os.homedir(), '.codex'));
}

function assertSafeDataDir(dataDir, realHome) {
  if (dataDir === repoRoot || isInside(repoRoot, dataDir)) {
    throw new BenchError(`--data-dir 不能位于仓库工作树内：${dataDir}`);
  }
  if (dataDir === os.homedir()) throw new BenchError(`--data-dir 不能是用户家目录：${dataDir}`);
  if (basename(dataDir).toLowerCase() === '.codex') {
    throw new BenchError(`--data-dir 不能是 .codex 目录：${dataDir}`);
  }
  if (dataDir === realHome) throw new BenchError(`--data-dir 不能是真实 CODEX_HOME：${dataDir}`);
  if (existsSync(dataDir) && !statSync(dataDir).isDirectory()) {
    throw new BenchError(`--data-dir 已存在且不是目录：${dataDir}`);
  }
}

function assertSafeCodexHome(codexHome, realHome, dataDir) {
  if (codexHome === realHome || isInside(realHome, codexHome)) {
    throw new BenchError(`--codex-home 不能是或位于真实 CODEX_HOME 内：${codexHome}`);
  }
  if (codexHome === os.homedir()) throw new BenchError(`--codex-home 不能是用户家目录`);
  if (codexHome === dataDir) throw new BenchError(`--codex-home 不能与 --data-dir 相同`);
}

/** 只记录复制了哪些文件名，不读取/输出内容。 */
function copyCredentials(realHome, codexHome) {
  const copied = [];
  const present = [];
  const missing = [];
  for (const name of ['auth.json', 'config.toml']) {
    const source = join(realHome, name);
    const target = join(codexHome, name);
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
  return { copied, present, missing };
}

// ---------------------------------------------------------------------------------------------
// 单档测量
// ---------------------------------------------------------------------------------------------

async function measureTier({ sessions, options, dataDir, codexHome, adapter }) {
  const tierDir = join(dataDir, `tier-${sessions}`);
  mkdirSync(tierDir, { recursive: true });
  const tier = {
    sessions,
    status: 'failed',
    startedAt: new Date().toISOString(),
    finishedAt: null,
    workspaceRoot: tierDir,
    baseline: null,
    establish: { wallMs: null, perSession: [], context: [] },
    established: null,
    cycles: [],
    afterAllClosed: null,
    release: null,
    errors: [],
  };
  const connection = new Connection({
    adapter,
    codexHome,
    cwd: tierDir,
    allowedRoot: dataDir,
    timeoutMs: options.timeoutMs,
    stderrLog: join(dataDir, `adapter-tier-${sessions}.stderr.log`),
  });
  let sessionIds = [];
  try {
    await connection.start();
    tier.baseline = { ...(await sampleProcessTree(connection.pid)), at: nowIso() };
    const capabilities = await connection.initialize();
    tier.capabilities = capabilities;

    const establishStart = performance.now();
    for (let index = 0; index < sessions; index += 1) {
      const workspace = join(tierDir, `session-${String(index).padStart(3, '0')}`);
      mkdirSync(workspace, { recursive: true });
      const started = performance.now();
      const created = await connection.request('session/new', {
        cwd: workspace,
        mcpServers: [],
      });
      const createdMs = performance.now() - started;
      const sessionId = created?.sessionId;
      if (typeof sessionId !== 'string' || sessionId.length === 0) {
        throw new BenchError(`第 ${index} 条会话的 session/new 没有返回 sessionId`);
      }
      sessionIds.push({ sessionId, workspace });
      tier.establish.perSession.push({ index, sessionId, createdMs: round(createdMs) });
    }

    for (let promptIndex = 0; promptIndex < options.contextPrompts; promptIndex += 1) {
      for (const entry of sessionIds) {
        const text = `Reply with the single word OK. (bench context ${promptIndex + 1}/${options.contextPrompts})`;
        const started = performance.now();
        const updatesBefore = connection.updates;
        await connection.request(
          'session/prompt',
          { sessionId: entry.sessionId, prompt: [{ type: 'text', text }] },
          { timeoutMs: options.timeoutMs },
        );
        tier.establish.context.push({
          sessionId: entry.sessionId,
          promptIndex,
          promptChars: text.length,
          promptMs: round(performance.now() - started),
          updates: connection.updates - updatesBefore,
        });
      }
    }
    tier.establish.wallMs = round(performance.now() - establishStart);
    // 先让流式输出落定，再采「保留资源」的内存（与执行任务的峰值区分）。
    await delay(500);
    tier.established = { ...(await sampleProcessTree(connection.pid)), at: nowIso() };

    for (let cycle = 1; cycle <= options.cycles; cycle += 1) {
      const closes = [];
      for (const entry of sessionIds) {
        const started = performance.now();
        await connection.request('session/close', { sessionId: entry.sessionId });
        closes.push(round(performance.now() - started));
      }
      await delay(500);
      const afterClose = await sampleProcessTree(connection.pid);
      const resumes = [];
      for (const entry of sessionIds) {
        const started = performance.now();
        await connection.request('session/resume', {
          sessionId: entry.sessionId,
          cwd: entry.workspace,
          mcpServers: [],
        });
        resumes.push(round(performance.now() - started));
      }
      await delay(500);
      const afterResume = await sampleProcessTree(connection.pid);
      tier.cycles.push({
        cycle,
        closedAllMs: round(closes.reduce((sum, value) => sum + value, 0)),
        resumedAllMs: round(resumes.reduce((sum, value) => sum + value, 0)),
        closeMs: distribution(closes),
        resumeMs: distribution(resumes),
        treeAfterClose: { ...afterClose, at: nowIso() },
        treeAfterResume: { ...afterResume, at: nowIso() },
      });
    }

    for (const entry of sessionIds) {
      try {
        await connection.request('session/close', { sessionId: entry.sessionId });
      } catch (error) {
        tier.errors.push(`close ${entry.sessionId}: ${shortError(error)}`);
      }
    }
    await delay(500);
    tier.afterAllClosed = { ...(await sampleProcessTree(connection.pid)), at: nowIso() };
    const pid = connection.pid;
    await connection.terminate();
    tier.release = {
      treeAfterExit: { ...(await waitForTreeGone(pid, 20_000)), at: nowIso() },
      exitInfo: connection.exitInfo,
    };
    tier.status = 'measured';
  } catch (error) {
    tier.errors.push(describeError(error));
    tier.status = 'failed';
    try {
      for (const entry of sessionIds) {
        await connection.request('session/close', { sessionId: entry.sessionId });
      }
    } catch {}
    try {
      if (connection.pid) {
        tier.release = tier.release ?? {};
        tier.release.treeAfterExit = { ...(await killAndWait(connection.pid)), at: nowIso() };
      }
    } catch {}
  } finally {
    await connection.shutdown();
    tier.finishedAt = new Date().toISOString();
    if (tier.status !== 'measured') {
      // 失败但仍保留了部分数据：显式标记为未验证，主检出不得把它当作通过。
      tier.status = 'failed';
      tier.unverified = true;
    }
  }
  return tier;
}

function distribution(values) {
  if (values.length === 0) {
    return { count: 0, minMs: null, p50Ms: null, p95Ms: null, maxMs: null, meanMs: null, totalMs: 0 };
  }
  const sorted = [...values].sort((left, right) => left - right);
  const at = (percentile) =>
    sorted[Math.min(sorted.length - 1, Math.max(0, Math.ceil(percentile * sorted.length) - 1))];
  const total = sorted.reduce((sum, value) => sum + value, 0);
  return {
    count: sorted.length,
    minMs: round(sorted[0]),
    p50Ms: round(at(0.5)),
    p95Ms: round(at(0.95)),
    maxMs: round(sorted[sorted.length - 1]),
    meanMs: round(total / sorted.length),
    totalMs: round(total),
  };
}

// ---------------------------------------------------------------------------------------------
// ACP 连接（真实 codex-acp 子进程）
// ---------------------------------------------------------------------------------------------

class Connection {
  constructor({ adapter, codexHome, cwd, allowedRoot, timeoutMs, stderrLog }) {
    this.adapter = adapter;
    this.codexHome = codexHome;
    this.cwd = cwd;
    this.allowedRoot = allowedRoot;
    this.timeoutMs = timeoutMs;
    this.stderrLog = stderrLog;
    this.child = null;
    this.pid = null;
    this.pending = new Map();
    this.nextId = 1;
    this.updates = 0;
    this.exitInfo = null;
    this.stderrBytes = 0;
    this.stderrStream = createWriteStream(stderrLog, { flags: 'a' });
    this.stderrStream.on('error', () => {});
  }

  async start() {
    const launch = buildLaunch(this.adapter.path);
    await new Promise((resolveStart, rejectStart) => {
      const child = spawn(launch.command, launch.args, {
        cwd: this.cwd,
        env: { ...process.env, CODEX_HOME: this.codexHome, NO_COLOR: '1' },
        stdio: ['pipe', 'pipe', 'pipe'],
        windowsHide: true,
      });
      this.child = child;
      const onError = (error) => {
        child.off('spawn', onSpawn);
        rejectStart(error);
      };
      const onSpawn = () => {
        child.off('error', onError);
        resolveStart();
      };
      child.once('error', onError);
      child.once('spawn', onSpawn);
    });
    this.pid = this.child.pid;
    this.child.stdout.setEncoding('utf8');
    createInterface({ input: this.child.stdout, crlfDelay: Infinity }).on('line', (line) =>
      this.onLine(line),
    );
    this.child.stderr.on('data', (chunk) => {
      this.stderrBytes += chunk.length;
      try {
        this.stderrStream.write(chunk);
      } catch {}
    });
    this.child.once('exit', (code, signal) => {
      this.exitInfo = { code, signal, at: nowIso() };
      for (const [id, pending] of this.pending) {
        clearTimeout(pending.timer);
        pending.reject(new BenchError(`适配器在请求 ${pending.method}（id=${id}）期间退出`));
      }
      this.pending.clear();
    });
    this.child.stdin.on('error', () => {});
  }

  async initialize() {
    const result = await this.request('initialize', {
      protocolVersion: PROTOCOL_VERSION,
      clientCapabilities: {
        fs: { readTextFile: true, writeTextFile: true },
        terminal: false,
      },
      clientInfo: { name: TOOL_NAME, version: TOOL_VERSION },
    });
    const sessionCapabilities =
      result?.agentCapabilities?.sessionCapabilities ?? result?.sessionCapabilities ?? null;
    return {
      protocolVersion: result?.protocolVersion ?? null,
      agentInfo: result?.agentInfo ?? null,
      sessionCapabilities,
    };
  }

  request(method, params, options = {}) {
    const id = this.nextId;
    this.nextId += 1;
    const timeoutMs = options.timeoutMs ?? this.timeoutMs;
    return new Promise((resolveRequest, rejectRequest) => {
      const timer = setTimeout(() => {
        if (!this.pending.has(id)) return;
        this.pending.delete(id);
        rejectRequest(new RequestTimeoutError(method, timeoutMs));
        try {
          if (method === 'session/prompt') this.notify('session/cancel', { sessionId: params.sessionId });
        } catch {}
      }, timeoutMs);
      this.pending.set(id, { method, timer, resolve: resolveRequest, reject: rejectRequest });
      try {
        this.send({ jsonrpc: '2.0', id, method, params });
      } catch (error) {
        clearTimeout(timer);
        this.pending.delete(id);
        rejectRequest(error);
      }
    });
  }

  notify(method, params) {
    this.send({ jsonrpc: '2.0', method, params });
  }

  send(message) {
    if (!this.child || this.child.stdin.destroyed) throw new BenchError('适配器 stdin 已关闭');
    this.child.stdin.write(`${JSON.stringify(message)}\n`);
  }

  onLine(line) {
    if (!line.trim()) return;
    let message;
    try {
      message = JSON.parse(line);
    } catch {
      return;
    }
    if (message && typeof message.method === 'string') {
      if (message.id !== undefined && message.id !== null) {
        this.handleServerRequest(message).catch(() => {
          try {
            this.respondError(message.id, -32603, 'benchmark client error');
          } catch {}
        });
      } else if (message.method === 'session/update') {
        this.updates += 1;
      }
      return;
    }
    if (message && message.id !== undefined && message.id !== null) {
      const pending = this.pending.get(message.id);
      if (!pending) return;
      this.pending.delete(message.id);
      clearTimeout(pending.timer);
      if (message.error) {
        pending.reject(
          new BenchError(`${pending.method} 失败：${JSON.stringify(message.error).slice(0, 400)}`),
        );
      } else {
        pending.resolve(message.result);
      }
    }
  }

  async handleServerRequest(message) {
    const params = message.params ?? {};
    switch (message.method) {
      case 'session/request_permission': {
        const choices = Array.isArray(params.options) ? params.options : [];
        const chosen =
          choices.find((option) => option?.kind === 'allow_once') ??
          choices.find((option) => String(option?.kind ?? '').startsWith('allow')) ??
          choices[0];
        this.respond(
          message.id,
          chosen
            ? { outcome: { outcome: 'selected', optionId: chosen.optionId } }
            : { outcome: { outcome: 'cancelled' } },
        );
        return;
      }
      case 'fs/read_text_file': {
        if (!this.allowed(params.path)) {
          this.respondError(message.id, -32602, 'path outside the isolated data-dir');
          return;
        }
        try {
          this.respond(message.id, { content: readFileSync(params.path, 'utf8') });
        } catch (error) {
          this.respondError(message.id, -32000, `read failed: ${shortError(error)}`);
        }
        return;
      }
      case 'fs/write_text_file': {
        if (!this.allowed(params.path) || typeof params.content !== 'string') {
          this.respondError(message.id, -32602, 'path outside the isolated data-dir');
          return;
        }
        try {
          mkdirSync(dirname(params.path), { recursive: true });
          writeFileSync(params.path, params.content);
          this.respond(message.id, {});
        } catch (error) {
          this.respondError(message.id, -32000, `write failed: ${shortError(error)}`);
        }
        return;
      }
      default:
        this.respondError(message.id, -32601, `unsupported client method ${message.method}`);
    }
  }

  allowed(path) {
    if (typeof path !== 'string' || !isAbsolute(path)) return false;
    const inside = relative(this.allowedRoot, resolve(path));
    return inside.length > 0 && !inside.startsWith('..') && !isAbsolute(inside);
  }

  respond(id, result) {
    this.send({ jsonrpc: '2.0', id, result });
  }

  respondError(id, code, message) {
    this.send({ jsonrpc: '2.0', id, error: { code, message } });
  }

  /** 主动结束适配器进程树（幂等）。 */
  async terminate() {
    if (!this.pid) return;
    const pid = this.pid;
    killProcessTree(pid);
    await waitForTreeGone(pid, 20_000);
    // `exit` 事件在轮询返回之后才派发：再让出一轮宏任务，`exitInfo` 才不会恒为 null。
    const deadline = Date.now() + 2_000;
    while (this.exitInfo === null && Date.now() < deadline) await delay(25);
  }

  async shutdown() {
    try {
      this.stderrStream.end();
    } catch {}
    if (this.pid) {
      await waitForTreeGone(this.pid, 10_000);
    }
  }
}

function buildLaunch(adapterPath) {
  const extension = adapterPath.toLowerCase();
  if (extension.endsWith('.js') || extension.endsWith('.mjs') || extension.endsWith('.cjs')) {
    return { command: process.execPath, args: [adapterPath] };
  }
  if (WINDOWS && (extension.endsWith('.cmd') || extension.endsWith('.bat'))) {
    return { command: 'cmd.exe', args: ['/d', '/s', '/c', adapterPath] };
  }
  return { command: adapterPath, args: [] };
}

// ---------------------------------------------------------------------------------------------
// 进程树采样 / 终止
// ---------------------------------------------------------------------------------------------

async function sampleProcessTree(pid) {
  if (!pid) return { pid: null, processCount: 0, workingSetMb: 0, rootPresent: false, method: 'none' };
  if (WINDOWS) {
    const script =
      '[Console]::OutputEncoding=[Text.Encoding]::UTF8; ' +
      'Get-CimInstance Win32_Process | Select-Object ProcessId,ParentProcessId,WorkingSetSize,Name | ConvertTo-Json -Compress';
    let output;
    try {
      output = execFileSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', script], {
        encoding: 'utf8',
        timeout: 30_000,
        maxBuffer: 32 * 1024 * 1024,
        windowsHide: true,
      });
    } catch (error) {
      return { pid, processCount: null, workingSetMb: null, rootPresent: null, method: 'win32-cim', error: shortError(error) };
    }
    const rows = parsePsJson(output);
    if (rows === null) {
      return { pid, processCount: null, workingSetMb: null, rootPresent: null, method: 'win32-cim', error: '无法解析进程表' };
    }
    const tree = accumulateTree(rows, pid, (row) => Number(row.WorkingSetSize) || 0);
    return {
      pid,
      processCount: tree.count,
      workingSetMb: Math.round((tree.bytes / 1024 / 1024) * 10) / 10,
      rootPresent: tree.hasRoot,
      method: 'win32-cim',
      names: tree.names,
    };
  }
  let output;
  try {
    output = execFileSync('ps', ['-eo', 'pid=,ppid=,rss=,comm='], {
      encoding: 'utf8',
      timeout: 30_000,
      maxBuffer: 16 * 1024 * 1024,
    });
  } catch (error) {
    return { pid, processCount: null, workingSetMb: null, rootPresent: null, method: 'ps', error: shortError(error) };
  }
  const rows = [];
  for (const line of String(output).split(/\r?\n/)) {
    const match = line.trim().match(/^(\d+)\s+(\d+)\s+(\d+)\s+(.*)$/);
    if (!match) continue;
    rows.push({
      ProcessId: Number(match[1]),
      ParentProcessId: Number(match[2]),
      WorkingSetSize: Number(match[3]) * 1024,
      Name: match[4],
    });
  }
  const tree = accumulateTree(rows, pid, (row) => Number(row.WorkingSetSize) || 0);
  return {
    pid,
    processCount: tree.count,
    workingSetMb: Math.round((tree.bytes / 1024 / 1024) * 10) / 10,
    rootPresent: tree.hasRoot,
    method: 'ps',
    names: tree.names,
  };
}

function parsePsJson(output) {
  try {
    const parsed = JSON.parse(String(output).replace(/^\uFEFF/, '') || '[]');
    return Array.isArray(parsed) ? parsed : [parsed];
  } catch {
    return null;
  }
}

function accumulateTree(rows, rootPid, weightOf) {
  const byParent = new Map();
  const byPid = new Map();
  for (const row of rows) {
    const pid = Number(row.ProcessId);
    if (!Number.isFinite(pid)) continue;
    byPid.set(pid, row);
    const parent = Number(row.ParentProcessId);
    if (!byParent.has(parent)) byParent.set(parent, []);
    byParent.get(parent).push(pid);
  }
  const seen = new Set();
  const queue = [Number(rootPid)];
  let bytes = 0;
  const names = {};
  while (queue.length > 0) {
    const pid = queue.shift();
    if (seen.has(pid)) continue;
    seen.add(pid);
    const row = byPid.get(pid);
    if (row) {
      bytes += weightOf(row);
      const name = String(row.Name ?? 'unknown');
      names[name] = (names[name] ?? 0) + 1;
    }
    for (const child of byParent.get(pid) ?? []) queue.push(child);
  }
  return {
    count: [...seen].filter((pid) => byPid.has(pid)).length,
    bytes,
    names,
    hasRoot: byPid.has(Number(rootPid)),
  };
}

function killProcessTree(pid) {
  if (!pid) return;
  try {
    if (WINDOWS) {
      spawnSync('taskkill', ['/PID', String(pid), '/T', '/F'], {
        stdio: 'ignore',
        timeout: 15_000,
        windowsHide: true,
      });
    } else {
      try {
        execFileSync('kill', ['-TERM', String(pid)], { stdio: 'ignore' });
      } catch {
        try {
          execFileSync('kill', ['-KILL', String(pid)], { stdio: 'ignore' });
        } catch {}
      }
    }
  } catch {}
}

async function waitForTreeGone(pid, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  let sample = await sampleProcessTree(pid);
  while (Date.now() < deadline) {
    if (sample.processCount === 0 && sample.rootPresent !== true) return sample;
    await delay(150);
    sample = await sampleProcessTree(pid);
  }
  return sample;
}

async function killAndWait(pid) {
  killProcessTree(pid);
  return waitForTreeGone(pid, 20_000);
}

// ---------------------------------------------------------------------------------------------
// 小工具
// ---------------------------------------------------------------------------------------------

function nowIso() {
  return new Date().toISOString();
}

function round(value) {
  return Math.round(value * 100) / 100;
}

function delay(ms) {
  return new Promise((resolveDelay) => setTimeout(resolveDelay, ms));
}

function shortError(error) {
  const text = error?.message ?? String(error);
  return text.length > 300 ? text.slice(0, 300) + '…' : text;
}

function describeError(error) {
  if (error instanceof BenchError || error instanceof UsageError || error instanceof RequestTimeoutError) {
    return error.message;
  }
  return error?.stack ?? String(error);
}

function sha256File(path) {
  try {
    return createHash('sha256').update(readFileSync(path)).digest('hex');
  } catch {
    return null;
  }
}

function isInside(parent, child) {
  const inside = relative(parent, child);
  return inside.length > 0 && !inside.startsWith('..') && !isAbsolute(inside);
}

main()
  .then((exitCode) => {
    process.exitCode = exitCode;
  })
  .catch((error) => {
    process.stderr.write(`${TOOL_NAME}: ${describeError(error)}\n`);
    process.exitCode = error instanceof UsageError || error instanceof BenchError ? 2 : 1;
  });
