import {writeFile} from 'node:fs/promises';
import {readIdentity} from './r2-state.mjs';
const task=await taskSpace(4);const page=task.page('p1');
const before=await readIdentity(page);
const hook=`window.__life={pending:false,opened:0,held:0};let armed=true;const original=IDBRequest.prototype.addEventListener;IDBRequest.prototype.addEventListener=function(type,listener,options){if(type==='success'&&this.source instanceof IDBObjectStore&&this.source.name==='identity'&&armed){armed=false;return original.call(this,type,event=>{window.__life.pending=true;window.__life.held++;window.__release=()=>{window.__life.pending=false;listener.call(this,event)}},options)}return original.call(this,type,listener,options)};const Native=window.WebSocket;window.WebSocket=class extends Native{constructor(...args){super(...args);window.__life.opened++}};`;
const results=[];
for(const earlyRestore of [false,true]){
  const script=await page.cdp('Page.addScriptToEvaluateOnNewDocument',{source:hook});
  await page.reload();
  await page.waitForFunction(()=>window.__life?.pending===true,undefined,{timeout:10000});
  await page.evaluate(()=>window.dispatchEvent(new PageTransitionEvent('pagehide',{persisted:true})));
  const hiddenText=await page.evaluate(()=>document.querySelector('#access-panel').innerText);
  if(earlyRestore)await page.evaluate(()=>{window.dispatchEvent(new PageTransitionEvent('pageshow',{persisted:true}));window.dispatchEvent(new PageTransitionEvent('pageshow',{persisted:true}))});
  const whileHeld=await page.evaluate(()=>({...window.__life}));
  if(whileHeld.opened!==0||whileHeld.held!==1)throw Error('connection while initialize held');
  await page.evaluate(()=>window.__release());
  if(!earlyRestore){
    await new Promise(ok=>setTimeout(ok,500));
    const hidden=await page.evaluate(()=>({text:document.querySelector('#access-panel').innerText,opened:window.__life.opened}));
    if(hidden.opened!==0||hidden.text!==hiddenText)throw Error('late initialize changed inactive page');
    await page.evaluate(()=>window.dispatchEvent(new PageTransitionEvent('pageshow',{persisted:true})));
  }
  await page.waitForFunction(()=>document.body.innerText.includes('节点管理'),undefined,{timeout:15000});
  const after=await readIdentity(page);
  for(const key of ['clientId','publicKey','nodeId'])if(after[key]!==before[key])throw Error(`identity changed ${key}`);
  const events=await page.evaluate(()=>({...window.__life}));
  if(events.opened!==1)throw Error('duplicate management connections');
  results.push({earlyRestore,whileHeld,events,identity:after});
  await page.cdp('Page.removeScriptToEvaluateOnNewDocument',{identifier:script.identifier});
}
await writeFile('D:/Project/acp-remote/.tmp/independent-acceptance-20261030/r2-initialize-lifecycle.json',JSON.stringify(results,null,2));console.log(results);
