#!/usr/bin/env node
/**
 * 会话准备（首条消息提交之前）的**真实浏览器**验收（`web-session-preparation` 任务 8.2；PV8）。
 *
 * 对端是**真实 Daemon**（真实 TLS、真实 Sync 握手、真实 SQLite 与真实附件端点）与**构建后的
 * 真实 PWA**；本脚本用 DOM 钩子与**真实出站/入站帧**做断言，不靠截图或文案推断。Agent 一侧由
 * `crates/app/tests/session_preparation_browser.rs` 的受控 stdio 服务提供（它同时记录
 * `session/new`/`prompt`/`set_mode`/`set_config_option` 证据），真实 `codex-acp` 的跨进程
 * 准备行为由 `scripts/probe-session-preparation.mjs` 覆盖——两层刻意分开，互不冒名。
 *
 * 每条场景的判据只在两个地方观察：
 *   - DOM：`[data-route="session"][data-session-state="preparing"]`、`[data-preparation-phase]`、
 *     `[data-preparation-input]`、`[data-preparation-mode]`、`[data-preparation-config]`、
 *     `[data-preparation-attachment]`、`[data-action="preparation-submit"][data-submit-enabled]`、
 *     `[data-preparation-lease-expired]`、`[data-action="preparation-add-file"]`；
 *   - 真实 wire：`session.preparation`（allocate/restore/configure/renew）、
 *     `session.preparation.result`、`session.start`（`kind: "initial"`）、`command.result`、
 *     独立附件连接上的 `attachment.begin`/`chunk`/`seal`/`manifest_begin`/`manifest_sealed`。
 *
 * 必需环境变量：
 *   SESSION_PREPARATION_ORIGIN     https://<host>:<port>（PWA 同源地址；自签证书由
 *                                  `--ignore-certificate-errors` 放宽）
 *   SESSION_PREPARATION_CODE       `device.pair.begin` 给出的配对码
 *   SESSION_PREPARATION_WORKSPACE  已存在的绝对目录（准备会话的工作目录）
 *   SESSION_PREPARATION_EVIDENCE   证据 JSON 输出路径；本脚本**写入**该文件
 * 可选环境变量：
 *   BROWSER_PATH                   指定 Chromium 内核可执行文件
 *   SESSION_PREPARATION_RENEW_WINDOW_MS  续租观察窗口（缺省 35000；必须大于固定 30 秒租期）
 *
 * 退出码：0 = 全部场景执行且通过；1 = 至少一条断言失败；2 = 配置/浏览器/证据前置失败。
 * 不把未执行的场景写成通过：本脚本不登记「结构性不在本层」的场景（例如需要重启 Daemon 的
 * `stale` 固定动作），这些边界写进 `docs/evidence/session-preparation.md`。
 *
 * 本文件只用 Node 内置模块，不依赖仓库 package.json。
 */

import { spawn } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';

class ConfigError extends Error {}
class AssertionError extends Error {}

if (process.argv.includes('--help') || process.argv.includes('-h')) {
  process.stdout.write(
    '用法：node clients/app/e2e/session-preparation-live.mjs\n' +
      '需要 SESSION_PREPARATION_ORIGIN / _CODE / _WORKSPACE / _EVIDENCE；见文件头注释。\n',
  );
  process.exit(0);
}

const origin = required('SESSION_PREPARATION_ORIGIN');
const code = required('SESSION_PREPARATION_CODE');
const workspace = required('SESSION_PREPARATION_WORKSPACE');
const evidencePath = required('SESSION_PREPARATION_EVIDENCE');
const renewWindowMs = Number(process.env.SESSION_PREPARATION_RENEW_WINDOW_MS ?? '35000');

function required(name) {
  const value = process.env[name];
  if (!value || !value.trim()) throw new ConfigError(`缺少必需的环境变量 ${name}`);
  return value.trim();
}

const executable = [
  process.env.BROWSER_PATH,
  'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe',
  'C:/Program Files/Microsoft/Edge/Application/msedge.exe',
  'C:/Program Files/Google/Chrome/Application/chrome.exe',
  '/usr/bin/chromium',
  '/usr/bin/chromium-browser',
  '/usr/bin/google-chrome',
].find((path) => path && existsSync(path));
if (!executable) {
  process.stderr.write('session-preparation-live: 找不到 Chromium 浏览器（可设 BROWSER_PATH）\n');
  process.exit(2);
}

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const debuggingPort = 10000 + Math.floor(Math.random() * 20000);
const userDataDir = mkdtempSync(join(tmpdir(), 'session-preparation-live-browser-'));
const browser = spawn(
  executable,
  [
    '--headless=new',
    '--disable-gpu',
    '--ignore-certificate-errors',
    '--no-first-run',
    '--no-default-browser-check',
    '--disable-extensions',
    '--window-size=1400,900',
    '--remote-debugging-port=' + debuggingPort,
    '--user-data-dir=' + userDataDir,
    'about:blank',
  ],
  { stdio: 'ignore', windowsHide: true },
);

const report = {
  tool: 'session-preparation-live',
  version: 1,
  startedAt: new Date().toISOString(),
  finishedAt: null,
  origin,
  workspace,
  scenarios: [],
  skipped: [],
  marker: null,
  firstMessage: null,
  selectedMode: null,
  selectedConfig: null,
  expectedAttachmentSha256: null,
  exitCode: null,
};

let browserCdp = null;
let mainPage = null;

