const fs=await import('node:fs/promises');
const task=await taskSpace(8),page=task.page('p1');
const before=await page.evaluate(async()=>{
 const db=await new Promise((res,rej)=>{const r=indexedDB.open('acp-remote-access-v1',1);r.onsuccess=()=>res(r.result);r.onerror=()=>rej(r.error)});
 const row=await new Promise((res,rej)=>{const r=db.transaction('identity').objectStore('identity').get('primary');r.onsuccess=()=>res(r.result);r.onerror=()=>rej(r.error)});
 const summary={origin:row.origin,publicKey:row.publicKey,status:row.status,clientId:row.clientId,type:row.privateKey.type,algorithm:row.privateKey.algorithm.name,extractable:row.privateKey.extractable,localStorageKeys:Object.keys(localStorage),sessionStorageKeys:Object.keys(sessionStorage)};
 const badKey=await crypto.subtle.generateKey({name:'HMAC',hash:'SHA-256'},false,['sign']);
 await new Promise((res,rej)=>{const tx=db.transaction('identity','readwrite');tx.objectStore('identity').put({...row,privateKey:badKey});tx.oncomplete=res;tx.onabort=()=>rej(tx.error)});db.close();return summary;
});
const hook=await page.cdp('Page.addScriptToEvaluateOnNewDocument',{source:`window.__na031={generated:0,writes:0};const g=crypto.subtle.generateKey.bind(crypto.subtle);crypto.subtle.generateKey=function(...args){window.__na031.generated++;return g(...args)};const p=IDBObjectStore.prototype.put;IDBObjectStore.prototype.put=function(...args){if(this.name==='identity'&&this.transaction.db.name==='acp-remote-access-v1')window.__na031.writes++;return p.apply(this,args)};`});
await page.reload();
await page.waitForFunction(()=>document.body.innerText.includes('本页没有生成替代密钥'),undefined,{timeout:10000});
const after=await page.evaluate(async()=>{
 const db=await new Promise(res=>{const r=indexedDB.open('acp-remote-access-v1',1);r.onsuccess=()=>res(r.result)});
 const row=await new Promise(res=>{const r=db.transaction('identity').objectStore('identity').get('primary');r.onsuccess=()=>res(r.result)});db.close();
 return {body:document.body.innerText,counters:window.__na031,publicKey:row.publicKey,status:row.status,storedAlgorithm:row.privateKey.algorithm.name};
});
await fs.writeFile('D:/Project/acp-remote/.tmp/browser-storage-acceptance-20261030/evidence/bad-key.json',JSON.stringify({before,after,hook:hook.identifier},null,2));
console.log({before:{...before,publicKey:'[public-key retained in evidence]'},after:{...after,publicKey:'[public-key retained in evidence]'},unchangedPublicKey:before.publicKey===after.publicKey,hook:hook.identifier});
console.log(await page.snapshot());
