#!/usr/bin/env node
/**
 * 本机会话生命周期的**真实浏览器**验收（WP6 / PV6）。
 *
 * 与 `clients/app/e2e/page/checks/session-lifecycle.ts`（假主机）互补：本脚本驱动**构建后的 PWA**
 * 对着**真实 Daemon**（真实 Sync 连接、真实 `session.view`/`session.resume` 帧、真实
 * `session.activity.changed`），用 DOM 与**真实出站帧**做断言，而不是只截图或信任 UI 文案。
 *
 * 每条场景的判据都必须能在两个地方之一被观察到：
 *   - DOM：`[data-session-activity]`、`[data-session-state-source]`、`[data-session-activation]`、
 *     `[data-session-activation-code]`、`[data-composer]`、`[data-composer-input]`、
 *     `[data-action="submit-prompt"][data-submit-enabled]`、会话路由标识；
 *   - 真实 wire：出站 `session.view`（sessionId/viewVersion 的顺序与续租）与 `session.resume`
 *     （payload 恒 `{}`），入站 `session.view.result`（accepted + leaseExpiresAt）与
 *     `session.activity.changed`。
 *
 * 必需环境变量：
 *   SESSION_LIFECYCLE_ORIGIN      https://<host>:<port>（PWA 的同源地址；证书可由 --ignore-certificate-errors 放宽）
 *   SESSION_LIFECYCLE_CODE        `device.pair.begin` 给出的配对码
 *   SESSION_LIFECYCLE_WORKSPACE   已存在的绝对目录（创建会话时作为工作目录）
 *   SESSION_LIFECYCLE_EVIDENCE    证据 JSON 的输出路径；本脚本**写入**该文件
 *
 * 可选环境变量（用于两条必须显式标注的场景；未配置时记为 SKIPPED，绝不写成 PASS）：
 *   SESSION_LIFECYCLE_FAIL_SESSION      一条**当前无法恢复**的本机会话 id（例如工作目录已被移走并被回收）；
 *                                       进入该会话后必须是明确的激活失败，且草稿保留、发送禁用。
 *   SESSION_LIFECYCLE_CAPACITY_SESSION  一条**因容量已满**而恢复失败的本机会话 id（宿主 `max_active_sessions`
 *                                       已被受保护的会话占满）；进入后 `session.capacity_exceeded` 必须可读。
 *
 * 退出码：0 = 全部场景执行且通过；1 = 至少一条断言失败；2 = 配置/浏览器/证据前置失败；
 *         3 = 有场景被显式 SKIP（其余通过）——这不是「通过」，脚本不会写 0。
 *
 * 本文件只用 Node 内置模块，不依赖仓库 package.json。
 */

import { spawn } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';

class ConfigError extends Error {}
class AssertionError extends Error {}

let origin;
let code;
let workspace;
let evidencePath;
let failSession;
let capacitySession;
try {
  origin = required('SESSION_LIFECYCLE_ORIGIN');
  code = required('SESSION_LIFECYCLE_CODE');
  workspace = required('SESSION_LIFECYCLE_WORKSPACE');
  evidencePath = required('SESSION_LIFECYCLE_EVIDENCE');
  failSession = optional('SESSION_LIFECYCLE_FAIL_SESSION');
  capacitySession = optional('SESSION_LIFECYCLE_CAPACITY_SESSION');
} catch (error) {
  process.stderr.write(`session-lifecycle-live: ${error.message}\n`);
  process.exit(2);
}

function required(name) {
  const value = process.env[name];
  if (!value || !value.trim()) throw new ConfigError(`缺少必需的环境变量 ${name}`);
  return value.trim();
}

function optional(name) {
  const value = process.env[name];
  return value && value.trim() ? value.trim() : null;
}

/** 生命周期续租间隔是固定 10 秒（服务端租期 30 秒）：观察一次续租需要跨过它。 */
const RENEW_WINDOW_MS = 22_000;
const HIDDEN_QUIET_WINDOW_MS = 12_000;

const executable = [
  process.env.BROWSER_PATH,
  'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe',
  'C:/Program Files/Google/Chrome/Application/chrome.exe',
  '/usr/bin/chromium',
  '/usr/bin/google-chrome',
].find((path) => path && existsSync(path));
if (!executable) {
  process.stderr.write('session-lifecycle-live: 找不到 Chromium 浏览器（可设 BROWSER_PATH）\n');
  process.exit(2);
}

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const debuggingPort = 10000 + Math.floor(Math.random() * 20000);
const userDataDir = mkdtempSync(join(tmpdir(), 'session-lifecycle-live-browser-'));
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
  tool: 'session-lifecycle-live',
  version: 1,
  startedAt: new Date().toISOString(),
  finishedAt: null,
  origin,
  workspace,
  scenarios: [],
  skipped: [],
  exitCode: null,
};

let browserCdp = null;
let mainPage = null;

