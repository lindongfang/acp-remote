# Pi 接入设计

## Context

动机及用户确认见 [proposal.md](proposal.md)。本变更当前处于远期需求与设计定义态（`.openspec.yaml` 标记 `x-deferred: true`），具体施工工作包划分与验收用例在正式排期移除 `x-deferred: true` 时基于当时真实代码树建立。当前行为权威为 `openspec/specs/`；归档的通用闭环资料仅解释现有实现边界，不继承旧 DeepSeek/ZCode 的孤立目录或历史结论。

已核对真实接入源码与平台边界：
- `app/src/adapters/acp/node_configuration_profile.rs`（`parse_profile`）严格按闭集字段解析 `format: 1` 配置：仅含单一 `launch`（`executable_utf16le_base64`、`arguments_utf16le_base64`、`environment`）与 `recipes`（`wire`、`mcp_servers`、`additional_directories`、`extra`），多出未知字段直接拒绝。
- `app/src/domain/native_catalog.rs`（`HarnessSummary`）、`catalog_draft_document.rs` 与 `app/src/node/catalog.rs` 仅维护单一 Harness 定义与显式 `source_id`；`app/src/adapters/acp/strategy.rs`（`Strategy::select`、`initialize`、`validate_setup_caps`）仅读取对端单一 `agentInfo`/`info`，固定发出空客户端能力（`clientCapabilities: {}` / `capabilities: {}`），并在下发非空 `mcp_servers` 或 `additional_directories` 前校验对端能力位；`app/src/adapters/acp/transport/state.rs`（`reserve_unsupported`）对 `session/request_permission` 之外的反向请求统一回 `-32601 Method not found`。
- `app/src/platform/process_owner.rs`（`OwnedProcess::spawn`）先强制要求 `executable` 与 `cwd` 均为绝对路径，再以挂起态直接调用 Win32 `CreateProcessW`（`lpApplicationName = spec.executable`，无任何隐式 Shell）并绑定 `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` 作业对象。
- `clients/web/src/management/workspace-catalog-view.ts`（`validateInstallHarnessInput`、`parseLaunchArgsLines`）已提供通用 `executable`、按行 `trim` 过滤空行的 `args`、`env`、`wire` 与 `additionalDirectories` 输入，但当前硬编码 `mcpServers: []`、`extra: {}`。

### 调研依据与证据等级

- 2026-11-16 读取 Pi 上游 README（https://raw.githubusercontent.com/badlogic/pi-mono/main/packages/coding-agent/README.md），当前指向 `earendil-works/pi`。用户已明确本机已安装 `pi` 且**本项目不绑定 Pi 版本**；实施时只读定位本机已有入口并对接，绝不在仓库锁死版本或擅自升级用户安装。
- 同日读取 `pi-acp` README（https://raw.githubusercontent.com/svkozak/pi-acp/main/README.md），其描述 ACP→Pi RPC 子进程链、会话映射及思考流、Client 文件/终端委托、MCP 传递限制。该仓库仍属待审候选，本地尚未实跑验证。

## Goals / Non-Goals

**Goals:**
- 在不修改通用产品代码、不绑定本机 Pi 版本的前提下，基于现有目录与进程边界建立可消费的 Windows 绝对路径原生可执行启动配置模板、`testing/probes/pi-harness/` 隔离探测与验收入口、逐能力实证报告及 `docs/implementation/` 使用说明。

**Non-Goals:**
- 扩充平台双组件目录字段、私改通用 Web 安装表单、实现 Pi RPC 客户端、私读映射或原生会话文件、在仓库打包/锁定 `pi` 本体版本、添加隐式 Shell 启动兜底或直接触碰用户正在使用的真实会话。

## Decisions

### 1. 不绑定 Pi 版本与外部 ACP 服务端隔离探测

本机 `pi` 视为用户外部运行环境：仓库根、`app/`、`clients/web/` 及 `testing/probes/pi-harness/` 的任何依赖清单、锁文件、配置模板或脚本均**不添加 `pi` 包、不硬编码或校验特定 Pi 版本号**。本地 `pi` 升级后直接沿通用 ACP 重新协商。

参照 `testing/probes/codex-session-loop/` 的成熟做法，正式开工的首个前置探测阶段在 `testing/probes/pi-harness/discovery/` 建立隔离探针：若本机未预装外部 ACP 服务端，仅在该探针子目录按项目供应链审查规则锁定候选适配器（如 `pi-acp`）自身测试依赖，不污染产品依赖，且通过环境变量或参数调用本机已安装的 `pi`。单次探测与验收报告只读采集当次实测观察到的 `pi` 版本与适配器版本作为证据上下文，不形成运行时白名单。若候选适配器许可证、安全或基础 ACP 语义不达标，在前置探测阶段直接出具阻断报告，不自制私有桥接。

### 2. 单一启动入口、Windows 绝对路径无 Shell 启动链与双组件证据分层

复用现有 `format: 1` 单一 `launch` 与单 `HarnessSummary` 模型，不在节点运行时拆分“适配器身份 + Pi 本体身份”。系统层保存完整启动命令链（`executable` + `arguments` + `environment`）与显式 `source` 并记录连接协商出的对端 `agentInfo`；双组件身份与当次观测版本的拆分对照放在 `testing/runs/integrate-pi-harness/` 报告与说明文档中完成。

