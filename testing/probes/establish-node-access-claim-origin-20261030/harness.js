(async()=>{
 const db=await new Promise((ok,no)=>{const q=indexedDB.open('acp-remote-access-v1');q.onsuccess=()=>ok(q.result);q.onerror=()=>no(q.error)});
 const record=await new Promise(ok=>{const q=db.transaction('identity').objectStore('identity').get('primary');q.onsuccess=()=>ok(q.result)});db.close();if(!record)throw Error('production identity missing');
 const b64=b=>btoa(String.fromCharCode(...new Uint8Array(b))).replace(/=/g,'').replace(/\+/g,'-').replace(/\//g,'_');
 const decode=s=>Uint8Array.from(atob(s.replace(/-/g,'+').replace(/_/g,'/').padEnd(Math.ceil(s.length/4)*4,'=')),c=>c.charCodeAt(0));
 const hash=async s=>Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',new TextEncoder().encode(s))),x=>x.toString(16).padStart(2,'0')).join('');
 const native=window.fetch.bind(window);let release;const gate=new Promise(ok=>release=ok);
 const h=window.__claimHarness={record,native,b64,decode,hash,ready:false,sentAt:null,response:null,lookup:null,originalProof:null,releaseAt:time=>setTimeout(release,Math.max(0,time-Date.now()))};
 h.post=async(path,body)=>{const r=await native('/api/access/v1/pair/'+path,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body),cache:'no-store'});return {status:r.status,body:await r.json()};};
 h.sign=async(payload,typ='acpr-pair-claim+jwt')=>{const input=b64(new TextEncoder().encode(JSON.stringify({alg:'ES256',typ})))+'.'+b64(new TextEncoder().encode(JSON.stringify(payload)));return input+'.'+b64(await crypto.subtle.sign({name:'ECDSA',hash:'SHA-256'},record.privateKey,new TextEncoder().encode(input)));};
 h.summary=async()=>({secure:isSecureContext,origin:location.origin,ready:h.ready,sentAt:h.sentAt,status:h.response?.status??null,bodyError:h.response?.body?.error??null,publicKeySha256:await hash(record.publicKey),privateExtractable:record.privateKey.extractable,sentinel:localStorage.getItem('na021-profile-sentinel')});
 window.fetch=async(...args)=>{const url=String(args[0]);if(url.endsWith('/pair/lookup')){const r=await native(...args);if(r.status===200)h.lookup=await r.clone().json();return r;}if(url.endsWith('/pair/claim')){h.originalProof=JSON.parse(args[1].body).proof;h.ready=true;await gate;h.sentAt=Date.now();const r=await native(...args);h.response={status:r.status,body:await r.clone().json()};return r;}return native(...args);};
 return {installed:true,publicKeySha256:await hash(record.publicKey),privateExtractable:record.privateKey.extractable};
})()
