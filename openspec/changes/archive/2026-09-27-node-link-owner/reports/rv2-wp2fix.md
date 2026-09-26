# RV2-WP2FIX Review 报告（recheck：WP2 fix1 / RV1-WP2 的 F1/F2/F3 + N2/N3）

> 说明：本轮 reviewer 子 Agent 无写文件工具（只读检视），报告由主 Agent 按其返回正文原样持久化到本路径。

## Shared Report

- **task_id**: `2.4` / `2.5` / `2.6`（WP2 交付前修复复核 / DU1）；复审对象 = RV1-WP2 的 `RV1-WP2-F1`/`F2`/`F3` 与 note `N2`/`N3`
- **role**: reviewer（独立代码检视子 Agent，只读；未参与实现与修复，未参与修复讨论）
- **phase**: recheck；**stage**: work-package（工作包交付前，fix1 轮次）
- **agent_context**: 新建任务级 reviewer 子 Agent（0efa93e0-5bb2-44e9-826d-8fa9e2b7c1ec；隔离上下文启动，fork_turns="none"）。工作目录 `D:\Project\acp-remote-wt\node-link-owner`；未修改任何文件、未切换分支、未提交、未创建端口/进程/临时目录（`watchdog_diff` = 无改动，HEAD = `50a5af42`）。权威规划根 `D:\Project\acp-remote`（只读）。
- **base_revision**: `4e8a1075a48e004808f498447b54e45dbff824ce`；**target_revision**: `50a5af420ec5c25e521dd93aeb3dc9bf3a732ab5`
- **版本稳定性**：`watchdog_diff` 报工作区相对 reviewer-launch HEAD 无任何改动，且 HEAD = 目标提交 → 读到的工作区内容 = target revision。
- **scope（实际检查范围）**：fix 提交触及的 6 个文件逐行读完（`crates/server/src/transport/net/{mod,host,listener,route,test_client,tests}.rs`）；为回归判定另读 `net/{http,ws}.rs`、`transport/mod.rs`；为证据真实性核对读 `reports/wp2-fix1-handoff.md`、`reports/wp2-transport-net.log`（§7–§9）、`reports/rv1-wp2.md`；为契约核对读 `openspec/changes/node-link-owner/{design.md,plan.md,tasks.md,verification.md}`、`specs/node-link-listener/spec.md`、`docs/CONFIG_REFERENCE.md` §1、`docs/SECURITY_DESIGN.md` §5、`crates/app/src/config.rs`、`crates/server/src/local_admin/pairing.rs`；为核实 fix 报告中对上游行为的断言读 **axum 0.8.9 实际源码**（`~/.cargo/registry/.../axum-0.8.9/src/{extract/ws.rs,serve/mod.rs,serve/listener.rs}`）。
- **changes（相对 base）**：1 个提交、6 个文件、+385/−44（无 shell/Git 权限，无法独立重算该 diff，该数字为转录；但 target 侧每个声称的改动都逐处核到了实体代码）。
- **checks**：本轮未执行任何命令（只读检视）。
  - `[PV3]`：日志 §9.2 记录在 `50a5af42`（工作区干净）上 **165 lib passed / 0 failed / 0 ignored** + 集成 14/6/4/0/4/0 全 ok；net 过滤轮次 **76 passed / 0 failed / 0 ignored**。与 base（161 + net 72）对比增量恰好 = 4 条新用例，无计数下降 → 判定既有用例未删未弱化。
  - `[PV5]`：fix 轮次只跑 `-p server`（Windows），WP2 平台用例随该轮次覆盖；`-p app` 部分未重跑，判为不影响本轮结论。
  - `[PV1]`/`[PV2]`：属 WP8/候选门禁，本轮不适用；fix 轮次 pre-commit 跑了 `npm run check`（16 passed）与 workspace clippy。
  - 回归有效性证据：日志 §9.1 记录两条新用例在**回退实现**上 FAILED、在 fix 实现上 ok；panic 位置与 target 文件对应关系已核。
- **issues**: 0 × CRITICAL、0 × MAJOR；3 条非阻断（2 × MINOR/P2、1 × SUGGESTION）+ 2 条 report-only note。未发现 specs 需求违反、依赖方向破坏、秘密进日志/错误、`unsafe`、证据造假。
- **result**: **PASS**（对应 `50a5af42…`；仅覆盖「RV1-WP2 修复项是否闭环 + 是否引入回归」，不代表 WP8 文档收口、候选验证、E2E 或合并已完成）
- **evidence_paths**: 本报告 `reports/rv2-wp2fix.md`；检视对象 `crates/server/src/transport/net/**`；证据 `reports/wp2-fix1-handoff.md`、`reports/wp2-transport-net.log`（§7–§9）、`reports/rv1-wp2.md`、`reports/pv5-windows-nodelink.log`（base 轮次）、`reports/du1-pv1.log`（RV1 引用）；上游源码 `axum-0.8.9/src/{extract/ws.rs,serve/mod.rs,serve/listener.rs}`
- **resource_cleanup**: 本轮无资源创建、无写入、无端口/进程/临时目录；未安装依赖、未改锁文件。