针对 `OwnedProcess::spawn` 强制要求绝对路径且直接将 `executable` 传给 `CreateProcessW` `lpApplicationName`（无 Shell）的真实约束：
- 标准配置模板将 **Windows 绝对路径**的原生 PE 可执行文件（如 `C:\...\node.exe` 占位路径，不接受裸 `node.exe` 相对文件名）设为 `executable`，将外部 ACP 服务端入口脚本绝对路径及转发参数放入独立 `arguments` 数组，通过 `environment`（覆盖或默认继承 `PATH` 及隔离配置根）让适配器拉起本机 `pi`；配置模板仅含占位路径与非敏感示例，不含用户真实绝对路径或密钥。
- 相对路径可执行名（触发 `InvalidSpec`）与直接把 `.ps1` 绝对路径设为 `executable`（触发 `CreateProcessW` 非可执行失败）分别作为启动拒绝用例验证；避免依赖 `.cmd` 间接包装层以免引入额外 `cmd.exe` 子进程干扰管道与退出观测。

### 3. 客户端空能力委托核验与底层 Profile / 通用 Web 表单分层

本平台连接初始化固定发送 `clientCapabilities: {}`，且对非 `session/request_permission` 的反向请求一律回 `-32601`。因此前置探测与后续闭环验收必须实证核查：在此空客户端能力下，`pi-acp` + 本机 `pi` 是由 Pi 本地内置工具直接执行读写/命令，还是因请求 `fs/*`/`terminal/*` 被 `-32601` 阻断；以及本地执行时是否仍产生 ACP `session/request_permission`（若不产生，如实记录审批能力未提供，绝不伪造审批请求或默认批准）。

配置传递按真实通道分层验证：
- **底层节点配置合同（`format: 1`）**：通过 `parse_profile` / 节点认证目录上传接口验证完整 UTF-16LE 参数（含空格、引号、非 ASCII、首尾空白）以及在对端声明相应能力位时的非空 `mcp_servers`、`extra` 字段解析与出站保真，并取证真实适配器是否向 `pi` 生效而非静默丢弃。
- **通用 Web 表单范围（`workspace-catalog-view.ts`）**：按页面当前实际能力验证绝对路径 `executable`、按行 `trim` 的非空 `args`、`env`、`wire` 与 `additionalDirectories` 登记、能力错误、历史不完整提示、审批及取消/释放分开展示；页面当前固定 `mcpServers: []`、`extra: {}` 如实写入 `docs/implementation/pi-harness-web.md` 与报告限制说明，不在本 change 越界修改 `clients/web/src/`。

### 4. 目录收敛、原子归档同步与 Windows 完整进程树关闭

全部专项资产对齐仓库既有目录体系：探测、无密钥配置模板、开发验证与验收脚本归入 `testing/probes/pi-harness/`（按 `discovery/`、`profiles/`、`verify.ps1`、`acceptance/` 划分子路径以隔离写范围），实施与使用说明归入 `docs/implementation/pi-harness.md` 与 `docs/implementation/pi-harness-web.md`，运行证据归入 `testing/runs/integrate-pi-harness/`；集成交付门禁通过并归档时，在同一提交按原子顺序同步长期规范 `openspec/specs/pi-harness-compatibility/spec.md`、移入 `openspec/changes/archive/` 并合并长期用例套件 `testing/suites/pi-harness-compatibility/tests.md`（含 `Source definitions:` 与四列 `## 需求关联` 100% 映射表，禁止长期套件跨目录指向活动 change），不新建 `configs/`、`scripts/`、`testing/harnesses/` 或 `docs/harnesses/`。

资源关闭遵循 `OwnedProcess` Job Object 与 `LESSONS.md` 硬约束：取消等待原生工作可靠终态（常驻 `pi-acp`/`pi` 进程可保留）；实例关闭或节点停止须协调原生 runtime 关闭并确认 Job 内全部后代进程（含 `pi-acp` 拉起的 `pi` RPC 子进程及工具孙进程）归零，超时保留清理与 join 责任。严禁按 `pi`/`node` 进程名批量强杀，严禁将父进程退出冒充后代已释放。

## Risks / Trade-offs

- [本机 `pi` 具体入口与候选适配器表现未实测] → 正式开工首阶段在隔离探针目录只读定位本机 `pi` 并实测候选适配器，未满足条件不推进后续配置与验收开发。
- [平台 `clientCapabilities: {}` 导致 `pi-acp` 工具委托报错或无审批事件] → 实证区分“-32601 阻断工具”与“Pi 本地直接执行无 ACP 审批”，按真实结果记录能力边界，不私加未授权客户端委托执行器。
- [通用 Web 表单未暴露 `mcpServers`/`extra`] → 分开验证底层 `format: 1` 文档通道与当前 Web 表单已暴露字段，不伪称页面已支持编辑。
- [本机认证、扩展与真实会话被测试污染，或升级改变行为] → 强制使用 `.tmp/` 下独立会话/配置根，不绑定 `pi` 版本；升级后旧报告归属原观测组合，未复验项标未验证。
- [适配器拉起多层后代进程] → 以 Job Object 活动进程计数与进程树快照取证，mock 不替代真实验收。

## Migration Plan

本轮无迁移、安装或产品依赖变更。后续实施仅在 `testing/probes/pi-harness/`、`docs/implementation/`、`openspec/specs/` 与 `testing/suites/` 交付专项资产，运行时复用现有 SQLite 目录模式，不在仓库绑定 `pi` 版本。探针如引入第三方适配器测试包须单独审查并在探针目录锁定自身解析结果。回滚只需移除专项探针/说明并协调关闭本次测试实例，不触碰用户本机 `pi` 安装、原生会话、认证与全局设置。

## Open Questions

本机 `pi` 实际可调用路径、候选外部 ACP 服务端的供应链/许可证审查、空客户端能力下的真实握手与委托表现、隔离配置/会话根环境变量及进程树验证方案留待移除 `x-deferred: true` 进入实施准备后的前置探测阶段实证解决。若实证结果要求改变通用 ACP 代码、扩充目录 schema 或引入客户端文件/终端委托能力，则超出本 change 边界，须停下报告并另行决策。
