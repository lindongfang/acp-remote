# Plan

## Execution Model

- 主 Agent 使用 Oh My Pi 的 batch `task` 派发独立工作包，每包一次 spawn；共享 context 必须包含中文沟通要求、固定设计、写范围及在途修改基线。
- 所有写包 `isolated: true`；`task.isolation` 使用 patch 模式且 `apply: false`。不让子 Agent 改主检出，不引入第二套调度或台账。
- 每包交付 `<id>.patch` + `agent://<id>` + `history://<id>`，不交付分支或提交。
- 主 Agent 先审查 patch，再串行执行 `git apply --check`、`git apply`；每次合入后在主检出执行该包 Verify。隔离区无 target/node_modules，不能在那里跑仓库检查。
- 独立 reviewer spawn 检视绝对路径 patch、报告、过程与调用方；作者自评不算检视。changes-requested 使用新修复和新复核 spawn，最多3轮，仅合入最终通过版本。
- 任务完成记录回本变更 tasks.md；只有实测证据足够才勾选。发布与远端合入遵循 docs/WORKFLOW.md §6，不包含在本次规划写入中。

## Work Packages

| ID | Goal / Scenarios | Dependencies | Agent | Write Scope | Deliverable | Verify |
| --- | --- | --- | --- | --- | --- | --- |
| WP1 | 短QR与错误合同、两语言共享向量和权威文档，覆盖格式/指纹/兼容 | none | wp-coder | `crates/sync-protocol/`；`clients/app/src/protocol/`；`schemas/sync/v1/`；`fixtures/sync/v1/`；`schemas/local-admin/v1/`；`fixtures/local-admin/v1/`；`compatibility/errors/v1/`；`scripts/check-*.mjs`；`docs/INITIAL_DESIGN.md`、`docs/SECURITY_DESIGN.md`、`docs/SYNC_PROTOCOL.md`、`docs/LOCAL_ADMIN_PROTOCOL.md`、`docs/IDENTITY_AND_AUTH_CONTRACT.md`、`docs/FRONTEND_DESIGN.md`、`docs/MODULE_ARCHITECTURE.md`；`docs/adr/`；`GLOSSARY.md` | WP1.patch + 合同变更清单与固定向量说明 | PV1、PV2 |
| WP2 | 内存凭据、独立随机生成、清理和认领争用，覆盖身份增量全部场景 | contract:WP1 | wp-coder | `crates/identity-auth/` | WP2.patch + 生命周期/失败矩阵 | PV3 |
| WP3 | 取回HTTP、本地管理短地址、CLI与组合根接线，覆盖并发和HTTP边界 | code:WP1, code:WP2 | wp-coder | `crates/server/src/sync/`；`crates/server/src/local_admin/pairing.rs`；`crates/server/src/transport/net/`；`crates/server/tests/`；`crates/app/src/`；`crates/app/tests/pairing_lookup.rs`、`crates/app/tests/cli_commands.rs`、`crates/app/tests/sync_e2e.rs`、`crates/app/tests/sync_commands_e2e.rs` | WP3.patch + 路由/状态/终端验证说明 | PV4 |
| WP4 | PWA仅码、误扫清理、旧格式退出；共享原生合同消费方与有限重试测试 | contract:WP1 | wp-coder | `clients/app/src/`（不含 `src/protocol/`）；`clients/app/app/` | WP4.patch + 桌面/手机与原生模拟边界说明 | PV5 |
| WP5 | 真实Daemon的C模拟链路与浏览器E2E、发布/回滚验收说明 | code:WP3, code:WP4 | wp-tester | `crates/app/tests/device_qr_reference_e2e.rs`；`clients/app/e2e/`；`docs/FRONTEND_DESIGN.md`；`README.md` | WP5.patch + E2E断言及观察证据；清楚登记原生真机未执行 | PV6、PV7、Main E2E |

WP4 的 `src/` 范围显式排除 WP1 的 protocol 前缀；WP5 新 Rust 测试文件不属于 WP3 的四个 app 测试文件。WP1 的通配资产脚本范围不授权修改工具链或无关门禁规则，必须在报告中列实际文件。WP3 若发现需要 core 端口或 storage 变更，停止该扩展并修订计划，不能越界写。

## Merge Order

| Order | Work Package | Reason | Re-verify After Merge |
| --- | --- | --- | --- |
| 1 | WP1 | 冻结机器合同与共享协议类型；文档注明新行为待实现 | PV1、PV2 |
| 2 | WP2 | 按新合同接身份材料 | PV3 |
| 3 | WP3 | 接入身份与协议后才能验证真实HTTP/CLI | PV4 |
| 4 | WP4 | 可与WP3在WP1完成后并行派发；串行合入，必须确认不覆盖WP1协议目录 | PV5 |
| 5 | WP5 | 与WP1在 `docs/FRONTEND_DESIGN.md` 重叠，必须以WP1已合入版本为基线；WP3/WP4真实链路就绪后验收 | 重新执行 PV1、PV5、PV6、PV7，核对前端阶段/实现状态文案无回退 |

