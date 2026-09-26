# WP8 Handoff — 收口：状态写回 + 全量验证（`node-link-owner` / DU1）

## Shared Report

- **task_id**：2.23（状态写回）、2.24（全量验证 `[PV1]` + `[PV2]`）。
- **role / phase**：coder / implement。
- **agent_context**：worker 子 Agent（本机 worktree 独占，未继承父会话实现对话）。工作目录
  `D:\Project\acp-remote-wt\node-link-owner`（分支 `agentic/node-link-owner`）；规划根 `D:\Project\acp-remote`。
  本轮从起点提交开始、一次派单内完成「读契约 → 写回 → 全量验证 → 提交 → 报告」。
- **target_revision**：`0161a7782f12e29278aa311b1e1784b08d4ae8b9`（单个 `docs(docs)` 提交，6 文件）。
  起点 `f9dbece12fd9104bc297cf97655f965fbc1dd4e9`（已核实与派单一致）。
- **scope**：
  - 修改：`README.md`、`AGENTS.md`、`docs/DEVELOPMENT_PLAN.md`、`docs/MODULE_ARCHITECTURE.md`、
    `docs/CONFIG_REFERENCE.md`、`docs/CORE_PORTS_AND_STORAGE.md`（最后一个是**主 Agent 批准的写范围扩展**，
    见「需要主 Agent / reviewer 知晓的偏差」第 1 条：只改第 11 行修订注记与 §11 开头现状句，不动 §5/§7）。
  - **未改**：任何 `crates/**`、`schemas/**`、`fixtures/**`、`compatibility/**`、`scripts/**`、`package.json`、
    `Cargo.toml`/`Cargo.lock`、`openspec/**`（含 `plan.md`/`tasks.md`/`verification.md`）。
- **changes**：见「改动与需求映射」。
- **checks**：
  - `[PV1]`（`npm run verify` / 仓库根）：**EXIT 0**，两轮——首次在未提交工作区、复跑在固定提交
    `0161a778` 上；两轮结论一致（十项合同门禁 + fmt/clippy 退 0 + `cargo test --locked --workspace
    --all-features` **85 目标 / 973 passed / 0 failed / 2 ignored**）。原始输出追加在
    `reports/du1-pv1.log` 的「W7/WP8 轮次」与「提交后复跑」两个分节。
  - `[PV2]`（`node scripts/check-crate-boundaries.mjs` / 仓库根）：**EXIT 0**，
    `crate boundaries OK: 12 个 crate 的依赖方向与 §5 矩阵一致（12 个已在矩阵登记）`（同样跑两轮）。
  - 文档 vs 实际（2.23/2.24 的完成条件）：`cargo metadata --locked --format-version 1 --no-deps` →
    `workspace_members = 12`；`cargo tree --locked -p core --edges normal` → 闭包只有
    `async-trait`/`p256`/`sha2`/`thiserror` 及其传递依赖。原始输出同日志。
- **issues**：无阻断项。**7 条**需要主 Agent / reviewer 知晓的事实列在「需要主 Agent / reviewer 知晓的偏差」
  （其中 1 条是主 Agent 已批准的写范围扩展，2 条是超出任务书列举、但同属「状态漂移」的收敛，均已逐条登记）。
- **result**：**PASS**（本工作包检查全部满足）。**不代表**独立 review（3.16 / RV1-WP8）、3.15 的交付前
  project verify、或 DU1 合入已完成——这些必须由不继承本轮实现对话的执行者判定。
- **evidence_paths**：`reports/du1-pv1.log`（[PV1]/[PV2] 两轮原始输出 + 文档/实际对账输出）、本文件。
- **resource_cleanup**：无容器、无数据库服务、无外部网络、无监听端口残留、无 detached task；用例全部由
  cargo 测试自行创建/删除临时目录，`npm`/`cargo` 只使用仓库内 `target/`（被 git 忽略）。本 Agent 自建的
  scratch `/tmp/wp8/` 仅存放命令原始输出的临时副本，报告落盘后可删；仓库内无新增未跟踪文件
  （`git status --short` 在提交后为空）。

## 改动与需求映射

