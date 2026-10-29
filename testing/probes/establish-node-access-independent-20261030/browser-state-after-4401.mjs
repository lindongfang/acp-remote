import {readFile,writeFile} from 'node:fs/promises';
const task=await taskSpace(3);const page=task.page('p1');
await page.waitForFunction(()=>!!document.querySelector('input[name="pairing-code"]'),undefined,{timeout:10000});
const state=await page.evaluate(async()=>{const db=await new Promise(ok=>{const q=indexedDB.open('acp-remote-access-v1');q.onsuccess=()=>ok(q.result)});const r=await new Promise(ok=>{const q=db.transaction('identity').objectStore('identity').get('primary');q.onsuccess=()=>ok(q.result)});db.close();return {clientId:r.clientId,publicKey:r.publicKey,status:r.status,body:document.body.innerText}});
const old=JSON.parse(await readFile('D:/Project/acp-remote/.tmp/independent-acceptance-20261030/renewed-4410-approved.json','utf8'));
if(state.clientId!==null||state.publicKey===old.publicKey)throw Error('identity not replaced');
await writeFile('D:/Project/acp-remote/.tmp/independent-acceptance-20261030/after-4401-pair-form.json',JSON.stringify(state,null,2));console.log(state);