// 准备附件的内容：很小的纯文本，便于逐字节核对封存摘要（不在证据里存 chunk 正文）。
const attachment = {
  name: 'preparation-note.txt',
  mediaType: 'text/plain',
  text: 'attachment bytes for web-session-preparation E2/E5\n',
};

try {
  const browserSocket = await waitForBrowserEndpoint(debuggingPort);
  browserCdp = await connectCdp(browserSocket);
  mainPage = await openPage(browserCdp, 'about:blank');
  await mainPage.call('Page.navigate', { url: origin + '/pair' });

  // ── 场景 1：真实分配 → 准备页、真实模式/配置、固定 URL、没有 session.create ──────────────
  await runScenario('allocate-shows-real-settings-and-keeps-the-url', async () => {
    await mainPage.evaluate(pairFlow, { code });
    const allocated = await mainPage.evaluate(allocateFlow, {
      workspace: workspace.replaceAll('\\', '/'),
    });
    const sessionId = allocated.sessionId;
    mainPage.sessionId = sessionId;

    const state = await waitForState(async () => {
      const snapshot = await mainPage.evaluate(preparationSnapshot);
      return snapshot.phase === 'ready' ? snapshot : null;
    }, '准备页进入 ready', 30_000);
    assert(
      state.pathname === '/session/' + encodeURIComponent(sessionId),
      `分配后必须停在准备页 URL：${state.pathname}`,
    );
    assert(state.preparing, `准备页必须有 data-session-state="preparing"：${JSON.stringify(state)}`);
    assert(state.routeSessionId === sessionId, `页面会话标识必须保持不变：${JSON.stringify(state)}`);
    assert(state.modes.length > 0, `必须显示真实 Agent 的模式：${JSON.stringify(state)}`);
    assert(state.configs.length > 0, `必须显示真实 Agent 的配置项：${JSON.stringify(state)}`);
    assert(state.submitEnabled === 'true', `准备就绪后发送必须可用：${JSON.stringify(state)}`);

    const log = await mainPage.readLog();
    const allocate = preparationFrames(log, 'allocate')[0];
    assert(allocate, '必须发出 session.preparation 的 allocate');
    assert(
      allocate.body?.sessionId === undefined,
      `allocate 的 body 不得携带 sessionId（由 Owner 分配）：${JSON.stringify(allocate.body)}`,
    );
    assert(
      allocate.body?.workspace?.workspacePath !== undefined ||
        allocate.body?.workspace?.directoryId !== undefined,
      `allocate 必须给出唯一一种目录引用：${JSON.stringify(allocate.body?.workspace)}`,
    );
    const ready = preparationResults(log).find(
      (result) => result.action === 'allocate' && result.status === 'ready',
    );
    assert(ready, '必须收到 allocate 的 ready 结果');
    assert(ready.sessionId === sessionId, `ready 的 sessionId 必须回显分配结果：${JSON.stringify(ready)}`);
    assert(
      Array.isArray(ready.modes) && ready.modes.length > 0,
      `ready 必须给出真实模式：${JSON.stringify(ready.modes)}`,
    );
    assert(
      Array.isArray(ready.configOptions) && ready.configOptions.length > 0,
      `ready 必须给出真实配置项：${JSON.stringify(ready.configOptions)}`,
    );
    assert(
      typeof ready.receipt === 'string' &&
        typeof ready.runtimeEpoch === 'string' &&
        typeof ready.generation === 'string' &&
        typeof ready.leaseExpiresAt === 'string',
      `ready 必须给出凭据/资源代/租约：${JSON.stringify(ready)}`,
    );
    assert(
      commandFrames(log, 'session.create').length === 0,
      '准备链路上 MUST NOT 退化为 session.create（那会立即持久化）',
    );
    report.selectedMode = state.modes.at(-1) ?? null;
    return {
      sessionId,
      url: mainPage.url,
      modeCount: state.modes.length,
      configCount: state.configs.length,
      leaseExpiresAt: ready.leaseExpiresAt,
    };
  });

  // ── 场景 2：草稿（文字 + 非默认模式/配置 + 附件）跨刷新恢复；restore 回到同一 ID ───────────
  await runScenario('draft-and-attachment-survive-reload', async () => {
    const sessionId = mainPage.sessionId;
    const draft = 'draft-' + Math.random().toString(36).slice(2, 10);
    report.firstMessage = 'Reply with exactly this token and nothing else: ' + draft;
    const attachmentInfo = await mainPage.evaluate(prepareDraft, {
      text: report.firstMessage,
      attachment: { name: attachment.name, mediaType: attachment.mediaType, text: attachment.text },
    });
    report.expectedAttachmentSha256 = attachmentInfo.sha256;
    const configured = await mainPage.evaluate(preparationSnapshot);
    // 选一个非默认模式（最后一个 chip）与一个非当前配置值（最后一个选项）。
    const mode = configured.modes.at(-1);
    const config = configured.configs.at(-1);
    await mainPage.evaluate(selectModeAndConfig, { mode, config });
    report.selectedMode = mode;
    report.selectedConfig = config;
    report.marker = draft;

    const configureFrames = await waitForFrames(async (log) => {
      const frames = preparationFrames(log, 'configure');
      const applied =
        frames.some((frame) => frame.body?.mode === mode) &&
        frames.some((frame) => frame.body?.configOptions?.[config.id] === config.value);
      return applied ? frames : null;
    }, 'configure 携带所选模式与配置', 20_000);
    assert(
      configureFrames.length >= 1,
      `选择后必须下发 configure：${JSON.stringify(configureFrames)}`,
    );
    const configuredReady = preparationResults(await mainPage.readLog()).filter(
      (result) => result.action === 'configure' && result.status === 'ready',
    );
    assert(configuredReady.length >= 1, 'configure 必须返回 ready');

    // 等草稿去抖（350ms）落盘，再刷新。
    await sleep(1_200);
    await mainPage.call('Page.reload');
    const restored = await waitForState(async () => {
      const snapshot = await mainPage.evaluate(preparationSnapshot);
      return snapshot.phase === 'ready' && snapshot.input === report.firstMessage ? snapshot : null;
    }, '刷新后草稿与准备页恢复', 40_000);

    assert(
      restored.pathname === '/session/' + encodeURIComponent(sessionId),
      `刷新后 URL 必须不变：${restored.pathname}`,
    );
    assert(restored.text.includes(attachment.name), `刷新后附件名必须恢复：${JSON.stringify(restored)}`);
    assert(
      restored.attachments.some((status) => status === 'selected'),
      `刷新后附件必须是可发送的已选状态：${JSON.stringify(restored.attachments)}`,
    );
    const log = await mainPage.readLog();
    const restore = preparationFrames(log, 'restore')[0];
    assert(restore, '刷新后必须发出 restore');
    assert(restore.body?.sessionId === sessionId, `restore 必须指向同一 ID：${JSON.stringify(restore.body)}`);
    assert(typeof restore.body?.receipt === 'string', 'restore 必须携带准备凭据');
    const ready = preparationResults(log).find(
      (result) => result.action === 'restore' && result.status === 'ready',
    );
    assert(ready && ready.sessionId === sessionId, `restore 必须回到同一 ready：${JSON.stringify(ready)}`);
    return {
      restoredInputLength: restored.input.length,
      attachmentStatuses: restored.attachments,
      restoreStatus: ready.status,
    };
  });

  // ── 场景 3：可见页面续租跨过 30 秒租约；页面仍可用、没有租约过期提示 ────────────────────
  await runScenario('lease-renewal-survives-thirty-seconds', async () => {
    const sessionId = mainPage.sessionId;
    const before = preparationFrames(await mainPage.readLog(), 'renew').length;
    await sleep(renewWindowMs);
    const log = await mainPage.readLog();
    const renews = preparationFrames(log, 'renew');
    assert(
      renews.length >= before + 3,
      `可见 ${renewWindowMs}ms 内必须至少续租 3 次（前 ${before}，后 ${renews.length}）`,
    );
    const fresh = renews.slice(before);
    assert(
      fresh.every((frame) => frame.body?.sessionId === sessionId),
      `续租必须指向当前准备：${JSON.stringify(fresh.map((frame) => frame.body?.sessionId))}`,
    );
    const readyRenews = preparationResults(log).filter(
      (result) => result.action === 'renew' && result.status === 'ready',
    );
    assert(readyRenews.length >= before + 3, `每次续租都必须收到 ready：${readyRenews.length}`);
    const snapshot = await mainPage.evaluate(preparationSnapshot);
    assert(!snapshot.leaseExpired, `续租期间 MUST NOT 显示租约过期：${JSON.stringify(snapshot)}`);
    assert(snapshot.phase === 'ready', `续租后必须保持 ready：${JSON.stringify(snapshot)}`);
    return { renewals: fresh.length, readyRenews: readyRenews.length };
  });

  // ── 场景 4：首条提交一次、URL/页面实例不变、附件单次上传与分段耗时 ───────────────────────
  await runScenario('first-submit-commits-once-and-keeps-the-url', async () => {
    const sessionId = mainPage.sessionId;
    const pageTag = Math.random().toString(36).slice(2);
    const before = await mainPage.readLog();
    const startsBefore = commandFrames(before, 'session.start').length;
    const beginsBefore = attachmentFrames(before, 'attachment.begin').length;
    await mainPage.evaluate(tagPage, pageTag);
    const submitAt = Date.now();
    await mainPage.evaluate(submitFirstMessage, {});
    const startFrame = await waitForFrames(async (log) => {
      const frames = commandFrames(log, 'session.start');
      return frames.length > startsBefore ? frames.at(-1) : null;
    }, 'session.start 已发出', 30_000);
    assert(startFrame.body?.payload?.kind === 'initial', `首条必须是 kind=initial：${JSON.stringify(startFrame.body)}`);
    assert(
      startFrame.body?.sessionId === sessionId,
      `session.start 必须指向同一准备 ID：${JSON.stringify(startFrame.body?.sessionId)}`,
    );
    const contentText = (startFrame.body?.payload?.content ?? [])
      .filter((block) => block.type === 'text')
      .map((block) => block.text)
      .join('');
    assert(
      contentText === report.firstMessage,
      `首条正文必须逐字提交：${JSON.stringify(contentText)}`,
    );
    assert(
      startFrame.body?.payload?.mode === report.selectedMode,
      `刷新后的非默认模式必须随首条提交：${JSON.stringify(startFrame.body?.payload?.mode)}`,
    );
    assert(
      startFrame.body?.payload?.configOptions?.[report.selectedConfig.id] === report.selectedConfig.value,
      `刷新后的非默认配置必须随首条提交：${JSON.stringify(startFrame.body?.payload?.configOptions)}`,
    );

    const startRequestId = startFrame.body?.requestId;
    const accepted = await waitForFrames(async (log) => {
      const result = commandResults(log).find(
        (candidate) => candidate.requestId === startRequestId && candidate.status === 'accepted',
      );
      return result ?? null;
    }, 'session.start accepted', 40_000);
    const target = await waitForState(async () => {
      const snapshot = await mainPage.evaluate(preparationSnapshot);
      return snapshot.preparing === false && snapshot.bodyText.includes(report.marker) ? snapshot : null;
    }, '正式会话页显示首条与回复', 120_000);
    assert(
      target.pathname === '/session/' + encodeURIComponent(sessionId),
      `保存成功后 URL 必须不变：${target.pathname}`,
    );
    const sameInstance = await mainPage.evaluate(pageTagStillThere, pageTag);
    assert(sameInstance, '保存成功必须停在同一个页面实例（不能整页跳转/重载）');

    const completed = await waitForFrames(async (log) => {
      const result = commandResults(log).find(
        (candidate) => candidate.requestId === startRequestId && candidate.status === 'completed',
      );
      return result ?? null;
    }, 'session.start completed', 120_000);
    assert(
      startFrame.body?.payload?.content?.some((block) => block.type === 'text'),
      '首条内容必须包含文本块',
    );

    const log = await mainPage.readLog();
    const begins = attachmentFrames(log, 'attachment.begin');
    assert(
      begins.length === beginsBefore + 1,
      `单次成功提交每个附件只能 begin 一次（前 ${beginsBefore}，后 ${begins.length}）`,
    );
    const begin = begins.at(-1);
    assert(
      begin.body?.target?.kind === 'preparation',
      `准备上传必须走 target=preparation：${JSON.stringify(begin.body?.target)}`,
    );
    assert(
      begin.body?.sessionId === undefined,
      `准备上传 MUST NOT 同时携带顶层 sessionId：${JSON.stringify(begin.body)}`,
    );
    assert(
      begin.body?.target?.sessionId === sessionId,
      `准备上传的 target.sessionId 必须是当前准备：${JSON.stringify(begin.body?.target)}`,
    );
    const chunks = attachmentFrames(log, 'attachment.chunk');
    assert(chunks.length >= 1, '附件必须至少上传一个分块');
    const seals = attachmentFrames(log, 'attachment.seal');
    assert(seals.length === 1, `附件只能封存一次：${seals.length}`);
    const manifestBegin = attachmentFrames(log, 'attachment.manifest_begin');
    assert(manifestBegin.length === 1, `清单只能 begin 一次：${manifestBegin.length}`);
    assert(
      manifestBegin[0].body?.target?.kind === 'preparation',
      `清单必须走准备 target：${JSON.stringify(manifestBegin[0].body?.target)}`,
    );
    const sealedInbound = inboundFrames(log).filter((frame) => frame.type === 'attachment.sealed');
    assert(sealedInbound.length === 1, `必须恰好收到一次 attachment.sealed：${sealedInbound.length}`);
    assert(
      sealedInbound[0].body?.sha256 === report.expectedAttachmentSha256,
      `封存摘要必须与本地字节一致：${JSON.stringify(sealedInbound[0].body)}`,
    );
    const manifestSealed = inboundFrames(log).filter((frame) => frame.type === 'attachment.manifest_sealed');
    assert(manifestSealed.length === 1, `必须恰好收到一次 attachment.manifest_sealed：${manifestSealed.length}`);
    assert(
      startFrame.body?.payload?.submissionId === manifestSealed[0].body?.submissionId,
      `session.start 必须引用同一份准备清单：${JSON.stringify(startFrame.body?.payload?.submissionId)}`,
    );
    assert(commandFrames(log, 'session.start').length === startsBefore + 1, '一次提交只能产生一条 session.start');

    const timings = startFrame.timings;
    const sealInboundAt = inboundFrames(log).find((frame) => frame.type === 'attachment.sealed')?.at;
    const manifestSealedAt = inboundFrames(log).find((frame) => frame.type === 'attachment.manifest_sealed')?.at;
    assert(sealInboundAt !== undefined && manifestSealedAt !== undefined, '附件时间点必须可观测');
    report.timings = {
      submitToAttachmentBeginMs: begin.at - submitAt,
      attachmentBeginToSealAckMs: sealInboundAt - begin.at,
      sealToManifestClaimMs: manifestSealedAt - sealInboundAt,
      manifestClaimToStartSentMs: startFrame.at - manifestSealedAt,
      submitToAcceptedMs: accepted.at - submitAt,
      acceptedToCompletedMs: completed.at - accepted.at,
      totalSubmitToCompletedMs: completed.at - submitAt,
    };
    void timings;
    return {
      sessionId,
      requestId: startRequestId,
      accepted: accepted.status,
      completed: completed.status,
      attachmentChunks: chunks.length,
      attachmentSha256: report.expectedAttachmentSha256,
      timings: report.timings,
      bodySnippet: target.bodyText.slice(-600),
    };
  });

  // ── 场景 5：刷新后的正式读取（同 URL、不重传附件、不重复首条）───────────────────────────
  await runScenario('formal-session-reads-back-after-reload', async () => {
    const sessionId = mainPage.sessionId;
    const before = await mainPage.readLog();
    const startsBefore = commandFrames(before, 'session.start').length;
    const beginsBefore = attachmentFrames(before, 'attachment.begin').length;
    await mainPage.call('Page.reload');
    const state = await waitForState(async () => {
      const snapshot = await mainPage.evaluate(preparationSnapshot);
      return snapshot.bodyText.includes(report.marker) && snapshot.preparing === false ? snapshot : null;
    }, '刷新后正式会话仍可读到首条', 60_000);
    assert(
      state.pathname === '/session/' + encodeURIComponent(sessionId),
      `正式读取必须保持原 URL：${state.pathname}`,
    );
    assert(state.composer !== null, `正式会话页必须有输入区：${JSON.stringify(state)}`);
    const log = await mainPage.readLog();
    assert(
      commandFrames(log, 'session.start').length === startsBefore,
      '刷新正式会话 MUST NOT 重新提交首条',
    );
    assert(
      attachmentFrames(log, 'attachment.begin').length === beginsBefore,
      '刷新正式会话 MUST NOT 重新上传附件',
    );
    const reads = commandFrames(log, 'session.read').filter(
      (frame) => frame.sessionId === sessionId,
    );
    assert(reads.length >= 1, '正式读取必须发出 session.read');
    return { reads: reads.length, bodySnippet: state.bodyText.slice(-400) };
  });

  // ── 场景 6：陌生 ID 只在权威不存在后回目录 ───────────────────────────────────────────────
  await runScenario('unknown-session-id-returns-to-directory', async () => {
    const unknown = crypto.randomUUID();
    await mainPage.call('Page.navigate', {
      url: origin + '/session/' + encodeURIComponent(unknown),
    });
    const redirected = await waitFor(async () => {
      const path = await mainPage.evaluateValue('location.pathname');
      return path === '/' ? path : null;
    }, '陌生 ID 回目录', 30_000);
    const log = await mainPage.readLog();
    const read = commandFrames(log, 'session.read').find((frame) => frame.sessionId === unknown);
    assert(read, '回目录前必须发出对该 ID 的权威读取');
    const rejected = inboundFrames(log).some(
      (frame) =>
        frame.type === 'command.result' &&
        frame.body?.requestId === read.requestId &&
        frame.body?.status === 'rejected' &&
        frame.body?.error?.code === 'session.not_found',
    );
    assert(rejected, '只有权威 session.not_found 才能触发回目录，不能按「读不到」猜');
    return { unknown, redirected };
  });

  report.exitCode = report.scenarios.some((scenario) => scenario.status === 'failed') ? 1 : 0;
} catch (error) {
  report.exitCode = error instanceof ConfigError ? 2 : 1;
  report.error = describe(error);
} finally {
  report.finishedAt = new Date().toISOString();
  try {
    mkdirSync(dirname(evidencePath), { recursive: true });
    writeFileSync(evidencePath, JSON.stringify(report, null, 2) + '\n', 'utf8');
  } catch (error) {
    process.stderr.write(`证据写入失败：${describe(error)}\n`);
    report.exitCode = 2;
  }
  try {
    mainPage?.close();
    browserCdp?.close();
  } catch {}
  browser.kill();
  await sleep(200);
  process.stderr.write(
    `session-preparation-live: ${report.scenarios.filter((scenario) => scenario.status === 'passed').length} passed, ` +
      `${report.scenarios.filter((scenario) => scenario.status === 'failed').length} failed → exit ${report.exitCode}\n`,
  );
  if (report.error) process.stderr.write(`error: ${report.error}\n`);
  process.exitCode = report.exitCode;
}

