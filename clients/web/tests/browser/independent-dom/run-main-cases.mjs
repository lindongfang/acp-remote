import { createHash } from "node:crypto";
import { access, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import path from "node:path";
import { pathToFileURL } from "node:url";

const REPO = process.cwd();
const WEB_ROOT = path.join(REPO, "clients", "web");
const OUTPUT_DIR = path.join(REPO, "testing", "runs", "deliver-codex-session-loop", "2026-11-13-wv-main-dom-acceptance-v2", "execution-v4");
const PLAYWRIGHT_ENTRY = "C:/Users/jack/AppData/Local/npm-cache/_npx/e41f203b7505f1fb/node_modules/playwright/index.mjs";
const EDGE_EXECUTABLE = "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe";
const VIRTUAL_ORIGIN = "https://127.0.0.1:4174";
const CASES = ["WV13", "WV14", "WV15"];
const EXPECTED_TEXT = Object.fromEntries(CASES.map((name) => [name, `WV-${name.slice(2)} 通过`]));
const INPUTS = [
  "clients/web/src/main.ts",
  "clients/web/src/management/client.ts",
  "clients/web/src/management/protocol.ts",
  "clients/web/src/management/session-protocol.ts",
  "clients/web/src/management/session-view-display.ts",
  "clients/web/src/management/session-view-status.ts",
  "clients/web/tests/browser/session-view-main.html",
  "clients/web/tests/browser/session-view-main.ts",
  "clients/web/tests/browser/independent-dom/management-peer.ts",
  "clients/web/tests/browser/independent-dom/view-wire.ts",
  "clients/web/tests/browser/independent-dom/run-main-cases.mjs",
  "clients/web/tests/browser/independent-dom/vite.https.config.mjs",
  "clients/web/package.json",
  "clients/web/node_modules/vite/package.json",
  "C:/Users/jack/AppData/Local/npm-cache/_npx/e41f203b7505f1fb/node_modules/playwright/package.json",
];
const ALLOWED_PATH = (pathname) => pathname === "/tests/browser/session-view-main.html" ||
  pathname === "/@vite/client" || pathname.startsWith("/src/") ||
  pathname.startsWith("/tests/browser/session-view-main.ts") ||
  pathname.startsWith("/tests/browser/independent-dom/");
const HMR_STUB = "export function createHotContext(){return {accept(){},decline(){},dispose(){},invalidate(){},prune(){},on(){},send(){}}} export function updateStyle(id,content){let s=[...document.querySelectorAll('style[data-vite-dev-id]')].find(x=>x.dataset.viteDevId===id);if(!s){s=document.createElement('style');s.setAttribute('data-vite-dev-id',id);document.head.appendChild(s)}s.textContent=content} export function removeStyle(id){[...document.querySelectorAll('style[data-vite-dev-id]')].find(x=>x.dataset.viteDevId===id)?.remove()}";

const exists = async (target) => access(target).then(() => true, () => false);
const sha256 = async (file) => createHash("sha256").update(await readFile(file)).digest("hex");
const hashInputs = async () => Object.fromEntries(await Promise.all(INPUTS.map(async (file) => {
  const absolute = path.isAbsolute(file) ? file : path.join(REPO, file);
  return [file, await sha256(absolute)];
})));

if (!(await exists(path.join(WEB_ROOT, "package.json")))) throw new Error("Run from the repository root.");
if (await exists(OUTPUT_DIR)) throw new Error(`Refusing to overwrite existing output directory: ${OUTPUT_DIR}`);
await mkdir(OUTPUT_DIR, { recursive: false });
const privateDirs = {
  TEMP: path.join(OUTPUT_DIR, "private-temp"),
  TMP: path.join(OUTPUT_DIR, "private-tmp"),
  LOCALAPPDATA: path.join(OUTPUT_DIR, "private-localappdata"),
  VITE_CACHE: path.join(OUTPUT_DIR, "vite-cache"),
};
for (const key of ["TEMP", "TMP", "LOCALAPPDATA"]) {
  await mkdir(privateDirs[key], { recursive: false });
  process.env[key] = privateDirs[key];
}

let beforeSha256 = {};
let afterSha256 = {};
let vite;
let browser;
let primaryError;
const results = [];
const allBlockedRequests = [];
const lifecycle = { viteClosed: false, browserClosed: false, privateDirsRemoved: false };
let localOrigin = "";

try {
  beforeSha256 = await hashInputs();
  const [{ createServer }, { chromium }] = await Promise.all([
    import("vite"),
    import(pathToFileURL(PLAYWRIGHT_ENTRY).href),
  ]);
  vite = await createServer({
    configFile: false,
    root: WEB_ROOT,
    cacheDir: privateDirs.VITE_CACHE,
    logLevel: "error",
    server: { host: "127.0.0.1", port: 0, strictPort: true, hmr: false, open: false },
  });
  await vite.listen();
  const address = vite.httpServer?.address();
  if (address === null || typeof address !== "object" || address.address !== "127.0.0.1") {
    throw new Error("Vite asset server must bind exclusively to IPv4 loopback.");
  }
  localOrigin = `http://127.0.0.1:${address.port}`;
  browser = await chromium.launch({ executablePath: EDGE_EXECUTABLE, headless: true });

  for (const caseName of CASES) {
    const context = await browser.newContext({ viewport: { width: 1440, height: 1100 } });
    const pageErrors = [];
    const consoleErrors = [];
    const blockedRequests = [];
    let page;
    const row = { case: caseName, expectedText: EXPECTED_TEXT[caseName], status: "not_run", resultText: "", diagnostics: null, pageErrors, consoleErrors, blockedRequests };
    results.push(row);
    try {
      await context.route("**/*", async (route) => {
        const request = route.request();
        let requested;
        try { requested = new URL(request.url()); }
        catch {
          blockedRequests.push("<invalid-url>");
          await route.abort("blockedbyclient");
          return;
        }
        if (requested.origin !== VIRTUAL_ORIGIN || !["GET", "HEAD"].includes(request.method()) || !ALLOWED_PATH(requested.pathname)) {
          blockedRequests.push(`${request.method()} ${requested.origin}${requested.pathname}`);
          await route.abort("blockedbyclient");
          return;
        }
        if (requested.pathname === "/@vite/client") {
          await route.fulfill({ status: 200, contentType: "text/javascript; charset=utf-8", body: HMR_STUB });
          return;
        }
        const upstreamUrl = `${localOrigin}${requested.pathname}${requested.search}`;
        try {
          const upstream = await fetch(upstreamUrl, { method: request.method(), redirect: "manual" });
          if (upstream.status >= 300 && upstream.status < 400) {
            blockedRequests.push(`upstream redirect refused: ${requested.pathname} -> ${upstream.headers.get("location") ?? "<missing>"}`);
            await route.abort("blockedbyclient");
            return;
          }
          const body = request.method() === "HEAD" ? Buffer.alloc(0) : Buffer.from(await upstream.arrayBuffer());
          const headers = Object.fromEntries(upstream.headers.entries());
          for (const header of ["connection", "content-encoding", "content-length", "keep-alive", "set-cookie", "transfer-encoding"]) delete headers[header];
          await route.fulfill({ status: upstream.status, headers, body });
        } catch (error) {
          blockedRequests.push(`local asset proxy error: ${error instanceof Error ? error.message : String(error)}`);
          await route.abort("failed");
        }
      });
      await context.addInitScript(() => {
        if (location.origin !== "https://127.0.0.1:4174" || !globalThis.isSecureContext ||
            !globalThis.crypto?.subtle || !globalThis.indexedDB) {
          throw new Error("expected synthetic secure origin with WebCrypto and IndexedDB");
        }
        window.__nativeWebSocketAttempts = 0;
        const NativeWebSocket = window.WebSocket;
        window.WebSocket = class extends NativeWebSocket {
          constructor(...args) {
            window.__nativeWebSocketAttempts += 1;
            throw new Error("native WebSocket is forbidden in synthetic-route acceptance");
          }
        };
      });
      page = await context.newPage();
      page.on("pageerror", (error) => pageErrors.push(error.message));
      page.on("console", (message) => { if (message.type() === "error") consoleErrors.push(message.text()); });
      await page.goto(`${VIRTUAL_ORIGIN}/tests/browser/session-view-main.html?case=${caseName}`, { waitUntil: "load", timeout: 30_000 });
      await page.waitForFunction((expected) => {
        const text = document.querySelector("#fixture-result")?.textContent ?? "";
        return text.includes(expected) || text.includes("未通过") || text.includes("夹具未启动");
      }, EXPECTED_TEXT[caseName], { timeout: 45_000 });
      row.resultText = (await page.locator("#fixture-result").textContent()) ?? "";
      row.diagnostics = await page.evaluate(() => window.__mainDomDiagnostics ?? null);
      row.nativeWebSocketAttempts = await page.evaluate(() => window.__nativeWebSocketAttempts);
      const screenshotPath = path.join(OUTPUT_DIR, `${caseName.toLowerCase()}.png`);
      await page.screenshot({ path: screenshotPath, fullPage: true });
      row.screenshot = path.basename(screenshotPath);
      row.status = row.resultText.includes(EXPECTED_TEXT[caseName]) &&
        row.nativeWebSocketAttempts === 0 && pageErrors.length === 0 &&
        consoleErrors.length === 0 && blockedRequests.length === 0 ? "passed" : "failed";
    } catch (error) {
      row.status = "failed";
      row.error = error instanceof Error ? error.message : String(error);
      if (page !== undefined) {
        try {
          row.diagnostics = await page.evaluate(() => window.__mainDomDiagnostics ?? null);
        } catch { /* Preserve original error. */ }
        try {
          const screenshotPath = path.join(OUTPUT_DIR, `${caseName.toLowerCase()}-failure.png`);
          await page.screenshot({ path: screenshotPath, fullPage: true, timeout: 5_000 });
          row.screenshot = path.basename(screenshotPath);
        } catch { /* Preserve the original error; screenshot is best effort. */ }
      }
    } finally {
      allBlockedRequests.push(...blockedRequests.map((value) => `${caseName}: ${value}`));
      await context.close();
    }
  }
} catch (error) {
  primaryError = error instanceof Error ? `${error.name}: ${error.message}` : String(error);
} finally {
  for (const [name, closer] of [["browser", () => browser?.close()], ["vite", () => vite?.close()]]) {
    try {
      await closer();
      lifecycle[`${name}Closed`] = true;
    } catch (error) {
      const note = `${name} close failed: ${error instanceof Error ? error.message : String(error)}`;
      primaryError = primaryError === undefined ? note : `${primaryError}; ${note}`;
    }
  }
  try {
    afterSha256 = await hashInputs();
  } catch (error) {
    primaryError = primaryError === undefined ? `post-run hash failed: ${String(error)}` : `${primaryError}; post-run hash failed: ${String(error)}`;
  }
  const drift = Object.keys(beforeSha256).filter((file) => beforeSha256[file] !== afterSha256[file]);
  const inputsComplete = Object.keys(beforeSha256).length === INPUTS.length && Object.keys(afterSha256).length === INPUTS.length;
  const allCasesPassed = results.length === CASES.length && CASES.every((name, index) => results[index]?.case === name && results[index]?.status === "passed");
  const report = {
    createdAt: new Date().toISOString(),
    scope: "WV13/WV14/WV15 production-main DOM acceptance with synthetic transport; not TLS/WSS/Harness evidence",
    transport: "Playwright route.fulfill virtual HTTPS origin; fixed local loopback Vite asset source; fixture FakeSocket; no external network or native WebSocket",
    browser: "Microsoft Edge via authorized Playwright executablePath; fresh context per case",
    virtualOrigin: VIRTUAL_ORIGIN,
    localAssetOrigin: localOrigin || null,
    viteBind: "127.0.0.1 ephemeral port; HMR disabled and /@vite/client stubbed; dedicated cacheDir",
    ignoreHTTPSErrors: false,
    processLocalTempDirs: privateDirs,
    blockedRequests: allBlockedRequests,
    inputSha256Before: beforeSha256,
    inputSha256After: afterSha256,
    inputDrift: drift,
    inputsComplete,
    results,
    primaryError: primaryError ?? null,
    lifecycle,
    overall: primaryError === undefined && inputsComplete && drift.length === 0 && allCasesPassed ? "passed" : "failed",
  };
  try {
    for (const target of [privateDirs.TEMP, privateDirs.TMP, privateDirs.LOCALAPPDATA, privateDirs.VITE_CACHE]) {
      if (!target.startsWith(`${OUTPUT_DIR}${path.sep}`)) throw new Error(`Refusing cleanup outside run output: ${target}`);
      await rm(target, { recursive: true, force: true });
    }
    lifecycle.privateDirsRemoved = true;
  } catch (error) {
    report.primaryError = `${report.primaryError ? `${report.primaryError}; ` : ""}private directory cleanup failed: ${String(error)}`;
    report.overall = "failed";
  }
  await writeFile(path.join(OUTPUT_DIR, "results.json"), `${JSON.stringify(report, null, 2)}\n`, "utf8");
  if (report.overall !== "passed") {
    console.error(JSON.stringify({ overall: report.overall, primaryError: report.primaryError, results, inputDrift: drift }, null, 2));
    process.exitCode = 1;
  } else {
    console.log(JSON.stringify({ overall: report.overall, results: results.map(({ case: name, status, screenshot }) => ({ case: name, status, screenshot })) }, null, 2));
  }
}