try {
  const browserSocket = await waitForBrowserEndpoint(debuggingPort);
  browserCdp = await connectCdp(browserSocket);
  mainPage = await openPage(browserCdp, 'about:blank');
  await mainPage.call('Page.enable');
  await mainPage.call('Runtime.enable');
  // 真实 PWA 的入口是 `/pair`：pageFlow 在页内等待配对输入，必须先导航到同源配对页。
  await mainPage.call('Page.navigate', { url: origin + '/pair' });

  // ── 场景 1：进入会话立即激活（先声明查看、后派发恢复），恢复后展示本机活跃 ────────────────
  await runScenario('enter-activates-immediately', async () => {
    const created = await mainPage.evaluate(pageFlow, {
      code,
      workspace: workspace.replaceAll('\\', '/'),
    });
    const sessionId = created.sessionId;
    mainPage.sessionId = sessionId;
    mainPage.marker = created.marker;

    const log = await mainPage.readLog();
    const frames = outboundFrames(log);
    const firstView = frames.find((frame) => frame.type === 'session.view');
    const firstResume = frames.find(
      (frame) => frame.type === 'command' && frame.body?.command === 'session.resume',
    );
    assert(firstView, '进入会话必须发出 session.view');
    assert(firstResume, '进入会话必须派发 session.resume');
    const viewAt = frames.indexOf(firstView);
    const resumeAt = frames.indexOf(firstResume);
    assert(
      viewAt < resumeAt,
      `wire 顺序必须是先 session.view 后 session.resume（view=${viewAt}, resume=${resumeAt}）`,
    );
    assert(
      firstView.body?.sessionId === sessionId,
      `首条查看声明必须指向当前会话：${JSON.stringify(firstView.body)}`,
    );
    assert(
      JSON.stringify(firstResume.body?.payload) === '{}',
      `session.resume 的 Sync payload 必须是空对象：${JSON.stringify(firstResume.body)}`,
    );

    const accepted = inboundFrames(log).find(
      (frame) => frame.type === 'session.view.result' && frame.body?.status === 'accepted',
    );
    assert(accepted, '必须收到 accepted 的 session.view.result');
    assert(
      typeof accepted.body?.leaseExpiresAt === 'string',
      `accepted 必须带租期：${JSON.stringify(accepted.body)}`,
    );

    const state = await mainPage.evaluate(domSnapshot);
    assert(
      state.activity === 'active' && state.stateSource === 'local',
      `恢复成功后必须显示本机活跃：${JSON.stringify(state)}`,
    );
    assert(state.activation === null, `恢复成功后不得残留激活提示：${JSON.stringify(state)}`);
    assert(state.composer === 'editable', `恢复成功后必须可发送：${JSON.stringify(state)}`);

    const activity = inboundFrames(log).find(
      (frame) =>
        frame.type === 'event' &&
        frame.body?.eventType === 'session.activity.changed' &&
        frame.body?.payload?.view?.session?.localActivity === 'active',
    );
    assert(activity, '必须收到 localActivity=active 的 session.activity.changed');
    return { sessionId, resumeRequests: commandFrames(log, 'session.resume').length };
  });

  // ── 场景 2：只读阅读保活（可见时每 10 秒续租且不重新激活）───────────────────────────────
  await runScenario('read-only-viewing-renews-the-lease', async () => {
    const sessionId = mainPage.sessionId;
    const before = viewFrames(await mainPage.readLog()).length;
    const resumesBefore = commandFrames(await mainPage.readLog(), 'session.resume').length;

    await sleep(RENEW_WINDOW_MS);

    const log = await mainPage.readLog();
    const views = viewFrames(log);
    assert(
      views.length >= before + 2,
      `可见阅读超过 20 秒必须至少续租两次（前 ${before}，后 ${views.length}）`,
    );
    const renewed = views.slice(before);
    assert(
      renewed.every((view) => view.sessionId === sessionId),
      `续租必须指向当前会话：${JSON.stringify(renewed)}`,
    );
    assert(
      renewed.every((view, index) => index === 0 || view.viewVersion > renewed[index - 1].viewVersion),
      `续租的 viewVersion 必须单调递增：${JSON.stringify(renewed)}`,
    );
    assert(
      commandFrames(log, 'session.resume').length === resumesBefore,
      '续租 MUST NOT 重新派发 session.resume',
    );

    const state = await mainPage.evaluate(domSnapshot);
    assert(
      state.activity === 'active',
      `只读阅读期间必须保持 active：${JSON.stringify(state)}`,
    );
    return { views: views.length, renewals: renewed.length };
  });

  // ── 场景 3：后台/锁屏停续租，返回前台立即重新声明并激活 ─────────────────────────────────
  await runScenario('hidden-stops-renewal-and-return-reactivates', async () => {
    const sessionId = mainPage.sessionId;
    await mainPage.evaluate(setVisibility, 'hidden');

    const hidden = await waitFor(async () => {
      const views = viewFrames(await mainPage.readLog());
      return views.at(-1)?.sessionId === null ? views : null;
    }, '隐藏后立即声明 null 查看者', 10_000);

    const resumeBefore = commandFrames(await mainPage.readLog(), 'session.resume').length;
    const viewsBefore = hidden.length;
    await sleep(HIDDEN_QUIET_WINDOW_MS);

    const log = await mainPage.readLog();
    assert(
      viewFrames(log).length === viewsBefore,
      `隐藏期间 MUST NOT 续租（隐藏前 ${viewsBefore}，隐藏后 ${viewFrames(log).length}）`,
    );
    assert(
      commandFrames(log, 'session.resume').length === resumeBefore,
      '隐藏期间 MUST NOT 重新派发 session.resume',
    );

    await mainPage.evaluate(setVisibility, 'visible');
    const returned = await waitFor(async () => {
      const views = viewFrames(await mainPage.readLog());
      return views.at(-1)?.sessionId === sessionId ? views.at(-1) : null;
    }, '返回可见立即重新声明当前会话', 10_000);
    await waitFor(async () => {
      const log = await mainPage.readLog();
      return commandFrames(log, 'session.resume').length > resumeBefore;
    }, '返回可见立即重新激活', 15_000);

    const state = await mainPage.evaluate(domSnapshot);
    assert(
      state.activity === 'active',
      `返回前台后必须重新可用：${JSON.stringify(state)}`,
    );
    return {
      hiddenViewVersion: hidden.at(-1)?.viewVersion ?? null,
      returnedViewVersion: returned.viewVersion,
      reactivations: commandFrames(await mainPage.readLog(), 'session.resume').length - resumeBefore,
    };
  });

  // ── 场景 4：A/B 快切：已开始的 A 恢复不影响 B，迟到结果不切回、不续租 A ────────────────
  await runScenario('switching-during-activation-keeps-the-new-page', async () => {
    const sessionA = mainPage.sessionId;
    const second = await mainPage.evaluate(createSessionFlow, {
      workspace: workspace.replaceAll('\\', '/'),
    });
    const sessionB = second.sessionId;
    mainPage.sessionId = sessionB;

    // 打开 A，等它真的派发恢复后立刻切到 B（切页时 A 的恢复可能仍在途中）。
    const resumesBeforeA = commandFrames(await mainPage.readLog(), 'session.resume').filter(
      (frame) => frame.sessionId === sessionA,
    ).length;
    await mainPage.evaluate(openSessionFlow, { sessionId: sessionA });
    await waitFor(async () => {
      const log = await mainPage.readLog();
      return (
        commandFrames(log, 'session.resume').filter((frame) => frame.sessionId === sessionA)
          .length > resumesBeforeA
      );
    }, 'A 的 session.resume 已派发', 15_000);

    await mainPage.evaluate(openSessionFlow, { sessionId: sessionB });
    await waitFor(async () => {
      const state = await mainPage.evaluate(domSnapshot);
      return state.routeSessionId === sessionB && state.activity === 'active';
    }, 'B 成为当前页并激活', 20_000);

    // 给迟到的 A 结果足够时间到达，然后确认页面仍是 B、查看声明仍是 B。
    await sleep(2_500);
    const log = await mainPage.readLog();
    const views = viewFrames(log);
    const lastB = views.map((view) => view.sessionId).lastIndexOf(sessionB);
    assert(lastB >= 0, '必须存在指向 B 的查看声明');
    assert(
      views.slice(lastB).every((view) => view.sessionId !== sessionA),
      `B 之后不得再出现 A 的查看声明：${JSON.stringify(views.slice(lastB))}`,
    );
    assert(
      views.at(-1)?.sessionId === sessionB,
      `当前查看者必须仍是 B：${JSON.stringify(views.at(-1))}`,
    );
    const lateA = commandFrames(log, 'session.resume').filter(
      (frame) => frame.sessionId === sessionA,
    );
    assert(
      lateA.length === resumesBeforeA + 1,
      `切页后不得为 A 再派发恢复（前 ${resumesBeforeA}，后 ${lateA.length}）：${JSON.stringify(lateA)}`,
    );

    const state = await mainPage.evaluate(domSnapshot);
    assert(
      state.routeSessionId === sessionB,
      `迟到的 A 恢复 MUST NOT 把页面切回 A：${JSON.stringify(state)}`,
    );

    // 回到 A：仍能激活并读到既有正文（切页没有取消已开始的恢复、也没有丢上下文）。
    const promptsBefore = commandFrames(await mainPage.readLog(), 'session.prompt').length;
    await mainPage.evaluate(openSessionFlow, { sessionId: sessionA });
    await waitFor(async () => {
      const state = await mainPage.evaluate(domSnapshot);
      const markerVisible = await mainPage.evaluateValue(
        `document.body.innerText.includes(${JSON.stringify(mainPage.marker)})`,
        false,
      );
      return state.routeSessionId === sessionA && state.activity === 'active' && markerVisible;
    }, '回到 A 后激活并仍能读到既有正文', 25_000);
    const afterReturnLog = await mainPage.readLog();
    assert(
      commandFrames(afterReturnLog, 'session.prompt').length === promptsBefore,
      '重新进入会话 MUST NOT 重放旧 prompt',
    );
    mainPage.sessionId = sessionA;
    return { sessionA, sessionB, prompts: promptsBefore };
  });

  // ── 场景 5：新页面独立激活同一会话；传输断开后立即重新声明与恢复 ──────────────────────
  //
  // 同一设备在同一时刻只保留一条 Sync 连接（后连的会顶掉前一条），因此「多页」在这里按
  // 「先关闭旧页面，再由新页面接管」验证：新页面必须自己发出 session.view/session.resume 并
  // 读到既有正文。真正的多设备共享查看由 app 套件（`only_the_last_viewer_starts_the_retention_window`）
  // 在真实 wire 上覆盖；本场景不伪造第二条设备身份。
  await runScenario('a-new-page-reactivates-and-reconnects', async () => {
    const sessionId = mainPage.sessionId;
    const marker = mainPage.marker;
    await browserCdp.call('Target.closeTarget', { targetId: mainPage.targetId });
    mainPage = null;
    await sleep(1_000);

    const second = await openPage(browserCdp, 'about:blank');
    await second.call('Page.navigate', {
      url: origin + '/session/' + encodeURIComponent(sessionId),
    });
    const activated = await waitFor(async () => {
      const state = await second.evaluate(domSnapshot);
      return state.routeSessionId === sessionId && state.activity === 'active' ? state : null;
    }, '新页面独立激活同一会话', 30_000);

    const secondLog = await second.readLog();
    assert(
      viewFrames(secondLog).some((view) => view.sessionId === sessionId),
      '新页面必须声明自己的查看者',
    );
    assert(
      commandFrames(secondLog, 'session.resume').length >= 1,
      '新页面必须派发自己的 session.resume',
    );

    // 断网：关掉页面自己持有的 WebSocket（Daemon 不动）。应用必须重连后立即重新声明并激活。
    const resumesBeforeDrop = commandFrames(secondLog, 'session.resume').length;
    const viewsBeforeDrop = viewFrames(secondLog).length;
    assert(await second.dropSocket(), '页面必须持有可关闭的 WebSocket');
    await waitFor(async () => {
      const log = await second.readLog();
      return (
        commandFrames(log, 'session.resume').length > resumesBeforeDrop &&
        viewFrames(log).length > viewsBeforeDrop
      );
    }, '断线重连后重新声明查看并激活', 30_000);
    const reconnected = await waitFor(async () => {
      const state = await second.evaluate(domSnapshot);
      return state.routeSessionId === sessionId && state.activity === 'active' ? state : null;
    }, '重连后页面仍可用', 20_000);

    mainPage = second;
    mainPage.sessionId = sessionId;
    mainPage.marker = marker;
    return {
      activatedActivity: activated.activity,
      resumesBeforeDrop,
      reconnectedActivity: reconnected.activity,
      markerVisible: await second.evaluateValue(
        `document.body.innerText.includes(${JSON.stringify(marker)})`,
        false,
      ),
    };
  });

  // ── 场景 6（可选）：恢复失败的明确提示 + 草稿保留 + 发送禁用 ────────────────────────────
  if (failSession === null) {
    skip(
      'resume-failure-notice-and-draft',
      '未配置 SESSION_LIFECYCLE_FAIL_SESSION：本场景需要一条当前无法恢复的本机会话（例如工作目录已被移走并被回收）',
    );
  } else {
    await runScenario('resume-failure-notice-and-draft', async () => {
      const page = await openPage(browserCdp, 'about:blank');
      await page.call('Page.enable');
      await page.call('Runtime.enable');
      await page.call('Page.navigate', {
        url: origin + '/session/' + encodeURIComponent(failSession),
      });
      const state = await waitFor(async () => {
        const snapshot = await page.evaluate(domSnapshot);
        return snapshot.activation === 'failed' ? snapshot : null;
      }, '恢复失败必须显式呈现', 30_000);
      assert(
        state.activationCode !== null && state.activationCode !== '',
        `失败必须带服务端错误码：${JSON.stringify(state)}`,
      );
      const draft = '恢复失败时必须保留的草稿';
      const after = await page.evaluate(typeDraft, draft);
      assert(
        after.composerInput === draft,
        `失败后草稿必须保留：${JSON.stringify(after)}`,
      );
      assert(
        after.submitEnabled === 'false',
        `失败后发送必须禁用：${JSON.stringify(after)}`,
      );
      const log = await page.readLog();
      assert(
        commandFrames(log, 'session.prompt').length === 0,
        '恢复失败时 MUST NOT 派发 prompt',
      );
      return { code: state.activationCode, retryVisible: state.retryVisible };
    });
  }

  // ── 场景 7（可选）：容量拒绝的明确提示 ─────────────────────────────────────────────────
  if (capacitySession === null) {
    skip(
      'capacity-notice',
      '未配置 SESSION_LIFECYCLE_CAPACITY_SESSION：本场景需要宿主名额已被受保护会话占满',
    );
  } else {
    await runScenario('capacity-notice', async () => {
      const page = await openPage(browserCdp, 'about:blank');
      await page.call('Page.enable');
      await page.call('Runtime.enable');
      await page.call('Page.navigate', {
        url: origin + '/session/' + encodeURIComponent(capacitySession),
      });
      const state = await waitFor(async () => {
        const snapshot = await page.evaluate(domSnapshot);
        return snapshot.activation === 'failed' ? snapshot : null;
      }, '容量拒绝必须显式呈现', 30_000);
      assert(
        state.activationCode === 'session.capacity_exceeded',
        `容量拒绝的错误码必须是 session.capacity_exceeded：${JSON.stringify(state)}`,
      );
      assert(state.retryVisible, `容量拒绝必须给出重试入口：${JSON.stringify(state)}`);
      const log = await page.readLog();
      assert(
        commandFrames(log, 'session.prompt').length === 0,
        '容量拒绝时 MUST NOT 派发 prompt',
      );
      const draft = '容量已满时保留的草稿';
      const after = await page.evaluate(typeDraft, draft);
      assert(
        after.composerInput === draft && after.submitEnabled === 'false',
        `容量拒绝后草稿保留且发送禁用：${JSON.stringify(after)}`,
      );
      return { code: state.activationCode };
    });
  }

  report.exitCode = report.scenarios.some((scenario) => scenario.status === 'failed')
    ? 1
    : report.skipped.length > 0
      ? 3
      : 0;
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
    `session-lifecycle-live: ${report.scenarios.filter((s) => s.status === 'passed').length} passed, ` +
      `${report.scenarios.filter((s) => s.status === 'failed').length} failed, ` +
      `${report.skipped.length} skipped → exit ${report.exitCode}\n`,
  );
  if (report.error) process.stderr.write(`error: ${report.error}\n`);
  // SKIP 不是 PASS：只要有场景未执行，退出码就是 3，不得以 0 结束。
  process.exitCode = report.exitCode;
}

