import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";

const DEFAULT_PLAYWRIGHT =
  "C:/Users/jack/AppData/Local/npm-cache/_npx/e41f203b7505f1fb/node_modules/playwright/index.mjs";
const DEFAULT_EDGE =
  "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

function parseConfigPath(argv) {
  for (let i = 0; i < argv.length; i += 1) {
    if (argv[i] === "--config" && i + 1 < argv.length) {
      return argv[i + 1];
    }
  }
  throw new Error("Missing --config <path>");
}

async function waitForFile(filePath, timeoutMs = 45_000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (existsSync(filePath)) {
      return JSON.parse(readFileSync(filePath, "utf8"));
    }
    await sleep(40);
  }
  throw new Error(`Timed out waiting for ${filePath}`);
}

function assert(cond, msg) {
  if (!cond) {
    throw new Error(`Assertion failed: ${msg}`);
  }
}

function waitingQueueSection(page) {
  return page.locator('h4:text-is("等待队列")').locator("..");
}

async function waitBanner(page, state, epoch, textIncludes) {
  await page.waitForFunction(
    ([expectedState, expectedEpoch, needle]) => {
      const banner = document.querySelector(".session-control-banner");
      if (!banner) return false;
      const actualState = banner.getAttribute("data-control-state");
      const actualEpoch = banner.getAttribute("data-control-epoch");
      const txt = banner.textContent ?? "";
      return (
        actualState === expectedState &&
        actualEpoch === expectedEpoch &&
        (!needle || txt.includes(needle))
      );
    },
    [state, epoch, textIncludes],
    { timeout: 30_000 },
  );
}

async function pairClient(page, origin, code, displayName) {
  await page.goto(origin, { waitUntil: "load", timeout: 30_000 });
  await page.locator("#pairing-code").waitFor({ state: "visible", timeout: 20_000 });
  await page.locator("#pairing-code").fill(code);
  await page.locator("#display-name").fill(displayName);
  await page.locator('button:has-text("认领这台浏览器")').click();
  await page.locator("#session-target-id").waitFor({ state: "visible", timeout: 30_000 });
}

const configPath = parseConfigPath(process.argv.slice(2));
const cfg = JSON.parse(readFileSync(configPath, "utf8"));
const playwrightEntry = cfg.playwrightEntry || DEFAULT_PLAYWRIGHT;
const edgeExecutable = cfg.edgeExecutable || DEFAULT_EDGE;
mkdirSync(cfg.workDir, { recursive: true });
mkdirSync(cfg.peerStateDir, { recursive: true });

let browser;
let contextA;
let contextB;
const evidence = {
  caseId: "SCT-016",
  origin: cfg.origin,
  steps: {},
  pageErrorsA: [],
  pageErrorsB: [],
  status: "failed",
};

