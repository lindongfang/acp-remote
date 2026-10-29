import {connectPrivate} from './private-cdp.mjs';
const fs=await import('node:fs/promises'),c=await connectPrivate();
try{
 await c.send('Page.enable',{},c.sessionId);
 const afterExit=await c.evaluate(`(async()=>({inspectionPageDoesNotBootApp:!document.querySelector('input[name="pairing-code"]'),applicationDbAbsent:!(await indexedDB.databases()).some(x=>x.name==='acp-remote-access-v1'),secure:isSecureContext}))()`);
 console.log({afterCompletePrivateExit:afterExit});
 const hook=await c.send('Page.addScriptToEvaluateOnNewDocument',{source:`window.__privateTest={writes:0,claims:0};const p=IDBObjectStore.prototype.put;IDBObjectStore.prototype.put=function(...a){if(this.name==='identity'&&this.transaction.db.name==='acp-remote-access-v1'){window.__privateTest.writes++;throw new DOMException('NA031 private isolated adapter','QuotaExceededError')}return p.apply(this,a)};const f=fetch.bind(window);window.fetch=function(...a){if(String(a[0]).endsWith('/pair/claim'))window.__privateTest.claims++;return f(...a)};`},c.sessionId);
 await c.send('Page.navigate',{url:'https://faraday.tail74842e.ts.net:30453/'},c.sessionId);
 let failure;
 for(let i=0;i<50;i++){
  failure=await c.evaluate(`({hookInstalled:!!window.__privateTest,writes:window.__privateTest?.writes??0,claims:window.__privateTest?.claims??0,writeFailureShown:document.body?.innerText.includes('浏览器身份无法写入 IndexedDB，认领未继续。')??false,noClaimForm:!document.querySelector('input[name="pairing-code"]')})`);
  if(failure.writeFailureShown)break;await new Promise(r=>setTimeout(r,100));
 }
 const noRecord=await c.evaluate(`(async()=>{const db=await new Promise(res=>{const r=indexedDB.open('acp-remote-access-v1',1);r.onsuccess=()=>res(r.result)});const absent=await new Promise(res=>{const r=db.transaction('identity').objectStore('identity').get('primary');r.onsuccess=()=>res(r.result===undefined)});db.close();return absent})()`);
 const result={afterCompletePrivateExit:afterExit,writeFailure:failure,noIdentityAfterWriteFailure:noRecord,privacy:'Only boolean assertions/counters; no private identity values exported'};
 console.log(result);
 await fs.writeFile('D:/Project/acp-remote/.tmp/browser-storage-acceptance-20261030/evidence/inprivate-retest.json',JSON.stringify(result,null,2));
 await c.send('Page.removeScriptToEvaluateOnNewDocument',{identifier:hook.identifier},c.sessionId);
 await c.send('Browser.close');
}finally{c.close()}
