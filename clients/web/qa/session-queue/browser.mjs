import assert from "node:assert/strict";
import crypto from "node:crypto";
import fs from "node:fs";
import http from "node:http";
import path from "node:path";
import { pathToFileURL } from "node:url";

const root = path.resolve(import.meta.dirname, "../..");
const out = process.env.SESSION_QUEUE_QA_OUT;
const playwrightEntry = process.env.SESSION_QUEUE_PLAYWRIGHT;
const edge = process.env.SESSION_QUEUE_EDGE;
if (!out || !playwrightEntry || !edge) {
  throw new Error(
    "Set SESSION_QUEUE_QA_OUT, SESSION_QUEUE_PLAYWRIGHT and SESSION_QUEUE_EDGE",
  );
}
if (fs.existsSync(out)) {
  throw new Error("SESSION_QUEUE_QA_OUT must be a new directory");
}
fs.mkdirSync(out, { recursive: true });
const temp = path.join(out, "browser-tmp");
fs.mkdirSync(temp);
process.env.TEMP = temp;
process.env.TMP = temp;

const hash = (file) =>
  crypto.createHash("sha256").update(fs.readFileSync(file)).digest("hex");
const sourceFiles = [
  "src/main.ts",
  "src/management/session-protocol.ts",
  "src/management/session-queue-view.ts",
  "src/management/session-resume.ts",
  "tests/browser/session-queue-development.html",
  "tests/browser/session-queue-development.ts",
  "qa/session-queue/browser.mjs",
  "package.json",
  "package-lock.json",
];
const report = {
  scope:
    "Actual Edge DOM rendering and click path from production session queue view and resume coordinator; local synthetic snapshots, no Native/WSS/Harness",
  source: sourceFiles.map((file) => ({
    path: file,
    sha256: hash(path.join(root, file)),
  })),
  tool: {
    node: process.version,
    playwright: {
      entry: path.resolve(playwrightEntry),
      sha256: hash(playwrightEntry),
    },
    browser: {
      executable: path.resolve(edge),
      sha256: hash(edge),
    },
  },
  cases: [],
  external: [],
  pageErrors: [],
  cleanup: {},
};

const dist = path.join(out, "browser-dist");
const { build } = await import(
  pathToFileURL(path.join(root, "node_modules/vite/dist/node/index.js")).href
);
await build({
  root,
  configFile: false,
  build: {
    outDir: dist,
    emptyOutDir: false,
    rollupOptions: {
      input: path.join(root, "tests/browser/session-queue-development.html"),
    },
  },
});

const types = new Map([
  [".html", "text/html; charset=utf-8"],
  [".js", "text/javascript; charset=utf-8"],
  [".css", "text/css; charset=utf-8"],
]);
const server = http.createServer((request, response) => {
  try {
    const pathname = decodeURIComponent(
      new URL(request.url ?? "/", "http://127.0.0.1").pathname,
    );
    const candidate = path.resolve(dist, `.${pathname}`);
    if (candidate !== dist && !candidate.startsWith(`${dist}${path.sep}`)) {
      response.writeHead(403).end();
      return;
    }
    const file = fs.statSync(candidate).isDirectory()
      ? path.join(candidate, "index.html")
      : candidate;
    response.writeHead(200, {
      "content-type": types.get(path.extname(file)) ?? "application/octet-stream",
      "cache-control": "no-store",
    });
    response.end(fs.readFileSync(file));
  } catch {
    response.writeHead(404).end();
  }
});
await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
const port = server.address().port;
const origin = `http://127.0.0.1:${port}`;

let browser;
let context;
let page;
const run = async (name, operation) => {
  try {
    const detail = await operation();
    report.cases.push({ name, status: "passed", detail });
  } catch (error) {
    report.cases.push({
      name,
      status: "failed",
      error: String(error?.stack ?? error),
    });
  }
};