/**
 * 执行一条场景：失败即记录并让整个脚本以 1 退出（场景是可观察的验收，不做「继续跑」）。
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

function skip(name, reason) {
  report.skipped.push({ name, reason });
  process.stderr.write(`SKIP ${name}: ${reason}\n`);
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

async function waitForBrowserEndpoint(port) {
  let lastError = null;
  for (let attempt = 0; attempt < 150; attempt++) {
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

/**
 * 一个 CDP 连接。页面与控制面共用同一实现：`Target.createTarget`/`closeTarget` 只在浏览器
 * 目标上可用，页面侧只用 `Page`/`Runtime`。
 */
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

/** 打开一个空白页并注入出站/入站帧记录器（`window.__acprLifecycleLog`）。 */
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

  return {
    targetId,
    call: page.call,
    close: page.close,
    sessionId: null,
    marker: null,
    evaluateValue,
    evaluate: (fn, arg) =>
      evaluateValue(`(${fn.toString()})(${JSON.stringify(arg ?? null)})`),
    readLog: async () => {
      const text = await evaluateValue(
        '(function () { const log = window.__acprLifecycleLog; return JSON.stringify({ outbound: log?.outbound ?? [], inbound: log?.inbound ?? [] }); })()',
      );
      return JSON.parse(text);
    },
    dropSocket: () =>
      evaluateValue(
        '(function () { const log = window.__acprLifecycleLog; const socket = log?.sockets?.at(-1); if (!socket) return false; socket.close(); return true; })()',
      ),
  };
}

