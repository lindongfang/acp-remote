const fs = await import("node:fs/promises");
const extra = JSON.parse(await fs.readFile("D:/Project/acp-remote/.tmp/independent-jws-20261030/extra-vectors.json","utf8"));
const task = await taskSpace('independent-jws-20261030');
const page = task.page("p1");
const result = await page.evaluate(async extra => {
  const jws = await import("/src/protocol/jws.ts");
  const protocol = await import("/src/management/protocol.ts");
  const f = await (await fetch("/tests/fixtures/jws-v1.json")).json();
  const accepted = ["acpr-auth-client+jwt"];
  const key = jws.decodeBase64urlStrict(f.clientPublicKey);
  const compact = signature => `${f.headerSegment}.${f.payloadSegment}.${signature}`;
  const checks=[];
  const reject=async(name,action,code)=>{
    let caught;try{await action();}catch(error){caught=error;}
    if(caught?.code!==code)throw new Error(`${name}: expected ${code}, got ${caught?.code??"accepted"}`);
    checks.push({name,code,passed:true});
  };
  await jws.verifyCompactJws(compact(extra.normalizedLowS),key,accepted);
  checks.push({name:"library_normalized_low_s_accepted",passed:true});
  await jws.verifyCompactJws(compact(f.highS.signatureSegment),key,accepted);
  checks.push({name:"fixed_high_s_accepted",passed:true});
  await reject("valid_der_rejected_by_p1363_protocol",()=>jws.verifyCompactJws(compact(extra.validDerSignature),key,accepted),"invalid_signature_encoding");
  await reject("valid_compressed_point_rejected_by_sec1_contract",()=>jws.verifyCompactJws(compact(f.rust.signatureSegment),jws.decodeBase64urlStrict(extra.validCompressedPoint),accepted),"invalid_public_key");
  const order=0xffffffff00000000ffffffffffffffffbce6faada7179e84f3b9cac2fc632551n;
  for(const [name,value] of [["zero",0n],["order",order],["above_order",order+1n]]){
    const scalar=Uint8Array.from(value.toString(16).padStart(64,"0").match(/../g),pair=>parseInt(pair,16));
    for(const [part,offset] of [["r",0],["s",32]]){
      const signature=jws.decodeBase64urlStrict(f.rust.signatureSegment);signature.set(scalar,offset);
      await reject(`${part}_${name}`,()=>jws.verifyCompactJws(compact(jws.encodeBase64url(signature)),key,accepted),"invalid_signature_encoding");
    }
  }
  const payload=JSON.parse(f.payload);
  const challenge={v:1,type:"challenge",nodeId:payload.nodeId,nodePublicKey:payload.nodePublicKey,origin:payload.origin,connectionId:payload.connectionId,challengeId:payload.challengeId,nonce:payload.nonce,iat:payload.iat,exp:payload.exp,hostProof:extra.validHostProof};
  const expected={origin:payload.origin,nodeId:payload.nodeId,nodePublicKey:payload.nodePublicKey,clientId:payload.clientId,clientPublicKey:payload.clientPublicKey};
  const hello=protocol.createHello(payload.clientId,payload.clientPublicKey,payload.clientNonce);
  const originalNow=Date.now;Date.now=()=>payload.iat*1000;
  try{
    await protocol.verifyChallenge(challenge,expected,hello);
    checks.push({name:"valid_host_payload_control",passed:true});
    await reject("signed_unknown_host_payload_field",()=>protocol.verifyChallenge({...challenge,hostProof:extra.unknownHostProof},expected,hello),"host_proof_invalid");
  }finally{Date.now=originalNow;}
  if(!extra.unknownClientPayloadRejectedByRust)throw new Error("Rust closure assertion missing");
  checks.push({name:"signed_unknown_client_payload_rejected_by_rust",passed:true});
  return{secureContext:isSecureContext,userAgent:navigator.userAgent,controlledClockForHostProof:true,checks};
},extra);
await fs.writeFile("D:/Project/acp-remote/.tmp/independent-jws-20261030/extra-browser.json",JSON.stringify(result,null,2));
console.log({checks:result.checks.length,passed:result.checks.every(check=>check.passed),secureContext:result.secureContext,controlledClockForHostProof:true});

