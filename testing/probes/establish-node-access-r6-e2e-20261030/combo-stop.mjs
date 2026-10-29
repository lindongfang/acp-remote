import {connect} from './cdp.mjs';import {DatabaseSync} from 'node:sqlite';
const fs=await import('node:fs/promises'),cp=await import('node:child_process');
const root=process.env.ACP_E2E_ROOT;if(!root)throw Error('Explicit root required');const workload=process.argv[2]==='workload';
const b=JSON.parse(await fs.readFile(`${root}/evidence/B-original.json`,'utf8')).identity.clientId;
const c=await connect('A');let db;
try{
 const p=await c.attach(c.targets[0].targetId);
 const ready=await p.evaluate(`(async()=>{
 const db=await new Promise(res=>{const r=indexedDB.open('acp-remote-access-v1',1);r.onsuccess=()=>res(r.result)});const row=await new Promise(res=>{const r=db.transaction('identity').objectStore('identity').get('primary');r.onsuccess=()=>res(r.result)});db.close();
 const enc=v=>btoa(String.fromCharCode(...new Uint8Array(v))).replace(/=/g,'').replace(/\\+/g,'-').replace(/\\//g,'_');const dec=v=>Uint8Array.from(atob(v.replace(/-/g,'+').replace(/_/g,'/')),x=>x.charCodeAt(0));
 const state={events:[],results:[],socket:new WebSocket(location.origin.replace('https:','wss:')+'/api/access/v1/control',['acpr.access.v1'])};window.__combo=state;
 const hello={v:1,type:'hello',clientId:row.clientId,clientPublicKey:row.publicKey,clientNonce:enc(crypto.getRandomValues(new Uint8Array(32)))};
 return await new Promise((resolve,reject)=>{const timeout=setTimeout(()=>reject(Error('Combo auth timeout')),10000);state.socket.onopen=()=>state.socket.send(JSON.stringify(hello));state.socket.onclose=e=>state.events.push({at:Date.now(),type:'close',code:e.code,reason:e.reason});state.socket.onmessage=async e=>{const m=JSON.parse(e.data);if(m.type==='challenge'){
 const parts=m.hostProof.split('.');const header=JSON.parse(new TextDecoder().decode(dec(parts[0])));const binding=JSON.parse(new TextDecoder().decode(dec(parts[1])));const pub=await crypto.subtle.importKey('raw',dec(row.nodePublicKey),{name:'ECDSA',namedCurve:'P-256'},false,['verify']);
 if(header.alg!=='ES256'||header.typ!=='acpr-auth-host+jwt'||!await crypto.subtle.verify({name:'ECDSA',hash:'SHA-256'},pub,dec(parts[2]),new TextEncoder().encode(parts[0]+'.'+parts[1]))||binding.nodeId!==row.nodeId||binding.origin!==row.origin||binding.clientPublicKey!==row.publicKey||binding.clientNonce!==hello.clientNonce)throw Error('Invalid host binding');
 const h=enc(new TextEncoder().encode(JSON.stringify({alg:'ES256',typ:'acpr-auth-client+jwt'})));const payload=enc(new TextEncoder().encode(JSON.stringify(binding)));const signing=h+'.'+payload;const sig=await crypto.subtle.sign({name:'ECDSA',hash:'SHA-256'},row.privateKey,new TextEncoder().encode(signing));state.socket.send(JSON.stringify({v:1,type:'proof',proof:signing+'.'+enc(sig)}));
 }else if(m.type==='authenticated'){state.authenticated=true;state.socket.send(JSON.stringify({v:1,type:'subscribe',requestId:'CCCCCCCCCCCCCCCCCCCCCA',resource:'devices'}))}
 else if(m.type==='result'){state.results.push({at:Date.now(),...m});if(m.requestId==='CCCCCCCCCCCCCCCCCCCCCA'){clearTimeout(timeout);resolve({authenticated:state.authenticated,subscribed:m.ok})}}
 };});})()`);
 db=new DatabaseSync(`${root}/data/node.sqlite3`);
 if(workload){db.exec("CREATE TRIGGER na015_test_workload BEFORE INSERT ON security_audit BEGIN SELECT sum(n) FROM (WITH RECURSIVE series(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM series WHERE n<4000000) SELECT n FROM series); END;");db.close();db=null}else db.exec('BEGIN IMMEDIATE');
 const sent=await p.evaluate(`(()=>{window.__combo.commandId='DDDDDDDDDDDDDDDDDDDDDA';window.__combo.sentAt=Date.now();window.__combo.socket.send(JSON.stringify({v:1,type:'command',requestId:window.__combo.commandId,action:'device.revoke',payload:{clientId:${JSON.stringify(b)}}}));return {sentAt:window.__combo.sentAt}})()`);
 await new Promise(r=>setTimeout(r,50));
 const beforeStop=await p.evaluate(`({at:Date.now(),outstanding:!window.__combo.results.some(x=>x.requestId===window.__combo.commandId),results:window.__combo.results.filter(x=>x.requestId===window.__combo.commandId)})`);
 const stopStarted=Date.now();const child=cp.spawn(`${root}/bin/acp-remote.exe`,['--config',`${root}/node.toml`,'daemon','stop'],{windowsHide:true});let out='',err='';child.stdout.on('data',x=>out+=x);child.stderr.on('data',x=>err+=x);
 const release=workload?Promise.resolve(null):new Promise(resolve=>setTimeout(()=>{db.exec('ROLLBACK');db.close();db=null;resolve(Date.now())},200));
 const [exit,releasedAt]=await Promise.all([new Promise(resolve=>child.on('exit',resolve)),release]);
 const after=await p.evaluate(`({at:Date.now(),events:window.__combo.events,results:window.__combo.results,readyState:window.__combo.socket.readyState})`);
 const pages=[];for(const t of c.targets){const page=await c.attach(t.targetId);pages.push({targetId:t.targetId,...await page.evaluate(`({events:window.__e2e?.events??[],body:document.body.innerText})`)})}
 const result={ready,sent,beforeStop,stopStarted,releasedAt,stopFinished:Date.now(),stopExit:exit,stopOutput:out,stopError:err,after,pages,coordination:workload?'Explicit test-only trigger in isolated SQLite: actual bounded recursive sum before audit insert; no product test hooks or internal scheduler barrier':'Actual external SQLite BEGIN IMMEDIATE lock; no product test hooks or internal acceptance barrier'};
 await fs.writeFile(`${root}/evidence/combo-stop${workload?'-workload':''}.json`,JSON.stringify(result,null,2));console.log({ready,sent,beforeStop,stopStarted,releasedAt,stopExit:exit,after});
}finally{if(db){db.exec('ROLLBACK');db.close()}if(workload){const cleanup=new DatabaseSync(`${root}/data/node.sqlite3`);cleanup.exec('DROP TRIGGER IF EXISTS na015_test_workload');cleanup.close()}c.close()}
