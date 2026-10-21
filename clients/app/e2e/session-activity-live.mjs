import { spawn } from 'node:child_process';
import { existsSync, mkdtempSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

/**
 * Opt-in real-browser acceptance for the three-state session activity.
 *
 * Drives the built PWA against a real TLS Daemon whose configured Agent is the Node mock written by
 * `crates/app/tests/session_activity_browser.rs`. The mock records every `session/prompt` and every
 * permission answer in an evidence file, so "the page switch did not cancel or re-dispatch the turn"
 * is asserted on the Agent side of the wire, not by trusting the UI.
 *
 * Required environment:
 *   SESSION_ACTIVITY_ORIGIN     https://localhost:<port>
 *   SESSION_ACTIVITY_CODE       pairing code from `device.pair.begin`
 *   SESSION_ACTIVITY_WORKSPACE  existing absolute directory used for the session
 *   SESSION_ACTIVITY_EVIDENCE   Agent evidence JSONL path (written by the mock)
 */

const origin = process.env.SESSION_ACTIVITY_ORIGIN;
const code = process.env.SESSION_ACTIVITY_CODE;
const workspace = process.env.SESSION_ACTIVITY_WORKSPACE;
const evidencePath = process.env.SESSION_ACTIVITY_EVIDENCE;
if (!origin || !code || !workspace || !evidencePath) {
  throw new Error('missing live test configuration');
}

const executable = [
  process.env.BROWSER_PATH,
  'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe',
  'C:/Program Files/Google/Chrome/Application/chrome.exe',
  '/usr/bin/chromium',
].find((path) => path && existsSync(path));
if (!executable) throw new Error('Chromium browser required');

const debuggingPort = 10000 + Math.floor(Math.random() * 20000);
const userDataDir = mkdtempSync(join(tmpdir(), 'session-activity-live-browser-'));
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
    origin + '/pair',
  ],
  { stdio: 'ignore', windowsHide: true },
);

let socket;
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

try {
  let target;
  for (let attempt = 0; attempt < 100 && !target; attempt++) {
    try {
      const pages = await (await fetch('http://127.0.0.1:' + debuggingPort + '/json/list', {
        signal: AbortSignal.timeout(1000),
      })).json();
      target = pages.find((page) => page.type === 'page');
    } catch {}
    if (!target) await sleep(100);
  }
  if (!target) throw new Error('browser debugging endpoint unavailable');

  socket = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    socket.addEventListener('open', resolve);
    socket.addEventListener('error', reject);
  });

  let id = 0;
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
      const next = ++id;
      pending.set(next, { resolve, reject });
      socket.send(JSON.stringify({ id: next, method, params }));
    });

  await call('Page.enable');
  // Track every WebSocket the app opens. Closing the live one is the test's only way to produce a
  // real transport disconnect without stopping the Daemon (the Agent must keep running).
  await call('Page.addScriptToEvaluateOnNewDocument', {
    source: `(() => {
      const NativeWebSocket = window.WebSocket;
      const sockets = [];
      class TrackedWebSocket extends NativeWebSocket {
        constructor(...args) {
          super(...args);
          sockets.push(this);
        }
      }
      Object.defineProperty(window, '__acprSockets', { value: sockets, configurable: true });
      window.WebSocket = TrackedWebSocket;
    })();`,
  });
  await call('Page.navigate', { url: origin + '/pair' });

  const config = {
    code,
    workspace: workspace.replaceAll('\\', '/'),
  };
  const result = await call('Runtime.evaluate', {
    expression: '(' + pageFlow.toString() + ')(' + JSON.stringify(config) + ')',
    awaitPromise: true,
    returnByValue: true,
  });
  if (result.exceptionDetails) {
    throw new Error(
      result.exceptionDetails.exception?.description ?? result.exceptionDetails.text,
    );
  }

  // Agent-side evidence: exactly one dispatch per turn and no cancellation.
  const evidenceExists = existsSync(evidencePath);
  const records = evidenceExists
    ? readFileSync(evidencePath, 'utf8')
        .split('\n')
        .filter((line) => line.trim() !== '')
        .map((line) => JSON.parse(line))
    : [];
  const prompts = records.filter((record) => record.type === 'prompt').map((record) => record.text);
  const expected = ['hold-turn', 'permissions-turn', 'fail-turn', 'recover-turn', 'disconnect-turn'];
  if (prompts.join('|') !== expected.join('|')) {
    throw new Error(
      'Agent received unexpected prompts: ' + JSON.stringify(prompts) + '\n' + JSON.stringify(records),
    );
  }
  if (records.some((record) => record.type === 'cancel')) {
    throw new Error('the running turn was cancelled by a page switch or reconnect');
  }
  const answers = records.filter((record) => record.type === 'permission_response');
  if (answers.length !== 2 || answers.some((record) => record.optionId !== 'allow_once')) {
    throw new Error('Agent did not receive both permission answers: ' + JSON.stringify(answers));
  }
  console.log(
    'real browser → TLS Daemon → ACP Agent: idle/running/pending/reconnect passed for ' +
      result.result.value.sessionId,
  );
} finally {
  try {
    socket?.close();
  } catch {}
  browser.kill();
  await sleep(200);
}

