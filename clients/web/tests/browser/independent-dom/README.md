# Production main 浏览器夹具

该入口只可运行于专用可信 HTTPS origin `https://127.0.0.1:4174`，因为真实 `main.ts` 会拒绝 HTTP，并会清理该 origin 的 `acp-remote-access-v1` IndexedDB。请使用专用浏览器 profile 和仅供本地测试的、SAN 包含 `127.0.0.1` 且已被该 profile 信任的测试证书；不要使用产品证书，不要跳过证书警告，也不要指向其它身份库。

若本机已有该测试证书，可在 PowerShell 里设置路径并启动本地 Vite HTTPS server：

```powershell
$env:ACP_MAIN_FIXTURE_CERT_PATH = 'D:\path\to\trusted-test-cert.pem'
$env:ACP_MAIN_FIXTURE_KEY_PATH = 'D:\path\to\trusted-test-key.pem'
npm.cmd --prefix clients/web run dev -- --config tests/browser/independent-dom/vite.https.config.mjs --host 127.0.0.1 --port 4174 --strictPort
```

再从可信浏览器访问 `https://127.0.0.1:4174/tests/browser/session-view-main.html?case=WV13`、`?case=WV14` 或 `?case=WV15`。每个场景需在新页面加载后运行；夹具在这个专用 origin 下重置自己的合成身份。页面中的 `FakeSocket` 是内存中的协议 peer，不创建实际 WSS 连接；client key 不可导出，node signing key 是一次性合成测试密钥。该夹具验证真实 WebCrypto client/server proof、真实 IndexedDB/IdentityVault、生产 `main.ts`、`ManagementConnection`、binary decoder 和 DOM 生命周期；它不验证真实服务端、真实 Harness 或 CL-030/031/123。

当前 repo 未发现本地 `playwright`、`@playwright/test`、`puppeteer` 依赖/CLI、Chrome/Edge/Chromium 命令或 HTTPS 证书配置。会话提供 Playwright automation tool，可在本地 HTTPS server 和可信证书准备后用于驱动页面；本次未起 server、访问浏览器或网络。由主执行准备测试证书与可访问 origin 后，可用该 tool 执行三个 query 用例。



