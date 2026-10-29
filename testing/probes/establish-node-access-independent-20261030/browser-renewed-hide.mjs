import {writeFile} from 'node:fs/promises';
const task=await taskSpace(3);const page=task.page('p1');
await page.waitForFunction(()=>document.body.innerText.includes('节点管理'),undefined,{timeout:15000});
const state=await page.evaluate(async()=>{const db=await new Promise(ok=>{const q=indexedDB.open('acp-remote-access-v1');q.onsuccess=()=>ok(q.result)});const r=await new Promise(ok=>{const q=db.transaction('identity').objectStore('identity').get('primary');q.onsuccess=()=>ok(q.result)});db.close();return{clientId:r.clientId,publicKey:r.publicKey,status:r.status,extractable:r.privateKey.extractable,events:window.__probe,body:document.body.innerText}});
if(state.clientId!=='ttwEClM52QxT0isxLD-G8w')throw Error('renewed ID incorrect');
await writeFile('D:/Project/acp-remote/.tmp/independent-acceptance-20261030/renewed-4410-approved.json',JSON.stringify(state,null,2));
await page.evaluate(()=>window.dispatchEvent(new PageTransitionEvent('pagehide',{persisted:true})));
console.log({clientId:state.clientId,extractable:state.extractable,action:'pagehide cancels management before offline revocation'});
