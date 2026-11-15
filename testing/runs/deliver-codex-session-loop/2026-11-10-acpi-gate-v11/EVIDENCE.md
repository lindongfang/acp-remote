# ACPI v11 开发验证证据

受测工作区 D:/Project/acp-remote；HEAD 65ec7815bb0280f4519989d93a7b4ed3b4b37994。未提交实现以 SOURCE.json 的895文件为准，source set SHA-256 EA84C8EB9AF368B6C1433D6313469D85CEE94AF0C65D76764423AE0BEBEF6F7C。2026-11-10补跑R10前逐文件核对，漂移0。

执行者为主执行/coder，本报告不是独立tester或reviewer签字。Windows owner前置为 testing/ci/windows-owner.ps1；使用非沙箱当前用户令牌、隔离TEMP=.tmp/acpi-route-v11-runtime-unrestricted、CARGO_TARGET_DIR=.tmp/acpi-route-v11-check；Cargo使用 --locked --offline。

## 路由结果

命令：cargo test --manifest-path app/Cargo.toml --lib --locked --offline acpi_ -- --nocapture --test-threads=1。
结果：13 passed、0 failed、899 filtered。原始输出 acpi-group.serial-unrestricted.stdout.txt。

| 范围 | 原始输出行 | 已执行开发断言 |
| --- | --- | --- |
| codec | 1383 | 头、方向、长度、溢出 |
| R01 | 1386 | 四命令、ACK、ready/body及原bundle |
| R02 | 1395 | 最近块重试、偏移冲突与错误阶段 |
| R03 | 1402 | preparing/ready/failed与原worker join |
| R04 | 1405 | 释放、跨连接及断线所有权隔离 |
| R05 | 1430 | ACPI/ACPV分类、坏帧及精确块边界 |
| R06 | 1433、1438、1453、1458 | 停止优先、调用前后取消、准备任务收敛 |
| R07 | 1465 | FIFO、in-flight/queued与permit回收 |
| R08 | 1472 | 撤销提交与发送gate顺序、其他actor隔离 |
| R09 | 1485 | 三类typed错误×status/Data六格矩阵 |

R07的帧饱和观察为8224字节/256 queued（另有1个in-flight ACK）；byte分支用测试资源预占MAX_OUTBOUND_BYTES-31，不能称自然网络已耗尽预算。R08按commit先和发送授权先分别断言，不宣称追回已写字节。R09每格一次故障命中，退出后原owner封存与连接计数归零；测试断言是取证依据，日志不含逐帧抓包，不把此报告当独立验收。

R10于同一source set分别执行以下过滤器（共同参数同上），三项各1 passed、0 failed，退出码均0；原始stdout/stderr按r10-<过滤器>命名，机器结果见r10-results.json。

- production_factory_opens_listener_only_after_the_bundle_is_complete：发布前地址不可连接、完整bundle后ready、原lifecycle join及目录锁回收。
- production_factory_service_rejection_opens_no_listener_and_joins_workers：ready失败、未开放listener、原preparation report all_joined和原lifecycle join。
- queue_read_control_only_does_not_report_an_empty_product：四条上传命令product_not_ready、无伪造uploadId/ACK；首连及重连owner计数归零。

R10的重复/foreign bundle与完整三服务路径由R01交叉覆盖；旧upload断线隔离由R04覆盖。r10-process-after.json记录指定进程名快照为空，这不是完整Windows Job树枚举证明；真实任务join由上述断言独立证明。

## 其他检查及失败

- fmt check：exit0；OpenSpec validate --all --strict：11/11；git diff --check：exit0（换行警告保留）。
- Web check/test/build：exit0；29测试文件、667测试通过。
- strict Clippy --all-targets --locked --offline -- -D warnings：exit101，414诊断，详见clippy.stdout.txt；没有抑制warnings或放宽门禁。
- 沙箱并行与沙箱串行ACPI各1 passed/12 failed，原始acpi-group.stdout.txt和acpi-group.serial.stdout.txt保留；数据根/owner named-pipe Windows error 5使其不构成有效产品通过。
- 完整Rust all-targets、当前完整npm check、真实Codex/DeepSeek+Web闭环、供应链最终检查与独立v11验收/检视未完成。npm audit按用户要求不执行。

本次仅补齐R10及证据，不修改产品/测试源码、不勾任务。4.1还需相关CL及认证/帧/深度/队列回归，1.4仍受strict失败阻断；总体9/28，尚不可归档。所有日志摘要见MANIFEST.json。
## 同版既有socket回归补充

使用相同Cargo参数、owner身份和target，以socket_independent_acceptance过滤器串行执行。首轮漏设P1_DATA_ROOT：1通过、14在初始化exclusive data root失败；原始socket-regression.*保留，不算有效业务验收。补齐预建绝对路径D:/Project/acp-remote/.tmp/acpi-route-v11-socket-regression后，15 passed、0 failed、897 filtered，18.02秒，exit0，见socket-regression-configured.*。

该组实际覆盖认证、撤销前块复制、排队字节不授发送权限、flush期间actor gate、严格形状/深度/命令限制、流式原始字节、credit与连接绑定、背压/队列界限、断开重连、真实flush错误/15秒期限和敏感字段格式。虽然测试资产名含independent，本次执行者仍为coder，不冒充独立角色签字，也不据函数名判定完整CL-009/017/024/030/031通过。

日志P1_RESOURCE明确记录retained_owned_directory和sqlx_worker_join=not_observed；本轮保留这些专用测试目录，不宣称SQLite物理worker退出或完整树清理已证明。未修改源码或断言。