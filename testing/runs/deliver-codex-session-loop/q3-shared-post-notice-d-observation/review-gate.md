# Shared D：notice 后同次拒绝与固定阶段诊断门槛

结论：**PASS-limited**。准Root在新的独占C451副本安装唯一测试叶D、编译并运行一次原quiet exact。仅错误消息/静态Context精度，不改InvalidCredit期待，不准后半重试、等待或产品修改。执行保留原[V1,V2]循环；首V1红则V2未到，自然推进也须按实际记录，不能称严格只跑V1或追加单版样本。

## 已实际核验

- D freeze `cb03d4dc3c4619cb89cfcb97168276d11b04454cbffd64cd73ea21b39d1078aa` 的17成员、25输入逐实际SHA/长度匹配。
- C freeze `70c9a47aec1714b7dd54c444e9d35a5bd8265e385c657fec66a6b7b86251d446` 的487成员全部匹配；451真实源与compile/quiet前后清单完全相同。归档EXE `bbdc5c1b5123b16507280ea426b59d6c49dd4f6838859f2903ca9c0c7af984cc` 实际hash相符，compiler fresh=false。
- 原raw实际V1 0/1、0.13秒：初始C受限重试后notice已收到，原B最终1/1/0已过；红停在credit1之前的第二follow，期待InvalidCredit、实际InvalidStage，未打印同call标签。两raw正文未读、V2未跑；healthy三join/owner/Weak/auth FIRST/IO FIRST尾真不能替代body。
- 本包六条IF原steps/expected与原f3af定义逐字相同；原5lint红不修，不把本次结构/编译提案当Shared83/IF完整通过。

## 唯一准许的后像

路径：`app/tests/shared_view_inflight_acceptance/transfer_cases.rs`。

before `e08f07a03c82362c24f455a012b94400e2b7685720eb4a8ebfc3e32cea27adec` → after `0f18dd02893abbf7e259dd20c8001b5a43eb0f77b3e508cc0736a6df4ff87c9c`。

patch `26432bfcb7f5b459e5dbb25e287dc4e1990f6f837733581975f6466edc5b4ccd` 在reviewer自有scratch严格check/apply0，实际后像raw等于提案。初始C及B flush段raw完全不变，body/cleanup后缀raw完全不变；ensure、timeout、follow、credit、collect、candidate、detach、yield、probe、await次数和双版循环均相同。没有新生产接口/字段/Source或Raw改动。

第二follow原timeout返回后立即复制现有`last_follow_rejection`，仍仅Err(InvalidCredit)进入成功分支；意外Ok/其它Err仍原失败。安全Rust同一`&mut transfer`方法返回后、下一次调用前读取的是该次保存的Copy字段，不是事后再查progress/lane。outer timeout失败在固定Context处传播，不能把没有读到标签当某分支。

错误消息三数明确命名`prior_snapshot_*`，只读原已存在Copy observed的const getter，不重新probe、加锁、await或验权；它们是调用前快照，不是拒绝瞬间水位/Ready或lane凭据。错误/标签是原bodyless有限Copy类型（至多静态operation/数字系统码/布尔），不新增raw、RPC值、actor或凭据输出。原错误bail仍失败；其它新增Context只标已有credit1/collect/candidate/detach的固定阶段，保原调用、期限和错误因果链，不吞错或替换成功条件。

## 原错误优先级必须照实解释

原follow先frame残留，再真实current auth/progress；Pending/Complete等可先InvalidStage，Failed(error)早返仍可能无tag。CandidateReady才继续原随机nonce和lane检查，Busy另有原lane分支；因此旧1/1/0快照不是得到AwaitCredit检查的联合许可。到lane后，AwaitCredit给InvalidCredit，Writing/Switching给InvalidStage；其它实际错误按原代码返回。D没有改变任何优先级。

Consumed防止再次线性rebind，不等同不能观察同原request进度，也不保证此刻必CandidateReady。Local flushed发布后，Sender还有published_flush/set_idle_lane，后续drive也可进入Writing；因此flushed1不保证第二次调用看到AwaitCredit。这里只列真实源码可能路径，**当前C第二次InvalidStage仍未知**，不预判Writing/Pending或测试过约束。

只有新D同call标签实际输出后才可缩小来源；None仍Unknown，不能排除原Failed、未标错误/取消。即使实际Writing，也不授权把后半InvalidStage纳入C初始重试；若要改变后置前件或排程，须另给精确依据与最小方案，仍保IF09/IF10两义务和完整quiet正文链。

## 有限安装/运行条件

只新C451副本、该叶变化，其他450源/原锁/原源清单保护；Root636/e127及旧冻结不动。先保存strictcheck/apply/新源/EXE，必要纯机械该叶rustfmt或新增诊断编译修正分层留首日志；不得洗原5lint、改guard寿命、加allow/pub/fakeuse。提案文本成功不等Rust已编译。

运行前核同owner、专属target/TMP、原Rig必需SHARED_IF_FIXTURE_ROOT。list原quiet全名唯一1，然后一次`--exact`，原版本循环/预算/初始C重试条件完全不动。没有再跑原C或加单独V2样本，也不因新tag重试后半。失败在真实阶段停止并保首红；自然绿只记本次到达，不回填旧C/6b6未知来源。

原Credit0/显式credit1、两个raw全字节/order/大数、同原H/request、native静默/0新RPC/provider和全部body/cleanup/firstphysical仍需原断言通过。健康尾不能遮正文失败；未确认、故意Io或残余仍按原界限记录，不二probe/晚清理刷绿。完成后冻结同源结果，再决定后续，不扩Shared83/IF30/RF18/CL023、RF06/WSS/坏Bytes/Harness或生产隔离。

机器记录 `post-notice-D-review/verification.json` 的SHA随交付消息提供，包含17/487/25实核、451前后/EXE、raw首红、精确roundtrip、原六IF期待及操作计数。reviewer Rust/SQL/产品运行0，未改Root或旧包。
