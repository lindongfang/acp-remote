const fs=await import('node:fs/promises');
const task=await taskSpace(8),page=task.page('p1');
await page.waitForFunction(()=>document.body.innerText.includes('客户端列表'),undefined,{timeout:15000});
const summary=await page.evaluate(async()=>{
 const db=await new Promise(res=>{const r=indexedDB.open('acp-remote-access-v1',1);r.onsuccess=()=>res(r.result)});
 const row=await new Promise(res=>{const r=db.transaction('identity').objectStore('identity').get('primary');r.onsuccess=()=>res(r.result)});db.close();
 return {origin:row.origin,publicKey:row.publicKey,nodeId:row.nodeId,clientId:row.clientId,status:row.status,extractable:row.privateKey.extractable,body:document.body.innerText};
});
await fs.writeFile('D:/Project/acp-remote/.tmp/browser-storage-acceptance-20261030/evidence/trusted-before-clear.json',JSON.stringify(summary,null,2));
console.log(summary);console.log(await page.snapshot());
