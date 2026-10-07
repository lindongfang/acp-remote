#!/usr/bin/env node
/**
 * TP4 浏览器级验证入口：在真实 Chromium 内核里驱动**真实页面**（`app/_layout.tsx` 的
 * 组合根 + `app/` 的真实路由组件），核对 R15 / R16 / R18 的可观察行为。
 *
 * ## 为什么与 `scripts/run-browser-check.mjs` 并存而不是复用它
 *
 * 那个 runner（WP5b）跑的是 `src/platform/testing/browser-checks.ts`：平台端口的浏览器
 * 事实（IndexedDB 往返、`crypto.subtle` 拒绝导出不可导出私钥）。它的入口是**一组断言函数**，
 * 页面里没有 React、没有路由、没有 `useEffect`，因此它**无法**回答「组合根接线通不通」。
 * TP4 需要的是另一件事：把真实组件树挂进真实 DOM 并点按钮。两者的用例集合与被测面互不重叠，
 * 因此本文件自带打包、服务与 CDP 驱动，不改动既有 runner（它也不在本工作包的写入范围内）。
 *
 * ## 与既有 runner 的关键差异（也是它登记过的缺陷）
 *
 * `scripts/run-browser-check.mjs` 用**字面路径**加载 esbuild：
 * `join(APP_ROOT, "node_modules", "esbuild", "lib", "main.js")`。那是**未声明的传递依赖**
 * （esbuild 只经 vite/vitest 间接安装），安装布局一变就 `ERR_MODULE_NOT_FOUND`。
 * 本文件一律走**模块解析**：先按裸说明符 `import("esbuild")`，失败再按
 * `createRequire(...).resolve("esbuild")` 由 `clients/app/package.json` 出发解析，
 * 两处都不出现 `node_modules/esbuild/lib/main.js` 这样的字面路径。
 *
 * 用法（cwd = clients/app）：
 *   node e2e/run-e2e.mjs
 *
 * 环境变量：
 *   BROWSER_PATH  指定 Chromium 内核可执行文件；缺省时按常见路径探测。
 *
 * 退出码：全部检查通过为 0；任一失败或无法启动浏览器为 1。
 */

import { spawn } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const APP_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const E2E_ROOT = join(APP_ROOT, "e2e");
const ENTRY = join(E2E_ROOT, "page", "index.tsx");
const WORK_DIR = join(tmpdir(), "tp4-e2e");

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

/**
 * 解析 esbuild：**只用模块解析**，不拼 `node_modules/esbuild/...` 字面路径。
 *
 * 两条路径都声明了「解析基准」：`clients/app/package.json`。esbuild 是 vite/vitest 的
 * 传递依赖，布局会变（pnpm 的 `.pnpm/` 嵌套、npm 的 hoist），字面路径对两者都脆。
 */
async function resolveEsbuild() {
  try {
    return await import("esbuild");
  } catch {
    const require = createRequire(join(APP_ROOT, "package.json"));
    return await import(pathToFileURL(require.resolve("esbuild")).href);
  }
}

/**
 * 打包页面入口。
 * `expo-router` 被本目录的替身顶掉（`e2e/page/router-stub.tsx`）：真实路由需要
 * expo-router 的原生运行时与 `react-native-web`，而本工作包无权新增依赖。替身只提供
 * `Stack` / `useRouter` / `useLocalSearchParams` 三个页面用到的东西——**导航本身不是被测面**，
 * 被测的是组合根、feature 判定与展示组件。
 *
 * 刻意**不**设 `globalName`：它会让 esbuild 产出 `var <name> = (() => {…})()`，
 * 那个 `var` 会用 IIFE 的返回值（`undefined`）覆盖同名全局。若这个全局正好是页面挂
 * 检查用的 `window.__tp4`，页面就会静悄悄地「什么都没挂上」——本目录第一版就踩了这个坑。
 */
async function bundlePage() {
  const esbuild = await resolveEsbuild();
  const build = esbuild.build ?? esbuild.default?.build;
  const result = await build({
    entryPoints: [ENTRY],
    bundle: true,
    platform: "browser",
    target: "es2022",
    jsx: "automatic",
    define: { "process.env.NODE_ENV": '"development"' },
    plugins: [
      {
        name: "tp4-expo-router-stub",
        setup(buildApi) {
          buildApi.onResolve({ filter: /^expo-router$/ }, () => ({
            path: join(E2E_ROOT, "page", "router-stub.tsx"),
          }));
        },
      },
    ],
    write: false,
    logLevel: "silent",
  });
  const output = result.outputFiles?.[0]?.text;
  if (typeof output !== "string") throw new Error("esbuild 未产出 bundle");
  return output;
}

