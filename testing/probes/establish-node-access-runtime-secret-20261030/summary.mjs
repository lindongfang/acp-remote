import fs from 'node:fs';
import crypto from 'node:crypto';
const root='D:/Project/acp-remote/.tmp/runtime-secret-acceptance-20261030/';
const read=n=>JSON.parse(fs.readFileSync(root+'evidence/'+n,'utf8'));
const a=read('artifacts.json'),auth=read('authentication-results.json');
const summary={binarySha256:crypto.createHash('sha256').update(fs.readFileSync(root+'bin/acp-remote.exe')).digest('hex'),case:'NA-014',assertions:{uniformAuthenticationFailures:auth.uniform,websocketFailures:auth.results.length,httpClaimStatusInvalidProof:auth.http.filter(x=>x.path.includes('/pair/')).every(x=>x.status===401),validSchemaInvalidCodeRejected:auth.http.find(x=>x.path==='lookup-valid-schema-unknown-code')?.status===401,responseEchoes:a.responseEchoes.length,externalMarkerHits:Object.values(a.scans).reduce((n,x)=>n+x.markerHits,0),storageHistoryMarkerHits:Object.values(a.storageAndHistory).reduce((n,x)=>n+x.markerHits,0)},audit:read('audit-summary.json'),networkRecords:a.network.length,candidateStrings:a.candidateStrings,candidateVariants:a.candidateVariants,cleanup:read('cleanup.json'),browserCleanup:read('browser-cleanup.json')};
fs.writeFileSync(root+'evidence/summary.json',JSON.stringify(summary,null,2));
const manifest=fs.readdirSync(root+'evidence').filter(n=>n!=='manifest.json').map(n=>({file:n,sha256:crypto.createHash('sha256').update(fs.readFileSync(root+'evidence/'+n)).digest('hex')}));
fs.writeFileSync(root+'evidence/manifest.json',JSON.stringify(manifest,null,2));console.log(JSON.stringify(summary,null,2));
