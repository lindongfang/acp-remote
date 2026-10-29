import {readFile} from 'node:fs/promises';
const task=await taskSpace(3);const page=task.page('p1');
await page.fill('input[name="pairing-code"]',JSON.parse(await readFile('D:/Project/acp-remote/.tmp/independent-acceptance-20261030/pair-input.json','utf8')).code);
await page.fill('input[name="display-name"]','Independent 20261030 A after4401');
await page.click('button[type="submit"]');
await page.waitForFunction(()=>document.body.innerText.includes('已认领，等待本机批准'),undefined,{timeout:12000});
console.log(await page.snapshot());
