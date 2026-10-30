import { spawn, execFile } from 'node:child_process';
import { readFile, writeFile, mkdir, access } from 'node:fs/promises';
import { createInterface } from 'node:readline';
import { promisify } from 'node:util';
import { dirname, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
const dependencies=resolve(dirname(fileURLToPath(import.meta.url)),'node_modules');
const ownedTemporaryRoot=resolve(dirname(fileURLToPath(import.meta.url)),'../../../.tmp');
const root=resolve(process.argv[3]??resolve(ownedTemporaryRoot,'cl-wp1-authenticated-run'));
if(!root.startsWith(ownedTemporaryRoot+sep))throw new Error('Probe output must stay inside the repository temporary directory');
const version=Number(process.argv[2]??2);
if(![1,2].includes(version))throw new Error('Protocol version must be 1 or 2');
const cwd=resolve(root,'workspace').replaceAll('\\','/');
const home=`${root}/native-v${version}`;
await access(`${home}/auth.json`);
const codex=`${dependencies}/@openai/codex-win32-x64/vendor/x86_64-pc-windows-msvc/bin/codex.exe`;
const adapter=`${dependencies}/@agentclientprotocol/codex-acp/dist/index.js`;
const run=promisify(execFile);
const evidence={version,steps:[],permissions:[],events:[],cleanup:[]};
await mkdir(cwd,{recursive:true});
let serial=0;
function connection() {
 const env={...process.env,CODEX_HOME:home,CODEX_PATH:codex,NO_BROWSER:'1',INITIAL_AGENT_MODE:'read-only',APP_SERVER_LOGS:`${root}/private-logs-v${version}`};
 for(const name of Object.keys(env))if(/^(CODEX_API_KEY|OPENAI_API_KEY|DEFAULT_AUTH_REQUEST|CODEX_CONFIG|MODEL_PROVIDER|CODEX_APP_|CODEX_THREAD_ID|CODEX_SESSION_ID|CODEX_INTERNAL_ORIGINATOR_OVERRIDE)/.test(name))delete env[name];
 const child=spawn(process.execPath,[adapter],{cwd,env,windowsHide:true,stdio:['pipe','pipe','pipe']});
 const pending=new Map();const updates=[];let stderrBytes=0;
 child.stderr.on('data',b=>{stderrBytes+=b.length;});
 const input=createInterface({input:child.stdout});
 input.on('line',line=>{
   let m;try{m=JSON.parse(line);}catch{return;}
   if(m.id!=null&&pending.has(m.id)){pending.get(m.id)(m);pending.delete(m.id);return;}
   if(m.method==='session/update'){updates.push(m.params.update);evidence.events.push({connection:child.pid,sessionId:m.params.sessionId,update:m.params.update});return;}
   if(m.method&&m.id!=null){
     if(m.method==='session/request_permission'){
       const tool=m.params.subject?.toolCall??m.params.toolCall;
       const raw=tool?.rawInput;
       const command=raw?.command??'';
       const exactCwd=String(raw?.cwd??'').replaceAll('\\','/').toLowerCase()===cwd.toLowerCase();
       const safe=exactCwd&&/^"?C:\\+Windows\\+System32\\+WindowsPowerShell\\+v1\.0\\+powershell\.exe"? -Command "Set-Content -LiteralPath 'approval-probe\.txt' -Value 'WP1_APPROVAL_PROBE' -NoNewline"$/i.test(command);
       const option=safe?m.params.options?.find(o=>o.kind==='allow_once'):null;
       const outcome=option?{outcome:'selected',optionId:option.optionId}:{outcome:'cancelled'};
       evidence.permissions.push({requestId:m.id,params:m.params,decision:outcome});
       child.stdin.write(JSON.stringify({jsonrpc:'2.0',id:m.id,result:{outcome}})+'\n');
     }else child.stdin.write(JSON.stringify({jsonrpc:'2.0',id:m.id,error:{code:-32601,message:'Unsupported probe client method'}})+'\n');
   }
 });
 const request=(method,params,limit=45000)=>new Promise((resolve,reject)=>{const id=++serial;const t=setTimeout(()=>{pending.delete(id);reject(new Error(`timeout ${method}`));},limit);pending.set(id,m=>{clearTimeout(t);resolve(m);});child.stdin.write(JSON.stringify({jsonrpc:'2.0',id,method,params})+'\n');});
 const notify=(method,params)=>child.stdin.write(JSON.stringify({jsonrpc:'2.0',method,params})+'\n');
 const waitIdle=async(start,limit=90000)=>{const deadline=Date.now()+limit;while(Date.now()<deadline){const found=updates.slice(start).find(u=>u.sessionUpdate==='state_update'&&u.state==='idle');if(found)return found;await new Promise(r=>setTimeout(r,100));}throw new Error('timeout idle');};
 const close=async()=>{child.stdin.end();await new Promise(r=>{if(child.exitCode!==null)return r();const t=setTimeout(r,5000);child.once('exit',()=>{clearTimeout(t);r();});});let cleanup='stdio exited';if(child.exitCode===null){try{await run('taskkill.exe',['/PID',String(child.pid),'/T','/F'],{windowsHide:true});cleanup='owned tree killed';}catch{cleanup='unconfirmed';}}input.close();evidence.cleanup.push({pid:child.pid,exitCode:child.exitCode,cleanup,stderrBytes});};
 return {request,notify,updates,waitIdle,close};
}
let client;
async function prompt(text,{cancel=false}={}){
 const start=client.updates.length;
 const answer=client.request('session/prompt',{sessionId:evidence.sessionId,prompt:[{type:'text',text}]},90000);
 if(cancel)setTimeout(()=>client.notify('session/cancel',{sessionId:evidence.sessionId}),1500);
 const response=await answer;
 if(response.error)return {code:response.error.code,error:response.error.message};
 const terminal=version===2?await client.waitIdle(start):response.result;
 return {response:response.result,terminal};
}
async function initialize(){return client.request('initialize',version===1?{protocolVersion:1,clientCapabilities:{},clientInfo:{name:'acp-remote-wp1',version:'0.0.0'}}:{protocolVersion:2,capabilities:{},info:{name:'acp-remote-wp1',version:'0.0.0'}});}
try {
 client=connection();const init=await initialize();evidence.steps.push({name:'initialize',response:init.result??init.error});
 const created=await client.request('session/new',{cwd,mcpServers:[]});
 if(created.error)throw new Error(`new-session code ${created.error.code}: ${created.error.message}`);
 evidence.sessionId=created.result.sessionId;
 evidence.steps.push({name:'create',sessionId:evidence.sessionId});
 const first=await prompt('This is a dedicated compatibility test. Remember the exact marker WP1_NATIVE_CONTEXT_63F9. Reply only ACK_63F9. Do not use any tools.');
 evidence.steps.push({name:'marker',...first});
 const closed=await client.request('session/close',{sessionId:evidence.sessionId});evidence.steps.push({name:'close',response:closed.result??closed.error});
 await client.close();client=null;
 client=connection();await initialize();const before=client.updates.length;
 const resumed=await client.request('session/resume',{sessionId:evidence.sessionId,cwd,mcpServers:[]});
 evidence.steps.push({name:'resume-same-native-id',response:resumed.error??{ok:true},updates:client.updates.slice(before)});
 if(resumed.error)throw new Error(`resume code ${resumed.error.code}`);
 evidence.steps.push({name:'recall',...await prompt('What exact marker did I ask you to remember? Reply only that marker. Do not use any tools.')});
 evidence.steps.push({name:'cancel',...await prompt('Run a PowerShell command that waits 60 seconds (Start-Sleep -Seconds 60) without reading or modifying any files. This is a dedicated cancellation compatibility test; do not do any other work.',{cancel:true})});
 evidence.steps.push({name:'permission',...await prompt('This is a permission compatibility test. Use exactly the PowerShell command Set-Content -LiteralPath \'approval-probe.txt\' -Value \'WP1_APPROVAL_PROBE\' -NoNewline in the current dedicated test directory. The session is read-only: request explicit approval for this write. Do not modify anything else. If approval is denied or cancelled, stop.')});
 evidence.approvedFileMatches=(await readFile(`${cwd}/approval-probe.txt`,'utf8').catch(()=>null))==='WP1_APPROVAL_PROBE';
 evidence.steps.push({name:'final-close',response:await client.request('session/close',{sessionId:evidence.sessionId})});
}catch(e){evidence.failure=e.message;}
finally{if(client)await client.close();await writeFile(`${root}/result-approved-v${version}.json`,JSON.stringify(evidence,null,2)+'\n');}
console.log(JSON.stringify({version,sessionId:evidence.sessionId,steps:evidence.steps,permissions:evidence.permissions.length,failure:evidence.failure,cleanup:evidence.cleanup},null,2));
