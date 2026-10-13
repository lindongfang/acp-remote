<!-- 本变更的方案：接口形状、接缝与取舍。动机见 proposal.md，行为见 specs/。 -->

# Design

## Context

动机见 `proposal.md`。前置是 `sync-server-connection` 与 `sync-server-commands`（Sync 端点已完整）。这里只记决定方案形状的现状与约束。

**现状事实（本次已核实）**

| 事实 | 证据 |
|---|---|
| 路由表只支持两类端点 | `crates/server/src/transport/net/route.rs:137-140` 的 `RouteTable { ws, post }` |
| 未注册路径一律 404，且有测试断言 | `route.rs:245` 的 `fallback(not_found)`；`net/tests.rs:301` 断言 `"/"` 必须 404 |
| 前端产物资源引用是**根相对绝对路径** | `clients/app/dist/index.html` 的 `src="/_expo/static/js/web/entry-*.js"` |
| 配对二维码是**根相对** | `crates/server/src/local_admin/pairing.rs:199` 的 `pairing_url(…, "/pair", …)` |
| 产物当前大小 | `clients/app/dist` 约 1.6 MB |
| 配置文档的 listener 清单**没有**根目录托管 | `docs/CONFIG_REFERENCE.md:46` 只列四个协议 path |
| CSP 基线 | `docs/FRONTEND_DESIGN.md:181`；`docs/SECURITY_DESIGN.md:328`「新增资源类型必须显式放行」 |
| 构建版本可检查 | `docs/FRONTEND_DESIGN.md:153`；`docs/SECURITY_DESIGN.md:520` |

**约束**

- `AGENTS.md` §8：新增依赖前必须检查必要性、维护状态、许可证、平台支持与安全风险。
- `AGENTS.md` §5：`server::transport::net` 只做连接与字节/帧规则，不解释协议语义；静态托管属接入面能力，放这里而不是新建模块。
- 普通 `cargo test` 必须在**没有前端构建产物**的开发机/CI 上也能通过 —— 否则门禁会被构建顺序绑死。
- 静态资源不承载任何 secret（`SECURITY_DESIGN.md` §13.4 的 `public-artifact` 类别）。

## Goals / Non-Goals

**Goals**

- 让 `GET /` 与前端路由深链能返回真实构建的页面，且 CSP 生效。
- 协议端点的行为与既有一字不差：静态能力是**纯增量**，不改变任何既有断言的含义。
- 构建产物缺失时行为明确，且不把「缺产物」变成测试环境下的失败。

**Non-Goals**

- 前端接线修正（属 `frontend-sync-wiring`）。
- TLS、证书、反代部署产物（部署决策，不是交付物）。
- 前端路由模式（井号路由 vs 正常路径）的最终裁定 —— 本变更只保证**两种都能被同源加载**，具体取舍留给前端变更。

## Decisions

### D1：路由表新增第三类「静态 GET」，而不是改成通配中间件

在 `RouteTable` 增加一类静态端点：一个「入口文档 + 嵌入资源集」的处理器，注册在**根路径**上。

**解析顺序**（可判定，写进 spec）：已注册的 ws/post path **优先**（`axum` 按精确 path 路由，天然优先）→ 命中嵌入资源 → 回退到入口文档 → 其余 404。

**备选与否决理由**

- *用 `tower_http::services::ServeDir`*：它服务磁盘目录，与本变更选的「嵌入二进制」冲突；而且需要额外依赖且要处理路径穿越。否决。
- *改成全局通配中间件*：会与既有的 `fallback(not_found)` 和 Host/代理头边界中间件的顺序纠缠，且「协议端点优先」将不再由路由表结构保证，而要手写优先级判断 —— 那是更容易出错的做法。否决。
- *注册多条精确路径（`/`、`/pair`、`/hosts`…）*：前端路由是变化的，每加一页就要改 Daemon。否决。

### D2：嵌入用 `rust-embed`，并在编译期失败关闭

选择 `rust-embed`：它把目录内容以 `include_bytes!` 形式编入二进制，运行期零 IO、零路径处理，天然免疫路径穿越（不存在的路径根本不在资源表里）。**候选对比**：

| 候选 | 理由 |
|---|---|
| `rust-embed` | 只做嵌入，无 HTTP 依赖；debug 模式下可选从磁盘读（便于前端迭代）；许可证宽松 |
| `include_dir` | 同样合格，但需要自己拼 MIME 表与回退逻辑；`rust-embed` 的 API 更直接 |
| `tower-http` 的 `ServeDir`/`ServeFile` | 服务磁盘，与「嵌入」裁定冲突 |

**MIME 类型**从嵌入资源的扩展名映射，映射表由本模块持有并测试覆盖（前端产物只有 html/js/css/json/图片/字体几类）。

**构建产物缺失时的行为**（关键取舍）：嵌入宏要求目录存在，否则编译失败。做法是**在 `clients/app/dist` 缺失时用一个占位入口**参与编译，使 `cargo test` 在没有前端构建的环境下仍能通过；而**发布构建**（或 `daemon start`）在检测到占位入口时以明确错误失败关闭，不静默提供一个空页面。