除声明的 WP1→WP5 前端文档重叠外，其余写范围不交叉。WP2 和 WP4 可在各自真实依赖满足后 batch 派发，不能为凑波次增加无关依赖。

## Project Verify

以下全部在主检出执行；命令失败不能以隔离区缺依赖为理由忽略。

| Check ID | Command / Working Directory | Scope / Pass Criteria |
| --- | --- | --- |
| PV1 | 根目录 `npm run check:schemas`、`npm run check:errors`、`npm run check:assets`、`npm run check:docs`、`npm run check:openspec` | 新 schema/fixture/错误码一致、旧设备资产退出、节点向量保留；跨包合同漂移最终以PV7为准 |
| PV2 | 根目录 `cargo test -p sync-protocol` | 65字节QR、非法编码/版本/公钥、共享向量回归通过 |
| PV3 | 根目录 `cargo test -p identity-auth` | 独立随机、清理、拒绝/失效/重启与码竞争回归通过 |
| PV4 | 根目录 `cargo test -p server`、`cargo test -p app --test pairing_lookup`、`cargo test -p app --test cli_commands`、`cargo test -p app --test sync_e2e`、`cargo test -p app --test sync_commands_e2e` | 取回路由/安全头/Origin/并发与CLI；旧app集成测试改为使用新入口而非恢复旧设备解析 |
| PV5 | `clients/app/`：`npm run check` | TS、UI与状态测试通过；PWA无扫码资料请求，共享原生消费测试不冒充App |
| PV6 | 根目录 `cargo test -p app --test device_qr_reference_e2e`；`clients/app/`：`node e2e/run-e2e.mjs` | 模拟原生对真实Daemon走完整取回/指纹/claim/status/WSS；浏览器覆盖码配对、误扫无请求、节点展示 |
| PV7 | 根目录 `npm run verify` | 合同、fmt、clippy、Rust全量门禁通过；再确认PV5/PV6前端独立门禁，不以根检查代替前端 |

现有页面 runner 使用假主机，只作为页面回归；真实 Daemon 浏览器验收另按 Main E2E 执行，不能互相替代。依赖公告/许可证/密钥 CI 检查只记录实际CI结果，不声称本地等价。

## Code Review

每次派发给内置 reviewer 明确 patch 绝对路径，使用结构化 verdict/findings；不得由对应作者实例检视。

| Work Package | Reviewer Agent | Review Focus | Pass Condition |
| --- | --- | --- | --- |
| WP1 | reviewer（独立spawn） | 编码/错误/fixture同源、主规范冲突、ADR部分替代、指纹算法不变 | 无未闭环 blocker/major |
| WP2 | reviewer（独立spawn） | 内存敏感材料、状态资格、全部清理路径与竞争 | 无未闭环 blocker/major |
| WP3 | reviewer（独立spawn） | 调用方、权威状态读取先后、限流/预处理、CLI不回显、节点未变 | 无未闭环 blocker/major |
| WP4 | reviewer（独立spawn） | PWA发请求前清片段、旧格式退出、码身份保护、原生模块未接入网页 | 无未闭环 blocker/major |
| WP5 | reviewer（独立spawn） | 测试是否真实覆盖完整链路、证据无秘密、阶段声明与回滚可执行 | 无未闭环 blocker/major |

## Main E2E

```yaml
mode: required
```

- 主 Agent 在主检出用 browser prelude 验证真实 Daemon 托管的 PWA：桌面与手机视口分别完成码输入、主机端确认、目录与认证连接；页面主题偏好保留。
- 打开受控模拟新短二维码和旧设备片段：断言地址被清除、固定指引/不支持文案出现，网络观察中没有 lookup/qr-lookup/claim/status；刷新为码输入。不得将真实凭据写入观察日志或截图证据。
- 打开原 node-link 二维码仍可展示节点资料，不认领、不倒计时、不进入目录。
- C链路用模拟原生HTTP/WSS消费方对真实Daemon完成，专门断言换指纹、换来源、丢包重试和两入口竞争；原生相机/keystore/真机扫码不在本轮产品验收，证据明确标记未执行。
- 在支持ANSI的终端以非秘密模拟载荷观察新QR完整、实心黑白、不折行，自动断言固定地址长度与矩阵预算；不得把单张截图当作扫码可识别率证明。

## Risks

- 主检出已有未提交主题与CLI修改；每次派发前记录 `git status`/diff，将其列为基线，patch不得夹带用户修改。主题变更若先归档，按已接受主规范重核PWA delta，不能恢复旧“禁止主题按钮”文案。
- WP1资产切换与WP3服务端切换间主检出可能暂不满足全合同门禁；按PV分阶段验证，最终合并发布以PV7全绿为前提，不发布中间态。
- 并发构建不共用同一target目录；仓库级检查由主Agent串行运行，避免资源争用污染证据。
- 本计划不宣称规划完成等于实现完成；所有实现任务保持未勾选。