/**
 * 执行一条场景：失败即记录并让整个脚本以 1 退出（可观察的验收，不做「继续跑」）。
 */
async function runScenario(name, body) {
  const startedAt = Date.now();
  try {
    const details = await body();
    report.scenarios.push({ name, status: 'passed', durationMs: Date.now() - startedAt, details });
    process.stderr.write(`PASS ${name}\n`);
  } catch (error) {
    report.scenarios.push({
      name,
      status: 'failed',
      durationMs: Date.now() - startedAt,
      error: describe(error),
    });
    process.stderr.write(`FAIL ${name}: ${describe(error)}\n`);
    throw error;
  }
}

function describe(error) {
  if (error instanceof AssertionError || error instanceof ConfigError) return error.message;
  return error?.stack ?? String(error);
}

function assert(condition, message) {
  if (!condition) throw new AssertionError(message);
}

async function waitFor(predicate, label, timeout = 20_000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    let value;
    try {
      value = await predicate();
    } catch {
      value = undefined;
    }
    if (value) return value;
    await sleep(50);
  }
  throw new AssertionError(`等待超时：${label}`);
}

async function waitForState(predicate, label, timeout) {
  return waitFor(predicate, label, timeout);
}

async function waitForFrames(predicate, label, timeout) {
  const log = mainPage;
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    const snapshot = await log.readLog();
    const value = predicate(snapshot);
    if (value) return value;
    await sleep(100);
  }
  const snapshot = await log.readLog();
  throw new AssertionError(
    `等待超时：${label}\nframes: ${JSON.stringify({
      outbound: outboundFrames(snapshot).map((frame) => frame.type),
      inbound: inboundFrames(snapshot).map((frame) => frame.type),
    })}`,
  );
}