## handoff_index

```yaml
handoff_index:
  - task_id: "2.4"
    role: reviewer
    phase: recheck
    stage: work-package
    target_revision: "50a5af420ec5c25e521dd93aeb3dc9bf3a732ab5"
    evidence_type: REVIEW
    evidence_id: RV2-WP2FIX
    report_path: "openspec/changes/node-link-owner/reports/rv2-wp2fix.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "在固定 target（工作区 = 50a5af42，watchdog_diff 无改动）上只读复核 fix1 的 6 个文件与 4 条新用例；对照 §9.2 的 PV3 日志判定既有 net 用例无回归。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.5"
    role: reviewer
    phase: recheck
    stage: work-package
    target_revision: "50a5af420ec5c25e521dd93aeb3dc9bf3a732ab5"
    evidence_type: REVIEW
    evidence_id: RV2-WP2FIX
    report_path: "openspec/changes/node-link-owner/reports/rv2-wp2fix.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "重点复核 F3（direct 模式 TLS 握手池）的任务所有权/取消路径/背压下界/关闭序列参与，以及 F2 对 public_origin 无条件校验是否破坏 direct/proxy 语义与启动失败关闭口径。"
    source_evidence: NOT_APPLICABLE
  - task_id: "2.6"
    role: reviewer
    phase: recheck
    stage: work-package
    target_revision: "50a5af420ec5c25e521dd93aeb3dc9bf3a732ab5"
    evidence_type: REVIEW
    evidence_id: RV2-WP2FIX
    report_path: "openspec/changes/node-link-owner/reports/rv2-wp2fix.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "交付前冻结形状复核：公开 API 面（mod.rs 再导出一行未增删、值类型仍直出）与 WP3/WP4 消费方式；F1 文档句已如实；新增 3 条非阻断项。"
    source_evidence: NOT_APPLICABLE
  - task_id: "RV1-WP2-F1"
    role: reviewer
    phase: recheck
    stage: work-package
    target_revision: "50a5af420ec5c25e521dd93aeb3dc9bf3a732ab5"
    evidence_type: REVIEW
    evidence_id: RV2-WP2FIX
    report_path: "openspec/changes/node-link-owner/reports/rv2-wp2fix.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "原问题在 base 4e8a1075 上提出（reports/rv1-wp2.md）；本轮在 50a5af42 上重读 mod.rs:9,16-22 与 http.rs/ws.rs/route.rs 公开签名，确认文档已改且与事实一致 → 已解决。"
    source_evidence: NOT_APPLICABLE
  - task_id: "RV1-WP2-F2"
    role: reviewer
    phase: recheck
    stage: work-package
    target_revision: "50a5af420ec5c25e521dd93aeb3dc9bf3a732ab5"
    evidence_type: REVIEW
    evidence_id: RV2-WP2FIX
    report_path: "openspec/changes/node-link-owner/reports/rv2-wp2fix.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "重读 host.rs:49-57（校验提到分支之前）与 host.rs:251-276（新用例含 4 种非法形态 + 合法组合仍按白名单判定），并核对 app/config.rs 与 local_admin/pairing.rs 的 public_origin 消费面 → 已解决、无合法配置回归。"
    source_evidence: NOT_APPLICABLE
  - task_id: "RV1-WP2-F3"
    role: reviewer
    phase: recheck
    stage: work-package
    target_revision: "50a5af420ec5c25e521dd93aeb3dc9bf3a732ab5"
    evidence_type: REVIEW
    evidence_id: RV2-WP2FIX
    report_path: "openspec/changes/node-link-owner/reports/rv2-wp2fix.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "按主 Agent 裁决 (a) 复核：listener.rs:49-66/480-577 的有界握手池（64 槽 + JoinSet + 容量 64 结果队列）、accept 只做 TCP accept、背压等待、任务随 NetAcceptor 丢弃中止；另核 axum 0.8.9 serve 的 drop(listener) 早于排空 → 关闭序列参与成立。"
    source_evidence: NOT_APPLICABLE
  - task_id: "N2"
    role: reviewer
    phase: recheck
    stage: work-package
    target_revision: "50a5af420ec5c25e521dd93aeb3dc9bf3a732ab5"
    evidence_type: REVIEW
    evidence_id: RV2-WP2FIX
    report_path: "openspec/changes/node-link-owner/reports/rv2-wp2fix.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "重读 tests.rs:578-616：错误 Host 在 /node-link/v1 → 400 且 CountingWsHandler 零调用，同用例内合法 Host → 101 作正对照 → note 已闭环。"
    source_evidence: NOT_APPLICABLE
  - task_id: "N3"
    role: reviewer
    phase: recheck
    stage: work-package
    target_revision: "50a5af420ec5c25e521dd93aeb3dc9bf3a732ab5"
    evidence_type: REVIEW
    evidence_id: RV2-WP2FIX
    report_path: "openspec/changes/node-link-owner/reports/rv2-wp2fix.md"
    result: PASS
    evidence_status: NEW
    applicability_basis: "独立核对 route.rs:376-386 的口径说明与 axum 0.8.9 源码（extract/ws.rs:498-506 用 get_all→按 `,` 切分→trim_ascii；protocols() 用 contains 精确匹配），确认双头口径一致；端到端探针 tests.rs:424-476 覆盖 3 组接受 + 2 组拒绝 → note 已闭环（口径=接受）。"
    source_evidence: NOT_APPLICABLE
```