| 任务书要求 | 落点 | 实际写入 |
| --- | --- | --- |
| 2.23-1 `MODULE_ARCHITECTURE.md` §4.9 `[现状]` + 文件头修订记录 | §4.9、文件头 | `[现状]` 写为「已落地四条路径」：`transport::local`、`local_admin`、`transport::net`（共享 HTTP/WSS listener、TLS `proxy`/`direct`、`Host` 边界、`/sync/v1`+`/node-link/v1`+两类 `/pairing/*` 的 path 路由）、`node_link`（配对 HTTP、握手、catalog、resource、command、撤销传播）；同时写明**仍未落地**的是 `sync`/`acp_facade` 与「Node Link 只覆盖 Owner 侧入站面（出站重连属切片 6 的 `node-link-client`）」。文件头新增 WP8 收口修订记录行。 |
| 2.23-2 `DEVELOPMENT_PLAN.md` §2 | 文件头基线 + §2 | 文件头基线加「2026-09-26 随 `node-link-owner` 补充切片 5 的落地状态」；§2 加切片 5 句（`transport::net` + `node_link` + `app` 接线；四条验收由受控路径端到端用例覆盖，证据 `reports/wp7-integration.log`、`reports/pv5-windows-nodelink.log`）；剩余清单更正为 `server::sync`/`server::acp_facade`/`node-link-client`。 |
| 2.23-3 `README.md`「仓库当前状态」 | `server` 行 + 「尚未开始」段 + 结尾现状段 | `server` 行改为四条路径并写明仍未落地项与 Owner 侧限定；「尚未开始」与结尾现状段删去 `server::node_link`；结尾补「Owner 侧 Node Link 入站面已落地」及「仍不能声称 Zed 远程闭环 / `acp-stdio` 可用」。 |
| 2.23-4 `AGENTS.md` §4 模块状态表 | §4 表 | `server` 行：`已落地（切片 4 本地通道 + local_admin；切片 5 的 transport::net + node_link；sync / acp_facade 待落地）`。 |
| 2.23-5 `CONFIG_REFERENCE.md` §1/§3 | §1 表 + §1 末尾 + §2/§3 | `daemon.allowed_hosts` 行补「两者都空（默认配置）时只接受 loopback 形态 `Host`（`127.0.0.0/8`、`[::1]`、`localhost`，忽略端口）」并指向 `SECURITY_DESIGN.md` §7.2 与判定实现；§1 末尾补 `public_origin` 的 canonical origin 口径（`https://<authority>`，与 `CanonicalOrigin::parse` 同口径）+ 实现现状；§3 补「本交付版本未接线」注记（`node_link.*` 只解析不消费、`NetConfig::default()` 与 `NodeLinkConfig::default()` 同值故行为无差异）；§2 补同机制的 `sync.*` 注记并指向 §3。 |
| 2.23-6 `CORE_PORTS_AND_STORAGE.md` §11（N-RV1-WP3-1） | 第 11 行 + §11 开头 | 两处现状句改为「仍未实现的是 `server::sync`/`server::acp_facade` 与 `node-link-client`（`server::transport::net` 与 `server::node_link` 的 Owner 侧入站面已随切片 5 落地，见 `MODULE_ARCHITECTURE.md` §4.9）」。 |
| 2.24 `[PV1]`/`[PV2]` 全绿 + 日志 | 仓库根 / 日志 | 见 checks；日志写入 `reports/du1-pv1.log`（追加两个分节，含提交后复跑）。 |
| 2.24 文档陈述 vs `cargo metadata`/`cargo tree` | — | 12 个成员、`core` 闭包 allow-list 与写入的文档口径逐项一致（原始输出在日志内）。 |

## 需要主 Agent / reviewer 知晓的偏差

1. **写范围扩展（已批准）**：任务书正文第 6 条要求改 `docs/CORE_PORTS_AND_STORAGE.md`，而写范围清单未列该
   文件。本 Agent 按角色契约停下询问，主 Agent 回复**选 A**：「批准把 `docs/CORE_PORTS_AND_STORAGE.md` 加入
   WP8 写范围，仅限这两处现状句（第 11 行修订注记 + §11 开头），不动 §5/§7，不触发 `check:drift` 判据」。
   实际改动与批准范围一致（`git show --stat 0161a778` 中该文件为 2 行改动），`check:drift` 在提交后仍绿。
