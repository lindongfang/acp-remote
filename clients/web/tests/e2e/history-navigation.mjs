import assert from "node:assert/strict";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { DatabaseSync } from "node:sqlite";
import { setTimeout as sleep } from "node:timers/promises";
import { pathToFileURL } from "node:url";
import { openNewTarget, pairClient } from "./paired-workbench.mjs";

const cfg = JSON.parse(readFileSync(process.argv[2], "utf8"));
const { chromium } = await import(pathToFileURL(process.env.PLAYWRIGHT_ENTRY ??
  "C:/Users/jack/AppData/Local/npm-cache/_npx/e41f203b7505f1fb/node_modules/playwright/index.mjs").href);
const browser = await chromium.launch({ executablePath: process.env.EDGE_EXECUTABLE ??
  "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe", headless: true });
const report = { status: "failed", steps: [] };
const button = (host, name) => host.getByRole("button", { name, exact: true });
const history = page => page.locator('section[aria-label="会话历史"]').first();
const wire = [];
function observe(page) {
  page.on("websocket", socket => {
    const requests = new Map();
    socket.on("framesent", ({ payload }) => {
      if (typeof payload !== "string") return;
      const frame = JSON.parse(payload);
      if (frame.type !== "command") return;
      requests.set(frame.requestId, frame.action);
      wire.push({ action: frame.action, mode: frame.payload.mode, reason: frame.payload.reason, payload: frame.payload, sentAt: Date.now() });
    });
    socket.on("framereceived", ({ payload }) => {
      if (typeof payload !== "string") return;
      const frame = JSON.parse(payload);
      if (!requests.delete(frame.requestId)) return;
      wire.push({ page: frame.result?.page, error: frame.error?.code ?? frame.error });
    });
  });
}
async function paired(name) {
  const ctx = await browser.newContext({ ignoreHTTPSErrors: true });
  const page = await ctx.newPage();
  if (!cfg.realCodexMode) await page.addInitScript(() => {
    const NativeSocket = window.WebSocket;
    window.WebSocket = class extends NativeSocket {
      constructor(...args) {
        super(...args);
        this.addEventListener("message", event => {
          if (!event.isTrusted) return;
          const deliver = data => this.dispatchEvent(new MessageEvent("message", {data}));
          if (window.hnFault && typeof event.data === "string") {
            const frame = JSON.parse(event.data);
            if (frame.type === "result" && frame.requestId === window.hnRequest) {
              if (window.hnFault === "retry") {
                event.stopImmediatePropagation();
                window.hnRelease = () => deliver(event.data);
              } else {
                window.hnLateResult = () => deliver(event.data);
                if (window.hnFault === "request") {
                  event.stopImmediatePropagation();
                  deliver(JSON.stringify({...frame,ok:false,result:null,error:{code:"not_found"}}));
                }
              }
            }
          }
          if (window.hnFault === "receive" && event.data instanceof ArrayBuffer && new Uint8Array(event.data)[56] === 4) {
            event.stopImmediatePropagation();
            const invalid = event.data.slice(0);
            const header = new DataView(invalid);
            header.setBigUint64(24,header.getBigUint64(24)+1n);
            window.hnFailReceive = () => deliver(invalid);
            window.hnLateSnapshot = () => deliver(event.data);
          }
          if (!window.hnCapture) return;
          if (typeof event.data === "string") {
            if (JSON.parse(event.data).type === "history.invalidated") {
              window.hnReplay = () => {
                this.dispatchEvent(new MessageEvent("message", {data:event.data}));
                this.dispatchEvent(new MessageEvent("message", {data:window.hnOldBody}));
              };
            }
          } else if (event.data instanceof ArrayBuffer && new Uint8Array(event.data)[56] === 1) {
            window.hnOldBody = event.data;
          }
        });
      }
      send(data) {
        if (window.hnFault && typeof data === "string") {
          const frame = JSON.parse(data);
          if (frame.action === "view.open") window.hnRequest = frame.requestId;
        }
        if (window.hnOrdinary && typeof data === "string") {
          const frame = JSON.parse(data);
          if (frame.action === "view.open") {
            window.hnOrdinary = false;
            delete frame.payload.page;
            frame.payload.reason = "reopen";
            data = JSON.stringify(frame);
          }
        }
        super.send(data);
      }
    };
  });
  if (name === "a") observe(page);
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
  await page.locator("#session-target-id").fill(targetId);
  await button(page, "读取会话队列").click();
  await button(page, "打开共享视图").first().click();
}
function initializeCount() {
  return readFileSync(path.join(cfg.peerStateDir, "peer-events.ndjson"), "utf8")
    .split("\n").filter(Boolean).map(JSON.parse).filter(e => e.method === "initialize").length;
}
try {
  report.stage = "pair clients";
  const a = await paired("a");
  if (cfg.realCodexMode) {
    const { runRealHistory } = await import("./real-history-navigation.mjs");
    await runRealHistory({page:a,cfg,report,openNewTarget,groups,history,button,wire,openBound});
  } else {
  const b = await paired("b");
  report.steps.push("both paired");
  report.stage = "create target";
  const { nativeSessionId, targetId } = await openNewTarget(a, cfg);
  const root = history(a);
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
  await button(history(b),"重新加载首页").click();
  await history(b).getByText("HN new turn 25",{exact:true}).first().waitFor();
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
  await a.evaluate(() => { window.hnCapture = true; });
  await count.fill("5");
  await button(root,"重新加载首页").click();
  await groups(root,8,12);
  await root.getByText("HN new turn 12",{exact:true}).first().waitFor();
  const beforeChange = wire.length;
  writeFileSync(path.join(cfg.peerStateDir,"edit-twelfth"),"1");
  await b.evaluate(() => { window.hnOrdinary = true; });
  await button(b,"打开共享视图").first().click();
  await root.getByText("历史已更新，请刷新",{exact:true}).waitFor();
  await a.evaluate(() => {
    if (!(window.hnOldBody instanceof ArrayBuffer)) throw Error("missing old body frame");
    window.hnReplay();
  });
  const reads = () => wire.slice(beforeChange).filter(e => ["view.open","view.open_source","history.page","view.rebind"].includes(e.action));
  assert.equal(reads().length,0);
  assert.equal(await button(root,"刷新历史").count(),1);
  await root.getByText("HN new turn 12",{exact:true}).first().waitFor();
  await a.evaluate(() => { window.hnCapture = false; });
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
    await b.evaluate(() => { window.hnOrdinary = true; });
    await button(b,"打开共享视图").first().click();
    await root.getByText("历史已更新，请刷新",{exact:true}).waitFor();
    const initial = reads().length;
    await a.evaluate(mode => { window.hnFault = mode; window.hnRelease = undefined; },failure);
    await button(root,"刷新历史").click();
    if (failure === "receive") {
      await a.waitForFunction(() => typeof window.hnFailReceive === "function");
      assert.equal(await root.getByText(newText,{exact:true}).count(),0);
      await root.getByText(oldText,{exact:true}).first().waitFor();
      await a.evaluate(() => window.hnFailReceive());
    }
    await root.getByText(/历史已更新，请刷新。/).waitFor();
    if (failure === "receive") assert.match(await root.innerText(),/invalid_sequence/);
    await root.getByText(oldText,{exact:true}).first().waitFor();
    assert.equal(reads().length,initial+1);
    await a.evaluate(() => { window.hnFault = "retry"; });
    await button(root,"刷新历史").click();
    await a.waitForFunction(() => typeof window.hnRelease === "function");
    const late = () => a.evaluate(() => { window.hnLateResult(); window.hnLateSnapshot?.(); });
    await late();
    await root.getByText(oldText,{exact:true}).first().waitFor();
    await a.evaluate(() => { window.hnFault = null; window.hnRelease(); });
    await root.getByText(newText,{exact:true}).first().waitFor();
    await late();
    await groups(root,8,12);
    assert.equal(await root.getByText(oldText,{exact:true}).count(),0);
    assert.deepEqual(reads().slice(initial).map(r=>r.payload),Array(2).fill({targetId,reason:"explicit_refresh",page:{turns:"5"}}));
    report.steps.push("HN-030 " + failure + " failure retained old page; stale response isolated during and after retry");
  }
  report.stage = "HN-030 revoke while reader awaits refreshed page";
  writeFileSync(path.join(cfg.peerStateDir,"edit-twelfth"),"0");
  await b.evaluate(() => { window.hnOrdinary = true; });
  await button(b,"打开共享视图").first().click();
  await root.getByText("历史已更新，请刷新",{exact:true}).waitFor();
  await a.evaluate(() => { window.hnFault = "retry"; window.hnRelease = undefined; });
  await button(root,"刷新历史").click();
  await a.waitForFunction(() => typeof window.hnRelease === "function");
  const revoked = b.locator("article.device-card").filter({has:b.getByRole("heading",{name:"HN a",exact:true})});
  await button(revoked,"撤销此客户端").click();
  await button(b,"确认撤销").click();
  await a.locator("#pairing-code").waitFor({state:"visible"});
  await a.evaluate(() => { window.hnReplay(); window.hnRelease(); window.hnLateSnapshot?.(); });
  assert.equal(await history(a).count(),0);
  assert.doesNotMatch(await a.locator("body").innerText(),/HN (edited|new) turn 12|历史已更新，请刷新/);
  report.steps.push("HN-030 revoked reader returned to pairing and late frames left protected content absent");
  }
  report.status = "passed";
} catch (error) {
  report.failureType = error.name;
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