async function waitForBrowserEndpoint(port) {
  let lastError = null;
  for (let attempt = 0; attempt < 150; attempt += 1) {
    try {
      const pages = await (
        await fetch('http://127.0.0.1:' + port + '/json/list', {
          signal: AbortSignal.timeout(1000),
        })
      ).json();
      const page = pages.find((entry) => entry.type === 'page');
      if (page?.webSocketDebuggerUrl) return page.webSocketDebuggerUrl;
    } catch (error) {
      lastError = error;
    }
    await sleep(100);
  }
  throw new ConfigError('浏览器调试端点不可用：' + describe(lastError));
}

async function connectCdp(url) {
  const socket = new WebSocket(url);
  await new Promise((resolve, reject) => {
    socket.addEventListener('open', resolve);
    socket.addEventListener('error', reject);
  });
  let nextId = 0;
  const pending = new Map();
  socket.addEventListener('message', ({ data }) => {
    const message = JSON.parse(String(data));
    const waiter = pending.get(message.id);
    if (!waiter) return;
    pending.delete(message.id);
    message.error
      ? waiter.reject(new Error(JSON.stringify(message.error)))
      : waiter.resolve(message.result);
  });
  const call = (method, params) =>
    new Promise((resolve, reject) => {
      const id = ++nextId;
      pending.set(id, { resolve, reject });
      socket.send(JSON.stringify({ id, method, params }));
    });
  return {
    call,
    close: () => {
      try {
        socket.close();
      } catch {}
    },
  };
}

