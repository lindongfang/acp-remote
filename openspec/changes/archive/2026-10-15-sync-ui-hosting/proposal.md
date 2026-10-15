<!-- 记录动机、范围、能力与影响；行为写 specs，方案写 design，协作写 plan。 -->

## Why

前两个变更做完后，Daemon 有了完整的 Sync 端点（配对、认证、订阅、快照、命令与事件），但**前端页面无处加载**：`server::transport::net` 的路由表只支持两类端点（WebSocket 升级与 POST），没有任何静态资源能力，`GET /` 与其余未注册路径一律 404。

而 PWA 必须与 Daemon **同源加载**——前端从自己的 canonical origin 反推 WebSocket 地址，没有任何 endpoint 配置项。因此「把前端交给浏览器」是「PWA 真正可用」的最后一块基础设施，也是 `SECURITY_DESIGN.md` §13.4「PWA 静态资源与 Daemon 同包发布，构建版本可检查」的落地。

## What Changes

**一、静态资源托管（页面挂根目录）**

- listener SHALL 在**根目录**提供前端静态资源：`GET /` 返回构建入口，`GET /<前端路由>`（如 `/pair`、`/hosts`、`/dir/<alias>`、`/session/<id>`）在磁盘上无同名文件时返回同一入口（单页应用回退），`GET /<构建产物路径>` 返回对应资源。
- 挂根目录的三个依据（用户 2026-10-12 裁定）：① 根目录当前只有 404，占它不损失任何既有行为；② 配对二维码是 `https://<origin>/pair#data=...`（根相对），挂根目录无须改二维码生成；③ 前端编译产物的资源引用是根相对绝对路径（`/_expo/static/js/web/entry-*.js`），无须配 Expo 基路径。
- 路由解析顺序 SHALL 是可判定的：注册的协议 path（`/sync/v1*`、`/node-link/v1*`）**优先于**静态回退；静态回退**不吞**任何已注册 path，也不吞 `/sync/v1/pairing/claim` 这类 POST 端点。
- 未知且非回退范围的路径仍 MUST 返回 404；返回给浏览器的入口与资源 MUST 带正确的 `Content-Type`。
- 静态资源 MUST 携带协议要求的响应头，且 CSP 至少满足 `docs/FRONTEND_DESIGN.md` §4.5 的基线（`default-src 'self'; script-src 'self'; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'`）。

**二、资源嵌入方式（嵌入二进制）**

- 前端构建产物 SHALL **嵌入 Rust 二进制**（用户 2026-10-12 裁定），Daemon 不依赖磁盘上的旁路资源目录。此裁定关闭 `docs/FRONTEND_DESIGN.md` §11 的「静态资源嵌入二进制还是作为旁路资源安装」待决项。
- 构建过程 SHALL 可重复：`clients/app` 的构建产物进入 Rust 构建的输入集合，且「构建产物缺失时」的行为 MUST 是明确的（见 design 的取舍）。

**三、构建版本与 Daemon 可检查关联**

- Daemon SHALL 能回答它携带的前端构建版本，且该版本 MUST 与所托管的资源出自同一次构建；版本不一致时 MUST 能被检查出来（`docs/FRONTEND_DESIGN.md` §4.3 要求「前端构建版本必须与 Daemon 可检查地关联」）。
- 该可检查关联 SHALL 通过一个**可机器读取**的入口暴露，使 `daemon.status` 或等价查询能给出该版本，供运维与客户端诊断使用。

**四、文档同步**

- `docs/FRONTEND_DESIGN.md`：§4.3 的分发流程图从 `Daemon /ui` 改为根目录、写回「嵌入二进制」的裁定、并更新 §11 的暂缓项结论。
- `docs/CONFIG_REFERENCE.md` §1 的 listener 路由清单**补上根目录静态托管**（当前只列了 `/sync/v1`、`/node-link/v1`、两处 `/pairing/*`），并说明它与协议 path 的优先级关系。
- `docs/SECURITY_DESIGN.md`：静态资源的 CSP 基线与「同包发布、构建版本可检查」的实绩写回。
- `docs/MODULE_ARCHITECTURE.md` §4.9 与 `README.md` 的现状刷新。

**本次不包含**：

- 前端那 4 处接线缺口（连接路径 `/sync`→`/sync/v1`、WS subprotocol、`approved` 响应落 `hostPublicKey`、配对页扫码入口）→ 属 `frontend-sync-wiring`。
- 二维码 `#data=` 与前端路由的争用（若 `frontend-sync-wiring` 需要处理，在那里解决）。
- TLS 证书生成、反向代理部署脚本或任何部署产物 → 属部署决策，不是本变更的交付物（用户已裁定走同机反代终止 TLS）。

## Capabilities

### New Capabilities

- `ui-asset-hosting`: Daemon 同源托管前端静态资源 —— 根目录入口与单页回退、与协议 path 的解析优先级、内容类型与 CSP 基线、未知路径仍 404，以及前端构建版本与 Daemon 的可检查关联。

### Modified Capabilities

（无。本变更只在 listener 上新增一类能力，不改写既有规范的要求；`node-link-listener` 对 `/sync/v1` 的口径已由 `sync-server-connection` 改写完毕。）

## Impact

- **Rust**：`crates/server/src/transport/net/`（路由表新增一类「静态 GET」端点，或等价的回退中间件）、`crates/app/src/daemon.rs`（注册静态资源）、`crates/server/Cargo.toml` 与根 `Cargo.toml`（嵌入所用的依赖）。
- **依赖**：**新增一个 crate 依赖**用于把构建产物嵌入二进制（候选与理由是 design 的决策项；`AGENTS.md` §8 要求检查必要性、维护状态、许可证与平台支持）。
- **构建**：`clients/app` 的构建产物成为 Rust 构建输入；需明确「产物缺失」时的行为，且不得让普通 `cargo test` 在没有前端构建的环境下失败。
- **存储**：无改动。
- **机器合同**：预期零改动（`schemas/`、`fixtures/`、`compatibility/` 不变）；`npm run check` 必须全绿。
- **权威文档**：`docs/FRONTEND_DESIGN.md` §4.3/§11、`docs/CONFIG_REFERENCE.md` §1、`docs/SECURITY_DESIGN.md`（CSP 与同包发布）、`docs/MODULE_ARCHITECTURE.md` §4.9、`README.md`。
- **验收边界**：本变更交付「浏览器能加载页面」这一半；**端到端可用**仍要等 `frontend-sync-wiring` 修完连接路径与 subprotocol。因此本变更的浏览器级验收只覆盖「页面与资源能加载、CSP 生效、未知路径仍 404」，不覆盖「能连上 Daemon」。
