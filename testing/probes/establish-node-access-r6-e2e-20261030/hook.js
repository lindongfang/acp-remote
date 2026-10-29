window.__e2e={events:[],sockets:[]};
window.__NativeWebSocket=WebSocket;
window.WebSocket=class extends WebSocket{
 constructor(...args){super(...args);const id=window.__e2e.sockets.length;window.__e2e.sockets.push(this);
 const log=(event,data)=>window.__e2e.events.push({at:Date.now(),id,event,...data});
 this.addEventListener('open',()=>log('open',{}));
 this.addEventListener('close',e=>log('close',{code:e.code,reason:e.reason,clean:e.wasClean}));
 this.addEventListener('message',e=>{try{const m=JSON.parse(e.data);if(m.type==='challenge')log('recv',{type:m.type,nodeId:m.nodeId});else if(m.type==='result')log('recv',{type:m.type,requestId:m.requestId,ok:m.ok,error:m.error,result:m.result});else log('recv',{type:m.type,clientId:m.clientId,revision:m.revision})}catch{log('malformed',{})}});
 }
 send(data){try{const m=JSON.parse(data);window.__e2e.events.push({at:Date.now(),id:window.__e2e.sockets.indexOf(this),event:'send',type:m.type,action:m.action,requestId:m.requestId,resource:m.resource})}catch{}return super.send(data)}
};