/** 注入的帧记录器：只记录应用自己通过 `WebSocket.send`/`message` 交换的文本帧。 */
function instrumentation() {
  return `(() => {
  const NativeWebSocket = window.WebSocket;
  const log = { outbound: [], inbound: [], sockets: [] };
  Object.defineProperty(window, '__acprLifecycleLog', { value: log, configurable: true });
  class TrackedWebSocket extends NativeWebSocket {
    constructor(...args) {
      super(...args);
      log.sockets.push(this);
      this.addEventListener('message', (event) => {
        try { log.inbound.push(String(event.data)); } catch {}
      });
    }
    send(data) {
      try { log.outbound.push(String(data)); } catch {}
      return super.send(data);
    }
  }
  window.WebSocket = TrackedWebSocket;
})();`;
}

function outboundFrames(log) {
  return log.outbound
    .map((text) => {
      try {
        return JSON.parse(text);
      } catch {
        return null;
      }
    })
    .filter((message) => message !== null);
}

function inboundFrames(log) {
  return log.inbound
    .map((text) => {
      try {
        return JSON.parse(text);
      } catch {
        return null;
      }
    })
    .filter((message) => message !== null);
}

function viewFrames(log) {
  return outboundFrames(log)
    .filter((message) => message.type === 'session.view')
    .map((message) => message.body);
}

