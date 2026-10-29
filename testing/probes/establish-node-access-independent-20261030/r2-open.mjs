const task=await taskSpace('independent-node-access-r2-20261030');
const page=task.page('p1');
await page.goto('https://faraday.tail74842e.ts.net:30452/');
console.log({spaceId:task.spaceId,secure:await page.evaluate(()=>isSecureContext)});
console.log(await page.snapshot());
