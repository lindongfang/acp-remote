// Temporary diagnosis: replay only initialize + resume; never send prompts or print history.
const { spawn, spawnSync } = require('node:child_process');
const { createInterface } = require('node:readline');
const { DatabaseSync } = require('node:sqlite');
const assert = require('node:assert/strict');

async function main() {
  let row;
  if (!process.env.RESUME_DEBUG_EMPTY) {
    const db = new DatabaseSync(process.env.RESUME_DEBUG_DB, { readOnly: true });
    row = db.prepare('SELECT agent_session_id, workspace_cwd FROM owned_session WHERE session_id=?').get(process.env.RESUME_DEBUG_SESSION);
    db.close();
    assert(row?.agent_session_id && row?.workspace_cwd, 'Persisted resume identity required');
  }
  const env = Object.fromEntries(['PATH', 'HOME', 'USERPROFILE', 'SystemRoot', 'TEMP', 'TMP'].flatMap(key => process.env[key] ? [[key, process.env[key]]] : []));
  let child;
  let lines;
  const pending = new Map();
  let nextId = 0;
  function start() {
    child = spawn(process.execPath, [process.env.RESUME_DEBUG_ADAPTER], { env, windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'] });
    child.stderr.resume(); // Never expose potentially sensitive Agent stderr.
    lines = createInterface({ input: child.stdout });
    lines.on('line', line => {
    let msg;
    try { msg = JSON.parse(line); } catch { return; }
    if (msg.id !== undefined && pending.has(msg.id)) {
      pending.get(msg.id)(msg);
      pending.delete(msg.id);
    }
    });
  }
  function stop() {
    lines.close();
    child.stdin.end();
    if (process.platform === 'win32') spawnSync('taskkill', ['/PID', String(child.pid), '/T', '/F'], { stdio: 'ignore', windowsHide: true });
    else child.kill('SIGTERM');
  }
  start();
  async function request(method, params) {
    const id = ++nextId;
    const result = new Promise(resolve => pending.set(id, resolve));
    child.stdin.write(JSON.stringify({ jsonrpc: '2.0', id, method, params }) + '\n');
    let timer;
    try {
      return await Promise.race([result, new Promise((_, reject) => { timer = setTimeout(() => reject(new Error(method + ' timed out')), method === 'session/new' ? 45000 : 15000); })]);
    } finally { clearTimeout(timer); }
  }
  try {
    const init = await request('initialize', { protocolVersion: 1, clientCapabilities: {} });
    assert(!init.error, 'initialize rejected');
    console.log('resume advertised:', !!init.result?.agentCapabilities?.sessionCapabilities?.resume);
    if (process.env.RESUME_DEBUG_EMPTY) {
      const created = await request('session/new', { cwd: process.cwd(), mcpServers: [] });
      assert(!created.error, 'empty session creation failed');
      row = { agent_session_id: created.result.sessionId, workspace_cwd: process.cwd() };
      const live = await request('session/resume', { sessionId: row.agent_session_id, cwd: row.workspace_cwd });
      assert(!live.error, 'live empty session resume failed');
      console.log('PASS: newly created empty session resumes while live');
      const closed = await request('session/close', { sessionId: row.agent_session_id });
      assert(!closed.error, 'empty session close failed');
      console.log('PASS: empty session close acknowledged');
      if (process.env.RESUME_DEBUG_RESTART) {
        stop();
        start();
        const restarted = await request('initialize', { protocolVersion: 1, clientCapabilities: {} });
        assert(!restarted.error, 'restarted initialize rejected');
        console.log('PASS: independent Agent process restarted');
      }
    }
    const result = await request('session/resume', { sessionId: row.agent_session_id, cwd: row.workspace_cwd.replace(/^\\\\\?\\/, '') });
    console.log('resume result keys:', Object.keys(result.result ?? {}));
    if (result.error) {
      console.log('resume error code:', result.error.code);
      console.log('resume error message:', String(result.error.message).replace(/(bearer\s+|(?:token|secret|api[_-]?key)\s*[=:]\s*)\S+/gi, '$1<REDACTED>'));
      console.log('resume error data:', JSON.stringify(result.error.data, (key, value) => /token|secret|authorization|api.?key|account|email/i.test(key) ? '<REDACTED>' : typeof value === 'string' ? value.replace(/(bearer\s+|(?:token|secret|api[_-]?key)\s*[=:]\s*)\S+/gi, '$1<REDACTED>') : value));
    }
    assert(!result.error, 'session/resume rejected');
    console.log('PASS: actual ACP resume replied successfully');
  } finally {
    stop();
  }
}
main().catch(error => { console.error(error.message); process.exitCode = 1; });
