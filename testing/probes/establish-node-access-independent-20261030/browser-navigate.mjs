const task = await taskSpace(3);
const page = task.page('p1');
await page.goto('https://faraday.tail74842e.ts.net:30451/');
console.log(await page.evaluate(() => ({ origin: location.origin, secure: isSecureContext })));
console.log(await page.snapshot());
