import {connect} from './cdp.mjs';const fs=await import('node:fs/promises'),root='D:/Project/acp-remote/.tmp/r6-e2e-acceptance-20261030/r18',c=await connect('B'),action=process.argv[2];
try{
 if(action==='boot'){
  const p=await c.attach(c.targets[0].targetId);
  const source=await fs.readFile(new URL('./hook.js',import.meta.url),'utf8')+'\n'+await fs.readFile(new URL('./late-close-hook.js',import.meta.url),'utf8');
  await c.send('Page.addScriptToEvaluateOnNewDocument',{source},p.sessionId);await c.send('Page.navigate',{url:'https://faraday.tail74842e.ts.net:30453/'},p.sessionId);
  for(let i=0;i<60;i++){if(await p.evaluate(`document.body.innerText.includes('NODE CONTROL / AUTHENTICATED')`))break;await new Promise(r=>setTimeout(r,100))}
  const before=await p.evaluate(`({authenticated:document.body.innerText.includes('NODE CONTROL / AUTHENTICATED'),holdInstalled:window.__lateHold,events:window.__e2e.events})`);
  if(!before.authenticated)throw Error('Need genuine product authentication before delay');
  await fs.writeFile(`${root}/evidence/late-original-authenticated.json`,JSON.stringify({targetId:c.targets[0].targetId,...before},null,2));console.log({action,authenticated:before.authenticated,holdInstalled:before.holdInstalled});
 }else{
  let old,other;for(const t of c.targets){const p=await c.attach(t.targetId);if(await p.evaluate(`typeof window.__releaseRealClose==='function'`))old={...p,targetId:t.targetId};else other={...p,targetId:t.targetId}}
  if(!old||!other)throw Error('Need delayed original and ordinary replacement page');
  if(action==='held'){
   for(let i=0;i<50;i++){if(await old.evaluate(`window.__lateQueue.length>0`))break;await new Promise(r=>setTimeout(r,100))}
   const result={old:await old.evaluate(`({stillAuthenticatedUI:document.body.innerText.includes('NODE CONTROL / AUTHENTICATED'),queued:window.__lateQueue.length,nativeCloses:window.__lateFacts,events:window.__e2e.events})`),other:await other.evaluate(`({pairForm:!!document.querySelector('form'),fatal:document.body.innerText.includes('无法准备重新配对')})`)};
   await fs.writeFile(`${root}/evidence/late-held.json`,JSON.stringify(result,null,2));console.log(result);if(!result.old.stillAuthenticatedUI||!result.old.queued)throw Error('Delay adapter did not isolate real close callback');
  }else if(action==='claim'){
   const {code}=JSON.parse(await fs.readFile(`${root}/pair-input.json`,'utf8'));
   console.log(await other.evaluate(`(()=>{const code=document.querySelector('input[name="pairing-code"]'),name=document.querySelector('input[name="display-name"]');if(!code||!name)throw Error('No replacement form');code.value=${JSON.stringify(code)};name.value='R18 late-close replacement';document.querySelector('form').requestSubmit();return {submitted:true}})()`));
  }else if(action==='release'){
   for(let i=0;i<60;i++){if(await other.evaluate(`document.body.innerText.includes('NODE CONTROL / AUTHENTICATED')`))break;await new Promise(r=>setTimeout(r,100))}
   const replacement=await other.evaluate(`(async()=>{const db=await new Promise(res=>{const r=indexedDB.open('acp-remote-access-v1',1);r.onsuccess=()=>res(r.result)});const row=await new Promise(res=>{const r=db.transaction('identity').objectStore('identity').get('primary');r.onsuccess=()=>res(r.result)});db.close();return {clientId:row.clientId,publicKey:row.publicKey,status:row.status,authenticated:document.body.innerText.includes('NODE CONTROL / AUTHENTICATED')}})()`);
   if(!replacement.authenticated||replacement.status!=='consumed'||!replacement.clientId)throw Error('Need genuinely consumed replacement before old callback');
   const released=await old.evaluate(`window.__releaseRealClose()`);if(released!==1)throw Error('Expected one real original close');
   for(let i=0;i<100;i++){if(await old.evaluate(`document.body.innerText.includes('NODE CONTROL / AUTHENTICATED')&&document.body.innerText.includes(${JSON.stringify(replacement.clientId)})`))break;await new Promise(r=>setTimeout(r,100))}
   const result={replacementBeforeRelease:replacement,released,old:await old.evaluate(`({authenticated:document.body.innerText.includes('NODE CONTROL / AUTHENTICATED'),pairForm:!!document.querySelector('form'),fatal:document.body.innerText.includes('STOPPED'),body:document.body.innerText,events:window.__e2e.events,nativeCloses:window.__lateFacts})`),other:await other.evaluate(`({authenticated:document.body.innerText.includes('NODE CONTROL / AUTHENTICATED'),pairForm:!!document.querySelector('form')})`)};
   await fs.writeFile(`${root}/evidence/late-released.json`,JSON.stringify(result,null,2));console.log({released,oldAuthenticated:result.old.authenticated,oldPairForm:result.old.pairForm,oldShowsReplacementId:result.old.body.includes(replacement.clientId),other:result.other});
  }
 }
}finally{c.close()}
