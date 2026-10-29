const task=await taskSpace(8),page=task.page('p1');
await page.fill('input[name="pairing-code"]','Z3B6-FM0Y');
await page.fill('input[name="display-name"]','NA031 storage isolated');
await page.click('text=认领这台浏览器');
console.log(await page.snapshot());