/** 打开一个空白页并注入帧记录器与文件选择替身（真实 OS 文件对话框不是本层的被测面）。 */
async function openPage(browserCdp, url) {
  const created = await browserCdp.call('Target.createTarget', { url: 'about:blank' });
  const targetId = created.targetId;
  const page = await (async () => {
    const deadline = Date.now() + 10_000;
    while (Date.now() < deadline) {
      const pages = await (
        await fetch('http://127.0.0.1:' + debuggingPort + '/json/list', {
          signal: AbortSignal.timeout(1000),
        })
      ).json();
      const entry = pages.find((candidate) => candidate.id === targetId);
      if (entry?.webSocketDebuggerUrl) return connectCdp(entry.webSocketDebuggerUrl);
      await sleep(50);
    }
    throw new ConfigError('新页面目标没有调试地址');
  })();
  await page.call('Page.enable');
  await page.call('Runtime.enable');
  await page.call('Page.addScriptToEvaluateOnNewDocument', { source: instrumentation() });
  if (url !== 'about:blank') await page.call('Page.navigate', { url });

  const evaluateValue = async (expression, awaitPromise = true) => {
    const result = await page.call('Runtime.evaluate', {
      expression,
      awaitPromise,
      returnByValue: true,
    });
    if (result.exceptionDetails) {
      throw new AssertionError(
        result.exceptionDetails.exception?.description ?? result.exceptionDetails.text,
      );
    }
    return result.result.value;
  };

  const handle = {
    targetId,
    call: page.call,
    close: page.close,
    sessionId: null,
    url: origin + '/pair',
    evaluateValue,
    evaluate: (fn, arg) => evaluateValue(`(${fn.toString()})(${JSON.stringify(arg ?? null)})`),
    readLog: async () => {
      handle.url = await evaluateValue('location.href');
      const text = await evaluateValue(
        '(function () { const log = window.__acprPreparationLog; return JSON.stringify({ outbound: log?.outbound ?? [], inbound: log?.inbound ?? [] }); })()',
      );
      return JSON.parse(text);
    },
  };
  return handle;
}