- *备选：嵌入宏直接指向 `dist/`，缺失即编译失败* —— 会让所有 Rust CI job 都必须先构建前端，把两个独立工程的构建顺序绑死，且 `docs/MODULE_ARCHITECTURE.md` §11 的测试约定没有这一前提。否决。
- *备选：运行期读磁盘目录* —— 与「嵌入二进制」的裁定冲突。否决。

### D3：构建版本通过前端注入的构建期常量承载

前端构建时把一个版本标识写进产物（`clients/app` 侧在 `app.json`/构建脚本里生成或从 `package.json`/git 描述派生），Daemon 侧从嵌入资源里读它并对外暴露。

**为什么不用「Daemon 自己算产物哈希」**：那能证明「资源没被改」，但不能回答「这是哪一次构建」，而 §4.3 要的是**可检查的关联**，即「这个 Daemon 携带的前端是哪个版本」——用于「刷新后仍拿到旧 JS 去连新 Daemon」这类诊断。

**暴露入口**：`daemon.status` 增加一个字段（组合根回答，不经业务用例层，与既有字段同形）。这样 CLI 与运行中的查询都能拿到，不需要新增本地管理方法（`LOCAL_ADMIN_PROTOCOL.md` 的方法集是封闭词表，新增方法要动机器合同）。

### D4：CSP 由接入层下发，取值不宽于基线

静态响应统一带 `Content-Security-Policy`，取 `docs/FRONTEND_DESIGN.md` §4.5 的基线原文。**必须由响应头下发**，不依赖文档内的元标记 —— 元标记无法约束部分指令，且响应头是 `docs/SECURITY_DESIGN.md` §11.1「新增资源类型必须显式放行」的落点。

**注意一个真实约束**：前端当前用内联 `<style id="expo-reset">`（`dist/index.html` 里能看到）。基线里没有 `style-src`，因此它会落到 `default-src 'self'`，**内联样式会被拒**。这需要在前端侧解决（把 reset 样式外置，或在前端变更里显式放行 `style-src`）——本变更**不**为了让页面好看而放宽基线，而是把这一冲突记录为已知项，交由 `frontend-sync-wiring` 处理。

### D5：不新增本地管理方法

构建版本只出现在 `daemon.status` 的既有响应里（加字段是 v1 允许的兼容变化），不新增方法。理由：`LOCAL_ADMIN_PROTOCOL.md` 的方法集与方法↔子命令映射是机器合同（`localCapabilities`），每加一个方法都要同步 schema、fixture、文档与命令目录；为一个只读诊断值付这个代价不值得。

## Risks / Trade-offs

- **[内联样式与 CSP 冲突]** Expo 生成的 `index.html` 含内联 `<style>`，基线 CSP 会拒它 → 本变更加 CSP 后**页面样式可能异常**；这不是本变更的实现缺陷，而是必须在前端侧修的一项。缓解：在变更报告中显式登记该冲突与现象，`frontend-sync-wiring` 必须处理（外置样式或显式放行）。**不**通过放宽 CSP 来掩盖。
- **[占位入口被误发布]** 缺产物时用占位入口编译，可能把「空页面」发出去 → 发布构建与 `daemon start` 检测到占位即失败关闭；测试环境才允许占位。
- **[嵌入增大二进制]** 约 1.6 MB 的增长（当前产物大小）→ 这是「嵌入」裁定的已知代价；若将来需要减小，正确路径是前端构建产物体积优化，不是改成磁盘旁路。
- **[新的 Rust 依赖]** `rust-embed` 进入供应链 → 按 `AGENTS.md` §8 检查必要性/维护状态/许可证/平台支持；CI 的 `deps` 与 `advisories` job 会各自判定（本地无等价物，不得声称本地已验证）。
- **[路由优先级回归]** 静态能力可能意外吞掉协议端点 → 既有测试已断言「`/` 与 `/unknown` 必须 404」，本变更必须**更新那些断言**为「`/` 返回入口文档、协议端点不受影响」，并新增「协议端点优先」的用例。删改既有断言必须说明理由。
- **[前端路由与二维码井号位置争用]** 配对二维码是 `/pair#data=...`，而前端某些路由注释写作 `#/pair` → 若前端走井号路由，两者争用同一个片段位置。本变更不裁定，但**必须在报告中登记**，因为它会直接影响 `frontend-sync-wiring` 的可用性。

## Migration Plan

- **schema 无变更**：不新增表、不推进文件格式版本，无 migration。
- **部署顺序**：纯增量。回滚即回到「根路径 404」，协议端点全程不受影响。
- **构建顺序**：`clients/app` 的 `npm run check`（含 `expo export`）产出 `dist/`；Rust 侧在发布构建前需要该产物。开发/CI 环境下缺产物走占位入口，不阻断 `cargo test`。
- **既有断言更新**：`net/tests.rs:301` 断言 `"/"` 必须 404 的用例必须改写为「根路径返回入口文档」；`"/unknown"` 的期望按新语义重新判定（单页回退 → 入口文档），并在用例注释里说明为什么改。

## Open Questions

- 前端构建版本的**具体载体**（写入 `app.json` 的字段、独立生成的文件、或从 git 描述派生）可在实现时定，不影响 specs、approach 或任务拆分。
- 内联样式与 CSP 的最终解法（外置 vs 显式放行 `style-src`）由 `frontend-sync-wiring` 决定；本变更只负责把冲突暴露出来。
