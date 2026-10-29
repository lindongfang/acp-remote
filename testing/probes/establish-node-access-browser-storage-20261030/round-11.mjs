const fs=await import('node:fs/promises');
const task=await taskSpace(8),page=task.page('p1');
async function read(page){return await page.evaluate(async()=>{
 const db=await new Promise(res=>{const r=indexedDB.open('acp-remote-access-v1',1);r.onsuccess=()=>res(r.result)});
 const row=await new Promise(res=>{const r=db.transaction('identity').objectStore('identity').get('primary');r.onsuccess=()=>res(r.result)});db.close();
 return {origin:row.origin,publicKey:row.publicKey,nodeId:row.nodeId,clientId:row.clientId,status:row.status,extractable:row.privateKey.extractable,body:document.body.innerText,localStorageKeys:Object.keys(localStorage),sessionStorageKeys:Object.keys(sessionStorage)};
})}
const before=await read(page);
await fs.writeFile('D:/Project/acp-remote/.tmp/browser-storage-acceptance-20261030/evidence/trusted-before-clear.json',JSON.stringify(before,null,2));
const second=await task.newPage();
await second.goto('https://faraday.tail74842e.ts.net:30454');
await second.waitForSelector('input[name="pairing-code"]',{state:'visible'});
const other=await read(second);
await fs.writeFile('D:/Project/acp-remote/.tmp/browser-storage-acceptance-20261030/evidence/new-origin.json',JSON.stringify({before,other,differentPublicKey:before.publicKey!==other.publicKey},null,2));
console.log({second:second.label,oldClientId:before.clientId,oldStatus:before.status,newClientId:other.clientId,newStatus:other.status,newOrigin:other.origin,differentPublicKey:before.publicKey!==other.publicKey,extractable:other.extractable});
console.log(await second.snapshot());
