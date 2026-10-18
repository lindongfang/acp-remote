import { spawn } from 'node:child_process';
import { existsSync, mkdtempSync, statSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, parse, sep } from 'node:path';

const origin = process.env.MKDIR_TEST_ORIGIN;
const code = process.env.MKDIR_TEST_CODE;
const parent = process.env.MKDIR_TEST_PARENT;
if (!origin || !code || !parent) throw new Error('missing live test configuration');
const executable = [process.env.BROWSER_PATH,
  'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe',
  'C:/Program Files/Google/Chrome/Application/chrome.exe', '/usr/bin/chromium',
].find((path) => path && existsSync(path));
if (!executable) throw new Error('Chromium browser required');
const port = 10000 + Math.floor(Math.random() * 20000);
const browser = spawn(executable, ['--headless=new', '--disable-gpu', '--ignore-certificate-errors',
  '--no-first-run', '--no-default-browser-check', '--disable-extensions',
  '--remote-debugging-port=' + port, '--user-data-dir=' + mkdtempSync(join(tmpdir(), 'mkdir-live-browser-')),
  origin + '/pair'], { stdio: 'ignore', windowsHide: true });
let socket;
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
try {
  let target;
  for (let i = 0; i < 100 && !target; i++) {
    try { target = (await (await fetch('http://127.0.0.1:' + port + '/json/list', { signal: AbortSignal.timeout(1000) })).json()).find((page) => page.type === 'page'); } catch {}
    if (!target) await sleep(100);
  }
  if (!target) throw new Error('browser debugging endpoint unavailable');
  socket = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => { socket.addEventListener('open', resolve); socket.addEventListener('error', reject); });
  let id = 0;
  const pending = new Map();
  socket.addEventListener('message', ({ data }) => {
    const message = JSON.parse(String(data)); const waiter = pending.get(message.id);
    if (!waiter) return;
    pending.delete(message.id);
    message.error ? waiter.reject(new Error(JSON.stringify(message.error))) : waiter.resolve(message.result);
  });
  const call = (method, params) => new Promise((resolve, reject) => { const next = ++id; pending.set(next, { resolve, reject }); socket.send(JSON.stringify({ id: next, method, params })); });
  await call('Page.navigate', { url: origin + '/pair' });
  const config = { code, parent: parent.replaceAll('\\', '/'), root: parse(parent).root.replace(/[\\/]+$/, '') || '/', parts: parent.slice(parse(parent).root.length).split(sep).filter(Boolean) };
  const result = await call('Runtime.evaluate', { expression: '(' + pageTest.toString() + ')(' + JSON.stringify(config) + ')', awaitPromise: true, returnByValue: true });
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description ?? result.exceptionDetails.text);
  for (const path of [join(parent, 'project', 'src', 'api'), join(parent, 'kept-after-cancel')]) {
    if (!statSync(path).isDirectory()) throw new Error('host directory was not created');
  }
  if (existsSync(process.env.MKDIR_TEST_AGENT_CWD + '.spawn')) throw new Error('directory creation or cancellation started an Agent');
  const final = await call('Runtime.evaluate', { expression: '(' + createSession.toString() + ')(' + JSON.stringify(config) + ')', awaitPromise: true, returnByValue: true });
  if (final.exceptionDetails) throw new Error(final.exceptionDetails.exception?.description ?? final.exceptionDetails.text);
  console.log('real browser → TLS Daemon → disk: passed');
} finally { socket?.close(); browser.kill(); }