try {
  const { chromium } = await import(pathToFileURL(playwrightEntry).href);
  browser = await chromium.launch({ executablePath: edgeExecutable, headless: true });

  contextA = await browser.newContext({
    viewport: { width: 1440, height: 1100 },
    ignoreHTTPSErrors: true,
  });
  contextB = await browser.newContext({
    viewport: { width: 1440, height: 1100 },
    ignoreHTTPSErrors: true,
  });
  const pageA = await contextA.newPage();
  const pageB = await contextB.newPage();
  pageA.on("pageerror", (e) => evidence.pageErrorsA.push(e.message));
  pageB.on("pageerror", (e) => evidence.pageErrorsB.push(e.message));

  const pairA = await waitForFile(path.join(cfg.workDir, "pairing-a.json"));
  await pairClient(pageA, cfg.origin, pairA.code, "Desktop A");
  writeFileSync(path.join(cfg.workDir, "pair-a-done.json"), "{}", "utf8");

  const pairB = await waitForFile(path.join(cfg.workDir, "pairing-b.json"));
  await pairClient(pageB, cfg.origin, pairB.code, "Laptop B");
  evidence.steps.pairing = "passed";

  // Setup Workspace, Controlled ACP V2 Harness, Source Session S1 & Target T1 on Page A
  await pageA.locator("#catalog-workspace-path").fill(cfg.workspaceDir);
  await pageA.locator('button:has-text("登记工作目录")').click();
  await pageA.locator('button:has-text("填入 Target / Source 表单")').waitFor({ state: "visible", timeout: 20_000 });
  await pageA.locator('button:has-text("填入 Target / Source 表单")').click();

  await pageA.locator("#catalog-harness-name").fill("Controlled ACP V2");
  await pageA.locator("#catalog-source-name").fill("E2E Source");
  await pageA.locator("#catalog-launch-executable").fill(process.execPath);
  await pageA.locator("#catalog-launch-args").fill(`${cfg.peerScript}\n--state-dir\n${cfg.peerStateDir}`);
  await pageA.locator("#catalog-recipe-wire").selectOption("2");
  await pageA.locator('button:has-text("上传并安装 Harness")').click();
  await pageA.locator('button:has-text("填入 Harness/Profile")').first().waitFor({ state: "visible", timeout: 20_000 });
  await pageA.locator('button:has-text("填入 Harness/Profile")').first().click();

  await pageA.locator('section[data-panel="source-session"] button:has-text("登记新建")').click();
  await pageA.locator('section[data-panel="source-session"] button:has-text("执行操作")').waitFor({ state: "visible", timeout: 20_000 });
  await pageA.locator('section[data-panel="source-session"] button:has-text("执行操作")').click();
  await pageA.locator('section[data-panel="source-session"] button:has-text("读取结果")').waitFor({ state: "visible", timeout: 25_000 });
  await pageA.locator('section[data-panel="source-session"] button:has-text("读取结果")').click();

  const createdNativeEl = pageA.locator('.detail-block:has(.detail-label:text-is("已创建原生会话 ID")) code');
  await createdNativeEl.waitFor({ state: "visible", timeout: 20_000 });
  const nativeSessionId = (await createdNativeEl.textContent())?.trim() ?? "";
  assert(nativeSessionId.startsWith("e2e-acp-session-"), `unexpected nativeSessionId: ${nativeSessionId}`);

  await pageA.locator("#catalog-target-native-id").fill(nativeSessionId);
  await pageA.locator('button:has-text("登记并绑定 Target")').click();
  const targetCard = pageA.locator(".device-card", { hasText: nativeSessionId });
  await targetCard.waitFor({ state: "visible", timeout: 20_000 });
  const targetId = (await targetCard.locator('.detail-block:has(.detail-label:text-is("Target ID")) code').textContent())?.trim() ?? "";
  assert(targetId.length === 22, `unexpected targetId: ${targetId}`);
  await targetCard.locator('button:has-text("载入队列")').click();
  await pageA.locator('button:has-text("打开共享视图")').first().click();

  await waitBanner(pageA, "self", "1", "当前由本设备接管 · 代际 1");
  assert(await pageA.locator('button:has-text("释放控制权")').first().isEnabled(), "release button enabled on A");

  // Substep 1: hold prompt + emit approval; submit P1 & queued P2 on A; B observes readonly
  writeFileSync(path.join(cfg.peerStateDir, "hold-prompt"), "1", "utf8");
  writeFileSync(path.join(cfg.peerStateDir, "emit-approval"), "1", "utf8");
  await pageA.locator("#session-input-body").fill("first streaming prompt");
  await pageA.locator('button:has-text("上传并提交")').click();
  await pageA.waitForFunction(
    () => (document.body.textContent ?? "").includes("E2E回复:"),
    null,
    { timeout: 20_000 },
  );
  await pageA.waitForFunction(
    () => {
      const btn = [...document.querySelectorAll("button")].find((x) => x.textContent === "上传并提交");
      return Boolean(btn && !btn.disabled);
    },
    null,
    { timeout: 20_000 },
  );
  await pageA.locator("#session-input-body").fill("queued second prompt");
  await pageA.locator('button:has-text("上传并提交")').click();
  await pageA.waitForFunction(
    () => document.querySelectorAll("[data-queue-entry]").length >= 2,
    null,
    { timeout: 20_000 },
  );
  await waitingQueueSection(pageA).locator("[data-queue-entry]").first().waitFor({ state: "visible", timeout: 20_000 });

  await pageB.locator("#session-target-id").fill(targetId);
  await pageB.locator('button:has-text("读取会话队列")').click();
  await pageB.locator("[data-queue-entry]").first().waitFor({ state: "visible", timeout: 20_000 });
  for (let attempt = 0; attempt < 10; attempt += 1) {
    if ((await waitingQueueSection(pageB).locator("[data-queue-entry]").count()) > 0) {
      break;
    }
    evidence.steps.queuePollAttempts = attempt + 1;
    await sleep(300);
    await pageB.locator('button:has-text("刷新此队列")').click();
    await pageB.locator("[data-queue-entry]").first().waitFor({ state: "visible", timeout: 20_000 });
  }
  await waitingQueueSection(pageB).locator("[data-queue-entry]").first().waitFor({ state: "visible", timeout: 20_000 });
  await pageB.locator('button:has-text("打开共享视图")').first().click();
  await waitBanner(pageB, "other", "1", "只读观察 · 当前由 Desktop A 接管");
  await pageB.waitForFunction(
    () => (document.body.textContent ?? "").includes("E2E回复:"),
    null,
    { timeout: 20_000 },
  );
  await pageB.locator('button:has-text("刷新审批列表")').click();
  await pageB.locator('button:has-text("读取此审批")').first().waitFor({ state: "visible", timeout: 20_000 });
  await pageB.locator('button:has-text("读取此审批")').first().click();
  await pageB.locator('button:has-text("allow_once")').first().waitFor({ state: "visible", timeout: 20_000 });

  assert(await pageB.locator('button:has-text("上传并提交")').isDisabled(), "submit disabled on B");
  assert(await pageB.locator('button[data-session-action="edit"]').first().isDisabled(), "edit disabled on B");
  assert(await pageB.locator('button[data-session-action="cancel"]').first().isDisabled(), "cancel disabled on B");
  assert(await pageB.locator('button[data-session-action="prioritize"]').first().isDisabled(), "prioritize disabled on B");
  assert(await pageB.locator('button:has-text("allow_once")').first().isDisabled(), "approval choice disabled on B");
  assert(await pageB.locator('button:has-text("顶号接管会话")').first().isEnabled(), "preempt button enabled on B");

  await waitingQueueSection(pageB).locator('button[data-session-action="body"]').first().click();
  await pageB.locator("#session-input-body").focus();
  await pageB.locator("#session-input-body").fill("readonly focus check");
  evidence.steps.substep1 = { targetId, nativeSessionId, status: "passed" };

  // Substep 2: Preserve A draft & cursor selection across B's preemptive takeover, then B edits & decides
  await pageA.evaluate(() => {
    const el = document.querySelector("#session-input-body");
    if (!el) throw new Error("missing #session-input-body on A");
    el.focus();
    el.value = "unsent draft hello world";
    el.dispatchEvent(new Event("input", { bubbles: true }));
    el.setSelectionRange(7, 12);
  });

  await pageB.locator('button:has-text("顶号接管会话")').first().click();
  await waitBanner(pageB, "self", "2", "当前由本设备接管 · 代际 2");
  await waitBanner(pageA, "other", "2", "只读观察 · 当前由 Laptop B 接管");
  assert(await pageA.locator('button:has-text("上传并提交")').isDisabled(), "submit disabled on A after preemption");

  const draftA = await pageA.evaluate(() => {
    const el = document.querySelector("#session-input-body");
    return el ? { value: el.value, start: el.selectionStart, end: el.selectionEnd } : null;
  });
  assert(
    draftA && draftA.value === "unsent draft hello world" && draftA.start === 7 && draftA.end === 12,
    `draft/selection lost on A: ${JSON.stringify(draftA)}`,
  );

  await waitingQueueSection(pageB).locator('button[data-session-action="edit"]').first().click();
  await pageB.locator("#session-edit-body").fill("edited by Laptop B");
  await pageB.locator('button:has-text("上传并保存编辑")').click();
  await pageB.waitForFunction(
    () => (document.body.textContent ?? "").includes("队列项编辑已结算"),
    null,
    { timeout: 20_000 },
  );

  await pageB.locator('button:has-text("allow_once")').first().click();
  await pageB.locator('button:has-text("提交已登记决定")').waitFor({ state: "visible", timeout: 20_000 });
  await pageB.locator('button:has-text("提交已登记决定")').click();
  const reply = await waitForFile(path.join(cfg.peerStateDir, "approval-reply.json"), 20_000);
  assert(reply?.result?.outcome?.optionId === "allow-once", `unexpected peer approval reply: ${JSON.stringify(reply)}`);
  rmSync(path.join(cfg.peerStateDir, "hold-prompt"), { force: true });
  evidence.steps.substep2 = { draftA, approvalReply: reply.result.outcome, status: "passed" };

  // Substep 3: Active release -> uncontrolled epoch 3 -> node restart -> epoch 0
  await pageB.locator('button:has-text("释放控制权")').first().click();
  await waitBanner(pageB, "uncontrolled", "3", "只读观察 · 当前无人接管 · 代际 3");
  await waitBanner(pageA, "uncontrolled", "3", "只读观察 · 当前无人接管 · 代际 3");

  writeFileSync(path.join(cfg.workDir, "request-restart.json"), "{}", "utf8");
  await waitForFile(path.join(cfg.workDir, "restart-done.json"), 45_000);

  for (const [label, p] of [["A", pageA], ["B", pageB]]) {
    await p.reload({ waitUntil: "load", timeout: 30_000 });
    await p.locator("#session-target-id").waitFor({ state: "visible", timeout: 30_000 });
    await p.locator("#session-target-id").fill(targetId);
    await p.locator('button:has-text("读取会话队列")').click();
    await waitBanner(p, "uncontrolled", "0", "只读观察 · 当前无人接管 · 代际 0");
    assert(await p.locator('button:has-text("上传并提交")').isDisabled(), `submit disabled after restart on ${label}`);
  }
  await pageA.locator('button:has-text("接管会话")').first().click();
  await waitBanner(pageA, "self", "1", "当前由本设备接管 · 代际 1");
  await waitBanner(pageB, "other", "1", "只读观察 · 当前由 Desktop A 接管");
  evidence.steps.substep3 = "passed";

  assert(evidence.pageErrorsA.length === 0, `pageErrorsA: ${evidence.pageErrorsA.join("; ")}`);
  assert(evidence.pageErrorsB.length === 0, `pageErrorsB: ${evidence.pageErrorsB.join("; ")}`);
  evidence.status = "passed";
} catch (err) {
  evidence.error = String(err?.stack || err);
  evidence.statusLinesA = await contextA?.pages()?.[0]?.evaluate(() => [...document.querySelectorAll(".status-line")].map((x) => x.textContent)).catch(() => []);
  evidence.statusLinesB = await contextB?.pages()?.[0]?.evaluate(() => [...document.querySelectorAll(".status-line")].map((x) => x.textContent)).catch(() => []);
  evidence.bodySnippetA = await contextA?.pages()?.[0]?.evaluate(() => (document.body.textContent ?? "").slice(0, 1200)).catch(() => "");
  evidence.bodySnippetB = await contextB?.pages()?.[0]?.evaluate(() => (document.body.textContent ?? "").slice(0, 1200)).catch(() => "");
  evidence.peerEvents = existsSync(path.join(cfg.peerStateDir, "peer-events.ndjson"))
    ? readFileSync(path.join(cfg.peerStateDir, "peer-events.ndjson"), "utf8")
    : "";
  throw err;
} finally {
  await contextA?.close().catch(() => {});
  await contextB?.close().catch(() => {});
  await browser?.close().catch(() => {});
  writeFileSync(cfg.evidenceOut, JSON.stringify(evidence, null, 2), "utf8");
}
if (evidence.status !== "passed") {
  process.exit(1);
}
