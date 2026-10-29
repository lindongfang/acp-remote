import {writeFile} from 'node:fs/promises';
import {readIdentity} from './r2-state.mjs';
const task=await taskSpace(4);const page=task.page('p1');
await page.reload();await page.waitForFunction(()=>document.body.innerText.includes('节点管理'),undefined,{timeout:15000});
const before=await readIdentity(page);const results=[];
for(const code of [4429,4401]){
  const source=`window.__clock={now:Date.now(),opened:0,timers:new Map(),next:-1};Date.now=()=>window.__clock.now;const st=window.setTimeout,ct=window.clearTimeout;window.setTimeout=(fn,delay,...args)=>{if(delay>=1000){const id=window.__clock.next--;window.__clock.timers.set(id,{at:window.__clock.now+delay,fn:()=>fn(...args)});return id}return st(fn,delay,...args)};window.clearTimeout=id=>{if(!window.__clock.timers.delete(id))ct(id)};window.__tick=ms=>{window.__clock.now+=ms;for(const [id,t]of [...window.__clock.timers])if(t.at<=window.__clock.now){window.__clock.timers.delete(id);t.fn()}};window.WebSocket=class extends EventTarget{readyState=0;constructor(){super();window.__clock.opened++;queueMicrotask(()=>{this.readyState=1;this.dispatchEvent(new Event('open'))})}send(){queueMicrotask(()=>{this.readyState=3;this.dispatchEvent(new CloseEvent('close',{code:${code},reason:'controlled close',wasClean:true}))})}close(){this.readyState=3}};`;
  const script=await page.cdp('Page.addScriptToEvaluateOnNewDocument',{source});
  await page.reload();
  await page.waitForFunction(()=>document.body.innerText.includes('正在恢复连接')||document.body.innerText.includes('管理连接已中断'),undefined,{timeout:12000});
  await page.evaluate(()=>{window.__tick(10000);window.dispatchEvent(new PageTransitionEvent('pagehide',{persisted:true}));window.dispatchEvent(new PageTransitionEvent('pageshow',{persisted:true}));window.dispatchEvent(new PageTransitionEvent('pageshow',{persisted:true}));window.dispatchEvent(new Event('online'))});
  await page.waitForFunction(()=>!document.body.innerText.includes('正在检查浏览器身份'),undefined,{timeout:12000});
  const restored=await page.evaluate(()=>({opened:window.__clock.opened,pending:[...window.__clock.timers.values()].map(t=>t.at-window.__clock.now),body:document.body.innerText}));
  if(restored.opened!==1)throw Error('restore reopened too early');
  if(code===4429){
    if(!restored.pending.includes(50000))throw Error('rate deadline lost');
    await page.evaluate(()=>window.__tick(49999));
    if(await page.evaluate(()=>window.__clock.opened)!==1)throw Error('early retry at 59999');
    await page.evaluate(()=>window.__tick(1));
    await page.waitForFunction(()=>window.__clock.opened===2,undefined,{timeout:3000});
  }else{
    await page.evaluate(()=>{window.__tick(120000);window.dispatchEvent(new Event('online'));window.dispatchEvent(new PageTransitionEvent('pagehide',{persisted:true}));window.dispatchEvent(new PageTransitionEvent('pageshow',{persisted:true}))});
    await page.waitForFunction(()=>document.body.innerText.includes('管理连接已停止'),undefined,{timeout:12000});
    if(await page.evaluate(()=>window.__clock.opened)!==1)throw Error('terminal stop resurrected');
  }
  const identity=await readIdentity(page);for(const key of ['clientId','publicKey','nodeId'])if(identity[key]!==before[key])throw Error('identity changed');
  results.push({code,restored,openedAfter:await page.evaluate(()=>window.__clock.opened),identityPreserved:true});
  await page.cdp('Page.removeScriptToEvaluateOnNewDocument',{identifier:script.identifier});
}
await page.reload();await page.waitForFunction(()=>document.body.innerText.includes('节点管理'),undefined,{timeout:15000});
await writeFile('D:/Project/acp-remote/.tmp/independent-acceptance-20261030/r3-restore-scheduler.json',JSON.stringify(results,null,2));console.log(results);
