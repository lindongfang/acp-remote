import https from 'node:https';
import fs from 'node:fs';
import {openControl,boundedNext,newIdentity} from './wire.mjs';
const root='.tmp/budget-acceptance-20261030/evidence';
const results=[];
function record(id,actual,expected){const pass=expected(actual);results.push({id,actual,pass});fs.writeFileSync(root+'/wire.json',JSON.stringify(results,null,2));console.log(id,pass?'PASS':'FAIL',JSON.stringify(actual));}
function post(path,body,extra={},source='127.0.0.1',port=30457){return new Promise((resolve,reject)=>{const start=performance.now();const req=https.request({host:'127.0.0.1',port,servername:'faraday.tail74842e.ts.net',rejectUnauthorized:true,localAddress:source,method:'POST',path:'/api/access/v1/'+path,headers:{Host:`faraday.tail74842e.ts.net:${port}`,Origin:`https://faraday.tail74842e.ts.net:${port}`,'Content-Type':'application/json','Content-Length':Buffer.byteLength(body),...extra}},res=>{let data='';res.on('data',c=>data+=c);res.on('end',()=>{let json;try{json=JSON.parse(data)}catch{}resolve({status:res.statusCode,json,retry:res.headers['retry-after'],ms:Math.round(performance.now()-start),bytes:Buffer.byteLength(body),source,authorized:res.socket?.authorized})})});req.on('error',reject);req.setTimeout(10000,()=>req.destroy(Error('timeout')));req.end(body)})}
const code=r=>r.json?.error?.code;
// Version 2 is deliberate: parser acceptance produces unsupported_version,
// while transport size or JSON depth rejection produces invalid_request.
for(const n of [16383,16384,16385]){const base='{"v":2}';const r=await post('pair/claim',base+' '.repeat(n-base.length));record('http-bytes-'+n,r,x=>x.status===400&&code(x)===(n>16384?'invalid_request':'unsupported_version'));}
for(const depth of [8,9]){const body='{"v":2,"x":'+ '['.repeat(depth-1)+'0'+']'.repeat(depth-1)+'}';record('http-depth-'+depth,await post('pair/claim',body),x=>code(x)===(depth===8?'unsupported_version':'invalid_request'));}
record('compression',await post('pair/claim','{"v":2}',{'Content-Encoding':'gzip'}),x=>code(x)==='invalid_request');
record('unknown-path',await post('not-listed','{"v":1}'),x=>x.status>=400);
record('unknown-version',await post('pair/challenge','{"v":2}'),x=>code(x)==='unsupported_version');
for(const endpoint of ['lookup','claim','challenge','status']){const port=30458;const limit=endpoint==='lookup'?5:30;const batch=[];for(let i=1;i<=limit+1;i++)batch.push(await post('pair/'+endpoint,'{',{'X-Forwarded-For':`192.0.2.${i}`,'X-Forwarded-Host':'forged.invalid'},'127.0.0.1',port));record('endpoint-'+endpoint,batch,x=>x.slice(0,limit).every(v=>v.status===400)&&x[limit].status===429&&code(x[limit])==='rate_limited'&&Number(x[limit].retry)===x[limit].json.error.retryAfterSeconds);}
const identity=newIdentity();
for(const n of [32767,32768,32769]){const ws=await openControl();try{const base=JSON.stringify({v:1,type:'hello',clientId:null,clientPublicKey:identity.publicKey,clientNonce:Buffer.alloc(32,1).toString('base64url')});ws.raw(base+' '.repeat(n-base.length));const r=await boundedNext(ws);record('ws-bytes-'+n,{...r,bytes:n,tls:ws.authorized},x=>n>32768?x.close?.code===4429:x.message?.type==='challenge');}finally{ws.destroy()}}
for(const depth of [8,9]){const ws=await openControl();try{ws.raw('{"v":1,"type":"command","requestId":"AAAAAAAAAAAAAAAAAAAAAA","action":"node.info","payload":'+'['.repeat(depth-1)+'0'+']'.repeat(depth-1)+'}');record('ws-depth-'+depth,await boundedNext(ws),x=>x.close?.code===(depth===8?4401:4400));}finally{ws.destroy()}}
// A real second loopback source, never a forwarded header.
try{const held=[];try{for(let i=0;i<33;i++)held.push(await openControl(i<24?'127.0.0.2':'127.0.0.3'));record('unauth-33',await boundedNext(held[32]),x=>x.close?.code===4429);record('unauth-existing-32',{closed:held.slice(0,32).filter(x=>x.closed).length,count:32},x=>x.closed===0);}finally{held.forEach(x=>x.destroy())}}catch(e){record('unauth-fixture',{error:e.message},()=>false)}
// Fresh source; all successful upgrades carry the required Origin/protocol.
const handshakes=[];for(let i=1;i<=31;i++){try{const ws=await openControl('127.0.0.4');handshakes.push({upgrade:101});ws.destroy()}catch(e){handshakes.push({error:e.message})}}
record('handshake-31',handshakes,x=>x.slice(0,30).every(v=>v.upgrade===101)&&x[30].error?.includes('429'));
console.log('DONE',results.length);