/** 注入的帧记录器：记录应用自己经 WebSocket 交换的文本帧，并把文件选择对话框换成可注入的空控件。 */
function instrumentation() {
  return `(() => {
  const NativeWebSocket = window.WebSocket;
  const log = { outbound: [], inbound: [] };
  Object.defineProperty(window, '__acprPreparationLog', { value: log, configurable: true });
  const stamp = (entry) => { entry.at = Date.now(); return entry; };
  class TrackedWebSocket extends NativeWebSocket {
    constructor(...args) {
      super(...args);
      this.addEventListener('message', (event) => {
        try { log.inbound.push(stamp({ text: String(event.data) })); } catch {}
      });
    }
    send(data) {
      try { log.outbound.push(stamp({ text: String(data) })); } catch {}
      return super.send(data);
    }
  }
  window.WebSocket = TrackedWebSocket;
  // 真实 OS 文件对话框不是被测面：把 file input 的 click 停在这里，测试用 DataTransfer 注入文件。
  const nativeClick = HTMLInputElement.prototype.click;
  HTMLInputElement.prototype.click = function (...args) {
    if (this && this.type === 'file') {
      Object.defineProperty(window, '__acprFileInput', { value: this, configurable: true });
      window.__acprFileInputReady = true;
      return;
    }
    return nativeClick.apply(this, args);
  };
})();`;
}

function outboundFrames(log) {
  return log.outbound
    .map((entry) => {
      try {
        return { ...JSON.parse(entry.text), at: entry.at };
      } catch {
        return null;
      }
    })
    .filter((message) => message !== null);
}

function inboundFrames(log) {
  return log.inbound
    .map((entry) => {
      try {
        return { ...JSON.parse(entry.text), at: entry.at };
      } catch {
        return null;
      }
    })
    .filter((message) => message !== null);
}

function preparationFrames(log, action) {
  return outboundFrames(log)
    .filter((frame) => frame.type === 'session.preparation' && frame.body?.action === action)
    .map((frame) => ({ body: frame.body, at: frame.at }));
}

function preparationResults(log) {
  return inboundFrames(log)
    .filter((frame) => frame.type === 'session.preparation.result')
    .map((frame) => ({ ...frame.body, at: frame.at }));
}

function commandFrames(log, command) {
  return outboundFrames(log)
    .filter((frame) => frame.type === 'command' && frame.body?.command === command)
    .map((frame) => ({ ...frame.body, at: frame.at }));
}

function commandResults(log) {
  return inboundFrames(log)
    .filter((frame) => frame.type === 'command.result')
    .map((frame) => ({ ...frame.body, at: frame.at }));
}

function attachmentFrames(log, type) {
  return outboundFrames(log)
    .filter((frame) => frame.type === type)
    .map((frame) => ({ body: frame.body, at: frame.at }));
}

