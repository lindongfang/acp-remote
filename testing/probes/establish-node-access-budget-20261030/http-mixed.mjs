import {spawn} from 'node:child_process';
import {createInterface} from 'node:readline';
import https from 'node:https';
import fs from 'node:fs';
import {newIdentity,randomId,compact,verified,openControl,boundedNext} from './wire.mjs';
const root='.tmp/budget-acceptance-20261030/';const results=[];const children=new Set(),sockets=[];
function save(id,actual,pass){results.push({id,actual,pass});fs.writeFileSync(root+'evidence/'+'r9-http-mixed.json',JSON.stringify(results,null,2));console.log(id,pass,JSON.stringify(actual));if(!pass)throw Error(id)}
async function bounded(p,ms=10000){let timer;try{return await Promise.race([p,new Promise((_,no)=>timer=setTimeout(()=>no(Error('deadline')),ms))])}finally{clearTimeout(timer)}}
function bridge(mode='pair',bytes=0){const p=spawn('powershell.exe',['-NoProfile','-ExecutionPolicy','Bypass','-File','testing/probes/establish-node-access-budget-20261030/owner-bridge.ps1','-Mode',mode,'-Bytes',String(bytes)],{windowsHide:true,stdio:['pipe','pipe','pipe']});children.add(p);const q=[],wait=[];let error='';p.stderr.on('data',c=>error+=c);createInterface({input:p.stdout}).on('line',l=>{const v=JSON.parse(l);if(wait.length)wait.shift().ok(v);else q.push(v)});const exited=new Promise((ok,no)=>p.on('exit',code=>{children.delete(p);for(const w of wait)w.no(Error('bridge exited '+code+' '+error));code===0?ok():no(Error('bridge failed '+code+' '+error))}));exited.catch(()=>{});return {next:()=>q.length?Promise.resolve(q.shift()):new Promise((ok,no)=>wait.push({ok,no})),send:v=>p.stdin.write(v+'\n'),end:()=>p.stdin.end(),exited,p}}
function post(endpoint,body,source){return new Promise((ok,no)=>{const bytes=typeof body==='string'?body:JSON.stringify(body);const req=https.request({host:'127.0.0.1',port:30457,servername:'faraday.tail74842e.ts.net',rejectUnauthorized:true,localAddress:source,method:'POST',path:'/api/access/v1/pair/'+endpoint,headers:{Host:'faraday.tail74842e.ts.net:30457',Origin:'https://faraday.tail74842e.ts.net:30457','Content-Type':'application/json','Content-Length':Buffer.byteLength(bytes)}},res=>{let b='';res.on('data',c=>b+=c);res.on('end',()=>ok({status:res.statusCode,body:JSON.parse(b),retry:res.headers['retry-after']}))});req.on('error',no);req.end(bytes)})}
const common=(l,key,pairingId)=>({v:1,nodeId:l.nodeId,origin:l.origin,requestId:l.requestId,challengeId:l.challengeId,nonce:l.nonce,iat:l.iat,exp:l.exp,pairingId,clientPublicKey:key.publicKey});
try{
 const b=bridge();const started=await bounded(b.next());if(!started.ok)throw Error('pair unavailable');const key=newIdentity();const source='127.0.0.90';
 const lookup=await post('lookup',{v:1,code:started.result.code,requestId:randomId()},source);const l=lookup.body;
 const validClaim={v:1,proof:compact('acpr-pair-claim+jwt',{...common(l,key,l.pairingId),displayName:'Budget mixed endpoints'},key.privateKey)};
 const claims=[];for(let i=0;i<31;i++)claims.push(await post('claim',i%2===0?validClaim:{v:1,proof:'invalid'},source));
 save('claim-mixed-30-31',{statuses:claims.map(x=>x.status),retry:claims[30].retry},claims.slice(0,30).every((r,i)=>r.status===(i%2===0?200:401))&&claims[30].status===429&&claims[30].retry==='60');
 b.send('next');await bounded(b.next());
 const challenges=[];for(let i=0;i<31;i++)challenges.push(await post('challenge',i%2===0?{v:1,pairingId:l.pairingId,clientPublicKey:key.publicKey,requestId:randomId()}:'{',source));
 save('challenge-mixed-30-31',{statuses:challenges.map(x=>x.status),retry:challenges[30].retry},challenges.slice(0,30).every((r,i)=>r.status===(i%2===0?200:400))&&challenges[30].status===429&&challenges[30].retry==='60');
 const c=await post('challenge',{v:1,pairingId:l.pairingId,clientPublicKey:key.publicKey,requestId:randomId()},'127.0.0.91');
 const validStatus={v:1,proof:compact('acpr-pair-status+jwt',common(c.body,key,l.pairingId),key.privateKey)};
 const statuses=[];for(let i=0;i<31;i++)statuses.push(await post('status',i===0?validStatus:{v:1,proof:'invalid'},source));
 save('status-mixed-30-31',{statuses:statuses.map(x=>x.status),retry:statuses[30].retry},statuses[0].status===200&&statuses.slice(1,30).every(r=>r.status===401)&&statuses[30].status===429&&statuses[30].retry==='60');
 b.send('reject');await bounded(b.next());b.end();await b.exited;
}finally{for(const p of children)p.stdin.end();await Promise.allSettled([...children].map(p=>new Promise(ok=>p.once('exit',ok))))}
