const task=await taskSpace(4);const page=task.page('p1');
await page.evaluate(()=>window.dispatchEvent(new PageTransitionEvent('pagehide',{persisted:true})));
console.log('pagehide dispatched before offline revoke');
