import {spawn,execFileSync} from 'node:child_process';
import {createInterface} from 'node:readline';
import {createHash} from 'node:crypto';
import fs from 'node:fs';
import {post,randomId} from './wire.mjs';
const root='D:/Project/acp-remote/.tmp/budget-acceptance-20261030';
const binary=root+'/bin/acp-remote.exe';const hash=createHash('sha256').update(fs.readFileSync(binary)).digest('hex').toUpperCase();
if(hash!=='CD6C2843899541AFB9AB6FD479960AD17E5003B6CC4F0E325618DE5A6DBA56A5')throw Error('Fixed R14 binary mismatch');
const evidence={binarySha256:hash,checks:[],passed:false};
function save(){fs.writeFileSync(root+'/evidence/pair-lifetime-real.json',JSON.stringify(evidence,null,2))}
function check(id,actual,pass){evidence.checks.push({id,actual,pass});save();console.log(id,pass,JSON.stringify(actual));if(!pass)throw Error(id)}
function trustSnapshot(){const out=execFileSync(binary,['--config',root+'/node-30457.toml','device','list'],{windowsHide:true,timeout:10000,encoding:'utf8'});return {sha256:createHash('sha256').update(out).digest('hex'),bytes:Buffer.byteLength(out)}}
const before=trustSnapshot();const child=spawn('powershell.exe',['-NoProfile','-ExecutionPolicy','Bypass','-File','testing/probes/establish-node-access-budget-20261030/pair-lifetime.ps1'],{windowsHide:true,stdio:['ignore','pipe','pipe']});
let stderr='',eventReceived=false,ended=false;child.stderr.on('data',c=>stderr+=c);const q=[],wait=[];
createInterface({input:child.stdout}).on('line',l=>{const value=JSON.parse(l);if(value.kind==='event')eventReceived=true;if(wait.length)wait.shift()(value);else q.push(value)});
const exited=new Promise(ok=>child.on('exit',code=>{ended=true;ok(code)}));
function next(ms=10000){let timer;const p=q.length?Promise.resolve(q.shift()):new Promise(ok=>wait.push(ok));return Promise.race([p,new Promise((_,no)=>timer=setTimeout(()=>no(Error('Expected owner IPC observation missing')),ms))]).finally(()=>clearTimeout(timer))}
try{
 const start=await next();if(start.kind!=='started'||!start.response.ok)throw Error('No successful pair start');const initial=start.response.result;const clock=performance.now();
 evidence.started={expiresAt:initial.expiresAt,receivedWallUnixMs:start.wallUnixMs,ipcElapsedMs:start.elapsedMs};save();console.log('PAIR_LIFETIME_STARTED',JSON.stringify(evidence.started));
 // The child timer starts immediately before writing device.pair; compensate for
 // its observed start-response latency. Every harness sleep is <=45 seconds.
 const target=299000-start.elapsedMs;
 while(performance.now()-clock<target){const remaining=target-(performance.now()-clock);await new Promise(ok=>setTimeout(ok,Math.min(45000,remaining)));if(target-(performance.now()-clock)>50)console.log('LIFETIME_PROGRESS_MS',Math.round(performance.now()-clock+start.elapsedMs));}
 const elapsed=performance.now()-clock+start.elapsedMs;
 check('still-live-at-299-seconds',{elapsedMs:Math.round(elapsed),eventReceived,childEnded:ended},elapsed>=299000&&elapsed<300000&&!eventReceived&&!ended);
 const lookup=await post('/api/access/v1/pair/lookup',{v:1,code:initial.code,requestId:randomId()});
 check('code-live-original-expiry',{elapsedMs:Math.round(performance.now()-clock+start.elapsedMs),status:lookup.status,expiresAt:lookup.body.expiresAt,originalExpiresAt:initial.expiresAt},lookup.status===200&&lookup.body.expiresAt===initial.expiresAt);
 const event=await next(15000);const value=event.response?.result;
 check('real-expired-event-near-300-seconds',{kind:event.kind,elapsedMs:event.elapsedMs,wallUnixMs:event.wallUnixMs,expiresAt:value?.expiresAt,event:value?.event,clientId:value?.clientId??null,requestMatched:event.response?.requestId===start.response.requestId},event.kind==='event'&&value?.event==='expired'&&value.expiresAt===initial.expiresAt&&value.clientId==null&&event.elapsedMs>=299900&&event.elapsedMs<=305000&&event.response.requestId===start.response.requestId);
 const closed=await next(6000);check('ipc-closed-after-expired',closed,closed.kind==='closed'&&closed.eof===true&&closed.elapsedMs-event.elapsedMs<=5000);
 const stale=await post('/api/access/v1/pair/lookup',{v:1,code:initial.code,requestId:randomId()});check('old-code-invalid-after-expiry',{status:stale.status,error:stale.body.error?.code},stale.status===401&&stale.body.error?.code==='pairing_invalid');
 const after=trustSnapshot();check('trust-list-unchanged',{before,after},before.sha256===after.sha256&&before.bytes===after.bytes);
 const exit=await exited;check('owner-client-exit',{exit,stderrEmpty:stderr.length===0},exit===0&&stderr.length===0);evidence.passed=true;save();
}finally{
 if(!ended){child.kill();await exited} // Only this directly spawned adapter; disposal closes its owned IPC client.
 evidence.clientExited=ended;save();
}
