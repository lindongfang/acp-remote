import { spawn, execFile } from 'node:child_process';
import { mkdir, writeFile, readFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { createInterface } from 'node:readline';
import { promisify } from 'node:util';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';
const dependencies = dirname(fileURLToPath(import.meta.url));
const root = resolve(process.argv[2] ?? '.tmp/cl-wp1-initialize');
await mkdir(root, {recursive:true});
const adapter = `${dependencies}/node_modules/@agentclientprotocol/codex-acp/dist/index.js`;
const codex = `${dependencies}/node_modules/@openai/codex-win32-x64/vendor/x86_64-pc-windows-msvc/bin/codex.exe`;
const run = promisify(execFile);
const sha = async p => createHash('sha256').update(await readFile(p)).digest('hex');
const facts = { adapterSha256: await sha(adapter), codexSha256: await sha(codex), codexVersion: (await run(codex, ['--version'], {windowsHide:true})).stdout.trim(), versions: [] };
for (const version of [1, 2]) {
  const home = `${root}/native-v${version}`;
  // Refuse an existing native scope so this unauthenticated probe cannot reuse user sessions.
  try { await mkdir(home, {recursive:false}); } catch(e) { throw new Error(`A fresh native scope is required (${e.code})`); }
  const env = {...process.env, CODEX_HOME: home, CODEX_PATH: codex, NO_BROWSER: '1', APP_SERVER_LOGS: `${root}/private-logs-v${version}`, INITIAL_AGENT_MODE: 'read-only'};
  for (const name of Object.keys(env)) if (/^(CODEX_API_KEY|OPENAI_API_KEY|DEFAULT_AUTH_REQUEST|CODEX_CONFIG|MODEL_PROVIDER|CODEX_APP_|CODEX_THREAD_ID|CODEX_SESSION_ID|CODEX_INTERNAL_ORIGINATOR_OVERRIDE)/.test(name)) delete env[name];
  const child = spawn(process.execPath, [adapter], {cwd:root,env,windowsHide:true,stdio:['pipe','pipe','pipe']});
  const row={version,pid:child.pid, notifications:[], cleanup:null};
  const pending=new Map(); let id=0; let stderrBytes=0;
  child.stderr.on('data',b=>{stderrBytes+=b.length;});
  const lines=createInterface({input:child.stdout});
  lines.on('line',line=>{try { const m=JSON.parse(line); if (m.id!=null&&pending.has(m.id)) {pending.get(m.id)(m);pending.delete(m.id);} else if(m.method) row.notifications.push(m.method); } catch { row.nonJson=true; }});
  const request=(method,params)=>new Promise((resolve,reject)=>{const n=++id;const timer=setTimeout(()=>{pending.delete(n);reject(new Error(`timeout ${method}`));},20000);pending.set(n,m=>{clearTimeout(timer);resolve(m);});child.stdin.write(JSON.stringify({jsonrpc:'2.0',id:n,method,params})+'\n');});
  try {
    row.initialize=await request('initialize',version===1?{protocolVersion:1,clientCapabilities:{},clientInfo:{name:'acp-remote-wp1',version:'0.0.0'}}:{protocolVersion:2,capabilities:{},info:{name:'acp-remote-wp1',version:'0.0.0'}});
    if(row.initialize.result) row.newSession=await request('session/new',{cwd:root,mcpServers:[]});
  } catch(e) { row.failure=e.message; }
  finally {
    child.stdin.end();
    await new Promise(resolve=>{if(child.exitCode!==null)return resolve();const t=setTimeout(resolve,3000);child.once('exit',()=>{clearTimeout(t);resolve();});});
    if(child.exitCode===null) {try { await run('taskkill.exe',['/PID',String(child.pid),'/T','/F'],{windowsHide:true});row.cleanup='owned tree taskkill';}catch{row.cleanup='tree cleanup unconfirmed';}}
    else row.cleanup='stdio close exited';
    lines.close(); row.stderrBytes=stderrBytes;row.exitCode=child.exitCode; facts.versions.push(row);
  }
}
await writeFile(`${root}/initialize-result.json`,JSON.stringify(facts,null,2)+'\n');
console.log(JSON.stringify(facts,null,2));