/** 页面侧的机器可读快照（只取已存在的 data-* 钩子与输入值）。 */
function preparationSnapshot() {
  const q = (selector) => document.querySelector(selector);
  const qa = (selector) => [...document.querySelectorAll(selector)];
  const phase = q('[data-preparation-phase]')?.getAttribute('data-preparation-phase') ?? null;
  const input = q('[data-preparation-input]');
  const submit = q('[data-action="preparation-submit"]');
  const attachmentStatuses = qa('[data-preparation-attachment]').map((node) =>
    node.getAttribute('data-preparation-attachment'),
  );
  return {
    phase,
    pathname: location.pathname,
    preparing: q('[data-route="session"][data-session-state="preparing"]') !== null,
    routeSessionId: q('[data-route="session"]')?.getAttribute('data-session-id') ?? null,
    input: input?.value ?? '',
    text: document.body.innerText,
    submitEnabled: submit?.getAttribute('data-submit-enabled') ?? null,
    leaseExpired: q('[data-preparation-lease-expired]') !== null,
    modes: qa('[data-action="preparation-select-mode"]').map((node) =>
      node.getAttribute('data-preparation-mode'),
    ),
    configs: qa('[data-action="preparation-select-config"]').map((node) => ({
      id: node.getAttribute('data-preparation-config'),
      value: node.getAttribute('data-preparation-config-value'),
    })),
    attachments: attachmentStatuses,
    composer: q('[data-composer]')?.getAttribute('data-composer') ?? null,
    bodyText: document.body.innerText.slice(-6000),
  };
}

/** 配对：只在 `/pair` 输入一次性配对码；不读取任何凭据。 */
async function pairFlow(config) {
  const q = (selector) => document.querySelector(selector);
  const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
  const wait = async (predicate, label, timeout = 20000) => {
    const deadline = Date.now() + timeout;
    while (Date.now() < deadline) {
      let value;
      try {
        value = predicate();
      } catch {
        value = undefined;
      }
      if (value) return value;
      await sleep(30);
    }
    throw new Error('timeout waiting for ' + label + '\n' + document.body.innerText.slice(-1200));
  };
  const setInput = (element, value) => {
    const prototype =
      element.tagName === 'TEXTAREA' ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
    Object.getOwnPropertyDescriptor(prototype, 'value').set.call(element, value);
    element.dispatchEvent(new Event('input', { bubbles: true }));
  };
  const cell = await wait(() => q('[data-pairing-code-cell="0"]'), 'pairing input');
  setInput(cell, config.code);
  await sleep(100);
  (await wait(() => q('[data-action="submit-pairing-code"]'), 'pairing submit')).click();
  await wait(() => q('[data-route="dirs"]'), 'authenticated directory page', 30000);
}

/** 目录入口 → 浏览选中工作目录 → 选 Agent → 创建（后端分配准备 ID 并跳到准备页）。 */
async function allocateFlow(config) {
  const q = (selector) => document.querySelector(selector);
  const qa = (selector) => [...document.querySelectorAll(selector)];
  const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
  const norm = (value) => {
    let text = String(value ?? '').replaceAll('\\', '/');
    if (text.length > 1) text = text.replace(/\/+$/, '');
    return text.toLowerCase();
  };
  const fail = (label) => {
    throw new Error(label + '\nurl: ' + location.href + '\ntext: ' + document.body.innerText.slice(-1600));
  };
  const wait = async (predicate, label, timeout = 20000) => {
    const deadline = Date.now() + timeout;
    while (Date.now() < deadline) {
      let value;
      try {
        value = predicate();
      } catch {
        value = undefined;
      }
      if (value) return value;
      await sleep(30);
    }
    fail('timeout waiting for ' + label);
  };
  const click = async (selector, label) => {
    const node = await wait(() => q(selector), label ?? selector);
    node.click();
    return node;
  };
  const here = () => q('[data-browse-path]')?.getAttribute('data-browse-path') ?? '';
  const browseLoaded = async (expected) => {
    const goal = norm(expected);
    await wait(() => {
      const state = q('[data-browse-state]')?.getAttribute('data-browse-state') ?? null;
      if (state === 'failed' || state === 'empty') return true;
      return norm(here()) === goal;
    }, 'browse path and entries ' + expected);
  };
  const enterPath = async (target) => {
    await wait(
      () => !q('[data-browse-state="loading"]') && (here() !== '' || qa('[data-action="enter-directory"]').length > 0),
      'directory picker listing',
    );
    const goal = norm(target);
    for (let guard = 0; guard < 64 && norm(here()) !== goal; guard += 1) {
      const current = norm(here());
      const crumb = qa('[data-crumb-path]').find((node) => norm(node.getAttribute('data-crumb-path')) === goal);
      if (crumb) {
        crumb.click();
        await browseLoaded(target);
        continue;
      }
      const prefix = current === '' ? '' : current === '/' ? '/' : current + '/';
      if (current === '' || current === '/' || goal.startsWith(prefix)) {
        const absoluteFromListing = current === '' && goal.startsWith('/');
        const segment = goal.slice(absoluteFromListing ? 1 : prefix.length).split('/')[0];
        const nextPath = absoluteFromListing ? '/' + segment : prefix + segment;
        const entries = qa('[data-action="enter-directory"]');
        const entry =
          entries.find((node) => norm(node.getAttribute('data-entry-name')) === norm(segment)) ??
          entries.find((node) => norm(node.getAttribute('data-entry-path')) === norm(nextPath)) ??
          entries.find((node) => norm(node.getAttribute('data-entry-path')) === goal);
        if (!entry) fail('directory entry not found: ' + segment + ' under ' + here());
        const expected = entry.getAttribute('data-entry-path') || nextPath;
        entry.click();
        await browseLoaded(expected);
        continue;
      }
      const up = q('[data-action="browse-up"]');
      if (!up) fail('cannot reach ' + target + ' from ' + here());
      const before = norm(here());
      up.click();
      await wait(() => norm(here()) !== before && !q('[data-browse-state="loading"]'), 'browse one level up');
    }
    if (norm(here()) !== goal) fail('directory browser did not reach ' + target);
  };

  if (q('[data-route="dirs"]') === null) {
    await click('[data-action="open-directory-list"]', 'directory list entry');
    await wait(() => q('[data-route="dirs"]'), 'directory page');
  }
  await click('[data-action="create-session-browse"]', 'create session entry');
  await click('[data-action="open-directory-picker"]', 'open directory picker');
  await enterPath(config.workspace);
  await click('[data-action="confirm-directory"]', 'confirm directory');
  await wait(
    () => norm(q('[data-create-directory-path]')?.getAttribute('data-create-directory-path')) === norm(config.workspace),
    'selected directory shows the workspace',
  );
  await wait(() => q('[data-directory-picker="open"]') === null, 'directory picker closed');
  await click('[data-action="select-agent"]', 'select Agent');
  const createButton = await wait(() => {
    const candidate = q('[data-action="submit-create"]');
    return candidate && candidate.getAttribute('aria-disabled') !== 'true' ? candidate : null;
  }, 'create button enabled');
  createButton.click();
  await wait(() => location.pathname.includes('/session/'), 'preparation route after allocation', 40000);
  const sessionId = decodeURIComponent(location.pathname.split('/').filter(Boolean).pop());
  await wait(
    () => q('[data-route="session"][data-session-id="' + sessionId + '"]') !== null,
    'preparation page identity',
    20000,
  );
  return { sessionId };
}

