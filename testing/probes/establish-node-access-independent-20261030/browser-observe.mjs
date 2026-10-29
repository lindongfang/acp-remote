import { writeFile } from 'node:fs/promises';
const task = await taskSpace(3);
const page = task.page('p1');
await page.waitForFunction(() => document.body.innerText.includes('节点管理'), undefined, {timeout:20000});
const state = await page.evaluate(async () => {
  const database = await new Promise((resolve,reject) => {const r=indexedDB.open('acp-remote-access-v1',1);r.onsuccess=()=>resolve(r.result);r.onerror=()=>reject(r.error);});
  const record = await new Promise((resolve,reject) => {const r=database.transaction('identity').objectStore('identity').get('primary');r.onsuccess=()=>resolve(r.result);r.onerror=()=>reject(r.error);});
  database.close();
  return { secure:isSecureContext, origin:location.origin, body:document.body.innerText, identity:{clientId:record.clientId,publicKey:record.publicKey,nodeId:record.nodeId,status:record.status,extractable:record.privateKey.extractable}, localStorage:Object.keys(localStorage),sessionStorage:Object.keys(sessionStorage) };
});
await writeFile('D:/Project/acp-remote/.tmp/independent-acceptance-20261030/a-approved.json',JSON.stringify(state,null,2));
const hook = `window.__probe={opened:0,closed:[],sent:[]};const Native=window.WebSocket;window.WebSocket=class extends Native{constructor(...args){super(...args);window.__probe.opened++;this.addEventListener('close',e=>window.__probe.closed.push({code:e.code,reason:e.reason}));}send(data){const m=JSON.parse(data);window.__probe.sent.push({type:m.type,action:m.action,requestId:m.requestId});return super.send(data);}};`;
await page.cdp('Page.addScriptToEvaluateOnNewDocument',{source:hook});
await page.reload();
await page.waitForFunction(() => document.body.innerText.includes('节点管理'), undefined, {timeout:20000});
console.log({identity:state.identity,secure:state.secure});
console.log(await page.snapshot());
