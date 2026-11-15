# 管理输入候选交付

2026-10-31，隔离 coder 候选；已停止编码，等待独立 tester/reviewer。本文件不是独立验收结论。未改主 D 工作区、schema 布局、node/trust/WSS/Web/依赖或验收预期，未读取实际数据库/凭据、未运行 Harness，未提交或推送。

## 固定版本与集成

- 候选工作区：本目录 `workspace/`；全部源码以 `implementation-source.frozen.json` 固定，SHA256：`a590e0e8d3ad57a6c659d41010db1c2b0406e9fef2c5806aa794312d97122a61`。
- 原始复制基线：`workspace/source-manifest.json`，覆盖原73个输入；不是仅凭 HEAD 声称版本。六个共享基线文件另保存在 `baseline/`，已逐一确认其 SHA256 与原复制清单一致。
- `management-input.candidate.patch` SHA256：`c1ec6a1a0e0caeb81e303be326ab55a9a4139cf0a5930e851dacc5b2b397c6aa`。共11文件，6修改、5新增。三个共享 mod 声明采用只含前置锚点的最小 hunk，保留主执行新增 views/ACP 声明；因此检查/应用需显式 `--unidiff-zero`，不能整文件覆盖共享声明。
- 实际只读检查：`git apply --check --no-index --unidiff-zero --whitespace=error <本目录绝对路径>/management-input.candidate.patch`，在隔离 `baseline/` 与当前 `D:/Project/acp-remote` 均 exit0。没有执行实际 apply。独立验收后父执行仍须按集成时实际差异重新检查与逐行合并。

## 已实现的限定范围

- 新内层命令/结果元数据与固定 input 摘要；`VerifiedProductRequest` 内移 domain，adapter 兼容 re-export；不可从外部普通字段构造/复制该证明。
- 应用定义 `ProductRepository`，SQLite adapter 实现，应用不依赖 SQL adapter。`CurrentActor`、`CommandOwnership` 不可外部构造；裸 `ActorContext` 不是新存储端口的 grant。真实认证、目标/原生事件、结构验证、签名与时钟通过可信装配端口提供，测试使用明确替身及真实 ES256 adapter。
- SQLite 受理、floor CAS、固定期限先于去重、独立 Command/Run/4槽原子写入、claim CAS、结果幂等/冲突、不可覆写终态槽及保守启动标记。每个事务使用从 pool detach 的自有连接；取消/未确认事务不回池。
- 非 Clone ticket/permit 不由查询、重试或重启重建。claim 接受自己的匹配 pending/Run；自发原生活动使本输入可靠零发送失败，不自动排队。最终围栏重新核对 actor、epoch、当前资源、可信事件及时间，首次写与插入/完成分开。
- 节点内存输入按16MiB/128MiB/64对象计预算，额外结构/引用保留量显式预留；主动 expire/revoke/stop，借出正文在 writer Drop 前继续计费。清理 supervisor 有 owner、取消与有界 shutdown。浏览器断开不得被装配成节点工作取消。
- gate/owner/SQL/首次 writer 的有界等待与取消；取消先失效发送 future，再释放 actor gate，结果保存不阻挡该 gate。取消实现显式 pin+enable 通知 future 后检查 sticky flag。
- `Storage` 一次持有并 clone 同一 ProductStore/health，不允许重新取 accessor 清空故障。启动发现遗留工作或未释放资源后仍关闭危险入口；本阶段没有最终协调或自行恢复 ready。

## 已执行的开发验证

以下均在隔离 workspace、独占 `target-management` 使用既有锁文件执行，未联网解析依赖。日志为本目录 `*.frozen.log`。

| 检查 | 实际结果 | 证据 |
| --- | --- | --- |
| cargo fmt -- --check | exit0 | fmt.frozen.log |
| cargo clippy --offline --lib --tests -- -D warnings | exit0 | clippy.frozen.log；测试目标编译不等于其所有测试均已执行 |
| cargo test --offline --lib product_commands --no-fail-fast | 27/27 | development-tests.frozen.log |
| cargo test --offline --test session_loop_acceptance | 原公开模型21/21回归 | public-model-regression.frozen.log；不是本阶段独立管理验收 |
| cargo test --offline --lib adapters::product_requests | 原签名开发8/8回归 | signature-regression.frozen.log |

27个开发测试包含：真实 SQLite 两独立 pool、同身份及同目标保护；受理9写点、claim5写点、结果4写点×3结果、启动6写点，真实延迟外键 COMMIT 失败、真实 COMMIT 成功但返回未知、COMMIT 前/后挂起取消和 CAS rows=0；floor 在 BEGIN 后取样；过期、改 epoch/资源、撤销后结果记录；原生自发活动零发送；1000次取消竞态及32等待者；正文预算/清理/借出引用；取消 writer 后驱动迟到队列零写、另一连接可取得 SQL 写锁、结果保存阻塞时撤销 gate 已释放。

默认命令边界使用完整合法的合成历史输入元数据（每条都有 Run 和4个 stage），在9999后由实际 API受理第10000条，再拒绝第10001条。审计12槽缩限真实 SQL、两 pool 争最后4槽及默认100000 checked算术分别验证；没有伪造“少于10000命令却有99996槽”的夹具。生产默认自然路径本阶段至多40000槽，不能签默认100000自然积累或完整 CL-083 通过。

## 未执行或未交付

- 当前真实 Windows 节点认证、实际 trust gate/撤销 writer 装配、系统时钟装配、WSS DTO/上传协议、真实 ACP 内容 schema/资源证据、原生 writer/会话、Windows 进程树和完整 L5/L6：not-run。本候选只提供可信端口和替身合同，不声称这些发行者已接线。
- 最终 reconciliation、来源/缺失范围持久记录及正式恢复 ready：未实现；其 slot 始终未发生，不写 placeholder/reconciled。正式元数据清理、恢复新请求 epoch、故障安全控制、审批和其他 action 仍由后续阶段负责，不据此关闭完整 CL-080–084。
- 安全/供应链独立结论：未执行。依用户决定不外发 Web 依赖元数据，未运行 Web npm audit；未新增依赖。
- 尚未进行本阶段独立 tester 执行和 reviewer 代码检视；未执行完整主工作区集成检查。不把上述开发自测代签为独立验收或整个 change 完成。

独立角色请读取固定合同 `8686d779...`、阶段定义 `009f48b0...`、规划 review、源码 manifest 和实际源码/日志；不依赖 coder 对话。发现阻断后由原提出者复验关闭，候选作者只按新任务修复并重新冻结。
