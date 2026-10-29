const task=await taskSpace(8),page=task.page('p1');
await page.cdp('Page.removeScriptToEvaluateOnNewDocument',{identifier:'2'});
await page.reload();
await page.waitForSelector('input[name="pairing-code"]',{state:'visible'});
console.log(await page.snapshot());
