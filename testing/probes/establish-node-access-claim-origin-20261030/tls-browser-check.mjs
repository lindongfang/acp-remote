import fs from 'node:fs/promises';import {connect,root} from './edge-cdp.mjs';
const c=await connect();
const readIdentity=`(async()=>{const db=await new Promise((ok,no)=>{const q=indexedDB.open('acp-remote-access-v1');q.onsuccess=()=>ok(q.result);q.onerror=()=>no(q.error)});const r=await new Promise(ok=>{const q=db.transaction('identity').objectStore('identity').get('primary');q.onsuccess=()=>ok(q.result)});db.close();const digest=await crypto.subtle.digest('SHA-256',new TextEncoder().encode(r.publicKey));return {nodeId:r.nodeId,nodePublicKey:r.nodePublicKey,clientId:r.clientId,status:r.status,publicKeySha256:Array.from(new Uint8Array(digest),x=>x.toString(16).padStart(2,'0')).join(''),privateExtractable:r.privateKey.extractable};})()`;
try{
 const before=await c.evaluate(readIdentity);if(before.status!=='consumed')throw Error('NA024 trusted baseline missing');await fs.writeFile(root+'evidence/pin-baseline.json',JSON.stringify(before,null,2));
 const hook=`window.__tlsAudit={hello:0,proof:0,authSign:0,challenge:0,authenticated:0};const nativeSign=crypto.subtle.sign.bind(crypto.subtle);crypto.subtle.sign=function(...args){try{const text=new TextDecoder().decode(args[2]);const header=JSON.parse(atob(text.split('.')[0].replace(/-/g,'+').replace(/_/g,'/')));if(header.typ==='acpr-auth-client+jwt')window.__tlsAudit.authSign++;}catch{}return nativeSign(...args);};const NativeSocket=window.WebSocket;window.WebSocket=class extends NativeSocket{constructor(...args){super(...args);this.addEventListener('message',e=>{try{const m=JSON.parse(e.data);if(m.type==='challenge')window.__tlsAudit.challenge++;if(m.type==='authenticated')window.__tlsAudit.authenticated++;}catch{}});}send(data){try{const m=JSON.parse(data);if(m.type==='hello')window.__tlsAudit.hello++;if(m.type==='proof')window.__tlsAudit.proof++;}catch{}return super.send(data);}};`;
 await c.send('Page.enable',{},c.sessionId);const added=await c.send('Page.addScriptToEvaluateOnNewDocument',{source:hook},c.sessionId);
 const results=[];
 for(const mode of ['replacement-key','wrong-node-id','wrong-origin','bad-host-proof','pass']){
  await fs.writeFile(root+'fixture-mode.json',JSON.stringify({mode}));await c.send('Page.reload',{},c.sessionId);
  for(let i=0;i<80;i++){if(await c.evaluate('!!window.__tlsAudit&&window.__tlsAudit.challenge>0'))break;await new Promise(ok=>setTimeout(ok,100));}
  await new Promise(ok=>setTimeout(ok,1200));
  const audit=await c.evaluate('window.__tlsAudit'),after=await c.evaluate(readIdentity);
  const page=await c.evaluate('({secure:isSecureContext,authenticated:document.body.innerText.includes("NODE CONTROL / AUTHENTICATED"),text:document.body.innerText.slice(0,1100),resourceOrigins:[...new Set(performance.getEntriesByType("resource").map(x=>new URL(x.name).origin))]})');
  const pinPreserved=before.nodeId===after.nodeId&&before.nodePublicKey===after.nodePublicKey&&before.clientId===after.clientId&&before.publicKeySha256===after.publicKeySha256;
  const passed=mode==='pass'?audit.authSign===1&&audit.proof===1&&audit.authenticated===1&&page.authenticated: audit.challenge===1&&audit.authSign===0&&audit.proof===0&&audit.authenticated===0&&!page.authenticated;
  results.push({mode,audit,pinPreserved,privateExtractable:after.privateExtractable,page,passed});await fs.writeFile(root+'evidence/tls-browser.json',JSON.stringify(results,null,2));console.log({mode,audit,pinPreserved,passed});
  if(!passed||!pinPreserved||after.privateExtractable||page.resourceOrigins.some(x=>x!=='https://faraday.tail74842e.ts.net:30455'))throw Error('NA024-CLIENT-001 production page failed '+mode);
 }
 await c.send('Page.removeScriptToEvaluateOnNewDocument',{identifier:added.identifier},c.sessionId);
}finally{c.close();}
