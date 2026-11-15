# Raw JSON 基础层：编码前限定门禁

2026-10-31。**方案与独立验收定义预审通过，仅放行隔离基础层实现。** RAW-PLAN-001–003 已关闭。没有产品功能或验收通过结论，不代表旧容量限制已移除。

## 同版输入

- 根合同 `openspec/changes/deliver-codex-session-loop/acp-raw-json-contract.md`：`9f2b236f325a0cceed3c1c34274d73ca953d9c7dfec571d22745b2b7ef70cce6`。
- 独立 tester `session-loop-q003-tester/wp3-raw-json/acceptance-definition.md`：`d172289c8018fba5d807896a3c33dd845276d10d9fd38e2e8c748c44cca1e400`。亲读全部 API 映射及 RJ01–30；合同绑定一致，未编码/未运行的状态明确。
- 亲自按 raw implementation 的 baseline-source.json 核对 baseline 与 workspace：各124项，全部同 SHA，无缺失/差异。源码原 serde_json feature 尚未变更、旧 API 未接新模块。主执行报告的 baseline 编译不是本 reviewer 亲自执行，亦不是功能验收。

## 定义充分性

RJ01–10 覆盖任意根、精确原字节/片段、只读浅 API、顺序/双端精确迭代、临时句柄借用及 owned/Clone 共享存活；RJ11–20 覆盖完整 escape/scalar、任意大数词法、整数边界与负零、整体验证优先和扫描首错、各层 decoded 重复键与不归一；RJ21–26 覆盖旧 token/正文/深度界外样本、20000层混合结构、共享克隆及成功/失败析构；RJ27–30 覆盖全部公开句柄/错误脱敏与 traits、管理8层和既有资产/锁文件保持。

深层测试在自有子进程的256KiB线程栈运行并实际 join，不用递归 Value 构造 oracle；时间上限只收敛测试。静态核对单次扫描、平面节点、共享索引及无隐式 DOM 回退，不能用耗时比值证明复杂度。Allocation 仅公开变体/文案动态加显式 try_reserve 静态，真实 OOM 明确 not-run。iter 不要求 Debug，五类公开句柄均要求实现脱敏 Debug。错误阶段及 -0 已与合同准确对齐。没有遗留的工程可执行性阻断。

## 放行写范围与顺序

主执行须先将隔离并行范围明确记入 plan：`session-loop-q003-raw-implementation/workspace` 中仅新增 `app/src/adapters/acp/raw_json.rs`、其子模块/开发测试、ACP 模块声明、现有 serde_json 的 raw_value feature 一处和 ACP README。不得修改 R3 文件、原 RawMessage/Strategy/transport/P1/P2 API 或接线；不得新增包/升级依赖或改 Cargo.lock。使用独占 target `D:/Project/acp-remote/.tmp/q3-raw-coder-target`，不与 tester target 或 R3 target 混用。

在这个范围内，孤立基础层开发可与 R3 正式验收并行；R3 冻结后的 reviewer 交付复查优先。先集成 R3，再集成 raw 基础层，最终组合重新运行适用检查。独立 tester 资产在其自有目录建立，交付时另按 SHA 同源纳入，不能由开发单测替代。

实现后仍必须冻结源码/资产清单，独立执行30定义ID并映射实际测试名/分支，保实际命令、退出码、前后 SHA；深层失败、正文完整比较和 canary 不得省略。固定源静态审查以及实际集成 fmt、严格 all/pedantic/nursery Clippy、check/build、原 ACP/JWS/管理及旧资产回归均须完成。若实现要求越出本范围或改变任一合同 oracle，重新预审，不能自行回填独立定义。

本通过仅是上述工程基础层的编码门禁，不关闭 CL-095/103 整体，不勾选任务3.1/3.2/6.3，不证明真实帧入口、NDJSON/outbox迁移、无上限共享缓存、reader/writer、Harness/WSS。旧32MiB/96层/131072-token限制及历史128MiB视图合同的状态不因此报告改变。
