import assert from "node:assert/strict";
import { existsSync, unlinkSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { DatabaseSync } from "node:sqlite";
import { setTimeout as sleep } from "node:timers/promises";
import { pathToFileURL } from "node:url";
import { createBoundTarget, openNewTarget, pairClient } from "./paired-workbench.mjs";

import { installHistoryProbe } from "./history-socket-probe.mjs";

const cfg = JSON.parse(readFileSync(process.argv[2], "utf8"));
const { chromium } = await import(pathToFileURL(process.env.PLAYWRIGHT_ENTRY ??
  "C:/Users/jack/AppData/Local/npm-cache/_npx/e41f203b7505f1fb/node_modules/playwright/index.mjs").href);
const browser = await chromium.launch({ executablePath: process.env.EDGE_EXECUTABLE ??
  "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe", headless: true });
const report = { status: "failed", steps: [] };
const button = (host, name) => host.getByRole("button", { name, exact: true });
const history = page => page.locator('section[aria-label="会话历史"]').first();
const wire = [];
const bWire = [];
function observe(page, events) {
  page.on("websocket", socket => {
    const requests = new Map();
    socket.on("framesent", ({ payload }) => {
      if (typeof payload !== "string") return;
      const frame = JSON.parse(payload);
      if (frame.type !== "command") return;
      requests.set(frame.requestId, frame.action);
      events.push({ action: frame.action, mode: frame.payload.mode, reason: frame.payload.reason, payload: frame.payload, sentAt: Date.now() });
    });
    socket.on("framereceived", ({ payload }) => {
      if (typeof payload !== "string") return;
      const frame = JSON.parse(payload);
      const action = requests.get(frame.requestId);
      if (!requests.delete(frame.requestId)) return;
      events.push({ resultFor: action, stream: frame.result?.streamId, page: frame.result?.page, error: frame.error?.code ?? frame.error });
    });
  });
}
async function paired(name) {
  const ctx = await browser.newContext({ ignoreHTTPSErrors: true });
  const page = await ctx.newPage();
  if (!cfg.realCodexMode) await page.addInitScript(installHistoryProbe);
  observe(page,name === "a" ? wire : bWire);
  const file = path.join(cfg.workDir, `pairing-${name}.json`);
  for (let i = 0; !existsSync(file) && i < 200; i++) await sleep(100);
  await pairClient(page, cfg.origin, JSON.parse(readFileSync(file)).code, "HN " + name);
  return page;
}
async function groups(host, first, last) {
  await host.getByText(`第 ${first}–${last} 轮`, { exact: false }).first().waitFor();
  const titles = await host.locator("h3").allTextContents();
  assert.deepEqual(titles.filter(t => /^第 \d+ 轮$/.test(t)), Array.from(
    { length: last - first + 1 }, (_, index) => `第 ${first + index} 轮`));
}
async function openBound(page, targetId) {
  const prior = await button(page,"打开共享视图").first().elementHandles();
  await page.locator("#session-target-id").fill(targetId);
  await button(page, "读取会话队列").click();
  if (prior.length) await page.waitForFunction(el=>!el.isConnected,prior[0]);
  await button(page, "打开共享视图").first().click();
}
function initializeCount() {
  return readFileSync(path.join(cfg.peerStateDir, "peer-events.ndjson"), "utf8")
    .split("\n").filter(Boolean).map(JSON.parse).filter(e => e.method === "initialize").length;
}
async function runControlledHistory(a) {
  const b = await paired("b");
  report.steps.push("both paired");
  report.stage = "create target";
  const { nativeSessionId, targetId } = await openNewTarget(a, cfg);
  const root = history(a);
  const loadCount = () => readFileSync(path.join(cfg.peerStateDir,"peer-events.ndjson"),"utf8")
    .split("\n").filter(Boolean).map(JSON.parse).filter(e => e.method === "fixture/replay" && e.sessionId === nativeSessionId).length;
  async function reopenB(marker) {
    const before = loadCount();
    const start = bWire.length;
    await b.evaluate(() => { window.hnOrdinary = true; });
    await button(b,"打开共享视图").first().click();
    const deadline = Date.now()+10000;
    while (!bWire.slice(start).some(e=>e.resultFor === "view.open" && e.stream) && Date.now()<deadline) await sleep(20);
    const opens = bWire.slice(start).filter(e=>e.action === "view.open");
    assert.deepEqual(opens.map(e=>e.payload),[{targetId,reason:"reopen"}]);
    assert.ok(bWire.slice(start).some(e=>e.resultFor === "view.open" && e.stream));
    // A normal unpaged view can rebind after native reconciliation.
    const display = b.locator("[data-stream-id]").filter({has:b.getByText(marker,{exact:true})});
    try { await display.getByText(marker,{exact:true}).first().waitFor({timeout:6000}); }
    catch (error) {
      report.missingOrdinary = {loads:loadCount()-before,visible:(await b.locator("body").innerText()).includes(marker),rebinds:bWire.slice(start).filter(e=>e.action==="view.rebind").length};
      throw error;
    }
    await display.getByText(/本次快照已接收/).first().waitFor();
    const displayed = await display.getAttribute("data-stream-id");
    assert.ok(bWire.slice(start).some(e=>e.stream === displayed));
    report.lastOrdinary = {nativeLoads:loadCount()-before,opens:opens.length};
    assert.equal(loadCount(),before+1);
  }
  async function revokeA() {
    const revoked = b.locator("article.device-card").filter({has:b.getByRole("heading",{name:"HN a",exact:true})});
    await button(revoked,"撤销此客户端").click();
    await button(b,"确认撤销").click();
    await a.locator("#pairing-code").waitFor({state:"visible"});
    assert.ok(await a.evaluate(()=>window.hnCloseCodes.includes(4410)));
    report.revocationCode = 4410;
  }

  if (cfg.scenario === "identical" || cfg.scenario === "revoke-before-notice") {
    writeFileSync(path.join(cfg.peerStateDir,"manual-refresh"),"1");
    writeFileSync(path.join(cfg.peerStateDir,"changed"),"1");
    writeFileSync(path.join(cfg.peerStateDir,"edit-twelfth"),"1");
    await openBound(b,targetId);
    await groups(history(b),3,12);
    await root.getByLabel("每页轮数").fill("5");
    await button(root,"重新加载首页").click();
    await groups(root,8,12);
    await root.getByText("HN edited turn 12",{exact:true}).first().waitFor();
    await root.getByText(/本次快照已接收/).first().waitFor();
    const reads = () => wire.filter(e=>["view.open","view.open_source","history.page","view.rebind"].includes(e.action));
    if (cfg.scenario === "revoke-before-notice") {
      report.stage = "HN-030 revoke before browser delivery";
      const visible = await root.locator("section:has(> h3)").allInnerTexts();
      const oldStream = wire.findLast(e=>e.page).stream;
      await a.evaluate(()=>{ window.hnHoldNotice = true; });
      writeFileSync(path.join(cfg.peerStateDir,"edit-twelfth"),"0");
      await reopenB("HN new turn 12");
      await a.waitForFunction(id=>window.hnInvalidations.includes(id),oldStream);
      assert.equal(await root.getByText("历史已更新，请刷新",{exact:true}).count(),0);
      assert.deepEqual(await root.locator("section:has(> h3)").allInnerTexts(),visible);
      await revokeA();
      await a.evaluate(()=>window.hnReplay());
      assert.equal(await history(a).count(),0);
      assert.doesNotMatch(await a.locator("body").innerText(),/HN (edited|new) turn 12|历史已更新，请刷新/);
      await reopenB("HN new turn 12");
      report.steps.push("notice withheld at browser, 4410 before delivery, replay did not restore history");
      return;
    }
    report.stage = "HN-030 identical reread and live turn";
    const identicalReads = reads().length;
    const liveStream = wire.findLast(e=>e.page).stream;
    await a.evaluate(stream => { window.hnTail = {stream,starts:0,markers:0}; },liveStream);
    const digest = () => readFileSync(path.join(cfg.peerStateDir,"peer-events.ndjson"),"utf8")
      .split("\n").filter(Boolean).map(JSON.parse).findLast(e=>e.method==="fixture/replay" && e.sessionId===nativeSessionId).digest;
    const priorDigest = digest();
    await reopenB("HN edited turn 12");
    assert.equal(digest(),priorDigest);
    if (await button(b,"上传并提交").isDisabled()) {
      await b.getByRole("button",{name:/^(顶号)?接管会话$/}).first().click();
    }
    await b.locator("#session-input-body").fill("HN live user 13");
    await button(b,"上传并提交").click();
    const lastGroup = root.locator("section").filter({has:a.getByRole("heading",{name:"第 13 轮",exact:true})}).last();
    try {
      await lastGroup.getByText("HN live reply 13",{exact:true}).first().waitFor({timeout:6000});
    } finally {
      const dom = await root.innerText();
      const methods = readFileSync(path.join(cfg.peerStateDir,"peer-events.ndjson"),"utf8").split("\n").filter(Boolean).map(JSON.parse);
      const peerPrompts = methods.filter(e=>e.method==="session/prompt" && e.rawPrompt?.includes("HN live user 13")).length;
      report.liveProbe = {peerPrompts,bShowsReply:(await b.locator("body").innerText()).includes("HN live reply 13"),
        bErrors:bWire.filter(e=>e.error).map(e=>e.error),nativeReplayIdentical:digest() === priorDigest,
        opens:reads().length-identicalReads,
        headings:await root.getByRole("heading",{name:"第 13 轮",exact:true}).count(),
        user:dom.includes("HN live user 13"),reply:dom.includes("HN live reply 13"),
        readFailed:dom.includes("读取或展示准备失败"),invalidated:dom.includes("历史已更新，请刷新"),
        liveTail:wire.findLast(e=>e.page).page.liveTail,
        streamActive:await root.locator(`[data-stream-id="${liveStream}"]`).count(),
        ...await a.evaluate(()=>({starts:window.hnTail.starts,markers:window.hnTail.markers,
          events:window.hnInvalidations.filter(id=>id===window.hnTail.stream).length}))};
    }
    assert.equal(await root.getByRole("heading",{name:"第 13 轮",exact:true}).count(),1);
    assert.equal(await lastGroup.getByText("HN live user 13",{exact:true}).count(),1);
    assert.equal(await lastGroup.getByText("HN live reply 13",{exact:true}).count(),1);
    assert.equal(await root.getByText("历史已更新，请刷新",{exact:true}).count(),0);
    assert.equal(reads().length,identicalReads);
    const tail = await a.evaluate(() => ({starts:window.hnTail.starts,markers:window.hnTail.markers,
      invalidations:window.hnInvalidations.filter(id=>id===window.hnTail.stream).length}));
    assert.deepEqual(tail,{starts:1,markers:1,invalidations:0});
    report.identicalTail = tail;
    return;
  }
  report.stage = "parent 16-25";
  await groups(root, 16, 25);
  await button(root, "更早").click();
  await groups(root, 6, 15);
  await button(root, "更早").click();
  await groups(root, 1, 5);
  await root.getByText("前导记录", { exact: true }).waitFor();
  report.steps.push("parent 16-25,6-15,1-5 with prelude");
  const count = root.getByLabel("每页轮数").first();
  await count.fill("5");
  await button(root, "重新加载首页").click();
  await groups(root, 21, 25);
  const sent = wire.filter(event => event.action).length;

  await count.fill("0");
  await button(root, "重新加载首页").click();
  await root.getByText("每页轮数须为 1 至 50 的整数。").waitFor();
  assert.equal(wire.filter(event => event.action).length, sent);
  await count.fill("5");
  report.steps.push("custom page and invalid input");
  report.stage = "record child";
  const db = new DatabaseSync(cfg.database);
  try {
    assert.equal(db.prepare(`INSERT INTO node_session_parents
      SELECT t.source_id,'child-history',o.native_session_id,o.source_instance_id,o.runtime_epoch,
      o.connection_epoch,X'0000000000000001',X'0000000000000001',zeroblob(32),0
      FROM product_targets t JOIN node_source_session_operations o ON t.native_id=o.native_session_id
      WHERE t.target_id=? AND t.native_id=?`).run(targetId,nativeSessionId).changes, 1);
    const source = db.prepare("SELECT source_id FROM product_targets WHERE target_id=?").get(targetId);
    writeFileSync(path.join(cfg.workDir,"seed-child.json"),JSON.stringify(source));
  } finally { db.close(); }
  for (let i=0;i<200 && !existsSync(path.join(cfg.workDir,"child-seeded"));i++) await sleep(100);
  assert.ok(existsSync(path.join(cfg.workDir,"child-seeded")));
  report.stage = "expand recorded child";
  await root.getByLabel("子会话原生标识").fill("child-history");
  await button(root,"展开子会话历史").click();
  const child = history(root);
  await groups(child,1,10);
  await button(child,"更新").click();
  report.stage = "child second page";
  await groups(child,11,12);
  await button(root,"折叠子会话历史").click();
  assert.deepEqual((await root.locator("h3").allTextContents()).filter(t => /^第 \d+ 轮$/.test(t)), [21,22,23,24,25].map(i => `第 ${i} 轮`));
  await button(root,"展开子会话历史").click();
  await groups(child,1,10);
  await button(root,"折叠子会话历史").click();
  report.steps.push("child next, collapse and reopen");
  report.stage = "release and offline paging";
  await b.locator("#native-release-target").fill(targetId);
  await button(b,"登记释放").click();
  await button(b,"执行原生释放").click();
  await button(b,"重试释放协调").waitFor();
  const before = initializeCount();
  await button(root,"更早").click();
  await groups(root,16,20);
  assert.equal(initializeCount(),before);
  report.steps.push("offline paging, zero initialize");
  report.stage = "replace generation from second browser";
  const oldPage = wire.findLast(event => event.page)?.page;
  assert.equal(oldPage.firstTurn, "16");
  const start = wire.length;
  writeFileSync(path.join(cfg.peerStateDir,"changed"),"1");
  await openBound(b,targetId);
  await history(b).getByText(/本次快照已接收|本页已接收/).first().waitFor();
  await button(history(b),"重新加载首页").click();
  try {
    await history(b).getByText("HN new turn 25",{exact:true}).first().waitFor({timeout:8000});
  } catch (error) {
    const visible = await b.locator("body").innerText();
    report.generationProbe = {changed:visible.includes("HN new turn 25"),old:visible.includes("HN original turn 25"),
      readFailed:visible.includes("读取或展示准备失败"),invalid:visible.includes("历史已更新，请刷新"),
      opens:bWire.filter(e=>e.action === "view.open").map(e=>e.reason),
      pages:bWire.filter(e=>e.page).map(e=>({source:e.page.source,live:e.page.liveTail,last:e.page.lastTurn}))};
    throw error;
  }
  await button(root,"更早").click();
  await groups(root,11,15);
  await root.getByText("已在最新同步代际中重新定位，与之前页面的连续性未经证明",{exact:false}).first().waitFor();

  const sequence = wire.slice(start).filter(e => e.action === "history.page" || e.error || e.page);
  assert.deepEqual(sequence[0].mode, {kind:"continue",cursor:oldPage.olderCursor,direction:"older"});
  assert.equal(sequence[1].error, "stale_cursor");
  assert.deepEqual(sequence[2].mode, {kind:"locate",fromTurn:"15",direction:"older"});
  assert.equal(sequence[3].page.relocated,true);
  assert.equal(sequence[3].page.continuity,"unproven");
  report.steps.push("older, stale_cursor, locate(15), relocated/unproven, warning visible");
  report.stage = "relocated page content";
  try {
    await root.getByText(/HN new turn 15\b/).first().waitFor({timeout:8000});
  } finally {
    const visibleText = await root.innerText();
    report.relocatedContent = {
      newTurns: [...visibleText.matchAll(/HN new turn (\d+)/g)].map(m => Number(m[1])),
      oldTurns: [...visibleText.matchAll(/HN original turn (\d+)/g)].map(m => Number(m[1])),
      snapshotReceived: visibleText.includes("本页已接收"),
      readFailure: visibleText.includes("读取或展示准备失败"),
    };
  }

  report.stage = "HN-030 two-browser manual refresh";
  writeFileSync(path.join(cfg.peerStateDir,"manual-refresh"),"1");
  await count.fill("5");
  await button(root,"重新加载首页").click();
  await groups(root,8,12);
  await root.getByText("HN new turn 12",{exact:true}).first().waitFor();
  const pageBody = root.locator(":scope > div").filter({has:a.getByRole("heading",{name:"第 12 轮",exact:true})});
  const turnContents = () => pageBody.locator("section:has(> h3)").allInnerTexts();
  const originalBody = await turnContents();
  assert.equal(originalBody.length,5);
  await a.evaluate(()=>{ window.hnWatchMarker = {text:"HN edited turn 12",hits:0}; });
  const beforeChange = wire.length;
  const originalStream = wire.findLast(e=>e.page).stream;
  writeFileSync(path.join(cfg.peerStateDir,"edit-twelfth"),"1");
  await reopenB("HN edited turn 12");
  await root.getByText("历史已更新，请刷新",{exact:true}).waitFor();
  assert.equal(await a.evaluate(id => window.hnInvalidations.filter(v=>v===id).length,originalStream),1);
  await a.evaluate(() => window.hnReplay());
  const reads = () => wire.slice(beforeChange).filter(e => ["view.open","view.open_source","history.page","view.rebind"].includes(e.action));
  assert.equal(reads().length,0);
  assert.equal(await a.evaluate(()=>window.hnWatchMarker.hits),0);
  assert.deepEqual(await turnContents(),originalBody);
  assert.equal(await root.getByText("历史已更新，请刷新",{exact:true}).count(),1);
  assert.equal(await button(root,"刷新历史").count(),1);
  await root.getByText("HN new turn 12",{exact:true}).first().waitFor();
  await button(root,"刷新历史").click();
  await root.getByText("HN edited turn 12",{exact:true}).first().waitFor();
  await a.evaluate(() => window.hnReplay());
  await groups(root,8,12);
  assert.deepEqual(reads().map(e => e.payload),[{targetId,reason:"explicit_refresh",page:{turns:"5"}}]);
  assert.equal(await root.getByText("历史已更新，请刷新",{exact:true}).count(),0);
  assert.equal(await root.getByText("HN new turn 12",{exact:true}).count(),0);
  report.steps.push("HN-030: old wire frames replayed before and after one explicit five-turn refresh; unchanged visible turns 8–12");
  for (const failure of ["request","receive"]) {
    report.stage = "HN-030 " + failure + " failure and manual retry";
    const oldText = failure === "request" ? "HN edited turn 12" : "HN new turn 12";
    const newText = failure === "request" ? "HN new turn 12" : "HN edited turn 12";
    writeFileSync(path.join(cfg.peerStateDir,"edit-twelfth"),failure === "request" ? "0" : "1");
    await reopenB(newText);
    await root.getByText("历史已更新，请刷新",{exact:true}).waitFor();
    const initial = reads().length;
    const body = root.locator(":scope > div").filter({has:a.getByRole("heading",{name:"第 12 轮",exact:true})});
    const oldBody = await body.innerText();
    assert.ok(oldBody.includes(oldText));
    for (const turn of [8,9,10,11]) assert.ok(oldBody.includes(`HN new turn ${turn}`));
    const failFlag = path.join(cfg.peerStateDir,"failed-attempt");
    writeFileSync(failFlag,"1");
    await a.evaluate(mode => window.hnArm(mode),failure);
    await button(root,"刷新历史").click();
    await a.waitForFunction(() => window.hnReady());
    const packetCount = await a.evaluate(marker => {
      const first = window.hnFirst = window.hnAttempt();
      if (!first.frames.some(f=>new Uint8Array(f)[56] === 1 && new TextDecoder().decode(f).includes(marker))) throw Error("attempt snapshot lacks marker");
      return first.frames.length;
    },"HN failed attempt turn 12");
    unlinkSync(failFlag);
    if (failure === "receive") {
      assert.equal(await body.innerText(),oldBody);
      await a.evaluate(() => window.hnFirst.fail());
    }
    await root.getByText(failure === "receive" ? /invalid_sequence/ : /请求的管理目标不存在/).first().waitFor();
    assert.equal(await body.innerText(),oldBody);
    assert.ok(await button(root,"刷新历史").isEnabled());
    if (failure === "request") await a.clock.install();
    await a.clock.fastForward(3000);
    assert.equal(reads().length,initial+1);
    await a.evaluate(() => window.hnArm("retry"));
    await button(root,"刷新历史").click();
    await a.waitForFunction(() => window.hnReady());
    const late = async () => {
      const delivered = await a.evaluate(() => {
        const retry = window.hnAttempt(), first = window.hnFirst;
        if (retry.id === first.id || retry.stream === first.stream) throw Error("attempt identities overlap");
        return first.replay();
      });
      assert.deepEqual(delivered,{responses:1,packets:packetCount});
      await a.evaluate(() => new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));
      return delivered;
    };
    const pending = await late();
    assert.equal(await body.innerText(),oldBody);
    assert.ok(await button(root,"刷新历史").isDisabled());
    await a.evaluate(degraded => window.hnAttempt().release(degraded),failure === "receive");
    await root.getByText(newText,{exact:true}).first().waitFor();
    await groups(root,8,12);
    const success = await root.innerText();
    const settled = await late();
    await a.evaluate(() => window.hnAttempt().release());
    assert.equal(await root.innerText(),success);
    assert.doesNotMatch(success,/历史已更新，请刷新|invalid_sequence|请求的管理目标不存在/);
    assert.equal(await root.getByText(oldText,{exact:true}).count(),0);
    const streams = await a.evaluate(() => ({old:window.hnFirst.stream,current:window.hnAttempt().stream}));
    const closes = id => wire.filter(e=>e.action==="view.close" && e.payload.streamId === id).length;
    assert.equal(closes(streams.old),1);
    assert.equal(closes(streams.current),0);
    (report.retryDeliveries ??= {})[failure] = {pending,settled,discardedCloses:closes(streams.old),currentCloses:closes(streams.current)};
    assert.deepEqual(reads().slice(initial).map(r=>r.payload),Array(2).fill({targetId,reason:"explicit_refresh",page:{turns:"5"}}));
    if (failure === "receive") {
      const beforeSync = reads().length;
      await button(root,"重试同步").click();
      await root.getByText(newText,{exact:true}).first().waitFor();
      await root.locator('[data-sync-state="synced"]').first().waitFor({state:"attached"});
      await groups(root,8,12);
      assert.deepEqual(reads().slice(beforeSync).map(e=>e.payload),[{targetId,reason:"explicit_refresh",page:{turns:"5"}}]);
      report.promotedSyncRetries = 1;
    }
    report.steps.push("HN-030 " + failure + " failure retained old page; stale response isolated during and after retry");
  }
  report.stage = "HN-030 switch target while refresh pending";
  writeFileSync(path.join(cfg.peerStateDir,"edit-twelfth"),"0");
  await reopenB("HN new turn 12");
  await a.evaluate(() => window.hnArm("hold"));
  await button(root,"刷新历史").click();
  await a.waitForFunction(() => window.hnReady());
  writeFileSync(path.join(cfg.peerStateDir,"first-native"),nativeSessionId);
  const switchAt = wire.length;
  const {targetId:otherId} = await createBoundTarget(a,cfg);
  assert.notEqual(otherId,targetId);
  try {
    await history(a).getByText("HN other target turn 12",{exact:true}).first().waitFor({timeout:6000});
  } catch (error) {
    const dom = await a.locator("body").innerText();
    report.otherProbe = {opens:wire.slice(switchAt).filter(e=>e.action==="view.open").map(e=>e.payload.targetId===otherId),
      pages:wire.slice(switchAt).filter(e=>e.page).map(e=>({last:e.page.lastTurn,source:e.page.source})),
      marker:dom.includes("HN other target turn 12"),retained:dom.includes("HN edited turn 12"),fail:dom.includes("读取或展示准备失败")};
    throw error;
  }
  await history(a).getByText(/本次快照已接收|本页已接收/).first().waitFor();
  const switched = await history(a).innerText();
  report.switchDelivery = await a.evaluate(() => window.hnAttempt().replay());
  assert.ok(wire.slice(switchAt).some(e=>e.action==="view.open" && e.payload.targetId===otherId));
  assert.equal(report.switchDelivery.responses,1);
  assert.ok(report.switchDelivery.packets > 1);
  await a.evaluate(() => new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve))));
  assert.equal(await history(a).innerText(),switched);
  await openBound(a,targetId);
  await history(a).getByText("HN new turn 12",{exact:true}).first().waitFor();
  report.stage = "HN-030 revoke while reader awaits refreshed page";
  writeFileSync(path.join(cfg.peerStateDir,"edit-twelfth"),"1");
  await b.evaluate(() => { window.hnOrdinary = true; });
  await button(b,"打开共享视图").first().click();
  await root.getByText("历史已更新，请刷新",{exact:true}).waitFor();
  await a.evaluate(() => window.hnArm("hold"));
  await button(root,"刷新历史").click();
  await a.waitForFunction(() => window.hnReady());
  await revokeA();
  report.revokedDelivery = await a.evaluate(() => { window.hnReplay(); return window.hnAttempt().replay(); });
  assert.equal(report.revokedDelivery.responses,1);
  assert.ok(report.revokedDelivery.packets > 1);
  assert.equal(await history(a).count(),0);
  assert.doesNotMatch(await a.locator("body").innerText(),/HN (edited|new) turn 12|历史已更新，请刷新/);
  await reopenB("HN edited turn 12");
  report.steps.push("HN-030 revoked reader returned to pairing and late frames left protected content absent");
}

