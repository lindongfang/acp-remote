const task=await taskSpace('independent-management-r16-20261030');
const page=task.page('p1');
await page.cdp('Page.addScriptToEvaluateOnNewDocument',{source:`(()=>{const Native=window.WebSocket;window.__managementAudit={requests:{},pages:[],notices:[],closes:[]};window.WebSocket=class extends Native{constructor(...args){super(...args);this.addEventListener('message',e=>{try{const m=JSON.parse(e.data),a=window.__managementAudit;if(m.type==='devices.changed')a.notices.push(m);if(m.type==='result'&&a.requests[m.requestId]==='device.list')a.pages.push({ok:m.ok,result:m.result,error:m.error});}catch{}});this.addEventListener('close',e=>window.__managementAudit.closes.push({code:e.code,reason:e.reason}));}send(data){try{const m=JSON.parse(data);if(m.type==='command')window.__managementAudit.requests[m.requestId]=m.action;}catch{}return super.send(data)}};})();`});
await page.goto('https://faraday.tail74842e.ts.net:30451');
console.log({spaceId:task.spaceId,ownership:task.ownership});
console.log(await page.snapshot());
