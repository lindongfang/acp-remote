import {readFile,writeFile} from 'node:fs/promises';
const task=await taskSpace(3);const page=task.page('p1');
await page.waitForFunction(()=>document.body.innerText.includes('节点管理'),undefined,{timeout:40000});
const state=await page.evaluate(async()=>{const db=await new Promise((ok,no)=>{const r=indexedDB.open('acp-remote-access-v1');r.onsuccess=()=>ok(r.result);r.onerror=()=>no(r.error)});const r=await new Promise(ok=>{const q=db.transaction('identity').objectStore('identity').get('primary');q.onsuccess=()=>ok(q.result)});db.close();return {clientId:r.clientId,publicKey:r.publicKey,nodeId:r.nodeId,extractable:r.privateKey.extractable,events:window.__probe,body:document.body.innerText}});
const old=JSON.parse(await readFile('D:/Project/acp-remote/.tmp/independent-acceptance-20261030/a-approved.json','utf8'));
for(const key of ['clientId','publicKey','nodeId'])if(state[key]!==old.identity[key])throw Error(`identity changed: ${key}`);
if(state.extractable!==false||state.events.opened<2)throw Error('reconnect evidence missing');
await writeFile('D:/Project/acp-remote/.tmp/independent-acceptance-20261030/recovered.json',JSON.stringify(state,null,2));
console.log({clientId:state.clientId,nodeId:state.nodeId,opened:state.events.opened,closed:state.events.closed});
