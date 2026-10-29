const task=await taskSpace(8);
const page=task.page('p1');
await page.goto('https://faraday.tail74842e.ts.net:30453');
console.log(await page.snapshot());
console.log(await page.evaluate(()=>({origin:location.origin,secure:isSecureContext,databases:null,userAgent:navigator.userAgent})));
