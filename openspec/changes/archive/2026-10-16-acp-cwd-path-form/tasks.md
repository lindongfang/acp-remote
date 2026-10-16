# Tasks

## 1. 剥前缀规则的可见性（WP1）

- [x] 1.1 让 `core` 的 `strip_verbatim` 可被 `agent-host` 复用，并在其文档注释里写清它处理两种前缀（`\\?\`、`\\?\UNC\`）与非 Windows 直通——判据：`cargo test -p core` 通过，且该函数不再是私有可见性
- [x] 1.2 在 `core` 侧补可证伪的断言：普通 verbatim 形态被剥除、UNC 形态还原为 `\\server\share`、非 Windows 平台逐字节直通——判据：`cargo test -p core` 中新增用例覆盖三态

## 2. 出站 ACP 参数的形式（WP2）

- [x] 2.1 `session/new` 与 `session/resume` 的参数构造应用转换，且只作用于出站参数——判据：`cargo test -p agent-host` 通过，diff 中不存在对持久化取值或解析结果的改写
- [x] 2.2 在 `docs/MODULE_ARCHITECTURE.md` §4.5 登记「出站路径参数的形式」这条实现约束——判据：`npm run check` 的 `check:docs` 通过

## 3. 出站字节的证伪断言（WP3、WP4）

- [x] 3.1（WP3）用假 Agent 记录它实际收到的 `session/new` 与 `session/resume` 参数，断言 `cwd` 不含 verbatim 前缀（含 UNC 还原与无前缀直通）——判据：`cargo test -p agent-host` 中该用例可独立失败
- [x] 3.2（WP4）在真实 Daemon + 真实 `agent-host` + 假 Agent 的 app 层用例上，同时断言会话行/恢复数据里的目录列仍是创建时 canonical 化取值、且假 Agent **实际收到**的是剥掉前缀的形式——判据：`cargo test -p app` 中该用例可独立失败

## 4. 合入与真机验收

- [x] 4.1 按 plan 的 Merge Order 串行合入 WP1→WP2→WP3→WP4，每次合入后跑该序的再验证——判据：四次 `git apply --check` 通过且随后的检查全绿
- [x] 4.2 主检出跑 PV1（`cargo test -p core -p agent-host -p app`）、PV2（`npm run check`）、PV3（`cargo clippy --workspace --all-targets -- -D warnings`）——判据：三条命令全绿
- [x] 4.3 执行 plan 的 Main E2E：经 Daemon 用真实 Agent（`omp acp`）真的建成一个会话——判据：浏览器里出现该会话且 Agent 进程被拉起（保留观察证据）；若仍失败，先定位是能力协商、`session/update` 还是 `configOptions` 形状，再判定本变更是否完成
