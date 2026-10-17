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
 * ## 本页按生产口径下发静态响应头（含 CSP）
 *
 * 假主机对**所有**响应下发与 `crates/server/src/transport/net/static_ui.rs` 的 `STATIC_HEADERS`
 * 逐字同值的五条（见该常量），因此这组头是否把页面拦坏，在本地就能被测到——此前本页不带任何
 * 安全响应头，内联样式形态在这里永远生效，「UI 完全没样式」只会在真实 Daemon 上出现。
 * 两处配套：页面错误收集脚本由内联改为同源外部文件（`script-src 'self'` 拦内联脚本），
 * 入口文档在 bundle 之前声明那份**必须是空的**同源样式表（RNW 复用它的 `.sheet`）。
 *
 * ## 等待语义：页面侧的就绪在 CDP 求值的**循环体内**等出来
 *
 * CDP 的 `Page.navigate` 返回的是「导航已发起」，紧随其后的 `Runtime.evaluate` 常常落在文档还没
 * 解析到入口文档里那些脚本的时刻：`window.__pageErrors`（外部脚本 `/page-errors.js` 建立）与
 * `window.__tp4`（bundle 建立）此时都还不存在。二者一律按「还没到」处理、在循环体内反复重读，
 * 直到就位；两个都没执行时，才由**等待预算耗尽**判定（见 `main()` 里那两个等待常量与求值表达式的注释）。
 *
 * 页面侧的这次求值抛出时，CDP 是把它当作 `exceptionDetails` **正常返回**的：`cdp.evaluate` 因此
 * resolve，外层那 6 次重试的 `catch` **不执行**，失败在循环之后一次性报出——重试只兜 CDP 传输层
 * 错误（求值超时、`Page.navigate` 被拒），**不会**重新导航。所以把瞬态未就绪当成立即失败，第一次
 * 求值就把整轮验证判死，没有任何重试会把它救回来。
 *
 * 但「bundle 已执行而收集器没执行」不是瞬态：入口文档里 `/page-errors.js` 排在 `/bundle.js` **之前**，
 * 两者都是解析器插入的经典脚本、按序执行，因此 `window.__tp4` 就位即蕴含收集器已执行。此刻
 * `window.__pageErrors` 不是数组就不是竞态，而是「永远不是」——收集器坏掉（脚本被删、被改坏、或
 * 又退回内联形态被 `script-src 'self'` 拦下）时必须当场判死，否则页面错误快照会退化成常量「（无）」，
 * 「页面里出了错」与「什么都没发生」重新变得不可区分。
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
import { createHash } from "node:crypto";
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
    // `global` 是 Expo 在生产 bundle 里提供的 shim；react-native-web 的 Animated 在模块求值期
    // 读取 `global.RN$Bridgeless`。本 runner 自己用 esbuild 打包、没有那层 shim，因此在这里补上
    // 等价定义，让 e2e 与生产环境的全局形态一致（不是放宽生产代码）。
    define: { "process.env.NODE_ENV": '"development"', global: "globalThis" },
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
 * 静态面的安全响应头：取值与 `crates/server/src/transport/net/static_ui.rs` 的 `STATIC_HEADERS`
 * （含它引用的 `CONTENT_SECURITY_POLICY` 常量原文）**逐字同值**。
 *
 * ## 为什么这里要抄一份
 *
 * 本文件在 Node 里起假主机，请求不经过 Rust 的托管层，这五个头不会「自动」出现。而**判据必须落在
 * 带这组头的面上**：CSP 由浏览器内核执行，缺了它，内联样式形态在本页照样生效，样式回归就永远
 * 测不出来（`ui-csp-inline-styles` 的 design D3 把这一层定为浏览器级判据的理由）。
 *
 * ## 改一处必须改两处
 *
 * 这是**第二份**声明（第一份是 `static_ui.rs` 的 `STATIC_HEADERS`）。取值逐字抄写，不做「大意
 * 相同」的改写：托管层改了策略（换指令集、增删一条头），这里必须跟着改，否则「e2e 全绿」证明的
 * 是另一套策略。`ui-csp-inline-styles` 本身**不改任何取值**，因此下面五条与那份声明一字不差。
 */
const STATIC_HEADERS = {
  "content-security-policy":
    "default-src 'self'; script-src 'self'; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'",
  "cache-control": "no-store",
  pragma: "no-cache",
  "referrer-policy": "no-referrer",
  "x-content-type-options": "nosniff",
};

