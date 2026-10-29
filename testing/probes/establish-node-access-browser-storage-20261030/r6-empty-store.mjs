import {connectPrivate} from './private-cdp.mjs';
const fs=await import('node:fs/promises'),c=await connectPrivate();
try{
 await c.send('Page.enable',{},c.sessionId);
 const before=await c.evaluate(`(async()=>({secure:isSecureContext,appDatabaseAbsent:!(await indexedDB.databases()).some(x=>x.name==='acp-remote-access-v1')}))()`);
 if(!before.appDatabaseAbsent)throw Error('Expected fresh private database');
 await c.send('Page.navigate',{url:'https://faraday.tail74842e.ts.net:30453/'},c.sessionId);
 let result;
 for(let i=0;i<50;i++){
  result=await c.evaluate(`({secure:isSecureContext,pairForm:!!document.querySelector('input[name="pairing-code"]'),guidance:document.body?.innerText.includes('若清除了浏览器数据或丢失身份，请先在节点终端撤销旧设备，再重新配对。')??false,buttonCount:document.querySelectorAll('form button').length,hasDetectionPromise:document.body?.innerText.includes('已检测到')??false})`);
  if(result.pairForm)break;await new Promise(r=>setTimeout(r,100));
 }
 console.log({emptyStoreBeforeBoot:before,rendered:result});
 await fs.writeFile('D:/Project/acp-remote/.tmp/browser-storage-acceptance-20261030/evidence/r6-empty-store.json',JSON.stringify({before,result,privacy:'Only booleans/counters; no private identity values'},null,2));
}finally{c.close()}
