# establish-node-access independent tester run

状态：独立 tester 的套件结构核对、覆盖率核对和当前操作系统中可执行的检查记录。本文不是产品功能验收，不代表 `NA-033`/`NA-034` 已由人员签署。

## 版本与范围

- 日期：2026-10-29（Asia/Shanghai）
- 基线 HEAD：`7a6e9ec4bdf4b41d9408b39cb3669313e52f41b2`
- 被检查版本：当前工作区未提交快照；测试结果只对应检查时的文件和依赖状态。
- 操作系统：Windows x64；Node `v22.20.0`；Cargo `1.98.1`。
- 测试角色：独立 tester 会话；未继承开发对话，开发方证据只作为待复核线索。
- 资源所有权：本次没有启动产品节点、真实浏览器、Tailscale 路由或跨用户进程；因此本次没有需要清理的产品进程、端口或测试账号。
- 写入边界：持久交付只写 `testing/suites/` 和 `testing/runs/`；未修改 `app/`、`clients/`、`.agents/`、`openspec/`、`README.md` 的产品源文件或配置，未删除或覆盖 `2026-10-29-development.md`。Cargo 检查更新了已被 `.gitignore` 忽略的 `app/target/` 构建缓存，这不是产品源文件修改，也未纳入交付。

## 新增资产

| 文件 | 内容 | SHA-256 |
| --- | --- | --- |
| `testing/suites/node-access/tests.md` | `node-access` 长期验收套件，12 个稳定用例 | `ec9890b0be56760e4ba84b1c8eba85a68b786b1b60062b700a0df8eba0871115` |
| `testing/suites/client-trust/tests.md` | `client-trust` 长期验收套件，22 个稳定用例 | `15bd32c7a0169ebf845b8f4150110ad1d6185e06c0b5b82a9482f66451d5c570` |
| `testing/suites/traceability/establish-node-access.md` | 15 个 Requirement、28 个 Scenario、34 个稳定用例的显式矩阵 | `b1264c7f3a200c59829707dd128070cd4bacb14b37538972ac0fa22a1ba30e49` |
| `testing/suites/verify-suite.mjs` | 只读结构、来源、字段、规格引用和矩阵一致性检查器 | `389aacdfc19b49ac1a11d46647577ebc894193030c6f3a878d6546d30e501aab` |

`testing/runs/establish-node-access/2026-10-29-development.md` 是开发方既有证据，本次只读保留；其内容不构成本次独立验收。

## 覆盖结果

- 稳定用例来源：`openspec/changes/establish-node-access/tests.md`。
- 来源中有 34 个 `NA-xxx` ID，范围为 `NA-001` 至 `NA-034`；ID 唯一、无缺号，长期套件中包含同一组 34 个 ID，无遗漏或额外 ID。
- 增量规格共 15 个 Requirement、28 个 Scenario：
  - `node-access`：5 个 Requirement、10 个 Scenario，12 个长期用例。
  - `client-trust`：10 个 Requirement、18 个 Scenario，22 个长期用例。
- 全部 28 个 Scenario 至少被一个长期用例的显式 `Requirement` 字段引用；完整逐行矩阵见 `testing/suites/traceability/establish-node-access.md`。
- `NA-033` 和 `NA-034` 保持 `LX` + `human`；本次未签署，必须由独立人员执行。

| Capability | Requirement | Scenario 数 | 显式引用的稳定用例 |
| --- | --- | ---: | --- |
| node-access | 完整节点独立启动 | 2 | `NA-001`、`NA-011`、`NA-032` |
| node-access | 网络可达不授予管理权限 | 3 | `NA-012`、`NA-013`、`NA-033` |
| node-access | 接入失败不泄漏敏感信息 | 1 | `NA-002`、`NA-014` |
| node-access | 节点关闭具有资源所有权 | 2 | `NA-015`、`NA-016` |
| node-access | 重启保持可信状态 | 2 | `NA-009`、`NA-017` |
| client-trust | 本机命令启动 Web 配对 | 1 | `NA-018` |
| client-trust | 本机确认后才建立信任 | 2 | `NA-019`、`NA-020`、`NA-034` |
| client-trust | 配对限时且不可转授 | 3 | `NA-005`、`NA-006`、`NA-021`、`NA-022`、`NA-023` |
| client-trust | 配对过程核实目标节点 | 1 | `NA-024` |
| client-trust | 身份必须经过验证 | 2 | `NA-003`、`NA-004`、`NA-025` |
| client-trust | 信任建立必须可靠保存 | 2 | `NA-007`、`NA-026`、`NA-031` |
| client-trust | 可信客户端可查看和撤销信任 | 3 | `NA-027` |
| client-trust | 撤销覆盖已有连接 | 2 | `NA-008`、`NA-028` |
| client-trust | 信任不代替运行时审批 | 1 | `NA-010`、`NA-030` |
| client-trust | 认证材料不得作为管理数据返回 | 1 | `NA-029` |

