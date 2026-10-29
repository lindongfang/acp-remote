import {writeFile} from 'node:fs/promises';
const task=await taskSpace(3);const page=task.page('p1');
await page.waitForFunction(()=>document.body.innerText.includes('节点管理'),undefined,{timeout:15000});
const state=await page.evaluate(async()=>{const db=await new Promise(ok=>{const q=indexedDB.open('acp-remote-access-v1');q.onsuccess=()=>ok(q.result)});const r=await new Promise(ok=>{const q=db.transaction('identity').objectStore('identity').get('primary');q.onsuccess=()=>ok(q.result)});db.close();return {secure:isSecureContext,origin:location.origin,clientId:r.clientId,publicKey:r.publicKey,nodeId:r.nodeId,status:r.status,privateKeyExtractable:r.privateKey.extractable,localStorageKeys:Object.keys(localStorage),sessionStorageKeys:Object.keys(sessionStorage),body:document.body.innerText}});
if(state.clientId!=='pfNbzGH2j1MGIlMdkmBA8Q'||state.privateKeyExtractable!==false)throw Error('final identity authentication missing');
await writeFile('D:/Project/acp-remote/.tmp/independent-acceptance-20261030/final-approved.json',JSON.stringify(state,null,2));
console.log({clientId:state.clientId,nodeId:state.nodeId,secure:state.secure,extractable:state.privateKeyExtractable});