/**
 * 极简静态服务：`index.html` 与 `bundle.js`。
 *
 * bundle **不内联进 HTML**：1.3MB 的内联脚本体积大，而且内联脚本要先过一遍 HTML 分词器
 * （脚本数据状态对 `<` 有专门的处理规则），当独立文件加载更接近真实页面的加载路径。
 */
function startServer(html, bundle) {
  const server = createServer((request, response) => {
    if ((request.url ?? "").startsWith("/bundle.js")) {
      response.writeHead(200, { "content-type": "text/javascript; charset=utf-8" });
      response.end(bundle);
      return;
    }
    response.writeHead(200, { "content-type": "text/html; charset=utf-8" });
    response.end(html);
  });
  return new Promise((resolveServer) => {
    server.listen(0, "127.0.0.1", () => {
      resolveServer({ server, port: server.address().port });
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
    navigate(url) {
      const id = nextId;
      nextId += 1;
      const promise = new Promise((resolveNav, rejectNav) => {
        pending.set(id, { resolve: resolveNav, reject: rejectNav });
      });
      socket.send(JSON.stringify({ id, method: "Page.navigate", params: { url } }));
      return promise;
    },
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
      return Promise.race([
        promise,
        new Promise((_resolveTimeout, rejectTimeout) =>
          setTimeout(() => rejectTimeout(new Error("CDP 求值超时")), 180_000),
        ),
      ]);
    },
  };
}

async function main() {
  const browserPath = locateBrowser();
  const bundle = await bundlePage();
  mkdirSync(WORK_DIR, { recursive: true });
  const html = [
    "<!doctype html><html><head><meta charset=\"utf-8\"><title>TP4 e2e</title></head>",
    "<body><div id=\"root\"></div>",
    // 页面级错误收集：脚本解析失败、未捕获异常与未处理拒绝都会进这里，
    // 否则「页面里什么都没发生」与「页面里出了错」在输出上长得一模一样。
    "<script>window.__pageErrors=[];",
    "window.addEventListener(\"error\",function(e){window.__pageErrors.push(String(e.message||e.error))});",
    "window.addEventListener(\"unhandledrejection\",function(e){window.__pageErrors.push(\"rejection: \"+String(e.reason))});",
    "</script>",
    "<script src=\"/bundle.js\"></script>",
    "</body></html>",
  ].join("\n");
  writeFileSync(join(WORK_DIR, "index.html"), html, "utf8");
  writeFileSync(join(WORK_DIR, "bundle.js"), bundle, "utf8");

  const { server, port } = await startServer(html, bundle);
  const pageUrl = `http://127.0.0.1:${port}/index.html`;
  const profileDir = mkdtempSync(join(tmpdir(), "tp4-e2e-profile-"));
  const debugPort = 9733 + Math.floor(Math.random() * 200);
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
    const targets = await fetchJson(`http://127.0.0.1:${debugPort}/json/list`);
    const page = targets.find((target) => target.type === "page" && target.webSocketDebuggerUrl);
    if (page === undefined) throw new Error("调试端口上没有页面目标");

    let evaluated;
    for (let attempt = 0; attempt < 6; attempt += 1) {
      const cdp = connectCdp(page.webSocketDebuggerUrl);
      try {
        await cdp.ready;
        // 明确导航一次：浏览器启动时可能先开出一个空白页，`/json/list` 的第一个
        // page 目标未必就是我们要的那一个。不导航就会出现「页面里什么都没有」的假象。
        await cdp.navigate(pageUrl);
        evaluated = await cdp.evaluate(`(async () => {
          for (let i = 0; i < 400; i += 1) {
            if (window.__pageErrors.length > 0) {
              throw new Error("页面里有错误：" + window.__pageErrors.join(" | "));
            }
            if (typeof window.__tp4?.runChecks === "function") {
              return await window.__tp4.runChecks();
            }
            await new Promise((r) => setTimeout(r, 50));
          }
          throw new Error("runChecks 未就位；页面错误：" + window.__pageErrors.join(" | "));
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
    if (!Array.isArray(results)) throw new Error("检查未返回结果数组");

    writeFileSync(
      join(WORK_DIR, "e2e-evidence.json"),
      JSON.stringify({ browser: browserPath, results }, null, 2),
      "utf8",
    );
    for (const result of results) {
      console.log(`${result.passed ? "PASS" : "FAIL"}  ${result.name}  —— ${result.detail}`);
    }
    const failed = results.filter((result) => !result.passed);
    console.log(`\n${results.length - failed.length}/${results.length} passed`);
    if (failed.length > 0 || results.length === 0) {
      console.error(`失败检查：${failed.map((result) => result.name).join("; ") || "（无结果）"}`);
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
  console.error(`e2e 运行失败：${error instanceof Error ? error.message : String(error)}`);
  process.exit(1);
});