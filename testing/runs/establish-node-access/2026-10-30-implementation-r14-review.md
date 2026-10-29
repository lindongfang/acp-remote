# R14 IPC容量修复独立检视

日期：2026-10-30。结论：**PASS（本包实现及两项真实管道回归）**。不改写原tester的NA-BUDGET-IPC001真实FAIL或33 native候选失败；由其针对最终ordinary binary另行复测关闭原问题。本报告不与R8故障注入子断言混同，也不声明NA012整个跨SID/远程范围通过。

R14清单自身SHA256 `AB503036ACA9D970C9CF3D8D8EE99F80572E6DCF292252DA119DDD3E3F422A37`独立核对。因当前已是R15，不把R14清单全部条目说成当前全匹配；实际执行使用已核对83项的R15冻结副本，其ipc.rs `1F388E3EBE46DB7F64FA6F52E5C6352E6B4562AA6247E91C6E275E650ED2C408`、node.rs `B9CEEC40BAB47BF521849637E491C64B90B108AD994C25B8977FBA720AFF3B99`、windows.rs `5221E4E5184183CB7464ED9CCFF36080A16D35767F6529B0D38CC4EE7107FF5A`与R14清单完全相同。R15仅资源嵌入选择不改变本轮IPC结论。

## 实现检查

windows.rs:348 将native max_instances改为有限254，不是应用放宽到254。ipc.rs:34,325仍只有32个application permit；connect成功后先try_acquire，超额先drop本轮server再创建下个listener，避免将被拒客户端保留的native实例直接算成active应用连接或使节点fatal。permit由连接任务持有，退出即释放。

ipc.rs:400–426的bind_next_available只对真实OS code231做10ms异步背压重试；每轮先读stop当前值，sleep与watch changed竞争，无阻塞线程，无无界原生实例扩容。stop早于订阅也可观测，不丢停止。其他Win32错误继续返回有上下文的AppError，不把所有错误都当容量。await期间current/permit由serve局部所有；stop返回None时break释放，错误退出时JoinSet所有权中止已拥有任务，未产生游离重试任务。已有handlers持有的pipe可以释放容量，native占满不会主动驱逐其32个active请求。

## 独立实际执行

reviewer独立target `.tmp/reviewer-implementation-r4-target/r10-build`，真实当前用户上下文，从R15 frozen app编译并运行：

1. `node::tests::owner_ipc_keeps_thirty_two_active_connections_and_rejects_only_the_next`：1 passed/0 failed/141 filtered；真实33个owner pipe，前32保持读取未完成，超额连接被拒；32原status请求全部成功，释放后新status成功，节点正常shutdown。
2. 同exe `ipc::tests::exhausted_os_pipe_instances_wait_for_release_and_remain_cancellable`：1 passed/0 failed/141 filtered；真实254 native server handles使下一bind得到231，等待30ms仍pending而不是退出，释放一个后恢复；再次满额时stop使等待返回None，释放全部handles并清理自有目录。

两测试都exit0；不是伪造错误码，也不是只查常量。第二项测试直接验证bind helper，不冒称同一真实端到端连接序列一次性覆盖了254个客户端留存句柄；普通节点的应用32限额由第一项覆盖。测试node的SecretProtector是fixture，不扩大为DPAPI重新验收。

未发现新需修问题。后续tester必须用含本修复的普通binary复测原FAIL；只读旧R8库测试不能替代。未运行全套旧测试、未占主方target、未操作用户30447/数据，未改产品/旧报告/清单/plan/tasks，不提交推送归档。