/** 写入首条正文并选一个附件（文件字节由测试注入，越过真实 OS 对话框）。 */
async function prepareDraft(config) {
  const q = (selector) => document.querySelector(selector);
  const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
  const wait = async (predicate, label, timeout = 20000) => {
    const deadline = Date.now() + timeout;
    while (Date.now() < deadline) {
      let value;
      try {
        value = predicate();
      } catch {
        value = undefined;
      }
      if (value) return value;
      await sleep(30);
    }
    throw new Error('timeout waiting for ' + label + '\n' + document.body.innerText.slice(-1200));
  };
  const input = await wait(
    () => q('[data-preparation-input]'),
    'preparation input',
  );
  const prototype = input.tagName === 'TEXTAREA' ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
  Object.getOwnPropertyDescriptor(prototype, 'value').set.call(input, config.text);
  input.dispatchEvent(new Event('input', { bubbles: true }));

  (await wait(() => q('[data-action="preparation-add-file"]'), 'add file entry')).click();
  const fileInput = await wait(() => window.__acprFileInput ?? null, 'file input available');
  const bytes = new TextEncoder().encode(config.attachment.text);
  const data = new DataTransfer();
  data.items.add(new File([bytes], config.attachment.name, { type: config.attachment.mediaType }));
  fileInput.files = data.files;
  fileInput.dispatchEvent(new Event('change', { bubbles: true }));
  await wait(
    () =>
      [...document.querySelectorAll('[data-preparation-attachment]')].some((node) =>
        node.textContent.includes(config.attachment.name),
      ),
    'attachment listed',
  );
  const digest = await crypto.subtle.digest('SHA-256', bytes);
  return {
    fileName: config.attachment.name,
    sha256: [...new Uint8Array(digest)].map((byte) => byte.toString(16).padStart(2, '0')).join(''),
    byteLength: bytes.length,
  };
}

/** 选择模式与配置（都取最后一个候选项：与 Agent 的默认值不同）。 */
async function selectModeAndConfig(config) {
  const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
  const nodes = [...document.querySelectorAll('[data-action="preparation-select-mode"]')];
  const modeNode = nodes.find((node) => node.getAttribute('data-preparation-mode') === config.mode);
  if (!modeNode) throw new Error('mode chip not found: ' + config.mode);
  modeNode.click();
  await sleep(300);
  const configNodes = [...document.querySelectorAll('[data-action="preparation-select-config"]')];
  const configNode = configNodes.find(
    (node) =>
      node.getAttribute('data-preparation-config') === config.config.id &&
      node.getAttribute('data-preparation-config-value') === config.config.value,
  );
  if (!configNode) throw new Error('config chip not found: ' + JSON.stringify(config.config));
  configNode.click();
  await sleep(500);
  return true;
}

function tagPage(tag) {
  Object.defineProperty(window, '__acprPreparationPageTag', { value: tag, configurable: true });
}

function pageTagStillThere(tag) {
  return window.__acprPreparationPageTag === tag;
}

/** 点击「发送首条」。 */
async function submitFirstMessage() {
  const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
  const deadline = Date.now() + 20000;
  let button = null;
  while (Date.now() < deadline) {
    button = document.querySelector('[data-action="preparation-submit"]');
    if (button && button.getAttribute('data-submit-enabled') === 'true') break;
    await sleep(50);
  }
  if (!button) throw new Error('preparation submit button not found');
  if (button.getAttribute('data-submit-enabled') !== 'true') {
    throw new Error('preparation submit is disabled:\n' + document.body.innerText.slice(-1200));
  }
  button.click();
  return true;
}
