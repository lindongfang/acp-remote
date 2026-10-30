# WP1 现有工程检查基线

日期：2026-10-30。独立 tester，仅收到 WP1 前置核验范围、仓库与文件依据；未继承 coder 对话，不以开发自评为证据。本报告是现有上游工程基线，**不是 CL-001/002 或会话产品交付通过结论**。

## 固定版本与环境

- HEAD：`65b46c01d6b95fc1f1a8223b1c92a729a885c185`；启动时 Git 工作区 clean。
- 固定范围：`git ls-files app clients/web package.json package-lock.json rust-toolchain.toml .cargo .github/workflows/ci.yml` 返回的实际文件；逐文件 SHA-256 清单位于 `.tmp/cl-wp1-baseline/source-before.sha256` 与 `source-after.sha256`，前后完全一致。清单 SHA-256：`BA98B556B25138AAAC7E3D8F96365C4E2B9553548C4434FD0AE8457957528967`。
- Windows NT 10.0.26300.0，Node v22.20.0，npm 10.9.3，rustc/cargo 1.98.1。根目录和 Web 的 node_modules 运行前已存在，没有执行依赖安装。
- Rust 独立输出目录：`D:/Project/acp-remote/.tmp/cl-wp1-baseline/target`；检查与构建串行执行。未启动 daemon、用户配置、实际数据库或原生会话。
- 结束时出现其他执行者的未跟踪 session-contract.md 及本 change 测试目录；本 tester 未修改它们。上述产品源码与锁文件摘要未变。

## 实际结果

| 项目 | 结果与覆盖 |
| --- | --- |
| `npm.cmd run check`（真实 owner 环境） | 退出 0。Web tsc、Vitest 和 Vite 构建通过：7 个文件、50 项测试。Rust fmt、clippy 全目标且警告视为错误通过；库测试 144 项、JWS 集成测试 4 项通过，0 失败、0 忽略；main 与两个 examples 各 0 项，不将其计为功能测试。 |
| `npm.cmd run build:rust`（上述检查完成后） | 退出 0。重新执行 Web tsc/Vite，成功构建包含 Web 资源的 Windows 开发 exe。 |
| `node_modules/.bin/openspec.cmd validate deliver-codex-session-loop --strict` | 退出 0，返回 change valid；仅规划结构检查。 |
| `git diff --check` | 退出 0；仅差异空白检查。 |

真实 owner 环境的命令前置为仓库 CI 已登记的 `. ./testing/ci/windows-owner.ps1`，其新建对象 owner probe 成功；仅修改当前进程令牌默认 owner，没有修改系统持久配置。随后设置上述 CARGO_TARGET_DIR，使用 `cmd /d /c` 将 npm stdout/stderr 重定向到专用日志。

有效日志：

- `.tmp/cl-wp1-baseline/check-owner-final.log`，SHA-256 `4C76AA20C75C1B8B10AE2071018534EBD79879169E009DDB92031BC333917F74`。
- `.tmp/cl-wp1-baseline/build-rust.log`，SHA-256 `AE54EF3830282DECA265C8C4CC1D24EF96B3949DB233EC9CE2B0F7499064B9B5`。
- 相应 `.exit` 文件记录退出码；OpenSpec 输出保留于 `.tmp/cl-wp1-baseline/openspec.log`。

## 失败尝试与环境边界

第一次直接在 sandbox 运行 README 根命令：Web 50 项通过，但 Rust 出现 data_dir、identity、node、Windows 权限相关失败，最终未取得汇总；正常取消该自有执行会话。第二次 sandbox 尝试加载 CI owner 脚本遭 AuthorizationManager 拒绝，正常取消。不能将这两次不完整执行记为通过，也不能据此判定产品故障。

使用 require_escalated 的真实 owner 环境后，首次日志重定向受 PowerShell 的 ErrorActionPreference=Stop 影响，将 cargo 正常 stderr 识别为终止错误；该次未作为有效结果。改为原生 cmd 重定向后，上述全部现有测试通过。升级后的 CIM 检查未发现前轮遗留 cargo/acp_remote 进程。保留各尝试日志，不改动产品代码以绕过权限检查。

## 合同入口与用例状态

现有 package.json 的 check 确实覆盖根 Rust 和 clients/web；Rust `app/tests/jws_vectors.rs` 与 Web `tests/jws.test.ts` 共享固定 JWS fixture，验证身份、签名互操作、高 S、篡改与编码等上游节点合同。Web 其他测试覆盖 pairing、management、reconnect、api 等；Rust 库测试覆盖节点访问、撤销、持久化、IPC/TLS 等现有实现。这些不是通用 ACP 会话、Codex、审批与历史合同测试。

源码入口 `app/src/lib.rs` 与 Web src 仍是上游节点接入模块，没有本 change 的 ACP adapter、Codex Harness 和完整会话产品目标。README 的真实 Chromium JWS 页面入口存在，但本次没有启动浏览器运行，不据 Vitest 结果声明真实 Chromium 合同通过。

| 用例 | 本次状态 |
| --- | --- |
| CL-001 | 现有上游真实检查及构建通过；本 change 实际会话目标尚未实现，最终交付验收受阻/未执行。规划校验与差异检查分别通过。 |
| CL-002 | 现有工程静态检查及上游 JWS 合同通过；会话专用合同入口尚未建立。npm audit（根/Web）、真实 BASE/HEAD commitlint 与 CI gitleaks 密钥扫描本次均未执行，不能声明供应链或安全验收通过。 |

本次未新增测试、依赖或产品实现，未修改规划/tasks。保留专用构建目录和脱敏日志供复核；本报告不代签 reviewer，不解除后续真实 Harness、会话测试与完整最终集成版本验收要求。