try {
  report.stage = "pair clients";
  const a = await paired("a");
  if (!cfg.realCodexMode) {
    const bundle = await a.locator('script[type="module"][src]').first().getAttribute("src");
    assert.ok(readFileSync(new URL("../../dist/index.html",import.meta.url),"utf8").includes(`src="${bundle}"`));
    report.webBundle = bundle;
  }
  if (cfg.realCodexMode) {
    const { runRealHistory } = await import("./real-history-navigation.mjs");
    await runRealHistory({page:a,cfg,report,openNewTarget,groups,history,button,wire,openBound});
  } else {
    await runControlledHistory(a);
  }
  report.status = "passed";
} catch (error) {
  report.failureType = error.name;
  if (error.code === "ERR_ASSERTION" && [error.actual,error.expected].every(v=>typeof v==="number")) {
    report.assertion = {actual:error.actual,expected:error.expected};
  }
  report.line = error.stack?.match(/history-navigation\.mjs:(\d+):/)?.[1];
  report.pages = wire.filter(event => event.page).map(({page}) => ({
    first:page.firstTurn,last:page.lastTurn,source:page.source,turnStatus:page.turnStatus,continuity:page.continuity
  }));
  report.errors = wire.map(e => e.error).filter(e => typeof e === "string" && /^[a-z_]+$/.test(e));
  process.exitCode = 1;
} finally {
  await browser.close();
  writeFileSync(cfg.evidenceOut,JSON.stringify(report,null,2));
}