/**
 * 入口文档里 `#react-native-stylesheet` 指向的那份同源样式表的**响应正文：必须为空**。
 *
 * ## 为什么一个字都不能写
 *
 * RNW 的 `createCSSStyleSheet('react-native-stylesheet')` 命中已存在的元素时直接复用它的 `.sheet`，
 * 紧接着 `createOrderedCSSStyleSheet` 会**遍历该表已有的规则**、按 `[stylesheet-group="N"]` 标记归组
 * （那是 RNW 为 SSR/hydration 保留的分支）。预置内容里只要有一条**没有标记**的规则，遍历就会走到
 * `groups[undefined].rules` 抛 TypeError——而且是在**模块求值期**抛，整页空白。
 *
 * 空表让那段遍历一次都不进：规则全部由 RNW 在运行时 `insertRule` 进来。所以这里不是「还没写」，
 * 而是「必须由运行时写」；往这个常量里加内容会 fail-loud 地把页面打空（不是静默无样式）。
 */
const RNW_STYLESHEET = "";

/**
 * 极简静态服务：入口文档、bundle、页面错误收集脚本与那份同源的**空**样式表。
 *
 * 所有响应按生产口径带同一组静态安全响应头（`STATIC_HEADERS`），json 端点也一样：托管层对静态面
 * 的每一条响应都下发它，本地假主机只有跟着发，「这组头下发之后页面是否仍然可用」才真的被测到。
 *
 * bundle **不内联进 HTML**：1.3MB 的内联脚本体积大，而且内联脚本要先过一遍 HTML 分词器
 * （脚本数据状态对 `<` 有专门的处理规则），当独立文件加载更接近真实页面的加载路径。
 * 页面错误收集脚本同理，且是**必须**外置的：`script-src 'self'` 会拦下内联脚本。
 */
