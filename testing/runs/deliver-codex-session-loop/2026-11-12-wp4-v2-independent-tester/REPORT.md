# WP4 v2 独立 tester 报告

## 结论

针对 `SOURCE.json` 的
`sourceSetSha256=f9693f4061118a7965cd2821244420322a668b0b7afcf689f94dceec99d971d3`，
WP4 原生释放持久化、真实认证 WSS actor 绑定和 ACPI 路由指定关键门复跑通过：

- schema10：2/2 通过，退出码 0。
- native release repository：2/2 通过，退出码 0。
- 真实认证 WSS actor 绑定：1/1 通过，退出码 0。
- ACPI R01–R09 与 codec：13/13 通过，退出码 0。
- ACPI R10 三个独立用例：各 1/1 通过，退出码 0。
- 合计复跑 21 个测试，0 failed；native child 构建另以退出码 0 通过。

本结论仅对上述 `sourceSetSha256` 有效。它不代签 reviewer，不证明
`deliver-codex-session-loop` 已可归档，也不把 OpenSpec 任务勾选当成已完成。

## 证据完整性

- `SOURCE.json` 哈希：声明值 `29d5d1cda89459cd534ce4fe243927beb24f83e04f926cb4ca9085315b4766b7`，实测一致。
- `MANIFEST.json` 的 75/75 个文件和字节数均匹配，0 mismatch。
- `SOURCE.json` 列出 512 个文件；冻结源码树实际 512 个文件，逐文件 SHA-256 均匹配，0 mismatch。
- 按 `path + NUL + sha256 + LF` 重新计算的 source set 哈希与声明值一致。
- 全部关键门复跑后再次核验冻结源码树，仍为 512/512 匹配。
- 详细机器记录见 `hash-verification.json`。

## 复跑结果

所有命令均从显式 workdir `D:\Project\acp-remote` 启动，使用冻结源码
`.tmp/wp4-native-release-regression-source-v2`、独立
`.tmp/wp4-v2-independent-tester-target` 和独立
`.tmp/wp4-v2-independent-tester-runtime`。

| Gate | Exact command file | Tests | Exit |
| --- | --- | ---: | ---: |
| native child build | `native-child-build.command.txt` | build | 0 |
| schema10 | `schema10.command.txt` | 2/2 | 0 |
| native release repository | `native-release-repository.command.txt` | 2/2 | 0 |
| real authenticated WSS actor binding | `actor-binding-wss.command.txt` | 1/1 | 0 |
| ACPI R01–R09 | `acpi-r01-r09.command.txt` | 13/13 | 0 |
| ACPI R10 open after bundle | `acpi-r10-open-after-bundle.command.txt` | 1/1 | 0 |
| ACPI R10 rejection, no listener | `acpi-r10-rejection-no-listener.command.txt` | 1/1 | 0 |
| ACPI R10 control only | `acpi-r10-control-only.command.txt` | 1/1 | 0 |

每组都有对应 `.stdout.txt`、`.exit.txt`、`.summary.json` 和
`.invocation.json`。原始开发输出中对应门禁的记录已由
`original-gate-summary.json` 复核，计数与本轮独立复跑一致。

## 进程与限制

- 全部门禁结束后独立检查 `acp-remote` 和
  `native_dispatch_dev_child` 两类进程，live count 为 0；见
  `process-cleanup.json`。
- 第一次启动在 dot-source `testing/ci/windows-owner.ps1` 时被本机
  PowerShell execution policy 拒绝。Cargo 和产品测试尚未启动。随后仅对当前
  PowerShell 进程使用 `ExecutionPolicy Bypass` 重试，未改机器或用户策略；见
  `preflight-failure-01.txt`。
- 后续封装工具曾有两次复合调用无输出结束，并遇到过 Windows PowerShell 5.1
  不提供 `Path.GetRelativePath` 的错误。无输出结束未改变任何测试输出；路径
  接口问题仅影响本轮证据清单生成，修正为路径前缀截取后成功生成
  `RESULT.json` 和 `MANIFEST.json`。这些均不是 Cargo 或产品测试失败。
- 没有 Cargo 测试失败。没有执行完整 WP4 回归、全仓 Rust/Clippy、Web、真实
  Harness 或 OpenSpec 改写；因此本报告只回答指定同版关键门。
- 没有操作真实数据库、原生会话或凭据，没有修改生产/测试源码，没有修改
  OpenSpec 任务、提交、推送、发布或归档。
