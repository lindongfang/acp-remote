import {writeFile} from 'node:fs/promises';
const task=await taskSpace(3);const page=task.page('p1');
await page.waitForFunction(()=>document.body.innerText.includes('节点管理'),undefined,{timeout:15000});
await writeFile('D:/Project/acp-remote/.tmp/independent-acceptance-20261030/after-4401-approved.txt',await page.evaluate(()=>document.body.innerText));
console.log(await page.evaluate(()=>document.body.innerText));
await page.evaluate(()=>window.dispatchEvent(new PageTransitionEvent('pagehide',{persisted:true})));