2. **任务书文本的一处精度调整**：`daemon.allowed_hosts` 的默认分支，任务书写「只接受 loopback 形态 Host
   （`127.0.0.1`/`[::1]`/`localhost`）」，实际实现是 `IpAddr::is_loopback()`，即接受整个 `127.0.0.0/8`
   （`crates/server/src/transport/net/host.rs::is_loopback_host`）。文档按**实现**写成 `127.0.0.0/8`
   （`127.0.0.1` 是其子集），避免写出比实现更窄的口径。
3. **`public_origin` 的 canonical origin 口径**：任务书要求「补一句必须是 canonical origin
   （`https://<authority>`），与身份侧 `CanonicalOrigin::parse` 同口径」。按「只写实绩」的要求，同时写明
   **实现现状（已知偏差）**：`HostPolicy::origin_host` 目前接受 `http` 与 `https`，因此 `http://` 字面量能过
   Host 边界、却会在配对 URL 派生处失败关闭（RV2-WP2 的 N-RV2-2）。本轮**只写文档、不改代码**——改代码不在
   写范围内，且会触碰 `SECURITY_DESIGN.md` §7.1/§7.2 的口径，属新的决策。请 reviewer 判断是否需要在 3.x 或
   后续切片单开一项收紧。
4. **超出任务书列举、但同属状态漂移的收敛（都在原写范围内的文件）**：任务书只点名了 §4.9/§2/README `server`
   行/`AGENTS.md` §4/`CONFIG_REFERENCE` §1/§3 六处，但同一批文件里还有 8 处**同样过期**的状态句，若不改就会
   与本次写入的实绩直接自相矛盾。已逐处最小改动：
   - `MODULE_ARCHITECTURE.md`：文件头 §1 状态行；§3 的 `[现状]`（追加切片 5 更新段）；§4.10 `[现状]`
     （补切片 5 接线范围）；§4.7 附近的管理能力现状句（原文含「仍未实现的是 …`server::node_link`…」）。
   - `AGENTS.md`：§4 表下注记「切片 4 的落地范围」→「切片 4/5」；`app` 行（切片 5 接线）；§9 测试要求清单
     （原文把 `server::node_link` 列为「尚未落地」）。
   - `README.md`：「尚未开始」段；结尾现状段；`import.add` 恒 `local.unavailable` 的原因（原文写「本切片没有
     可用的 Node Link catalog 快照」，实际缺的是 Access 侧 `node-link-client`，Owner 侧 `node_link` 已落地）。
   - `DEVELOPMENT_PLAN.md`：切片 1 段落里的后续切片清单。
   - `CONFIG_REFERENCE.md`：§2 与 §3 同一机制的未接线注记（单边只写 §3 会让 §2 继续「文档说有、实际不生效」）；
     文件头新增一条 0.8 修订记录行（沿用该文件既有的修订记录惯例；该文件的 `版本` 行仍写 0.6，与既有 0.7
     记录并存属**既有**状态，本 WP 未改 `版本` 行）。
   - 未改 `docs/SECURITY_DESIGN.md`、`docs/LOCAL_ADMIN_PROTOCOL.md` 等库外文件（越出批准写范围）。
5. **`server::node_link` 的测试条目已生效**：`AGENTS.md` §9 原文把它的条目列为「适配器落地时生效」。本轮按
   已落地改为生效（其测试覆盖由 `[PV3]`/`[PV5]` 在前序 WP 记录，本 WP 不复跑）。
6. **`clippy` 的诚实说明**：`[PV1]` 里 `cargo clippy` 只报 `Finished dev profile … in 0.71s`、无 `Checking` 行
   （指纹缓存命中）。本 WP 未改任何 Rust 源，差异面为零；`cargo test` 同轮真实编译并执行了全部 85 个目标。
