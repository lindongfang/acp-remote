# P1全量检查中的既有IPC测试端口夹具复核

2026-10-31。只读实际node.rs/config.rs及主提供的明确日志，没有修改产品或测试。

原完整检查`.tmp/session-loop-p1-check.log`实际Rust库389过/1失败，失败位于node.rs:1283的HTTPS listener启动，Windows AddrInUse10048；build未继续。已读随后原源码exact单测`.tmp/session-loop-p1-ipc-recheck.log`实际1/1通过。定向绿不抹掉完整失败，也不足单独证明环境原因。

实际free_address在cfg(test)内std bind localhost:0，仅返回local_addr，退出函数即Drop listener；随后NodeRuntime进行身份/存储/TLS等启动，再TcpListener::bind该已释放地址。端口未保留，竞争窗口在源码中确实存在。

已完整核`owner_ipc_lists_and_revokes_clients_without_a_web_identity`：测试通过local IPC列出合成client、撤销、停止/等待真实runtime、重启再确认撤销状态，始终只使用ipc_address，不读取runtime.listen，也不连接HTTPS。TestArea直接构造NodeConfig，listen是SocketAddr；现有NodeConfig不禁端口0，NodeRuntime直接bind该地址。无需产品新API或改生产绑定逻辑。

因此**允许仅该cfg(test)case将`let listener=free_address()`一行改为localhost:0地址**，让实际NodeRuntime bind时OS选端口，首启/重启都用0；不改断言、存储/身份/IPC/stop流程、不添加重试、不吞真正启动错误、不改其他需要固定HTTPS地址的测试。这是已授权的测试夹具工程修正，不需新用户产品决策。

实现后仍须检查实际唯一hunk、新node源摘要与exact单验，再固定全量check/build快照。原失败与定向原源绿都保留，不回填P1已冻结十源或声称P2A已实现。本报告是该窄改动的实施前核对，不提前签改后全工程通过。

## 改后最终独立复核

实际当前node.rs SHA256为`b959b31c0f2c7bfeeeb3097341b4040efce497589f577da290eb15b5843587f6`。内存中只撤回该case的注释和port0替换，整文件重算恰为既有已验schema3-source中node摘要`c97a0f51f3c878801bcc259452242fac9bd6f3764ec45da50443184fc38c0067`；唯一匹配1处，证明其余生产/测试源未夹带修改。该注释说明IPC不连接HTTPS；地址改动与批准范围一致，无其他断言、重试或错误吞没。

已读根`2026-10-31-p1-ipc-port0-recheck.txt`：该exact case实际1/1、0忽略，通过；并读`2026-10-31-p1-full-check.txt`/`full-build.txt`的命令、完成与结果：Web50；Rust库390（含该case）、JWS4、平台12、公共69、P0新6、P1新11、预算20和allocator6均实际通过，fmt/all-targets clippy与Web/Rust构建完成。主执行记录完整check/build实际exit0；本reviewer未重跑，不将日志读取称为自己执行。

据此**该cfg(test)窄修及此次固定根组合工程检查通过**。原10048/389过1失败、原源定向绿与改后单验/全量绿各自保留，既没有删失败，也不靠环境推断代替复验。结论不推广为所有free_address测试均无竞争，也不改变产品NodeRuntime的端口行为或承诺真实会话/Harness闭环完成。