async function pageTest(config) {
  const q = (selector) => document.querySelector(selector);
  const wait = async (test, label) => { const until = Date.now() + 15000; while (Date.now() < until) { const value = test(); if (value) return value; await new Promise(r => setTimeout(r, 30)); } throw new Error(label + ': ' + document.body.innerText.slice(-1000)); };
  const click = async (selector) => { (await wait(() => q(selector), selector)).click(); };
  const input = (element, value) => { Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(element, value); element.dispatchEvent(new Event('input', { bubbles: true })); };
  input(await wait(() => q('[data-pairing-code-cell="0"]'), 'pairing input'), config.code);
  await new Promise(r => setTimeout(r, 100));
  await click('[data-action="submit-pairing-code"]');
  await wait(() => q('[data-route="dirs"]'), 'authenticated directory page');
  await click('[data-action="create-session-browse"]');
  await click('[data-action="open-directory-picker"]');
  const entries = () => [...document.querySelectorAll('[data-action="enter-directory"]')];
  const enter = async (name) => {
    const entry = await wait(() => entries().find(e => e.getAttribute('data-browse-entry') === name || e.getAttribute('data-entry-name') === name || e.textContent.trim().replace(/[›\s]+$/g, '') === name), 'directory ' + name);
    const expected = entry.getAttribute('data-entry-path');
    entry.click();
    await wait(() => q('[data-browse-path]')?.getAttribute('data-browse-path') === expected && !q('[data-browse-state="loading"]'), 'directory loaded');
  };
  await enter(config.root);
  for (const part of config.parts) await enter(part);
  const create = async (name) => {
    if (!q('[data-browse-folder-name]')) await click('[data-action="open-new-folder"]');
    input(await wait(() => q('[data-browse-folder-name]'), 'folder input'), name);
    await new Promise(r => setTimeout(r, 50));
    await click('[data-action="create-folder"]');
    await wait(() => q('[data-folder-creation-state="completed"]') && q('[data-browse-path]')?.getAttribute('data-browse-path')?.replaceAll('\\', '/').endsWith('/' + name) && !q('[data-browse-state="loading"]'), 'automatically entered directory ' + name);
    if (!q('[data-directory-picker="open"]')) throw new Error('creation closed the picker');
  };
  for (const name of ['project', 'src', 'api']) await create(name);
  await click('[data-action="confirm-directory"]');
  const selected = await wait(() => q('[data-create-directory-path]'), 'selected directory');
  if (!selected.getAttribute('data-create-directory-path').replaceAll('\\', '/').endsWith('/project/src/api')) throw new Error('wrong selected directory');
  await click('[data-action="cancel-create-sheet"]');
  await click('[data-action="create-session-browse"]');
  await click('[data-action="open-directory-picker"]');
  const crumb = await wait(() => [...document.querySelectorAll('[data-crumb-path]')].find(e => e.getAttribute('data-crumb-path')?.replaceAll('\\', '/') === config.parent), 'test parent breadcrumb');
  crumb.click();
  await wait(() => !q('[data-browse-state="loading"]'), 'parent loaded');
  await create('kept-after-cancel');
  await click('[data-action="close-directory-picker"]');
  await click('[data-action="cancel-create-sheet"]');
}

async function createSession(config) {
  const q = (selector) => document.querySelector(selector);
  const wait = async (test) => { const until = Date.now() + 15000; while (Date.now() < until) { const value = test(); if (value) return value; await new Promise(r => setTimeout(r, 30)); } throw new Error('session creation: ' + document.body.innerText.slice(-1000)); };
  (await wait(() => q('[data-action="create-session-browse"]'))).click();
  (await wait(() => q('[data-action="open-directory-picker"]'))).click();
  const crumb = await wait(() => [...document.querySelectorAll('[data-crumb-path]')].find(e => e.getAttribute('data-crumb-path')?.replaceAll('\\', '/') === config.parent));
  crumb.click();
  await wait(() => q('[data-browse-path]')?.getAttribute('data-browse-path')?.replaceAll('\\', '/') === config.parent && !q('[data-browse-state="loading"]'));
  for (const name of ['project', 'src', 'api']) {
    const entry = await wait(() => [...document.querySelectorAll('[data-action="enter-directory"]')].find(e => e.getAttribute('data-entry-name') === name));
    const path = entry.getAttribute('data-entry-path'); entry.click();
    await wait(() => q('[data-browse-path]')?.getAttribute('data-browse-path') === path && !q('[data-browse-state="loading"]'));
  }
  (await wait(() => q('[data-action="confirm-directory"]'))).click();
  await wait(() => !q('[data-directory-picker="open"]'));
  (await wait(() => q('[data-action="select-agent"]'))).click();
  (await wait(() => { const button = q('[data-action="submit-create"]'); return button?.getAttribute('aria-disabled') !== 'true' && button; })).click();
  await wait(() => location.pathname.includes('/session/'));
}