## Findings

### 复核原问题（按原问题 ID）

| 原问题 ID | 原级别 | 本轮结论 | 复核依据（target = `50a5af42`） | 回归检查 |
|---|---|---|---|---|
| RV1-WP2-F1 | MINOR(P2) | **已解决** | `net/mod.rs:15-22` 已改为三条边界的如实表述；与事实一致（`pub use` 清单未含这批值类型；公开签名确实直出）；`mod.rs:9` 同时补了握手池行 | 公开项零增删；无行为变化 |
| RV1-WP2-F2 | MINOR(P2) | **已解决** | `host.rs:49-57`：`origin_host(...)` 校验（`Some(_)` 即校验、非法即 `Err(InvalidPublicOrigin)`）已提到白名单分支之前；新用例 `host.rs:251-276` 覆盖 4 种非法形态 + 合法组合 | 合法配置无回归；消费面（app/config.rs、local_admin/pairing.rs）已核 |
| RV1-WP2-F3 | MINOR(P2)，裁决 (a) | **已解决**（实现正确，含背压取舍） | 见下「F3 专项复核」7 点 | 既有 direct/关闭用例全绿；新用例在旧实现 FAILED → 新实现 ok |
| N2（note） | report-only | **已闭环** | `tests.rs:578-616`：错误 Host 在 /node-link/v1 → 400 且处理器零调用；合法 Host → 101 正对照 | 新用例；覆盖表已登记 |
| N3（note） | report-only | **已闭环**（口径 = 接受） | `route.rs:376-386` 写明与上游同口径；独立核对 axum 0.8.9 源码确认双头口径一致；端到端探针覆盖 3 组接受 + 2 组拒绝 | 用例只增不删（计数 72→76 与新增 4 条一致） |

**F3 专项复核（新引入的并发代码，逐点）**

1. **任务所有权（无 detached task）**：握手任务由 `NetAcceptor.handshakes: JoinSet<()>` 持有；`accept` 顺带收割（带非空守卫）。✔
2. **取消路径**：`JoinSet` 随 `NetAcceptor` 丢弃即中止；按 axum 0.8.9 源码确认 `serve` 收到关闭信号后先 `drop(listener)` 再做连接排空 → 握手任务在关闭信号处即被中止。✔
3. **背压下界**：`acquire_owned()` 取槽位；池满等待而非内联；在途握手 ≤ 64、排队结果 ≤ 64、等槽位连接 ≤ 1；单次握手占用 ≤ `TLS_HANDSHAKE_TIMEOUT`（10 s）。✔
4. **死锁自由性（关键顺序）**：`drop(permit)` 先于 `ready.send(...)`——持有槽位的任务永不等待结果队列；「队列满 + 接收端阻塞」不可同时成立，无死锁。✔
5. **投递与关闭的边界**：`ready_tx` 由 acceptor 自持，`recv()` 不会返回 `None`；任务侧 send 失败即放弃连接。✔
6. **direct 语义未被削弱**：明文连接不占槽位直接交出；握手失败/超时只记 debug（无证书/密钥材料）；已建立连接不受影响。✔
7. **关闭后端口释放/会话取消**：既有用例未改且全绿。✔

### 新发现（RV2-WP2FIX-F<n>）