function commandFrames(log, command) {
  return outboundFrames(log)
    .filter((message) => message.type === 'command' && message.body?.command === command)
    .map((message) => message.body);
}

/** 页面侧的机器可读快照（只取 DOM 上已存在的钩子，不解析自然语言以外的推断）。 */
function domSnapshot() {
  const q = (selector) => document.querySelector(selector);
  // 当前会话的状态证据只取会话头里的胶囊：左栏每个会话行也带
  // data-session-activity / data-session-state-source，整页第一个命中会指向别的会话。
  const header = q('[data-session-header]');
  const activity =
    header?.querySelector('[data-session-activity]')?.getAttribute('data-session-activity') ?? null;
  const stateSource =
    header
      ?.querySelector('[data-session-state-source]')
      ?.getAttribute('data-session-state-source') ?? null;
  const activation = q('[data-session-activation]')?.getAttribute('data-session-activation') ?? null;
  const activationCode =
    q('[data-session-activation-code]')?.getAttribute('data-session-activation-code') ?? null;
  const composer = q('[data-composer]')?.getAttribute('data-composer') ?? null;
  const input = q('[data-composer-input]');
  const submit = q('[data-action="submit-prompt"]');
  return {
    route: q('[data-route="session"]') !== null,
    routeSessionId: q('[data-route="session"]')?.getAttribute('data-session-id') ?? null,
    activity,
    stateSource,
    activation,
    activationCode,
    retryVisible: q('[data-action="retry-activation"]') !== null,
    composer,
    composerInputState: input?.getAttribute('data-composer-input') ?? null,
    composerInput: input?.value ?? null,
    submitEnabled: submit?.getAttribute('data-submit-enabled') ?? null,
    bodyText: document.body.innerText.slice(-4000),
  };
}