7. **未执行项**：`cargo-deny` 与 `gitleaks` 只在 CI 运行（`deps`/`advisories`/`secrets` 三个 job），本地无等价
   物，未在本地执行、也不声称通过；macOS/Linux 的 `#[cfg(unix)]` keystore 与权限路径在 Windows 开发机上不会
   跑到（不经手工声称）。

## 未覆盖 / 待补（不由本报告 PASS 覆盖）

- 独立 review（3.16 / `reports/rv1-wp8.md`）：待独立 reviewer；本轮自检不能替代。
- 3.15 的交付前 project verify：`[PV1]`/`[PV2]` 已在固定提交上通过，但仍需独立的交付前复核记录。
- `[PV3]`/`[PV4]`/`[PV5]` 与 RV1-WP1..WP7：属前序 WP 证据（见各自报告），本 WP 不改动其源码，未复跑。
- DU1 合入（5.x）：不在本角色授权内。

## handoff_index

```yaml
handoff_index:
  - task_id: "2.23"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "0161a7782f12e29278aa311b1e1784b08d4ae8b9"
    evidence_type: CHECK
    evidence_id: PV2
    report_path: "reports/wp8-handoff.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "在固定提交 0161a778（6 个文档文件）上执行 node scripts/check-crate-boundaries.mjs，exit 0；该检查以 cargo metadata 实际依赖为判据，因此同时承担 2.23 完成条件「文档陈述与 cargo metadata/cargo tree 实际一致」的证据（另有 cargo metadata --no-deps = 12 members 与 cargo tree -p core --edges normal 的原始输出）。环境：Windows x64、rust-toolchain.toml 固定的 1.98.1、Node v24.19.0。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.23"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "0161a7782f12e29278aa311b1e1784b08d4ae8b9"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "reports/wp8-handoff.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "同一固定提交上执行 npm run verify，exit 0；其中十项合同门禁含文档引用门禁（380 链接 / 4150 §引用 / 257 文件），即本轮新增的 §7.2/§3/§4.9/§4.10 等引用全部可解析，可用作文档改动的门禁证据。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.24"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "0161a7782f12e29278aa311b1e1784b08d4ae8b9"
    evidence_type: CHECK
    evidence_id: PV1
    report_path: "reports/wp8-handoff.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "npm run verify 在固定提交上 exit 0：npm run check 十项门禁全绿；cargo fmt --all -- --check 退 0；cargo clippy --locked --workspace --all-targets --all-features -- -D warnings 退 0（指纹缓存命中，本轮无 Rust 源码改动）；cargo test --locked --workspace --all-features 退 0，85 目标 / 973 passed / 0 failed / 2 ignored（ignored 为 storage-sqlite 既有 #[ignore] 探针与夹具生成器）。原始输出见 reports/du1-pv1.log 的「W7/WP8 轮次 · 提交后复跑」分节。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.24"
    role: coder
    phase: implement
    stage: work-package
    target_revision: "0161a7782f12e29278aa311b1e1784b08d4ae8b9"
    evidence_type: CHECK
    evidence_id: PV2
    report_path: "reports/wp8-handoff.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "node scripts/check-crate-boundaries.mjs 在固定提交上 exit 0，输出 crate boundaries OK: 12 个 crate 的依赖方向与 §5 矩阵一致；矩阵含 server→acpr-wire 格，本轮未改任何依赖。原始输出见 reports/du1-pv1.log 的「W7/WP8 轮次 · 提交后复跑」分节。"
    source_evidence: NOT_APPLICABLE
```

## 复用与失效声明

- 本报告所有 `[PV1]`/`[PV2]` 行都是 `NEW`（在 `0161a778` 上本轮执行），未复用任何旧记录；前序 WP 的
  `[PV3]`–`[PV5]` 证据仍以其原报告为准，本 WP 不改动其覆盖的源码，因此未重跑、也不在此声明其适用性。
- 若本变更在此提交之后再次改动 `docs/**`/`README.md`/`AGENTS.md`，则上述四行的文档门禁部分（`[PV1]` 的
  `check:docs`）与 2.23 的文档一致性命中该提交的改动；Rust 侧结论（fmt/clippy/test/矩阵）不受影响。
