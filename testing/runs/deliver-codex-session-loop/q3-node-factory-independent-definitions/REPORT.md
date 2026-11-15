# D 普通 Node 工厂：独立盘点与定义结论 v1

结论：已完成只读工程盘点和24项精确定义（DNF-01–24），未执行任何产品测试或夹具，未形成编码/交付通过。普通工厂尚缺已冻结的owned protector入口、聚合持有者、owned parse/hash任务及关闭receipt方案；C配置provider和A/Native/Root组合也仍需各自门。不能把借用延寿、静态配置或模块自测当正常Node生产装配。

## 独立来源核验

当前Root Git HEAD为65ec7815bb0280f4519989d93a7b4ed3b4b37994，工作区已有大量未提交变更；只读git status识别后未改索引或产品文件。已读AGENTS/config/tester与当前设计/规格。Root库存INVENTORY.md只作文件定位线索，实际判断均自行读源。

“90de781”精确含义是候选freeze SHA 90de43e0f289ad0691142613a164c59d2990cd89c71e7a5a7cf601e11e4102fd，source-manifest SHA 2c67da9d94392015ab18e9850efdb66fff2e952b0dede9b1729f7258bb10fa0f，781份源码，不是Git提交。独立核原candidate/source全部781 SHA，并另核C author只读baseline副本781完全相同；未以其自评代证据，未读C正在改的workspace。freeze原件/manifest及实际选读18叶保存在inputs/native-90de，完整逐项核对在INPUT-VERIFICATION.json。未声称运行旧freeze中的日志或检查。

当前规范/实际Root26叶快照在inputs/current，用于区分正常入口与较新独立Native候选。A的PromptDocument独立固定候选freeze cbc8120e4a0386f55a78852668c3b2cc3cfd8b3a3d432d958c5fad2a72878c14，prompt_input.rs叶逐SHA核同并读全；该A叶不在90de781或当前Root，不能默认为已合。C只读取冻结设计ed87c7b6fd4117125383d310578119223a33fffb18df7e8587bb6559a72a444f及CORE-ABI，不把在编码状态升为生产能力。

## 实际发现与影响

| 已核实际接口/状态 | 对D的影响 |
|---|---|
| Root node.rs:61 start(config,&dyn SecretProtector)，main.rs start_node传&DpapiProtector；NodeRuntime字段无Native/Queue/catalog/CPU owner | 新普通入口必须真实拥有protector及服务组；旧借用不能延寿 |
| 90de node/native_views.rs:228 start_native_views返回StartedNativeViews，显式提供所有protocol/config/read选项 | 可复用组合口，但返回物未由NodeRuntime长期持有，不是已建好普通工厂 |
| NodeRequestAuthority持同policy及SqliteRequestRegistry，verify_owner_scope核实际matches；approval_owner用同native绑定 | 同Node owner正反例是必要工程验收，不能第二pool或裸ActorContext |
| Storage.native_runtime CombinedSupervisor原native→queue→claim，Storage.close再registry/pool；Drop仅cancel/seal | 新增owner必须进入显式关闭；超时/callerDrop不能冒join，原receipt需保留 |
| A parse消费完整Vec，validate同步Sha256全上传字节，retain共享原根 | 必须把实际parse和hash放在同Node监督的CPU工作，而非WSS/actor循环；不能detach blocking任务 |
| NativeRuntimeOptions没有Default且分别区分write/response用途 | Root需明确工程取值与取消，无权在本定义新填超时/总量规则；prompt执行Unbounded边界保留 |

这些是装配缺口/需细化接口，不是通过新测试复现的产品缺陷。ABI-REQUEST.md逐项区分实际已有和拟需ABI，tests.md按当前模板写24项；cases.json仅机器可读定义，全部Defined-not-executed。

## A/B 的具体拼接边界

独立补查A窄改动四叶，旧mod/strategy before与90de精确一致、新PromptDocument两叶尚缺；仅按窄hunk受控重基，不能整树移植。旧A Storage普通queue工厂有显式audit第五参数，90de四参数绑定原owner audit；Storage不在A四叶改动中，现cfg边界不得被旧工厂覆盖。B WindowsSourceLauncher真实实现已在90de platform/native_source.rs，并非尚待编码。精确before/after摘要、现Root对照和旧接口原文在SPLICE-POINTS.md/json与inputs/prompt-a。没有apply或执行。

## 需求与用例边界

DNF-01–04、13–14覆盖owned启动及真实配置/唯一组合；05–10覆盖正文真实保真、CPU归属与取消竞态；11–12覆盖分阶段失败与启动等待取消；15–19覆盖关闭偏序、独立view/native事实、并发关闭/SQL未知/Windows真实树；20–23覆盖接管与撤销、重启保护、隐私、既有等待/容量边界；24沿实际工程入口建立未来同源检查。

暂停点和被动probe尚是请求，未有F-D实际挂载门；没有新增空测试模块或可mint权限对象的假Controller。普通工厂L4真实Node/registry正链不能被纯trait演示替代；L5 WSS真实sink和L6真实Harness未执行且不推断通过。旧WB02/WB03受阻不被本定义关闭，C34/F与Rust strict等其他包限制不由此改写。

## 交接与安全结束

需Root形成精确D工程方案/ABI/阶段失败表/关闭责任及options，固定C候选和A/90de/Root组合后，由独立reviewer检视，再由tester在获准独立目录编码非空测试。此包只交规划定义与真实源码核验，不交产品patch、不运行未批准fixture。

仅新写session-loop-q003-node-factory-independent-definitions-v1，无产品/旧包/作者目录写，无提交推送发布、网络或audit；无新进程、端口、用户数据或Rust target占用。productRuns=0；fixtureRuns=0；casesExecuted=0；implementationApproved=false；productPass=false；SAFE_FINAL=true。
