const fs=await import('node:fs/promises');
const task=await taskSpace(8),page=task.page('p1');
await page.cdp('Page.removeScriptToEvaluateOnNewDocument',{identifier:'1'});
console.log(await page.evaluate(async()=>{
 if(location.origin!=='https://faraday.tail74842e.ts.net:30453')throw Error('Wrong origin');
 await new Promise((res,rej)=>{const r=indexedDB.deleteDatabase('acp-remote-access-v1');r.onsuccess=res;r.onerror=()=>rej(r.error);r.onblocked=()=>rej(Error('blocked'))});return 'Only dedicated origin application database deleted';
}));
const hook=await page.cdp('Page.addScriptToEvaluateOnNewDocument',{source:`window.__na031={writes:0,claims:0,generated:0};const g=crypto.subtle.generateKey.bind(crypto.subtle);crypto.subtle.generateKey=function(...a){window.__na031.generated++;return g(...a)};const f=window.fetch.bind(window);window.fetch=function(...a){if(String(a[0]).endsWith('/api/access/v1/pair/claim'))window.__na031.claims++;return f(...a)};const p=IDBObjectStore.prototype.put;IDBObjectStore.prototype.put=function(...a){if(this.name==='identity'&&this.transaction.db.name==='acp-remote-access-v1'){window.__na031.writes++;throw new DOMException('NA031 isolated write adapter','QuotaExceededError')}return p.apply(this,a)};`});
await page.reload();
await page.waitForFunction(()=>document.body.innerText.includes('无法读取浏览器身份'),undefined,{timeout:10000});
const result=await page.evaluate(async()=>{
 const db=await new Promise(res=>{const r=indexedDB.open('acp-remote-access-v1',1);r.onsuccess=()=>res(r.result)});
 const row=await new Promise(res=>{const r=db.transaction('identity').objectStore('identity').get('primary');r.onsuccess=()=>res(r.result)});db.close();
 return {origin:location.origin,counters:window.__na031,recordExists:row!==undefined,body:document.body.innerText,localStorageKeys:Object.keys(localStorage),sessionStorageKeys:Object.keys(sessionStorage)};
});
await fs.writeFile('D:/Project/acp-remote/.tmp/browser-storage-acceptance-20261030/evidence/initial-write-failure.json',JSON.stringify({result,hook:hook.identifier},null,2));
console.log({result,hook:hook.identifier});
console.log(await page.snapshot());