| ID | 级别 | Location | Trigger / Evidence | Impact | Recommendation | Recheck |
|---|---|---|---|---|---|---|
| RV2-WP2FIX-F1 | MINOR(P2) | `listener.rs:516-527` + `tests.rs:1035-1090` | 「池满 → 背压等待槽位」是唯一无用例覆盖的新路径；死锁自由性取决于 `drop(permit)` 先于 `send` 这一承重顺序，现有用例抓不到未来回归 | 非当前缺陷；未来回归无守卫 | 主 Agent 已裁决：补饱和用例（允许测试专用注入缩小池容量以保持用例快速） | 不适用 |
| RV2-WP2FIX-F2 | MINOR(P2) / report-only | `listener.rs:58,66,516-518` | 背压方案的攻击成本提高到 64 条并发预认证连接；本层暂无按源 IP 的准入闸门（限流器属 WP3/WP4 的 N5） | 有界退化，不越权、不影响已建立连接 | 主 Agent 已裁决：记为已知取舍（池 64 / 单次 10 s）；WP4 接线认证限流时顺带评估「按 IP 限制在途握手数」 | 不适用 |
| RV2-WP2FIX-F3 | SUGGESTION | `listener.rs:557-562` | `ready_rx.recv()` 的 `None` 分支当前不可达但为空循环体——边界条件若被破坏会变忙等 | 仅可维护性 | 主 Agent 已裁决：补注释说明不可达依据（随 F1 同批） | 不适用 |

### report-only note（不构成 finding）

- **N-RV2-1**：F3 用例登记在覆盖表的 `[R12]/[R13]` 行，但 spec 没有「预先认证接入可用性」需求——建议 WP8 收口时显式注明其为「非需求性回归保护」。
- **N-RV2-2**（范围外、非本 diff 引入）：`host.rs::origin_host` 接受 `http://` 与 `https://`，而协议侧 `CanonicalOrigin::parse` 只接受 `https://`；`public_origin = "http://host"` 能过 HostPolicy 却在配对 URL 派生时失败。base 版本同样如此。建议后续 WP 统一为 `https://` 口径（主 Agent 注：列入 WP7/WP8 评估，config 加载处决定）。

## Assessment（Check / 证据逐条）

| Check / 重点项 | 已核对的证据 | 差异 / 待补项 | 是否影响本轮判断 |
|---|---|---|---|
| [PV3] | 日志 §9.2（target = 50a5af42）：165 lib + 集成全绿、net 76 passed；与 base 对比增量恰为 4 条新用例 | 未跑整 workspace；未逐名列出用例名 | 否 |
| [PV5] | §9.2 平台为 Windows | fix 轮次未跑 `-p app`；PV5 正式轮次仍应在候选门禁前补齐 | 否（公开 API 零变更、app 尚未接线、pre-commit workspace clippy 通过） |
| [PV1]/[PV2] | 不适用（WP8/候选门禁） | N6 已由 3.3 对齐 | 否 |
| F1/F2/F3/N2/N3 复核 | Findings 复核表 + F3 专项 7 点 | 无 | 否 |
| 既有 net 用例回归 | §8 vs §9.2 计数对比；§9.1 双向回归有效性 | 无法独立重算 diff（限制 L1） | 否 |
| 门禁/计划是否被弱化 | 未改 plan/verification/脚本/配置；无用例删除、无 ignore | 无 | 否 |

### 未验证内容与限制（必须与结论一起读）

1. **L1（无 shell/Git）**：base→target 逐文件 diff 未独立重算；一致性靠「计数增量 = 新增用例数」「每条声称的改动都在 target 文件里找到实体」交叉验证。主 Agent 可用 `git show --stat 50a5af42` 一行确认。
2. **L2（未执行）**：本轮未运行任何命令；PV3/PV5 结论为对日志的静态可信性核对。代码检视 PASS 不等于 Project Verify PASS。
3. **L3（平台/环境）**：Linux `cfg(unix)` 权限用例由 Linux CI 覆盖；真实跨用户 ACL、非 loopback 可达性、第三方 TLS 互操作、cargo-deny/gitleaks 均不在本轮。
4. **E2E**：plan.md 记 not-applicable（已获批准），本轮不涉及。

## 结论

- **判定：PASS**（Target Revision `50a5af420ec5c25e521dd93aeb3dc9bf3a732ab5`）。原 3 条 MINOR 与 2 条 note 全部闭环；无未解决 CRITICAL/MAJOR；3 条新发现与 2 条 report-only note 不阻断 WP2 修复放行。
- 残余风险（随 WP2 交付记录一并声明）：direct 模式下背压方案的成本门槛 = 64 条并发预认证连接 / 单次 10 s（F2）；池饱和路径缺守卫用例（F1，主 Agent 已裁决补）；池容量与握手超时是硬编码常量、CONFIG_REFERENCE.md 无对应键。
- 本报告不代替主 Agent 写入 verification.md，也不宣称候选验证、E2E 或合并已完成。
