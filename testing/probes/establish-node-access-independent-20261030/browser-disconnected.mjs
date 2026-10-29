import {writeFile} from 'node:fs/promises';
const task=await taskSpace(3);const page=task.page('p1');
await page.waitForFunction(()=>document.body.innerText.includes('正在恢复连接'),undefined,{timeout:10000});
const state=await page.evaluate(()=>({body:document.body.innerText,events:window.__probe,retryButton:[...document.querySelectorAll('button')].some(b=>b.textContent.includes('重新尝试认证'))}));
if(state.retryButton)throw Error('retry button exists');
await writeFile('D:/Project/acp-remote/.tmp/independent-acceptance-20261030/disconnected.json',JSON.stringify(state,null,2));console.log(state);
