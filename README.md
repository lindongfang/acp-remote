# ACP Remote

ACP Remote 是面向个人使用的 Harness 管理平台，计划在一个用户控制的节点内，统一管理会话、执行、权限审批、历史与审计。

Harness 是实际运行任务的程序，负责模型推理和工具调用；ACP（Agent Client Protocol）是平台与这些程序交互的协议。首批计划接入 Codex、Claude Code / Claude Agent SDK 和 DeepSeek Harness。

## 当前状态

项目处于新架构设计阶段，尚无可运行的后台服务、命令行工具或前端产品。当前设计目标不代表已经实现的能力，最终接口、数据库和状态转换仍需补齐。

完整产品目标见 [Harness Control Plane 设计](docs/HARNESS_CONTROL_PLANE_REDESIGN.md)。

## 开始参与

开发工具使用 Node.js，版本以 [.nvmrc](.nvmrc) 为准。在仓库根目录执行：

```sh
npm ci
```

`npm ci` 安装开发工具并配置本地 Git 钩子。当前尚无产品构建和测试入口；相关命令将在实现落地时提供。

当前没有 Rust 工作区；后续 Rust 开发使用 [rust-toolchain.toml](rust-toolchain.toml) 指定的工具链。

## 仓库导航

| 位置 | 用途 |
|---|---|
| [docs/](docs/) | 产品与技术设计，目前包含已确认的产品方向 |
| [AGENTS.md](AGENTS.md) | 编码 Agent 的工作约束 |
| [openspec/config.yaml](openspec/config.yaml) | 变更流程规则 |
| [openspec/schemas/](openspec/schemas/) | 变更产物结构与模板 |
| [openspec/role/](openspec/role/) | 开发、测试与审查职责 |
| [.agents/](.agents/) | 技能与工具资源 |
| [.github/](.github/) | 持续集成与协作配置 |

## 贡献与检查

提交信息规则以 [commitlint.config.mjs](commitlint.config.mjs) 为准。本地 Git 钩子校验提交信息；持续集成另执行开发工具依赖审计与密钥扫描，具体步骤见 [CI 配置](.github/workflows/ci.yml)。

## 许可证

[Apache-2.0](LICENSE)。
