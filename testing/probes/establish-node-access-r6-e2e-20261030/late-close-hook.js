window.__lateHold=true;
window.__lateQueue=[];
window.__lateFacts=[];
const ObservedWebSocket=window.WebSocket;
window.WebSocket=class extends ObservedWebSocket {
 constructor(...args){
  super(...args);
  const nativeAdd=this.addEventListener.bind(this);
  this.addEventListener=(type,listener,options)=>{
   if(type!=='close')return nativeAdd(type,listener,options);
   return nativeAdd(type,event=>{
    const invoke=()=>typeof listener==='function'?listener.call(this,event):listener.handleEvent(event);
    if(window.__lateHold&&event.code===4410){
     window.__lateFacts.push({at:Date.now(),code:event.code,reason:event.reason,isTrusted:event.isTrusted,held:true});
     window.__lateQueue.push(invoke);
    }else invoke();
   },options);
  };
 }
};
window.__releaseRealClose=()=>{window.__lateHold=false;const queue=window.__lateQueue.splice(0);for(const invoke of queue)invoke();return queue.length};
