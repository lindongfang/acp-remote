# Q-002 累计连接阈值移除：执行记录摘要

2026-10-31，主执行实际工具调用记录。只实现transport累计阈值删除，队列新行为仅规格/用例定义。现有用户未提交修改保留，不提交、不改真实数据和旧证据。

## 实际检查

| 命令 | 实际结果 | 范围 |
| --- | --- | --- |
| cargo test --manifest-path app/Cargo.toml --lib adapters::acp::tests::request_lifetime::sequential_peer_requests_do_not_exhaust_connection_lifetime --locked -- --exact（修改前） | exit1，0通过/1失败，sequence4096（第4097个）Capacity，0.22秒 | 旧阈值真实红回归 |
| 同一定向命令（修改后） | exit0，1通过/0失败，0.55秒 | v1/v2各5000个顺序审批/其他请求，之后客户端正常往返，最后旧ID仍拒绝 |
| cargo test --manifest-path app/Cargo.toml --lib adapters::acp::tests --locked | exit0，37通过/0失败，0.84秒 | 公共ACP开发夹具，377其他库例未执行 |
| cargo fmt --manifest-path app/Cargo.toml --all --check | exit0 | Rust格式 |
| cargo clippy --manifest-path app/Cargo.toml --all-targets --locked -- -D warnings | exit0 | 全目标静态检查；环境报告增量cache硬链接转复制，无Rust lint失败 |
| cargo build --manifest-path app/Cargo.toml --locked | exit0 | 实际Rust节点构建，未重新验收Web功能 |
| openspec.cmd validate --all --strict | 11通过/0失败 | 当前全部规格结构 |
| git -c core.safecrlf=false diff --check | exit0 | 差异格式，不改索引或仓库配置 |

## 红绿工具输出摘录

修改前：`request 4096 on V1: ACP request or buffer capacity is exhausted`，`0 passed; 1 failed`。

修改后：`sequential_peer_requests_do_not_exhaust_connection_lifetime ... ok`，`1 passed; 0 failed`。扩大到ACP子范围：`37 passed; 0 failed`。

## 独立复查与未执行范围

独立reviewer逐字核对transport的两处删除、复用固定SHA测试二进制独立运行定向1/1及ACP37/37，并复查116个用例定义和Q-002范围，见[独立报告](2026-10-31-q002-review.md)。该报告没有把主执行摘要冒作独立重编译或旧版本红复现。

CL-108定义的8193请求完整矩阵、CL-109全部未决64/65动态分支及CL-116真实节点审批owner集成未由本次5000开发例代签。完整队列、失败后新输入优先、Web及真实Harness仍待实现/验收，task6.7保持待办。

## 固定源码与定义SHA-256

| 文件 | SHA-256 |
| --- | --- |
| app/src/adapters/acp/transport.rs | 66f92868d13b556f502cb5a5992177c96440eac1f609da629397ba5cecc42a76 |
| app/src/adapters/acp/tests.rs | 56e152eccf99de55f9ef63aa770be07e5a33362b8306da47a18e84701c5f565a |
| app/src/adapters/acp/tests/request_lifetime.rs | 562b7f4bf03b40fd0d910b7faa4308304aad277b02416acd197d1afe6f5da81b |
| openspec/changes/deliver-codex-session-loop/specs/session-execution/spec.md | 051f1609527ee13e79c8e0e235a19b7085499c77fde164028cbd9b1cf7b7694d |
| openspec/changes/deliver-codex-session-loop/specs/acp-platform-constraints/spec.md | ba9b7d791efaa7a6e061060ecf67cda75f963225ca98318a9f092207e6a6cbe1 |
| openspec/changes/deliver-codex-session-loop/tests.md | 4bbb4d67676bbe6bf7fe31cc520b1d357a99f54831a98fcdbc734eefd65c7bcb |