function startServer({ html, bundle, pageErrors }) {
  // 本轮**最后一次** WebSocket 升级请求里的两件事：请求行里的路径与 `Sec-WebSocket-Protocol`
  // 的原始值。
  //
  // 这是这两件事唯一能被观察到的地方：升级是浏览器内核发出的 HTTP 请求，页面里看不到自己的
  // 出站头与请求行（`WebSocket.protocol` 只在服务端**选择**了某个 subprotocol 之后才非空，
  // 缺失该头时两边都不留痕）。于是由 Node 侧的服务端记录原始请求，页面侧再取回来断言。
  let observedUpgrade = { path: null, subprotocol: null };
  const upgradedSockets = new Set();
  const server = createServer((request, response) => {
    if ((request.url ?? "").startsWith("/observed-upgrade-headers")) {
      response.writeHead(200, { ...STATIC_HEADERS, "content-type": "application/json; charset=utf-8" });
      response.end(JSON.stringify(observedUpgrade));
      return;
    }
    if ((request.url ?? "").startsWith("/page-errors.js")) {
      response.writeHead(200, { ...STATIC_HEADERS, "content-type": "text/javascript; charset=utf-8" });
      response.end(pageErrors);
      return;
    }
    if ((request.url ?? "").startsWith("/rnw-stylesheet.css")) {
      // 内容类型必须与托管层一致（`static_ui::MIME_TYPES` 的 `css`）：不是 `text/css` 时浏览器会
      // 拒绝这张表，RNW 读到 `sheet === null`（乃至读 `cssRules` 抛 SecurityError，整页空白）。
      response.writeHead(200, { ...STATIC_HEADERS, "content-type": "text/css; charset=utf-8" });
      response.end(RNW_STYLESHEET);
      return;
    }
    if ((request.url ?? "").startsWith("/bundle.js")) {
      response.writeHead(200, { ...STATIC_HEADERS, "content-type": "text/javascript; charset=utf-8" });
      response.end(bundle);
      return;
    }
    response.writeHead(200, { ...STATIC_HEADERS, "content-type": "text/html; charset=utf-8" });
    response.end(html);
  });
  // 极简 RFC 6455 握手：只为把**出站请求头**记录下来。校验 `Sec-WebSocket-Accept`
  // 的算式与浏览器内核相同，因此页面里那条真实 `WebSocket` 会正常 open。
  server.on("upgrade", (request, socket, head) => {
    void head;
    const requested = request.headers["sec-websocket-protocol"];
    observedUpgrade = {
      // 请求行里的路径（`GET /sync/v1 HTTP/1.1`）：让「出站地址的权威端点」能从**真实连接**
      // 上被断言，而不是只断言源码里的常量。
      path: typeof request.url === "string" ? request.url : null,
      subprotocol: typeof requested === "string" ? requested : null,
    };
    const key = request.headers["sec-websocket-key"];
    if (typeof key !== "string") {
      socket.destroy();
      return;
    }
    const accept = createHash("sha1").update(`${key}258EAFA5-E914-47DA-95CA-C5AB0DC85B11`).digest("base64");
    const lines = [
      "HTTP/1.1 101 Switching Protocols",
      "Upgrade: websocket",
      "Connection: Upgrade",
      `Sec-WebSocket-Accept: ${accept}`,
    ];
    // 回显请求里的 subprotocol（客户端只声明一个；回显非请求值会让内核主动断开）。
    const first = typeof requested === "string" ? requested.split(",")[0].trim() : "";
    if (first.length > 0) lines.push(`Sec-WebSocket-Protocol: ${first}`);
    socket.write(`${lines.join("\r\n")}\r\n\r\n`);
    upgradedSockets.add(socket);
    socket.on("close", () => upgradedSockets.delete(socket));
    // 断言只依赖上面记录的头，因此升级完成后立刻收线，避免残留 socket 拖住进程退出。
    setTimeout(() => socket.destroy(), 250);
  });
  return new Promise((resolveServer) => {
    server.listen(0, "127.0.0.1", () => {
      resolveServer({
        server,
        port: server.address().port,
        closeSockets: () => {
          for (const socket of upgradedSockets) socket.destroy();
          upgradedSockets.clear();
        },
      });
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
  // 页面级错误收集：脚本解析失败、未捕获异常与未处理拒绝都会进这里，
  // 否则「页面里什么都没发生」与「页面里出了错」在输出上长得一模一样。
  //
  // 语句与原先的内联版本一字不差（`window.__pageErrors` 数组 + 两个监听器），`support.tsx` 的读取
  // 语义因此不变；变的是**装载形态**：入口文档现在按生产口径带 `script-src 'self'`，内联脚本会被
  // 内核拒掉，收集器只能从同源文件引入。
  //
  // 它没执行时由下面的 `Runtime.evaluate` 说出来（「页面错误收集脚本未执行」），判定时机有两条：
  // bundle 已执行而它缺席就**当场**判死（入口文档里它排在 `/bundle.js` 之前，那不是竞态）；两者
  // 都没执行时，才等满**等待预算**再判——导航返回的是「导航已发起」，紧随其后的求值常常赶在这个
  // 外部脚本被解析/执行之前，此刻读到的是 `undefined`，含义是「还没到」。详见求值表达式里那段
  // 等待语义的注释。
  const pageErrors = [
    "window.__pageErrors=[];",
    "window.addEventListener(\"error\",function(e){window.__pageErrors.push(String(e.message||e.error))});",
    "window.addEventListener(\"unhandledrejection\",function(e){window.__pageErrors.push(\"rejection: \"+String(e.reason))});",
    "",
  ].join("\n");
  const html = [
    "<!doctype html><html><head><meta charset=\"utf-8\"><title>TP4 e2e</title>",
    // RNW 的样式表挂点：**必须是 bundle 之前的同源 `<link>`，且指向的样式表必须为空**。
    //
    // RNW 0.21 的 `StyleSheet` 在**模块求值期**执行 `createSheet()` →
    // `createCSSStyleSheet('react-native-stylesheet')`：先 `document.getElementById(id)`，
    // **命中就复用该元素的 `.sheet`**，查不到才 `document.createElement('style')`。后者在既定
    // CSP 下（`style-src` 与 `style-src-attr` 都回落到 `default-src 'self'`）`element.sheet === null`，
    // 此后 RNW 的每一次规则写入都被 `if (sheet != null)` 挡掉，全部原子类失效、UI 整表丢失；
    // 前者让这些规则落在一张**被允许的同源表**上。空表约束见 `RNW_STYLESHEET`。
    //
    // 位置是硬约束不是风格问题：RNW 取的是 `link.sheet`，样式表没加载完时它是 `null`，而 RNW
    // 既不会退回内联 `<style>`、也不会重试（`sheet` 在模块求值期就固定了）。这里 link 在 `<head>`、
    // 脚本是同文档里的解析器插入脚本，而解析器插入的样式表会**阻塞后续脚本执行**，因此脚本跑到时
    // 表已就绪。link 挪到 `<script>` 之后，本页就会退回「RNW 自己造 `<style>`」那条被拦下的路径。
    "<link id=\"react-native-stylesheet\" rel=\"stylesheet\" href=\"/rnw-stylesheet.css\">",
    "</head>",
    "<body><div id=\"root\"></div>",
    "<script src=\"/page-errors.js\"></script>",
    "<script src=\"/bundle.js\"></script>",
    "</body></html>",
  ].join("\n");
  writeFileSync(join(WORK_DIR, "index.html"), html, "utf8");
  writeFileSync(join(WORK_DIR, "bundle.js"), bundle, "utf8");
  writeFileSync(join(WORK_DIR, "page-errors.js"), pageErrors, "utf8");

  const { server, port, closeSockets } = await startServer({ html, bundle, pageErrors });
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
    closeSockets();
    server.close();
  };

  try {
    const targets = await fetchJson(`http://127.0.0.1:${debugPort}/json/list`);
    const page = targets.find((target) => target.type === "page" && target.webSocketDebuggerUrl);
    if (page === undefined) throw new Error("调试端口上没有页面目标");

    // 页面就绪的等待预算（下面求值表达式里的循环用它）：`Page.navigate` 返回的是「导航已发起」，
    // 入口文档里的两个页面侧全局（`window.__pageErrors` 由外部脚本 `/page-errors.js` 建立、
    // `window.__tp4` 由 bundle 建立）都可能在这一刻还没就位，因此二者都由**循环体内**重读来等，
    // 预算耗尽之后两者仍然缺席才是「脚本永远不执行」那种失败；而「bundle 已执行、收集器却缺席」
    // 在循环体内当场判死，不归这里管。调小它会把慢机器上的正常加载判成失败。
    const READY_POLL_INTERVAL_MS = 50;
    const READY_TIMEOUT_MS = 20_000;

    let evaluated;
    for (let attempt = 0; attempt < 6; attempt += 1) {
      const cdp = connectCdp(page.webSocketDebuggerUrl);
      try {
        await cdp.ready;
        // 明确导航一次：浏览器启动时可能先开出一个空白页，`/json/list` 的第一个
        // page 目标未必就是我们要的那一个。不导航就会出现「页面里什么都没有」的假象。
        await cdp.navigate(pageUrl);
        evaluated = await cdp.evaluate(`(async () => {
          // ── 页面侧的就绪**全部**在这个循环里等出来 ──────────────────────────────────
          //
          // Page.navigate 返回的是「导航已发起」，紧随其后的这次求值常常落在文档还没解析到
          // /page-errors.js 的时刻：那是解析器插入的外部脚本，浏览器要先取回再执行。此刻
          // window.__pageErrors 还不存在，含义是「还没到」，不是「永远不会有」。
          //
          // 所以每一轮都**重新读**页面全局，而不是进循环前读一次。读一次就把瞬态未就绪当成了
          // 「该脚本不执行」：页面侧的这个求值抛出时，CDP 是把它当作 exceptionDetails **正常返回**
          // 的（外层那 6 次重试只兜 CDP 传输层错误，不会重新导航），于是第一次求值就把整轮验证判死。
          // 「收集器与 bundle 都没执行」只能由**循环耗尽**来判定，见循环之后那条消息；而「bundle 已执行
          // 却读不到收集器的数组」不可能是竞态，由下面 runChecks 分支当场判死。
          const deadline = Date.now() + ${READY_TIMEOUT_MS};
          for (;;) {
            // 就绪判据是「收集脚本真的建立了它的数组」：不是数组（含 undefined）一律按「还没到」
            // 处理——既不据此抛错，也不去读它的 length（在读到的值不是数组时会抛 TypeError，
            // 正是这里要避免的那种「拿未就绪当硬失败」）。
            const pageErrors = window.__pageErrors;
            if (Array.isArray(pageErrors) && pageErrors.length > 0) {
              throw new Error("页面里有错误：" + pageErrors.join(" | "));
            }
            if (typeof window.__tp4?.runChecks === "function") {
              // bundle 已执行：入口文档里 /page-errors.js 排在它之前，解析器插入的脚本按序执行，
              // 因此此刻 window.__pageErrors 还不是数组就不是竞态，而是「永远不是」。
              if (!Array.isArray(window.__pageErrors)) {
                throw new Error("页面错误收集脚本未执行（/page-errors.js 未加载或被 CSP 拦下）：" +
                  "bundle 已执行，但 window.__pageErrors 不是数组（实际 " +
                  String(window.__pageErrors) + "）。");
              }
              return await window.__tp4.runChecks();
            }
            if (Date.now() >= deadline) break;
            await new Promise((r) => setTimeout(r, ${READY_POLL_INTERVAL_MS}));
          }
          // 到这里才是「等满预算仍然缺席」。两条诊断互斥，不要合并成一句：
          if (!Array.isArray(window.__pageErrors)) {
            throw new Error("页面错误收集脚本未执行（/page-errors.js 未加载或被 CSP 拦下）：" +
              "等待 ${READY_TIMEOUT_MS}ms 之后 window.__pageErrors 仍不是数组（实际 " +
              String(window.__pageErrors) + "）。" +
              "这条诊断的是「脚本从未执行」；导航刚发起时读到 undefined 只是「还没解析到它」，" +
              "那种情况由上面的循环继续等，不算失败。");
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