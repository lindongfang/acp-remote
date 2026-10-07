#!/usr/bin/env node
/**
 * 在真实 Chromium 内核里运行 `src/platform/testing/browser-checks.ts` 的全部断言。
 *
 * 归属 WP5b。这是 `src/platform/secure-storage.test.ts` 与 `testing/fake-indexeddb.ts`
 * 注释所指向的 runner：Node 侧没有 `indexedDB`，替身也**不**声称结构化克隆保真，
 * 因此「不可导出私钥经真实 IndexedDB 往返仍是 `extractable === false`」「真实
 * `crypto.subtle` 拒绝导出不可导出私钥」这两条只能在这里核对。
 *
 * 依赖：Node 内建的 `http`/`fs`/`os`/`child_process`/`fetch`/`WebSocket`，外加 `esbuild`
 * （打包 TS，产出写到系统临时目录）。`esbuild` 已在 `clients/app/package.json` 里**显式**声明为
 * devDependency——它此前只经 vite/vitest 间接安装，属未声明的传递依赖，靠字面路径加载，
 * 安装布局一变就会失败（见 `loadEsbuild` 的注释）。
 *
 * 用法（cwd = clients/app）：
 *   node scripts/run-browser-check.mjs
 *
 * 环境变量：
 *   BROWSER_PATH  指定 Chromium 内核可执行文件；缺省时按常见路径探测
 *                 （Edge / Chrome）。
 *
 * 退出码：全部断言通过为 0；任一失败或无法启动浏览器为 1。
 * 结果 JSON 同时写 $TMPDIR/wp5b-browser/browser-evidence.json。
 */

import { spawn } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const APP_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const ENTRY = join(APP_ROOT, "src", "platform", "testing", "browser-checks.ts");
const WORK_DIR = join(tmpdir(), "wp5b-browser");

const BROWSER_CANDIDATES = [
  process.env.BROWSER_PATH,
  "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe",
  "C:/Program Files/Microsoft/Edge/Application/msedge.exe",
  "C:/Program Files/Google/Chrome/Application/chrome.exe",
  "/usr/bin/chromium",
  "/usr/bin/chromium-browser",
  "/usr/bin/google-chrome",
  "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
].filter((candidate) => typeof candidate === "string");

function locateBrowser() {
  for (const candidate of BROWSER_CANDIDATES) {
    if (existsSync(candidate)) return candidate;
  }
  throw new Error("找不到 Chromium 内核浏览器；用 BROWSER_PATH 指定可执行文件");
}

/** 把 app 自带的 esbuild 打成自包含 IIFE。
 *
 * 用**模块解析**而不是字面路径：`join(APP_ROOT, "node_modules", "esbuild", "lib", "main.js")`
 * 这类写法把「esbuild 被提升到顶层」当成既定事实，而安装布局会随依赖树变化——
 * vitest 5 的依赖里不再有 esbuild，升上去就会 `ERR_MODULE_NOT_FOUND`（`e2e/run-e2e.mjs`
 * 的头部注释记的就是这次事故，`review-wp5b-r2` 也把它登记为遗留 P3）。
 * 先按裸说明符解析，失败再经 `createRequire` 从 `clients/app/package.json` 出发解析。
 * esbuild 现已在 `package.json` 里显式声明为 devDependency，不再是传递依赖。
 */
async function loadEsbuild() {
  try {
    return await import("esbuild");
  } catch {
    const require = createRequire(join(APP_ROOT, "package.json"));
    return await import(pathToFileURL(require.resolve("esbuild")).href);
  }
}

async function bundleChecks() {
  const esbuild = await loadEsbuild();
  const build = esbuild.build ?? esbuild.default?.build;
  const result = await build({
    entryPoints: [ENTRY],
    bundle: true,
    format: "iife",
    globalName: "__wp5b",
    platform: "browser",
    target: "es2022",
    write: false,
    logLevel: "silent",
  });
  const output = result.outputFiles?.[0]?.text;
  if (typeof output !== "string") throw new Error("esbuild 未产出 bundle");
  return output;
}

/** 极简静态文件服务：只服务临时目录里的 index.html。 */
function startServer(html) {
  const server = createServer((_request, response) => {
    response.writeHead(200, { "content-type": "text/html; charset=utf-8" });
    response.end(html);
  });
  return new Promise((resolveServer) => {
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      resolveServer({ server, port: address.port });
    });
  });
}

async function fetchJson(url, attempts = 60) {
  for (let attempt = 0; attempt < attempts; attempt += 1) {
    try {
      const response = await fetch(url);
      if (response.ok) return await response.json();
    } catch {
      // 浏览器尚未监听调试端口：短等后重试。
    }
    await new Promise((resolveWait) => setTimeout(resolveWait, 250));
  }
  throw new Error(`等待 ${url} 超时`);
}

