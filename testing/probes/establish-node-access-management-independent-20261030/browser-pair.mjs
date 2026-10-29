const fs=await import('node:fs/promises');const root='D:/Project/acp-remote/.tmp/independent-management-r16-20261030';
const ticket=JSON.parse(await fs.readFile(root+'/browser-ticket.json','utf8'));
const task=await taskSpace('independent-management-r16-20261030');const page=task.page('p1');
await page.fill('input[name="pairing-code"]',ticket.code);await page.fill('input[name="display-name"]','Independent paging Chromium');await page.click('form button');
await page.waitForFunction(()=>document.body.innerText.includes('等待')||document.body.innerText.includes('待确认'),undefined,{timeout:10000});
await fs.writeFile(root+'/approve-browser.flag','approve owned claimant');
await page.waitForFunction(()=>document.body.innerText.includes('AUTHENTICATED'),undefined,{timeout:20000});
console.log({spaceId:task.spaceId,authenticated:true,secureContext:await page.evaluate(()=>isSecureContext)});
