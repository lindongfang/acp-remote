# 最新用例集成检查

基线：2026-10-30-acceptance.sha256，69 项，清单 SHA-256 为 27AAB89FA0092439FB9FC16754C21113921620166C179263F481F8D4C7B40932。原始清单生成于源码冻结时；报告新增不改变清单内产品版本。角色对该版本独立判断证据复用，不自动继承旧版逐例结果。

## 本轮实际执行

- 真实 Windows 用户环境，以 `.tmp/acceptance-20261030-target` 为 CARGO_TARGET_DIR 执行 `npm run check`，退出 0。Web 类型检查、7 文件/38 测试、生产构建通过；Rust 格式、Clippy、118 项库测试、4 项 JWS 向量通过。main/examples 的零测试目标不单独计为功能测试。
- 同隔离目录执行 `npm run build:rust`，退出 0，包含 Web 构建及单节点嵌入资源编译。构建曾等待审计安装的 package cache 锁；取消挂起安装后正常完成。未停止用户 30447 节点。
- 根 `npm audit --json`：0 漏洞，125 个依赖；Web audit：0 漏洞，81 个依赖。沙箱首次尝试 DNS 失败，切换真实网络环境后重新执行成功，未将初次命令串的末尾退出码当审计成功。
- `openspec validate establish-node-access --strict` 与 `git diff --check` 通过；换行提示不代表功能结论。
- CI 固定 Gitleaks 8.30.1 的 GitHub 官方 zip 下载超时，零字节；未执行扫描。
- `cargo install cargo-audit --locked --root .tmp/cargo-audit-tool` 解析到 0.22.2 后长时间无下载进展，按确认后的本任务 cargo 安装进程取消以释放缓存锁；未执行 Rust 公告库扫描。

## 验收边界

NA-033 用户确认通过见同日 user-acceptance.md。本报告仅开发集成检查，不代签独立 tester/reviewer、不将局部断言升级为整例通过、不改变待验收任务。新版测试定义没有删除 Windows 隔离、故障注入、审批边界或供应链检查的必要断言。