async function pageFlow(config) {
  const q = (selector, root) => (root ?? document).querySelector(selector);
  const qa = (selector, root) => [...(root ?? document).querySelectorAll(selector)];
  const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
  const norm = (value) => {
    let text = String(value ?? '').replaceAll('\\', '/');
    if (text.length > 1) text = text.replace(/\/+$/, '');
    return text.toLowerCase();
  };
  const snapshot = () => {
    const hooks = qa(
      '[data-session-state], [data-pending-jump-bar], [data-capsule-availability], ' +
        '[data-composer], [data-turn-state], [data-submission], [data-directory-count], [role="alert"]',
    )
      .slice(-40)
      .map(
        (node) =>
          node.tagName.toLowerCase() +
          ' ' +
          [...node.attributes]
            .filter((attribute) => attribute.name.startsWith('data-'))
            .map((attribute) => attribute.name + '=' + attribute.value)
            .join(' '),
      );
    return ['url: ' + location.href, 'text: ' + document.body.innerText.slice(-1600), ...hooks].join(
      '\n',
    );
  };
  const fail = (label) => {
    throw new Error(label + '\n' + snapshot());
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
  const headerState = () =>
    q('[data-session-header] [data-session-state]')?.getAttribute('data-session-state') ?? null;
  const jumpCount = () =>
    q('[data-pending-jump-bar]')?.getAttribute('data-pending-jump-bar') ?? null;
  const rowState = (row) =>
    row?.querySelector('[data-session-state]')?.getAttribute('data-session-state') ?? null;
  const here = () => q('[data-browse-path]')?.getAttribute('data-browse-path') ?? '';
  const browseLoaded = async (expected) => {
    if (expected === undefined) {
      await wait(() => !q('[data-browse-state="loading"]'), 'browse request settles');
      return;
    }
    const goal = norm(expected);
    const prefix = goal.endsWith('/') ? goal : goal + '/';
    // 路径可能在响应前就乐观更新；必须等到这一层的条目（或明确的 empty/failed）也到达，
    // 否则下一段查找会读到上一层的旧行。
    await wait(() => {
      const state = q('[data-browse-state]')?.getAttribute('data-browse-state') ?? null;
      if (state === 'failed' || state === 'empty') return true;
      if (norm(here()) !== goal) return false;
      return qa('[data-action="enter-directory"]').some((node) =>
        norm(node.getAttribute('data-entry-path')).startsWith(prefix),
      );
    }, 'browse path and entries ' + expected);
  };

  /** Walk the server-side folder browser to an absolute path (root list → each segment). */
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
        // The root listing shows volumes/drives rather than their children; in that case the first
        // segment of an absolute path is not listed directly (e.g. "/" or "D:").
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
        if (!entry) {
          fail('directory entry not found: ' + segment + ' under ' + here());
        }
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

  const submitPrompt = async (text) => {
    const input = await wait(() => q('[data-composer-input]'), 'composer input');
    setInput(input, text);
    await wait(() => q('[data-composer-input]')?.value === text, 'composer draft is applied');
    const button = await wait(() => {
      const candidate = q('[data-action="submit-prompt"]');
      return candidate && candidate.getAttribute('data-submit-enabled') === 'true' && candidate;
    }, 'prompt submission is enabled');
    button.click();
  };

  const openDirectoryPage = async (sessionId) => {
    await click('[data-action="open-directory-list"]', 'directory list entry');
    await wait(() => q('[data-route="dirs"]'), 'directory page');
    return wait(
      () => q('[data-route="dirs"] [data-session-id="' + sessionId + '"]'),
      'session row in directory page',
    );
  };

  const openSessionFromDirectory = async (sessionId) => {
    const row = await wait(
      () => q('[data-route="dirs"] [data-session-id="' + sessionId + '"]'),
      'session row in directory page',
    );
    const action = row.querySelector('[data-action="open-session"]') ?? row;
    action.click();
    await wait(
      () => q('[data-route="session"][data-session-id="' + sessionId + '"]'),
      'conversation page',
    );
  };

  const directoryCount = (row, kind) =>
    row
      ?.closest('[data-directory-id]')
      ?.querySelector('[data-directory-count="' + kind + '"]')
      ?.textContent?.trim() ?? null;

  // ── Pair, create a session, observe the initial idle summary ──────────────────────────────────
  setInput(await wait(() => q('[data-pairing-code-cell="0"]'), 'pairing input'), config.code);
  await sleep(100);
  await click('[data-action="submit-pairing-code"]', 'pairing submit');
  await wait(() => q('[data-route="dirs"]'), 'authenticated directory page');

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
    return candidate && candidate.getAttribute('aria-disabled') !== 'true' && candidate;
  }, 'create session enabled');
  createButton.click();
  await wait(() => location.pathname.includes('/session/'), 'conversation route after creation', 30000);

  const sessionId = decodeURIComponent(location.pathname.split('/').filter(Boolean).pop());
  await wait(
    () => q('[data-route="session"][data-session-id="' + sessionId + '"]'),
    'conversation page identity',
  );
  await wait(() => headerState() === 'idle', 'new session is idle', 30000);
  await wait(() => q('[data-composer="editable"]'), 'new session composer is editable');

  // The directory row carries the same state as the header.
  let row = await openDirectoryPage(sessionId);
  if (rowState(row) !== 'idle') fail('directory row for the new session is not idle: ' + rowState(row));
  await openSessionFromDirectory(sessionId);
  await wait(() => headerState() === 'idle', 'idle after returning to the session');

  // ── A delayed turn is running, survives a page switch, and completes once ─────────────────────
  await submitPrompt('hold-turn');
  await wait(() => headerState() === 'running', 'session is running after prompt submission');
  await wait(() => q('[data-composer="in_progress"]'), 'composer reports the running turn');
  if (q('[data-action="submit-prompt"]')?.getAttribute('data-submit-enabled') !== 'false') {
    fail('running turn did not disable prompt submission');
  }
  const sidebarRow = await wait(
    () => q('[data-route="session"] [data-session-row-variant][data-session-id="' + sessionId + '"]'),
    'sidebar session row',
  );
  if (rowState(sidebarRow) !== 'running') fail('sidebar row is not running: ' + rowState(sidebarRow));

  row = await openDirectoryPage(sessionId);
  if (rowState(row) !== 'running') fail('directory row is not running: ' + rowState(row));
  if (directoryCount(row, 'active') !== '1') {
    fail('directory active session count is not one: ' + directoryCount(row, 'active'));
  }
  await openSessionFromDirectory(sessionId);
  await wait(() => headerState() === 'running', 'turn still running after switching pages');
  await wait(() => headerState() === 'idle', 'delayed turn completed exactly once', 30000);
  await wait(() => document.body.innerText.includes('hold-done'), 'agent output of the delayed turn');

  // ── Two permission requests: one pending session, jump bar counts interactions ────────────────
  await submitPrompt('permissions-turn');
  await wait(() => headerState() === 'pending', 'session is pending while permission requests are open');
  // Real-time request views do not carry `requestVersion`; the negotiated contract requires the
  // client to re-confirm the current effective request before it can answer.
  await wait(() => q('[data-action="refresh-interactions"]'), 'refresh interactions action');
  q('[data-action="refresh-interactions"]').click();
  await wait(() => jumpCount() === '2', 'jump bar counts both answerable interactions');

  row = await openDirectoryPage(sessionId);
  if (rowState(row) !== 'pending') fail('directory row is not pending: ' + rowState(row));
  if (directoryCount(row, 'pending') !== '1') {
    fail('two interactions changed the pending session count: ' + directoryCount(row, 'pending'));
  }
  await openSessionFromDirectory(sessionId);
  await wait(() => headerState() === 'pending', 'pending survives the page switch');
  await wait(() => jumpCount() === '2', 'both interactions are answerable after returning');

  const answerableInteractions = () =>
    new Set(
      qa('[data-answer-option]').map((node) => node.getAttribute('data-answer-interaction-id')),
    );

  await wait(() => answerableInteractions().size === 2, 'two answerable interactions after returning');
  let answers = qa('[data-answer-option]');
  answers[0].click();
  await wait(() => jumpCount() === '1', 'jump bar counts the remaining interaction');
  if (headerState() !== 'pending') fail('session stopped being pending after only one answer');
  await wait(() => answerableInteractions().size === 1, 'first interaction is handled');
  qa('[data-answer-option]')[0].click();
  await wait(() => jumpCount() === null, 'no answerable interactions remain');
  await wait(() => headerState() === 'running', 'unfinished turn is running once interactions resolve');
  await wait(() => headerState() === 'idle', 'turn is idle after the last answer', 30000);
  await wait(() => document.body.innerText.includes('permissions-done'), 'agent output after permission turn');

  // ── A failed turn ends idle with the failure visible and accepts a new prompt ─────────────────
  await submitPrompt('fail-turn');
  await wait(() => q('[data-turn-state="failed"]'), 'failed turn card', 30000);
  await wait(() => headerState() === 'idle', 'failed turn leaves the session idle');
  const failure = q('[data-turn-failure="true"]');
  if (!failure || !failure.textContent.includes('模拟失败')) {
    fail('failure reason is not visible');
  }
  await wait(() => q('[data-composer="editable"]'), 'composer is editable after a failure');
  await submitPrompt('recover-turn');
  await wait(() => headerState() === 'idle', 'session returns to idle after the recovering turn', 30000);
  await wait(() => document.body.innerText.includes('recover-done'), 'agent output after recovery');

  // ── A transport disconnect preserves the confirmed state and re-converges ─────────────────────
  await submitPrompt('disconnect-turn');
  await wait(() => headerState() === 'running', 'session is running before the disconnect');
  if (!Array.isArray(window.__acprSockets)) fail('WebSocket instrumentation is missing');
  const socketsBefore = window.__acprSockets.length;
  const live = window.__acprSockets.filter((socket) => socket.readyState === 1);
  if (live.length === 0) fail('no live WebSocket to disconnect');
  live.forEach((socket) => socket.close(1000, 'e2e_client_disconnect'));
  await wait(
    () => q('[data-capsule-availability="offline"]'),
    'device capsule reports offline while disconnected',
    10000,
  );
  // 内容来源条与同步时刻已移除：页面不得再出现这两个机器锚点，连接状态只由设备胶囊表达。
  if (q('[data-content-origin]') || q('[data-connection-strip-state]') || q('[data-synced-at]')) {
    fail('content-origin / sync-time markers must not be rendered');
  }
  if (headerState() !== 'running') {
    fail('last confirmed state was not preserved across the disconnect: ' + String(headerState()));
  }
  await wait(() => window.__acprSockets.length > socketsBefore, 'new socket after reconnect', 30000);
  await wait(
    () => q('[data-capsule-availability="online"]'),
    'device capsule reports online after reconnect',
    30000,
  );
  if (q('[data-content-origin]') || q('[data-connection-strip-state]') || q('[data-synced-at]')) {
    fail('content-origin / sync-time markers must not be rendered after reconnect');
  }
  await wait(() => headerState() === 'idle', 'state re-converged after reconnect', 30000);
  await wait(
    () => document.body.innerText.includes('disconnect-done'),
    'events that happened while disconnected were replayed',
  );

  return { sessionId };
}