覆盖含义仅限：用例的 `Requirement` 字段显式写出了长期规格中的 Requirement 和 Scenario，结构检查器能验证引用关系。它不证明断言完整、异常边界充分、代码实现正确或产品实际通过。

## 实际执行结果

| 检查 | 结果 | 说明 |
| --- | --- | --- |
| `node testing/suites/verify-suite.mjs` | 通过 | 34 个用例（node-access 12、client-trust 22），15/28 个 Requirement/Scenario，28 行矩阵，`NA-033`/`NA-034` 为人工 LX。该检查器明确不验证语义充分性、产品行为或执行结果。 |
| `node --check testing/suites/verify-suite.mjs` | 通过 | 脚本语法可解析。 |
| `node clients/web/node_modules/typescript/bin/tsc --noEmit -p clients/web/tsconfig.json` | 通过 | Web TypeScript 检查退出码为 0。 |
| `node clients/web/node_modules/vitest/vitest.mjs run --root clients/web` | 通过 | 6 个测试文件、26 个测试通过。 |
| `node node_modules/@fission-ai/openspec/bin/openspec.js validate establish-node-access --strict` | 通过 | 输出 `Change 'establish-node-access' is valid`。 |
| `cargo fmt --manifest-path app/Cargo.toml --all --check` | 通过 | Rust 格式检查退出码为 0。 |
| `cargo check --manifest-path app/Cargo.toml --all-targets --locked` | 通过 | 编译检查退出码为 0；仅有增量缓存 hard-link 回退警告。 |
| `cargo clippy --manifest-path app/Cargo.toml --all-targets --locked -- -D warnings` | 通过 | 退出码为 0；仅有增量缓存 hard-link 回退警告。 |
| `cargo test --manifest-path app/Cargo.toml --test jws_vectors --locked` | 通过 | 4 个 JWS 向量测试通过。 |
| `git diff --check` | 通过（有限范围） | 退出码为 0，只有 LF→CRLF 提示；该命令不检查未跟踪的 `testing/` 文件，因此不能作为新增资产的完整空白检查。 |

### 沙箱失败，不是产品失败

`cargo test --manifest-path app/Cargo.toml --lib --locked` 在沙箱中启动了 109 个测试，但观察到至少 32 个 `FAILED`，且 `platform::windows::tests::named_pipe_enforces_owner_dacl_sid_and_first_instance` 超过 60 秒仍未完成；为避免挂起已中断进程。单独重跑 `data_dir::tests::secures_existing_managed_files` 得到：

```text
Io { context: "opening the data directory lock",
     source: Os { code: 5, kind: PermissionDenied, message: "拒绝访问。" } }
```

这是当前沙箱/终端权限环境限制，不能记录为产品通过或产品失败；需要非沙箱 Windows 会话重新执行完整 Rust 测试。

### 未执行或不能作为通过

- `npm --prefix clients/web run check`、`npm --prefix clients/web run test`：当前终端对 `npm.ps1`/`npm.cmd` 的授权拦截未完成；直接 Node 入口的 TypeScript 和 Vitest 已通过，但不声称 npm 脚本或构建通过。
- `npm audit`、提交范围 lint、CI secrets 扫描、真实完整 `npm run check`：未执行。
- L5/L6 真实产品链路、真实直接 TLS、受信证书、Tailscale、真实 Chromium 全进程重启、跨 Windows 用户、DPAPI/DACL/命名管道所有权、故障注入和 15 秒关闭期限：未执行。
- `NA-010`/`NA-030` 依赖后续 Harness 审批边界；本 change 没有可运行的后续 Harness，未执行，不能据此声明真实 Harness 集成。
- `NA-033`/`NA-034`：人工验收未签署，`not signed; human acceptance pending`。

## 稳定用例状态

状态定义：`部分执行`只表示本报告列出的静态/工程检查结果，不表示对应用例的产品行为通过；`未执行`表示没有完成该用例的步骤和断言；`未签`表示需独立人员落签。