/** 通过 CDP 在页面里求值；返回 JSON 化后的结果。 */
function connectCdp(webSocketDebuggerUrl) {
  const socket = new WebSocket(webSocketDebuggerUrl);
  let nextId = 1;
  const pending = new Map();
  const ready = new Promise((resolveOpen, rejectOpen) => {
    socket.addEventListener("open", () => resolveOpen());
    socket.addEventListener("error", () => rejectOpen(new Error("CDP 连接失败")));
  });
  socket.addEventListener("message", (event) => {
    const message = JSON.parse(String(event.data));
    const entry = pending.get(message.id);
    if (entry === undefined) return;
    pending.delete(message.id);
    if (message.error) entry.reject(new Error(JSON.stringify(message.error)));
    else entry.resolve(message.result);
  });
  return {
    ready,
    close: () => socket.close(),
    evaluate(expression) {
      const id = nextId;
      nextId += 1;
      const promise = new Promise((resolveEval, rejectEval) => {
        pending.set(id, { resolve: resolveEval, reject: rejectEval });
      });
      socket.send(
        JSON.stringify({
          id,
          method: "Runtime.evaluate",
          params: { expression, awaitPromise: true, returnByValue: true },
        }),
      );
      // 页面导航会销毁执行上下文并让该 id 永不返回：加超时，交给外层重试。
      return Promise.race([
        promise,
        new Promise((_resolveTimeout, rejectTimeout) =>
          setTimeout(() => rejectTimeout(new Error("CDP 求值超时")), 30_000),
        ),
      ]);
    },
  };
}

async function main() {
  const browserPath = locateBrowser();
  const bundle = await bundleChecks();
  mkdirSync(WORK_DIR, { recursive: true });
  const html = [
    "<!doctype html><html><head><meta charset=\"utf-8\"><title>WP5b browser checks</title></head>",
    "<body><script>",
    bundle,
    "</script></body></html>",
  ].join("\n");
  writeFileSync(join(WORK_DIR, "index.html"), html, "utf8");

  const { server, port } = await startServer(html);
  const pageUrl = `http://127.0.0.1:${port}/index.html`;
  const profileDir = mkdtempSync(join(tmpdir(), "wp5b-profile-"));
  const debugPort = 9333 + Math.floor(Math.random() * 200);
  const browser = spawn(
    browserPath,
    [
      "--headless=new",
      "--disable-gpu",
      `--remote-debugging-port=${debugPort}`,
      `--user-data-dir=${profileDir}`,
      "--no-first-run",
      "--no-default-browser-check",
      "--disable-extensions",
      pageUrl,
    ],
    { stdio: "ignore" },
  );

  const cleanup = () => {
    try {
      browser.kill("SIGKILL");
    } catch {
      // 已退出。
    }
    server.close();
  };

  try {
    // 页面首次导航会销毁早期执行上下文：等页面稳定后再连、并对求值做重试。
    const targets = await fetchJson(`http://127.0.0.1:${debugPort}/json/list`);
    const page = targets.find((target) => target.type === "page" && target.webSocketDebuggerUrl);
    if (page === undefined) throw new Error("调试端口上没有页面目标");

    let evaluated;
    for (let attempt = 0; attempt < 6; attempt += 1) {
      const cdp = connectCdp(page.webSocketDebuggerUrl);
      try {
        await cdp.ready;
        evaluated = await cdp.evaluate(`(async () => {
          for (let i = 0; i < 200; i += 1) {
            if (typeof window.__wp5b?.runBrowserChecks === "function") {
              return await window.__wp5b.runBrowserChecks();
            }
            await new Promise((r) => setTimeout(r, 50));
          }
          throw new Error("runBrowserChecks 未就位");
        })()`);
        cdp.close();
        break;
      } catch (error) {
        cdp.close();
        if (attempt === 5) throw error;
        await new Promise((resolveWait) => setTimeout(resolveWait, 400));
      }
    }

    if (evaluated?.exceptionDetails) {
      throw new Error(`页面内异常：${JSON.stringify(evaluated.exceptionDetails)}`);
    }
    const results = evaluated?.result?.value;
    if (!Array.isArray(results)) throw new Error("断言未返回结果数组");

    const evidence = { browser: browserPath, results };
    writeFileSync(join(WORK_DIR, "browser-evidence.json"), JSON.stringify(evidence, null, 2), "utf8");
    for (const result of results) {
      console.log(`${result.passed ? "PASS" : "FAIL"}  ${result.name}  —— ${result.detail}`);
    }
    const failed = results.filter((result) => !result.passed);
    console.log(`\n${results.length - failed.length}/${results.length} passed`);
    if (failed.length > 0 || results.length === 0) {
      console.error(`失败断言：${failed.map((result) => result.name).join("; ") || "（无结果）"}`);
      cleanup();
      process.exitCode = 1;
      return;
    }
    cleanup();
  } catch (error) {
    cleanup();
    throw error;
  }
}

main().catch((error) => {
  console.error(`browser check 运行失败：${error instanceof Error ? error.message : String(error)}`);
  process.exit(1);
});
