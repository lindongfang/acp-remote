import {writeFile} from 'node:fs/promises';
import {readIdentity} from './r2-state.mjs';
const task=await taskSpace(4);const page=task.page('p1');
const before=await readIdentity(page);const results=[];
for(const code of [4401,4410]){
  const source=`window.__fault={opened:0,sent:[]};window.WebSocket=class extends EventTarget{readyState=0;constructor(){super();window.__fault.opened++;queueMicrotask(()=>{this.readyState=1;this.dispatchEvent(new Event('open'))})}send(data){window.__fault.sent.push(JSON.parse(data).type);queueMicrotask(()=>{this.readyState=3;this.dispatchEvent(new CloseEvent('close',{code:${code},reason:'unverified peer',wasClean:true}))})}close(){this.readyState=3}};`;
  const script=await page.cdp('Page.addScriptToEvaluateOnNewDocument',{source});
  await page.reload();
  await page.waitForFunction(()=>document.body.innerText.includes('管理连接已中断'),undefined,{timeout:15000});
  const after=await readIdentity(page);for(const key of ['clientId','publicKey','nodeId'])if(before[key]!==after[key])throw Error(`unverified close clears ${key}`);
  if(await page.evaluate(()=>!!document.querySelector('input[name="pairing-code"]')))throw Error('unverified close enters pairing');
  await page.evaluate(()=>window.dispatchEvent(new Event('online')));
  await new Promise(ok=>setTimeout(ok,2500));
  const fault=await page.evaluate(()=>window.__fault);if(fault.opened!==1||fault.sent.join(',')!=='hello')throw Error('unverified close retried/proved');
  results.push({code,identityPreserved:true,fault});
  await page.cdp('Page.removeScriptToEvaluateOnNewDocument',{identifier:script.identifier});
}
await page.reload();await page.waitForFunction(()=>document.body.innerText.includes('节点管理'),undefined,{timeout:15000});
await writeFile('D:/Project/acp-remote/.tmp/independent-acceptance-20261030/r2-unverified-close.json',JSON.stringify(results,null,2));console.log(results);