try {
  const { chromium } = await import(pathToFileURL(playwrightEntry).href);
  browser = await chromium.launch({ executablePath: edge, headless: true });
  context = await browser.newContext({
    viewport: { width: 1280, height: 900 },
    reducedMotion: "reduce",
    serviceWorkers: "block",
  });
  await context.route("**/*", async (route) => {
    const url = new URL(route.request().url());
    if (url.origin === origin) {
      await route.continue();
    } else {
      report.external.push(route.request().url());
      await route.abort();
    }
  });
  page = await context.newPage();
  page.setDefaultTimeout(5_000);
  page.on("pageerror", (error) => report.pageErrors.push(String(error)));
  await page.goto(
    `${origin}/tests/browser/session-queue-development.html`,
  );
  await page.waitForFunction(() => window.sessionQueueDevelopment !== undefined);

  const labels = [
    ["max_tokens", "达到 token 上限"],
    ["max_turn_requests", "达到请求轮数上限"],
    ["refusal", "模型拒绝"],
    ["cancelled", "用户取消"],
  ];
  for (const [reason, label] of labels) {
    await run(`renders ${reason} and keeps X/Z paused`, async () => {
      const result = await page.evaluate(
        (value) => window.sessionQueueDevelopment.showStopReason(value),
        reason,
      );
      assert.match(result.text, new RegExp(label));
      assert.match(result.text, /执行已停止/);
      assert.equal(result.waiting.length, 2);
      assert.match(result.waiting[0], /已暂停，需显式继续/);
      assert.match(result.waiting[1], /已暂停，需显式继续/);
      return { reason, label, waiting: result.waiting };
    });
  }

  const priorityFailures = [
    ["selection_changed", "已选择立即发送的旧项已变更或失效", /未回退发送其他输入/],
    ["interrupt_unconfirmed", "中断结果未确认", /未派发优先项或回退其他输入/],
  ];
  for (const [reason, label, noFallback] of priorityFailures) {
    await run(`renders ${reason} without fallback`, async () => {
      const result = await page.evaluate(
        (value) => window.sessionQueueDevelopment.showPriorityFailure(value),
        reason,
      );
      assert.match(result.text, new RegExp(label));
      assert.match(result.text, noFallback);
      assert.equal(result.waiting.length, 2);
      assert.match(result.waiting[0], /已暂停，需显式继续/);
      assert.match(result.waiting[1], /已暂停，需显式继续/);
      return { reason, label, waiting: result.waiting };
    });
  }

  await run("retries resume with the same registration and consume identity", async () => {
    await page.evaluate(() => window.sessionQueueDevelopment.showResumeRetry());
    await page.locator('[data-session-action="resume"]').click();
    await page.waitForFunction(
      () => window.sessionQueueDevelopment.resumeState().operationStatus ===
        "failed:transport",
    );
    const failed = await page.evaluate(
      () => window.sessionQueueDevelopment.resumeState(),
    );
    assert.equal(failed.consumeAttempts, 1);
    assert.equal(failed.generatedIds, 1);
    assert.equal(await page.locator('[data-session-action="resume"]').isEnabled(), true);

    await page.locator('[data-session-action="resume"]').click();
    await page.waitForFunction(
      () => window.sessionQueueDevelopment.resumeState().operationStatus ===
        "succeeded:2",
    );
    const succeeded = await page.evaluate(
      () => window.sessionQueueDevelopment.resumeState(),
    );
    assert.equal(succeeded.consumeAttempts, 2);
    assert.equal(succeeded.generatedIds, 1);
    assert.deepEqual(succeeded.registrationCalls[0], succeeded.registrationCalls[1]);
    assert.deepEqual(succeeded.consumeCalls[0], succeeded.consumeCalls[1]);
    assert.equal(succeeded.consumeCalls[0][0], succeeded.consumeCalls[1][0]);
    assert.equal(succeeded.consumeCalls[0][1], succeeded.registrationCalls[0][1]);
    assert.equal(succeeded.consumeCalls[0][2], succeeded.consumeCalls[1][2]);
    return { failed, succeeded };
  });

  await page.screenshot({
    path: path.join(out, "session-queue-development.png"),
    fullPage: true,
  });
  report.screenshot = "session-queue-development.png";
} catch (error) {
  report.runnerFailure = {
    name: error?.name ?? "Error",
    message: String(error?.message ?? error),
    stack: String(error?.stack ?? error),
  };
} finally {
  if (context) {
    await context.close();
    report.cleanup.contextClosed = true;
  }
  if (browser) {
    await browser.close();
    report.cleanup.browserClosed = true;
  }
  await new Promise((resolve) => server.close(resolve));
  report.cleanup.serverClosed = !server.listening;
  fs.rmSync(temp, { recursive: true, force: true });
  report.cleanup.tempRemoved = !fs.existsSync(temp);
  report.passed = report.cases.filter((item) => item.status === "passed").length;
  report.failed = report.cases.filter((item) => item.status === "failed").length;
  report.ok =
    report.failed === 0 &&
    report.runnerFailure === undefined &&
    report.external.length === 0 &&
    report.pageErrors.length === 0 &&
    report.cleanup.contextClosed === true &&
    report.cleanup.browserClosed === true &&
    report.cleanup.serverClosed === true &&
    report.cleanup.tempRemoved === true;
  fs.writeFileSync(
    path.join(out, "report.json"),
    `${JSON.stringify(report, null, 2)}\n`,
  );
  console.log(
    JSON.stringify({
      ok: report.ok,
      passed: report.passed,
      failed: report.failed,
      runnerFailure: report.runnerFailure,
      cleanup: report.cleanup,
    }),
  );
  if (!report.ok) process.exitCode = 1;
}
