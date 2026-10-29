import {connectPrivate} from './private-cdp.mjs';
const fs=await import('node:fs/promises'),c=await connectPrivate();
try{
 await c.send('Page.enable',{},c.sessionId);
 const before=await c.evaluate(`(async()=>{const db=await new Promise(res=>{const r=indexedDB.open('acp-remote-access-v1',1);r.onsuccess=()=>res(r.result)});const row=await new Promise(res=>{const r=db.transaction('identity').objectStore('identity').get('primary');r.onsuccess=()=>res(r.result)});db.close();return {trustedIdentity:row.status==='consumed'&&row.clientId!==null,privateKeyNonExtractable:row.privateKey.extractable===false,authenticated:document.body.innerText.includes('NODE CONTROL / AUTHENTICATED')}})()`);
 if(!before.trustedIdentity||!before.authenticated)throw Error('Need real trusted identity before clearing');
 await c.evaluate(`new Promise((res,rej)=>{if(location.origin!=='https://faraday.tail74842e.ts.net:30453')throw Error('Wrong origin');const r=indexedDB.deleteDatabase('acp-remote-access-v1');r.onsuccess=()=>res(true);r.onerror=()=>rej(r.error);r.onblocked=()=>rej(Error('blocked'))})`);
 await c.send('Page.reload',{},c.sessionId);
 let result;
 for(let i=0;i<50;i++){
  result=await c.evaluate(`({pairForm:!!document.querySelector('input[name="pairing-code"]'),guidance:document.body?.innerText.includes('若清除了浏览器数据或丢失身份，请先在节点终端撤销旧设备，再重新配对。')??false,hasDetectionPromise:document.body?.innerText.includes('已检测到')??false,buttonCount:document.querySelectorAll('form button').length,secure:isSecureContext})`);
  if(result.pairForm)break;await new Promise(r=>setTimeout(r,100));
 }
 const fresh=await c.evaluate(`(async()=>{const db=await new Promise(res=>{const r=indexedDB.open('acp-remote-access-v1',1);r.onsuccess=()=>res(r.result)});const row=await new Promise(res=>{const r=db.transaction('identity').objectStore('identity').get('primary');r.onsuccess=()=>res(r.result)});db.close();return {unbound:row.status==='unbound'&&row.clientId===null&&row.nodeId===null,privateKeyNonExtractable:row.privateKey.extractable===false}})()`);
 const evidence={before,after:{...result,...fresh},privacy:'Only booleans and button count; no private identity values exported'};
 await fs.writeFile('D:/Project/acp-remote/.tmp/browser-storage-acceptance-20261030/evidence/r6-cleared-trusted.json',JSON.stringify(evidence,null,2));
 console.log(evidence);
 await c.send('Browser.close');
}finally{c.close()}
