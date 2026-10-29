import {readFile} from 'node:fs/promises';
const input=JSON.parse(await readFile('D:/Project/acp-remote/.tmp/independent-acceptance-20261030/r2-pair-input.json','utf8'));
const task=await taskSpace(4);const page=task.page('p1');
await page.fill('input[name="pairing-code"]',input.code);
await page.fill('input[name="display-name"]',input.name);
await page.click('button[type="submit"]');
await page.waitForFunction(()=>document.body.innerText.includes('已认领，等待本机批准'),undefined,{timeout:15000});
console.log(await page.snapshot());
