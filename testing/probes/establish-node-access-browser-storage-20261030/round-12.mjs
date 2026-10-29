const fs=await import('node:fs/promises');
const task=await taskSpace(8),page=task.page('p1');
const original=JSON.parse(await fs.readFile('D:/Project/acp-remote/.tmp/browser-storage-acceptance-20261030/evidence/trusted-before-clear.json','utf8'));
// Only this task's dedicated origin/database. Never clear profile cookies/cache.
await page.evaluate(async()=>{
 if(location.origin!=='https://faraday.tail74842e.ts.net:30453')throw Error('Wrong origin');
 await new Promise((res,rej)=>{const r=indexedDB.deleteDatabase('acp-remote-access-v1');r.onsuccess=res;r.onerror=()=>rej(r.error);r.onblocked=()=>rej(Error('blocked'))});
});
await page.reload();
await page.waitForSelector('input[name="pairing-code"]',{state:'visible'});
const result=await page.evaluate(async()=>{
 const db=await new Promise(res=>{const r=indexedDB.open('acp-remote-access-v1',1);r.onsuccess=()=>res(r.result)});
 const row=await new Promise(res=>{const r=db.transaction('identity').objectStore('identity').get('primary');r.onsuccess=()=>res(r.result)});db.close();
 return {origin:row.origin,publicKey:row.publicKey,status:row.status,clientId:row.clientId,nodeId:row.nodeId,body:document.body.innerText,localStorageKeys:Object.keys(localStorage),sessionStorageKeys:Object.keys(sessionStorage)};
});
const assertions={differentPublicKey:result.publicKey!==original.publicKey,unbound:result.status==='unbound'&&result.clientId===null,revocationGuidance:result.body.includes('撤销'),newPairing:result.body.includes('配对')};
await fs.writeFile('D:/Project/acp-remote/.tmp/browser-storage-acceptance-20261030/evidence/cleared-site.json',JSON.stringify({original,result,assertions},null,2));
console.log({assertions,result:{...result,publicKey:'[public-key retained in evidence]'}});
console.log(await page.snapshot());
