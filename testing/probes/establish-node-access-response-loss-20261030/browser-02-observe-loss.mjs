import fs from 'node:fs/promises';
import {state} from './browser-state.mjs';
const root='D:/Project/acp-remote/.tmp/approval-response-loss-20261030/';
const t=await taskSpace(10),p=t.page('p1');
await p.waitForFunction(()=>window.__responseLoss?.dropped===1,undefined,{timeout:15000});
const result=await state(p);
if(result.status!=='pending_confirmation'||result.clientId!==null||result.loss.dropped!==1||!result.loss.observations[0].signatureVerified)throw Error('response loss precondition failed');
await fs.writeFile(root+'evidence/after-loss.json',JSON.stringify(result,null,2));
console.log(result);console.log(await p.snapshot());
