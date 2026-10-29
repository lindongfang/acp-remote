import net from 'node:net';
import fs from 'node:fs';
import {post,newIdentity,randomId,compact} from './wire.mjs';
const root='.tmp/budget-acceptance-20261030/';
const {pipe}=JSON.parse(fs.readFileSync(root+'pipe.json','utf8'));
const results=[];
function save(id,actual,pass){results.push({id,actual,pass});fs.writeFileSync(root+'evidence/ipc-time.json',JSON.stringify(results,null,2));console.log(id,pass,JSON.stringify(actual));if(!pass)process.exitCode=1;}
function connect(){return new Promise((ok,no)=>{const s=net.connect(pipe);s.once('connect',()=>ok(s));s.once('error',no)})}
function lines(s){let b='';const q=[],wait=[];s.on('data',c=>{b+=c;while(b.includes('\n')){const i=b.indexOf('\n');const v=JSON.parse(b.slice(0,i));b=b.slice(i+1);if(wait.length)wait.shift()(v);else q.push(v)}});return ()=>q.length?Promise.resolve(q.shift()):new Promise(ok=>wait.push(ok));}
async function bounded(p,ms=10000){let timer;try{return await Promise.race([p,new Promise((_,no)=>timer=setTimeout(()=>no(Error('observation timeout')),ms))])}finally{clearTimeout(timer)}}
// Start both clocks from successful pipe connection, without simulated time.
const short=await connect();const shortStart=performance.now();let shortClosedAt=null;
short.on('close',()=>{shortClosedAt=performance.now()});
short.write('{'); // Incomplete short request: no newline, no valid request yet.
const pair=await connect();const next=lines(pair);const requestId=randomId();
pair.write(JSON.stringify({v:1,requestId,command:'device.pair'})+'\n');
try{
 const started=await bounded(next());if(!started.ok||!started.result?.code)throw Error('pair start rejected');const pairStart=performance.now();const deadline=started.result.expiresAt;
 await new Promise(ok=>setTimeout(ok,29000));save('short-before-30s',{elapsedMs:Math.round(performance.now()-shortStart),closed:shortClosedAt!==null},shortClosedAt===null);
 await new Promise(ok=>setTimeout(ok,2200));
 save('short-after-30s',{observedMs:Math.round(performance.now()-shortStart),closedElapsedMs:shortClosedAt===null?null:Math.round(shortClosedAt-shortStart)},shortClosedAt!==null&&shortClosedAt-shortStart>=29900&&shortClosedAt-shortStart<32000);
 const key=newIdentity();const lookup=await post('/api/access/v1/pair/lookup',{v:1,code:started.result.code,requestId:randomId()});if(lookup.status!==200)throw Error('late lookup failed');const l=lookup.body;
 const proof=compact('acpr-pair-claim+jwt',{v:1,nodeId:l.nodeId,origin:l.origin,requestId:l.requestId,challengeId:l.challengeId,nonce:l.nonce,iat:l.iat,exp:l.exp,pairingId:l.pairingId,clientPublicKey:key.publicKey,displayName:'Budget IPC after 30 seconds'},key.privateKey);
 const claim=await post('/api/access/v1/pair/claim',{v:1,proof});const event=await bounded(next());
 save('pair-stream-after-30s',{elapsedMs:Math.round(performance.now()-pairStart),claimStatus:claim.status,event:event.result?.event,originalExpiresAt:deadline,lookupExpiresAt:l.expiresAt},claim.status===200&&event.result?.event==='pending_confirmation'&&l.expiresAt===deadline&&performance.now()-pairStart>30000);
 // Disconnect is the real owner cancellation path; no trust is approved.
}finally{short.destroy();pair.destroy()}
