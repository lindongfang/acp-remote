import {connect} from './cdp.mjs';
const fs=await import('node:fs/promises');const [action,name='A',label=action]=process.argv.slice(2);
const root=process.env.ACP_E2E_ROOT??'D:/Project/acp-remote/.tmp/r6-e2e-acceptance-20261030';const c=await connect(name);
try{
 let target=c.targets[0];
 if(action==='tab'){const t=await c.send('Target.createTarget',{url:'https://faraday.tail74842e.ts.net:30453/na032-inspection'});target={targetId:t.targetId}}
 const p=await c.attach(target.targetId);
 if(action==='boot'||action==='tab'){
  await c.send('Page.addScriptToEvaluateOnNewDocument',{source:await fs.readFile(new URL('./hook.js',import.meta.url),'utf8')},p.sessionId);
  await c.send('Page.navigate',{url:'https://faraday.tail74842e.ts.net:30453/'},p.sessionId);
  for(let i=0;i<50;i++){if(await p.evaluate(`!!document.querySelector('form')||document.body.innerText.includes('NODE CONTROL / AUTHENTICATED')`))break;await new Promise(r=>setTimeout(r,100))}
 }
 if(action==='claim'){
  const {code}=JSON.parse(await fs.readFile(`${root}/pair-input.json`,'utf8'));
  const displayName=process.env.ACP_DISPLAY_NAME??`NA032 ${name} isolated`;
  await p.evaluate(`(()=>{document.querySelector('input[name="pairing-code"]').value=${JSON.stringify(code)};document.querySelector('input[name="display-name"]').value=${JSON.stringify(displayName)};document.querySelector('form').requestSubmit();return true})()`);
 }
 if(action==='wait'){
  let authenticated=false;for(let i=0;i<100;i++){authenticated=await p.evaluate(`document.body.innerText.includes('NODE CONTROL / AUTHENTICATED')`);if(authenticated)break;await new Promise(r=>setTimeout(r,100))}if(!authenticated)throw Error('Expected automatic authentication');
 }
 if(action==='preserve')await p.evaluate(`(async()=>{const db=await new Promise(res=>{const r=indexedDB.open('acp-remote-access-v1',1);r.onsuccess=()=>res(r.result)});window.__saved=await new Promise(res=>{const r=db.transaction('identity').objectStore('identity').get('primary');r.onsuccess=()=>res(r.result)});db.close();return !!window.__saved.privateKey})()`);
 const summary=await p.evaluate(`(async()=>{let identity=null;const db=await new Promise(res=>{const r=indexedDB.open('acp-remote-access-v1',1);r.onsuccess=()=>res(r.result)});if(db.objectStoreNames.contains('identity')){const row=await new Promise(res=>{const r=db.transaction('identity').objectStore('identity').get('primary');r.onsuccess=()=>res(r.result)});if(row)identity={publicKey:row.publicKey,clientId:row.clientId,nodeId:row.nodeId,status:row.status,extractable:row.privateKey.extractable}}db.close();return {secure:isSecureContext,url:location.href,title:document.title,identity,body:document.body.innerText,events:window.__e2e?.events??[]}})()`);
 await fs.writeFile(`${root}/evidence/${name}-${label}.json`,JSON.stringify(summary,null,2));
 console.log({action,name,targetId:target.targetId,secure:summary.secure,identity:summary.identity,body:summary.body,events:summary.events.slice(-8)});
}finally{c.close()}