| ID | 当前状态 |
| --- | --- |
| `NA-001` | 部分执行：格式、编译、Clippy、Web 类型/Vitest 和 JWS 向量通过；完整 `npm run check` 及产品启动未执行。 |
| `NA-002` | 部分执行：OpenSpec 严格校验和差异检查通过；audit、提交 lint、secrets 与产品供应链检查未执行。 |
| `NA-003` | 未执行（仅长期定义；开发方向量证据不作为独立验收）。 |
| `NA-004` | 未执行。 |
| `NA-005` | 未执行。 |
| `NA-006` | 未执行。 |
| `NA-007` | 未执行；提交失败/响应丢失的独立故障探针未建立。 |
| `NA-008` | 未执行；撤销并发栅栏和故障注入未独立执行。 |
| `NA-009` | 未执行；身份初始化崩溃一致性未独立执行。 |
| `NA-010` | 未执行；后续运行时审批边界未实现。 |
| `NA-011` | 未执行；真实启动、失败释放和端口/存储故障未独立执行。 |
| `NA-012` | 未执行；跨 Windows 用户、DACL、命名管道和独占锁未独立执行。 |
| `NA-013` | 未执行；localhost/Tailscale 认证边界未独立执行。 |
| `NA-014` | 未执行；无效认证材料及日志/secret 扫描未独立执行。 |
| `NA-015` | 未执行；正常关闭、订阅收敛和重启未独立执行。 |
| `NA-016` | 未执行；关闭超时/失败注入和诚实报告未独立执行。 |
| `NA-017` | 未执行；正常重启、损坏身份和保护失败未独立执行。 |
| `NA-018` | 未执行；真实 Web 输入配对码流程未独立执行。 |
| `NA-019` | 未执行。 |
| `NA-020` | 未执行；默认回车、取消和拒绝路径未独立执行。 |
| `NA-021` | 未执行。 |
| `NA-022` | 未执行。 |
| `NA-023` | 未执行。 |
| `NA-024` | 未执行；节点替换、来源和 Host 边界未独立执行。 |
| `NA-025` | 未执行；公钥持有证明与有效身份认证未独立执行。 |
| `NA-026` | 未执行；持久提交成功/失败和响应丢失未独立执行。 |
| `NA-027` | 未执行；列表、撤销幂等、过期结果和存储失败未独立执行。 |
| `NA-028` | 未执行；已有连接、订阅、发送队列和自撤销未独立执行。 |
| `NA-029` | 未执行；列表输出最小化和静态/浏览器泄漏检查未独立执行。 |
| `NA-030` | 未执行；后续 Harness 审批隔离不可运行。 |
| `NA-031` | 未执行；IndexedDB、浏览器全进程退出和密钥丢失未独立执行。 |
| `NA-032` | 未执行；Windows + Web + 受信 TLS + Tailscale 完整闭环未执行。 |
| `NA-033` | 未签；需独立人工安全/依赖检视。 |
| `NA-034` | 未签；需独立人工核对配对提示和错误可理解性。 |

## 发现的问题

### NA-QA-01：`pending_confirmation` 后丢失批准响应/节点重启，Web 缺少公钥恢复转移

- 严重性：Medium（静态初判，未动态复现，未独立验收）。
- 依据：`clients/web/src/main.ts:112` 对已持久化 `pending_confirmation` 只显示待确认并轮询旧配对；`clients/web/src/pairing/session.ts:138` 的轮询要求 `pairingId` 非空且状态仍为待确认；`clients/web/src/main.ts:374` 的管理入口要求节点绑定资料完整。
- 影响：信任可能已在节点提交，但浏览器仍停留在旧配对状态；节点重启会清除临时配对，而现有服务端测试覆盖了 `clientId=null` 的持有证明恢复路径（`app/src/node.rs:1796`），UI 没有展示相同转移。
- 建议：强化 `NA-007`、`NA-026`、`NA-031` 的“批准响应丢失 + 节点重启”步骤，验证浏览器能否用已保存私钥和公钥发起 WSS 恢复；在动态确认前不要写成已确认产品缺陷。

### NA-QA-02：关闭成功报告可能早于 IPC 任务最终清理结果

- 严重性：High（静态初判，未动态确认；涉及关闭诚实性）。
- 依据：`app/src/node.rs:340` 先根据当前 `failures` 构造 `ShutdownResult`，`:345` 调用 `self.control.complete(report.clone())`，之后 `:347-359` 才等待并检查 `ipc_task` 的超时或错误；后续失败只进入最终返回的错误，不会更新已经发送的 `report`。
- 影响：IPC 任务在成功报告之后清理失败或超时时，控制通道可能已报告 `Completed`，与“关闭失败不得报告完全成功”的规格边界不一致。
- 建议：在 `NA-016` 增加“IPC 收尾在报告后失败/超时”的故障场景，分别核对 `daemon stop` 响应、控制通道报告和最终退出结果；修复后必须用新版本重测。

### NA-QA-03：批准/撤销的故障矩阵和竞态仍主要依赖开发方测试

- 严重性：Medium（测试缺口，不等于已确认产品缺陷）。
- 依据：`app/src/trust.rs:854` 已有撤销语句/提交回滚开发测试，`app/src/node.rs:1568` 已有撤销清空队列开发测试；但尚无独立验收探针覆盖批准提交各阶段失败、响应丢失后的真实浏览器恢复，以及撤销与受理/缓存/入队/发送的独立并发栅栏。
- 影响：`NA-007`、`NA-008`、`NA-026`、`NA-027`、`NA-028` 的关键断言目前只有设计和单元测试线索，不能据此关闭独立验收。
- 建议：建立可控存储/时钟/调度故障探针，分别覆盖提交前、提交中、提交后响应丢失和撤销并发顺序；保留失败证据，不以开发方自测代替。

## 独立性边界

- 本报告只证明长期套件存在、34 个 ID 保留、28 个 Scenario 有显式引用，以及若干工程检查在当前沙箱中通过。
- 本报告不证明任何 L5/L6 产品功能实际工作，不证明安全边界、供应链、真实 Harness、真实 TLS 或人工可用性通过。
- 未提交、推送、归档；未签署 `NA-033`/`NA-034`；未删除任何现有证据。
