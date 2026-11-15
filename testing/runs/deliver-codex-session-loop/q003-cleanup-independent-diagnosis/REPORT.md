# Q3-COMP-CLEANUP001 独立定位

2026-11-02。**真实sqlx0.8.6已固定复现关闭假完成窗口：取消acquire后pool.close返回且size0，自己的SQLite worker仍持有DB/WAL/SHM，首次物理删除报Windows32。** 产品问题仍Open；没有修改产品，也未声称已追踪到原模块的具体那个被取消future。

## 输入与独立首红

Root file-probe02的787源before/after manifest SHA2a0385139524fe59b9528b4c35fae52498880cf26023a30ef3cc9b3a797a73ee逐项核同后完整复制。与固定787产品只有shared_view_test_support.rs一处文件变化：原首次remove_dir_all失败后，才做静态错误码和每个合成文件一次只读share0探针；原删除、断言、预算保持。原日志/NOTE保存在inputs/root-probe。

自有独占workspace/target/tmp/data，离线locked。独立模块首轮29项，28绿1红、退出101；原remove raw32，owner/tasks/resources0，IO/pump/views joined。失败是detach_cancels_full_result_gate_without_release。紧随后置DB探针已独占成功，WAL/SHM已不存在；与Root02三文件当时均raw32有差异，说明此次抓到较短释放窗口，不把两种观察混写。只执行module01，未为碰绿执行module02–04。

## 最小化与实际对照

第一套直接使用真实sqlx/SQLite，控制before_acquire取消、默认ping取消、普通归还、显式close各20个独立区域：4测试80次首次删除全成功。这组没有抓到窗口，不是取消安全证明。

第二套只实例化真实SQLx池和SQLite，在其worker线程执行自有user function并等待本进程Condvar。建池完成后才启用before_acquire屏障，保证acquire已取连接而pending。取消future后pool.close返回且size0，先唯一首次remove，再每个文件一次只读share0观察；没有改sqlx/产品，没有重复删除。

| 同一资产运行 | 取消组 | 保留获取任务/显式ACK对照 |
| --- | --- | --- |
| barrier02 | 1红；首次remove32，主库/WAL/SHM各32 | 1绿；未放屏障时close pending/size1，释放后显式connection.close ACK，首次删除成功 |
| barrier03 | 同样1红 | 同样1绿 |
| barrier04 | 同样1红 | 同样1绿 |

每轮实际2项，1绿1红，退出101。三轮首红均保留；测试体约0.01–0.06秒，缓存后完整命令约0.8秒。固定回归叶assets/sqlx_worker_cancel_barrier.rs；执行入口run-barrier-repeat.mjs barrier03/barrier04。证据标签已存在，不能覆盖重跑，需新自有标签。

屏障v1在connect_with内部建池自检时提前触发，夹具自身等待。核验exe位于独占target后仅停止PID24104，退出4294967295，无完整测试结论；原v1、原日志保留。v2只改为建池返回后启用。此夹具错误没有冒充产品红。

## 机制及真实调用口

sqlx-core/sqlx-sqlite0.8.6的5个选读源文件已逐字核对本机crate压缩包；压缩包SHA与app/Cargo.lock checksum一致。无网络，原文件及校验表在sqlx-source。

1. poolMod340–355明确说明acquire取到idle/新连接后取消会drop连接，不做graceful cancellation。
2. poolConnection38–40 Floating持raw和计数guard；poolInner610–617 guard Drop立即减size并归还permit。poolInner97–113在size0时可结束close。
3. sqliteConnection219–234显式close发送Shutdown并等ACK；238–242 hard close仅Drop。sqliteWorker323–329显式Shutdown分支在drop conn/shared后发ACK；普通Drop没有等价调用者收据。
4. Storage.close481–488的registry close→pool.close→size0栅栏，看不到已被drop的Floating留下的worker。
5. 当前真实链：node/request_authority.rs45–66 current→RegistryOwnerScope.while_open（application/request_registry.rs268–280 seal分支会取消future）→access_policy.rs231–243 require_current_actor→trust.rs545–555 client_by_id.fetch_optional(&pool)。Trust隐式acquire不在registry已attach槽内。
6. registry自身另有request_registry.rs61/89/142/146 lease.attach(pool.acquire().await?.detach())，await完成/attach前同样有取消窗口。仅改已attach RegistryConnection Drop不够。

受控最小过程的占用者由本进程自己worker屏障直接固定，因此registry或外部进程都不是产生该窗口的必要条件。但没有枚举原模块时所有进程句柄，未证明原首红具体取消的是哪次Trust/registry acquire；不能把机制证明冒充原实例的完整因果追踪，也不能猜成杀软。

## 待审修复及限制

FIX-AND-EXPECTATIONS.md给出单一用途的同Storage获取所有权：首次poll前登记获取任务；caller放弃不abort获取；成功无人接收只显式close并等ACK，不继续被取消业务；成功移交registry需原子转移登记。Trust隐式pool executor及registry未attach段均须覆盖。封新获取后协同推进持有连接归还及获取任务终结，避免close等acquire而acquire等未关闭lease。保留原Node预算、失败类别、首次删除和原断言。

**仅持有外层future仍不自动解决SQLx内部acquire_timeout/连接错误。** 若拿不到worker真实关闭收据，owner必须保留未确认失败，不能用任务结束/size0报告成功；要把这些分支正常关闭，需要受审的更底层启动/关闭收据边界。此处未实现或批准ABI。

CLDB01–07定义：受控SQLx窗口、真实Trust authority取消、registry未attach取消、无人接收/内部timeout/关闭失败、新建连接取消、共享pool关闭推进、修复后原模块/Trust/Node回归。只有CLDB01机制及控制已执行，其他产品用例均未执行。详见提案，不把控制绿代签产品修复。

本诊断workspace最终789为探针787+2诊断叶，**不是A789交付候选**。全部6条命令已退出；原首红合成区域/DB和target保留，不重试删除。因owner-only ACL仅提升读取自有tmp/data的10个文件元数据/哈希，没有输出文件内容、改ACL或删用户数据。没有全机进程审计声明。

Root、固定787、A旧包、产品、依赖和索引未改；无网络/audit/提交/推送。SAFE_FINAL（诊断证据与待审提案）=true；产品修复/问题关闭=false。
