import {writeFile} from 'node:fs/promises';
import {readIdentity} from './r2-state.mjs';
const task=await taskSpace(4);const page=task.page('p1');
await page.waitForFunction(()=>document.body.innerText.includes('节点管理'),undefined,{timeout:15000});
const identity=await readIdentity(page);
await writeFile('D:/Project/acp-remote/.tmp/independent-acceptance-20261030/r2-current-approved.json',JSON.stringify(identity,null,2));console.log(identity);
