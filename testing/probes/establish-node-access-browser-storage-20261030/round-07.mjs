const fs=await import('node:fs/promises');
const task=await taskSpace(8),page=task.page('p1');
await page.reload();await page.waitForSelector('input[name="pairing-code"]',{state:'visible'});
console.log(await page.snapshot());
await page.evaluate(()=>{
 window.__na031={writes:0,lookups:0,claims:0};
 const f=window.fetch.bind(window);window.fetch=function(...a){const url=String(a[0]);if(url.endsWith('/pair/lookup'))window.__na031.lookups++;if(url.endsWith('/pair/claim'))window.__na031.claims++;return f(...a)};
 const p=IDBObjectStore.prototype.put;window.__restorePut=()=>{IDBObjectStore.prototype.put=p};IDBObjectStore.prototype.put=function(...a){if(this.name==='identity'&&this.transaction.db.name==='acp-remote-access-v1'){window.__na031.writes++;throw new DOMException('NA031 pre-claim write adapter','QuotaExceededError')}return p.apply(this,a)};
});
await page.fill('input[name="pairing-code"]','Z3B6-FM0Y');
await page.fill('input[name="display-name"]','NA031 storage isolated');
await page.click('text=认领这台浏览器');
await page.waitForFunction(()=>document.body.innerText.includes('浏览器身份无法写入 IndexedDB'),undefined,{timeout:10000});
const result=await page.evaluate(async()=>{
 const db=await new Promise(res=>{const r=indexedDB.open('acp-remote-access-v1',1);r.onsuccess=()=>res(r.result)});
 const row=await new Promise(res=>{const r=db.transaction('identity').objectStore('identity').get('primary');r.onsuccess=()=>res(r.result)});db.close();
 window.__restorePut();return {body:document.body.innerText,counters:window.__na031,status:row.status,clientId:row.clientId,pendingClaim:row.pendingClaim===null?'null':'present',localStorageKeys:Object.keys(localStorage)};
});
await fs.writeFile('D:/Project/acp-remote/.tmp/browser-storage-acceptance-20261030/evidence/preclaim-write-failure.json',JSON.stringify(result,null,2));
console.log(result);console.log(await page.snapshot());
