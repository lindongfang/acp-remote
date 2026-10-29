import {connectPrivate} from './private-cdp.mjs';
const fs=await import('node:fs/promises');const c=await connectPrivate();
try{
 const healthy=await c.evaluate(`(async()=>{
 const db=await new Promise((res,rej)=>{const r=indexedDB.open('acp-remote-access-v1',1);r.onsuccess=()=>res(r.result);r.onerror=()=>rej(r.error)});
 const row=await new Promise((res,rej)=>{const r=db.transaction('identity').objectStore('identity').get('primary');r.onsuccess=()=>res(r.result);r.onerror=()=>rej(r.error)});db.close();
 const bytes=Uint8Array.from(atob(row.publicKey.replace(/-/g,'+').replace(/_/g,'/')),x=>x.charCodeAt(0));
 const publicKey=await crypto.subtle.importKey('raw',bytes,{name:'ECDSA',namedCurve:'P-256'},false,['verify']);
 const challenge=crypto.getRandomValues(new Uint8Array(32));
 const sig=await crypto.subtle.sign({name:'ECDSA',hash:'SHA-256'},row.privateKey,challenge);
 return {secure:isSecureContext,privateKeyNonExtractable:row.privateKey.extractable===false,privateKeyECDSA:row.privateKey.algorithm.name==='ECDSA',readBackSignVerify:await crypto.subtle.verify({name:'ECDSA',hash:'SHA-256'},publicKey,sig,challenge),unbound:row.status==='unbound'&&row.clientId===null,pairForm:!!document.querySelector('input[name="pairing-code"]'),noLocalStorage:Object.keys(localStorage).length===0};
 })()`);
 console.log({privateHealthy:healthy});
 await c.evaluate(`new Promise((res,rej)=>{const r=indexedDB.deleteDatabase('acp-remote-access-v1');r.onsuccess=()=>res(true);r.onerror=()=>rej(r.error);r.onblocked=()=>rej(Error('blocked'))})`);
 const hook=await c.send('Page.addScriptToEvaluateOnNewDocument',{source:`window.__privateTest={writes:0,claims:0};const p=IDBObjectStore.prototype.put;IDBObjectStore.prototype.put=function(...a){if(this.name==='identity'&&this.transaction.db.name==='acp-remote-access-v1'){window.__privateTest.writes++;throw new DOMException('NA031 private isolated adapter','QuotaExceededError')}return p.apply(this,a)};const f=fetch.bind(window);window.fetch=function(...a){if(String(a[0]).endsWith('/pair/claim'))window.__privateTest.claims++;return f(...a)};`},c.sessionId);
 await c.send('Page.reload',{},c.sessionId);
 let failure;
 for(let i=0;i<30;i++){
  failure=await c.evaluate(`({writes:window.__privateTest?.writes??0,claims:window.__privateTest?.claims??0,writeFailureShown:document.body.innerText.includes('浏览器身份无法写入 IndexedDB，认领未继续。'),noClaimForm:!document.querySelector('input[name="pairing-code"]')})`);
  if(failure.writeFailureShown)break;await new Promise(r=>setTimeout(r,100));
 }
 await c.send('Page.removeScriptToEvaluateOnNewDocument',{identifier:hook.identifier},c.sessionId);
 await c.send('Page.reload',{},c.sessionId);
 for(let i=0;i<30;i++){if(await c.evaluate(`!!document.querySelector('input[name="pairing-code"]')`))break;await new Promise(r=>setTimeout(r,100))}
 const restored=await c.evaluate(`!!document.querySelector('input[name="pairing-code"]')`);
 const result={healthy,failure,restoredPairForm:restored,privacy:'No identity/key/record values exported; boolean assertions only'};
 await fs.writeFile('D:/Project/acp-remote/.tmp/browser-storage-acceptance-20261030/evidence/inprivate-assertions.json',JSON.stringify(result,null,2));
 console.log({failure,restoredPairForm:restored});
 await c.send('Browser.close');
}finally{c.close()}