/** 只把浏览器可见性事实换成 hidden/visible 并派发真实事件（应用只通过平台端口读取它）。 */
function setVisibility(state) {
  Object.defineProperty(document, 'visibilityState', {
    configurable: true,
    get: () => state,
  });
  document.dispatchEvent(new Event('visibilitychange'));
}

/** 在失败的会话上输入草稿并返回输入框状态（只改 DOM 值并派发 input，与用户输入同一条路径）。 */
function typeDraft(draft) {
  const input = document.querySelector('[data-composer-input]');
  if (!input) throw new Error('没有找到输入框');
  const prototype = HTMLTextAreaElement.prototype;
  Object.getOwnPropertyDescriptor(prototype, 'value').set.call(input, draft);
  input.dispatchEvent(new Event('input', { bubbles: true }));
  return new Promise((resolve) => {
    setTimeout(() => {
      const submit = document.querySelector('[data-action="submit-prompt"]');
      resolve({
        composerInput: document.querySelector('[data-composer-input]')?.value ?? null,
        submitEnabled: submit?.getAttribute('data-submit-enabled') ?? null,
      });
    }, 200);
  });
}

/** 配对 → 创建一条会话 → 发一轮真实 prompt → 等本机活跃。返回会话标识与标记。 */
async function pageFlow(config) {
  const q = (selector, root) => (root ?? document).querySelector(selector);
  const qa = (selector, root) => [...(root ?? document).querySelectorAll(selector)];
  const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
  const norm = (value) => {
    let text = String(value ?? '').replaceAll('\\', '/');
    if (text.length > 1) text = text.replace(/\/+$/, '');
    return text.toLowerCase();
  };
  const fail = (label) => {
    throw new Error(
      label +
        '\nurl: ' +
        location.href +
        '\ntext: ' +
        document.body.innerText.slice(-1600),
    );
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
  const setInput = (element, value) => {
    const prototype =
      element.tagName === 'TEXTAREA' ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
    Object.getOwnPropertyDescriptor(prototype, 'value').set.call(element, value);
    element.dispatchEvent(new Event('input', { bubbles: true }));
  };
  const here = () => q('[data-browse-path]')?.getAttribute('data-browse-path') ?? '';
  const browseLoaded = async (expected) => {
    if (expected === undefined) {
      await wait(() => !q('[data-browse-state="loading"]'), 'browse request settles');
      return;
    }
    const goal = norm(expected);
    const prefix = goal.endsWith('/') ? goal : goal + '/';
    await wait(() => {
      const state = q('[data-browse-state]')?.getAttribute('data-browse-state') ?? null;
      if (state === 'failed' || state === 'empty') return true;
      if (norm(here()) !== goal) return false;
      // 目标目录可以是空的：到达路径即可，子项不是必要条件。
      return true;
    }, 'browse path and entries ' + expected);
  };
  const enterPath = async (target) => {
    await wait(
      () =>
        !q('[data-browse-state="loading"]') &&
        (here() !== '' || qa('[data-action="enter-directory"]').length > 0),
      'directory picker listing',
    );
    const goal = norm(target);
    for (let guard = 0; guard < 64 && norm(here()) !== goal; guard++) {
      const current = norm(here());
      const crumb = qa('[data-crumb-path]').find(
        (node) => norm(node.getAttribute('data-crumb-path')) === goal,
      );
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
          entries.find((node) => norm(node.getAttribute('data-entry-path')) === goal) ??
          entries
            .map((node) => ({ node, path: norm(node.getAttribute('data-entry-path')) }))
            .filter(({ path }) =>
              path === current
                ? false
                : path === '/'
                  ? goal.startsWith('/')
                  : path !== '' && goal.startsWith(path + '/'),
            )
            .sort((left, right) => right.path.length - left.path.length)[0]?.node;
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
      await wait(
        () => norm(here()) !== before && !q('[data-browse-state="loading"]'),
        'browse one level up',
      );
    }
    if (norm(here()) !== goal) fail('directory browser did not reach ' + target);
  };

  const openDirectoryList = async () => {
    if (q('[data-route="dirs"]')) return;
    await click('[data-action="open-directory-list"]', 'directory list entry');
    await wait(() => q('[data-route="dirs"]'), 'directory page');
  };

  const createSession = async () => {
    await openDirectoryList();
    await click('[data-action="create-session-browse"]', 'create session entry');
    await click('[data-action="open-directory-picker"]', 'open directory picker');
    await enterPath(config.workspace);
    await click('[data-action="confirm-directory"]', 'confirm directory');
    await wait(
      () =>
        norm(q('[data-create-directory-path]')?.getAttribute('data-create-directory-path')) ===
        norm(config.workspace),
      'selected directory shows the workspace',
    );
    await wait(() => q('[data-directory-picker="open"]') === null, 'directory picker closed');
    await click('[data-action="select-agent"]', 'select Agent');
    const createButton = await wait(() => {
      const candidate = q('[data-action="submit-create"]');
      return candidate && candidate.getAttribute('aria-disabled') !== 'true' && candidate;
    }, 'create session enabled');
    createButton.click();
    await wait(() => location.pathname.includes('/session/'), 'conversation route after creation', 30000);
    const sessionId = decodeURIComponent(location.pathname.split('/').filter(Boolean).pop());
    await wait(
      () => q('[data-route="session"][data-session-id="' + sessionId + '"]'),
      'conversation page identity',
    );
    await wait(
      () =>
        q('[data-session-activity="active"]') !== null &&
        q('[data-composer="editable"]') !== null &&
        q('[data-session-activation]') === null,
      'session is activated and editable',
      30000,
    );
    return sessionId;
  };

  // ── 配对 ──
  setInput(await wait(() => q('[data-pairing-code-cell="0"]'), 'pairing input'), config.code);
  await sleep(100);
  await click('[data-action="submit-pairing-code"]', 'pairing submit');
  await wait(() => q('[data-route="dirs"]'), 'authenticated directory page');

  // 场景 4 需要的第二条会话稍后创建；这里先创建第一条并写入一轮真实对话。
  const sessionId = await createSession();
  const marker = 'lifecycle-live-' + Math.random().toString(36).slice(2, 10);
  const input = await wait(() => q('[data-composer-input]'), 'composer input');
  setInput(input, 'Reply with exactly this token and nothing else: ' + marker);
  const submit = await wait(() => {
    const candidate = q('[data-action="submit-prompt"]');
    return candidate && candidate.getAttribute('data-submit-enabled') === 'true' && candidate;
  }, 'prompt submission is enabled');
  submit.click();
  await wait(
    () => document.body.innerText.includes(marker),
    'agent replies with the token',
    120000,
  );
  await wait(
    () =>
      q('[data-session-activity="active"]') !== null && q('[data-composer="editable"]') !== null,
    'session remains active after the turn',
    30000,
  );
  return { sessionId, marker };
}

/** 从目录页创建并打开一条新会话（切页场景需要第二条会话）。 */
async function createSessionFlow(config) {
  const q = (selector) => document.querySelector(selector);
  const qa = (selector) => [...document.querySelectorAll(selector)];
  const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
  const norm = (value) => {
    let text = String(value ?? '').replaceAll('\\', '/');
    if (text.length > 1) text = text.replace(/\/+$/, '');
    return text.toLowerCase();
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
    throw new Error('timeout waiting for ' + label + '\n' + document.body.innerText.slice(-1200));
  };
  const click = async (selector, label) => {
    const node = await wait(() => q(selector), label ?? selector);
    node.click();
    return node;
  };
  const here = () => q('[data-browse-path]')?.getAttribute('data-browse-path') ?? '';
  const goal = norm(config.workspace);
  const prefix = goal.endsWith('/') ? goal : goal + '/';
  const browseLoaded = async () => {
    await wait(() => {
      const state = q('[data-browse-state]')?.getAttribute('data-browse-state') ?? null;
      if (state === 'failed' || state === 'empty') return true;
      if (norm(here()) !== goal) return false;
      // 目标目录可以是空的：到达路径即可，子项不是必要条件。
      return true;
    }, 'browse path and entries ' + config.workspace);
  };
  const enterPath = async () => {
    await wait(
      () =>
        !q('[data-browse-state="loading"]') &&
        (here() !== '' || qa('[data-action="enter-directory"]').length > 0),
      'directory picker listing',
    );
    for (let guard = 0; guard < 64 && norm(here()) !== goal; guard++) {
      const current = norm(here());
      const crumb = qa('[data-crumb-path]').find(
        (node) => norm(node.getAttribute('data-crumb-path')) === goal,
      );
      if (crumb) {
        crumb.click();
        await browseLoaded();
        continue;
      }
      const readPrefix = current === '' ? '' : current === '/' ? '/' : current + '/';
      if (current === '' || current === '/' || goal.startsWith(readPrefix)) {
        const absoluteFromListing = current === '' && goal.startsWith('/');
        const segment = goal.slice(absoluteFromListing ? 1 : readPrefix.length).split('/')[0];
        const nextPath = absoluteFromListing ? '/' + segment : readPrefix + segment;
        const entries = qa('[data-action="enter-directory"]');
        const entry =
          entries.find((node) => norm(node.getAttribute('data-entry-name')) === norm(segment)) ??
          entries.find((node) => norm(node.getAttribute('data-entry-path')) === norm(nextPath)) ??
          entries.find((node) => norm(node.getAttribute('data-entry-path')) === goal);
        if (!entry) throw new Error('directory entry not found: ' + segment);
        const entryPath = entry.getAttribute('data-entry-path') || nextPath;
        entry.click();
        if (norm(entryPath) === goal) {
          // 目标目录被点中后，选择器可能直接选中并关闭而不是进入下一层。
          await wait(
            () => q('[data-browse-path]') === null || norm(here()) === goal,
            'target directory selection',
            5000,
          );
          return;
        }
        await browseLoaded();
        continue;
      }
      const up = q('[data-action="browse-up"]');
      if (!up) throw new Error('cannot reach ' + config.workspace);
      const before = norm(here());
      up.click();
      await wait(() => norm(here()) !== before, 'browse one level up');
    }
  };

  if (q('[data-route="dirs"]') === null) {
    await click('[data-action="open-directory-list"]', 'directory list entry');
    await wait(() => q('[data-route="dirs"]'), 'directory page');
  }
  await click('[data-action="create-session-browse"]', 'create session entry');
  const preselected = () =>
    q('[data-create-directory-path]')?.getAttribute('data-create-directory-path') ?? '';
  if (norm(preselected()) !== goal) {
    await click('[data-action="open-directory-picker"]', 'open directory picker');
    await enterPath();
    await click('[data-action="confirm-directory"]', 'confirm directory');
    await wait(() => q('[data-directory-picker="open"]') === null, 'directory picker closed');
  }
  await click('[data-action="select-agent"]', 'select Agent');
  const createButton = await wait(() => {
    const candidate = q('[data-action="submit-create"]');
    return candidate && candidate.getAttribute('aria-disabled') !== 'true' && candidate;
  }, 'create session enabled');
  createButton.click();
  await wait(() => location.pathname.includes('/session/'), 'conversation route after creation', 30000);
  const sessionId = decodeURIComponent(location.pathname.split('/').filter(Boolean).pop());
  await wait(
    () =>
      q('[data-route="session"][data-session-id="' + sessionId + '"]') &&
      q('[data-session-activity="active"]') &&
      q('[data-composer="editable"]'),
    'second session is activated',
    30000,
  );
  return { sessionId };
}

/** 从当前会话（或目录页）打开指定会话（SPA 路由，不整页重载）。 */
async function openSessionFlow(config) {
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
  if (!q('[data-route="dirs"]')) {
    const entry = await wait(() => q('[data-action="open-directory-list"]'), 'directory list entry');
    entry.click();
    await wait(() => q('[data-route="dirs"]'), 'directory page');
  }
  const row = await wait(
    () => q('[data-route="dirs"] [data-session-id="' + config.sessionId + '"]'),
    'session row in directory page',
  );
  (row.querySelector('[data-action="open-session"]') ?? row).click();
  await wait(
    () => q('[data-route="session"][data-session-id="' + config.sessionId + '"]'),
    'conversation page identity',
  );
  return { sessionId: config.sessionId };
}
