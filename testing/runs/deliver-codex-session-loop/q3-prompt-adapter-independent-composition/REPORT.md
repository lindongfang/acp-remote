# A PromptDocument 固定组合独立重验

2026-11-02。结论：**A 数据组件有限验收通过：公开7、私有5，首次均绿；完整组合尚未闭环。** 本次没有修改产品或验收预期。Root 的 Q3-COMP-STARTUP001、Q3-COMP-CLEANUP001 继续开放；strict 603 条保持开放。单次143绿不关闭间歇问题。

## 固定输入与安装

- 组合 freeze SHA805516613647a63053a5b23228bb0ca21d3b6ba1c55537b05b45d6d866132a12，810成员逐项核SHA并记录实际长度。783源 manifest f0c04322e75867f8335f020ad00ae373085e366d29a9bcda770fa425f8d85be9，完整复制到本包 workspace 并逐文件核同；不使用活作者目录。
- reviewer门 b957c293dd00e0ae7b01b334b75e4ac0d0fb4b4a03aaa5574f133dd7c8e77434，19成员已核；原提案 c714f789c32621482eb1e4cf6804e08ff2470a25e5b7a96e2897edc8860a8a24 的15成员也已核，保存在 inputs。
- 仅三路径资产差异：新增公开叶 a40379915fae73fba1befdb7cb552bae9608968a35cbc2121d6f5bd9226da97f；新增私有叶7587db52c6bd5dde0da6665caea83eaa98c4b3788e86ad84299b77a94976e5ab；lib局部追加原三行挂载，后像edf312134c8961ddd3b3238faa3f435a8e91a3e179369b1f71ccac0e9391f1f0。固定lib先核无同名/重复模块，原前缀不变。
- 私有叶反向恢复唯一方法名后与原6b97755e3803db52e36fa043060f19d61dc2bdd73707cdba7c00edf5673c0aff逐字一致；五实参、Audit、所有断言、catch_unwind与关闭尾不变。新方法是已有cfg(test/registration-acceptance-fixtures)入口，没有重开生产第五参数。
- 安装后785源，manifest SHA c1523765558bbd8497a4d613c1c3b4a68c0cb2de6ac9ce1da11b4820f873bbf7。所有11条Cargo命令前后785源/锁逐SHA不变，782个既有文件不变，lib为唯一修改既有文件。
- 独立核关联类型：Arc<AcpNativeAdapter> 的 ValidatedBodyPort Raw/Body=PromptDocument，哈希覆盖完整uploaded_bytes；RetainPrompt 返回同底层根的OwnedRawValue；唯一NativeProtocolPort实现仍在原native_dispatch.rs。PA07用原NativeQueueDriver消费/capture token私有来源，未造authority/proof入口。细节见 evidence/association-source-review.json。

## 实际执行

原始命令、cwd、开始/结束、PID、退出、源码前后、stdout/stderr位于 evidence/<mode>/。环境为Windows11 x64、cargo1.98.1、rustc1.98.1、Node22.20.0；准确工具输出见environment.json。Cargo使用本包独占target；TEMP/TMP和PA_DATA_ROOT均在本包。test/check/clippy均--offline --locked；fmt不解析依赖。没有网络、audit、提交、推送。

| 实际入口 | 退出 | 实际主用例 | 结论 |
| --- | ---: | ---: | --- |
| public / --test prompt_document_acceptance | 0 | 7/7 | 首次通过 |
| private / --lib prompt_document_acceptance:: | 0 | 5/5 | 首次通过，过滤669，不计为执行 |
| check-default / check --all-targets | 0 | 无测试 | 编译通过 |
| check-all / check --all-targets --all-features | 0 | 无测试 | 编译通过 |
| fmt / fmt --all -- --check | 0 | 无测试 | 通过 |
| acp-regression 沙箱首轮 | 101 | 114绿/29红 | 首红保留；29项在实际临时Node启动报ReadFailure |
| platform-precondition 沙箱 | 101 | 0绿/1红 | 原DPAPI测试CryptProtectData os error2；不改平台实现 |
| acp-regression-elevated 同源同命令用户上下文 | 0 | 143/143 | 本轮通过，不能关闭Root间歇问题 |
| r3-regression 两个integration入口 | 0 | 7+39=46 | 通过 |
| protocol-regression 两个integration入口 | 0 | 5+33=38 | 通过；另4条子进程1绿摘要是内部复验，不重复加数 |
| strict / clippy --all-targets --all-features -- -D warnings | 101 | 无测试 | 603条；与固定候选code/message/全部spans逐条相同，新增0移除0，A四路径直接spans0 |

公开7覆盖原PA01、PA02两项、PA03三项、PA08。私有5逐条为PA05实际大帧/v1回退、PA06完整字节去重冲突、PA06 parse前置/同Arc、PA07 prepared/drop/edit/stale/newrevision、PA07 direct真实消费来源。完整测试名与逐条PASS见RESULT.json。没有另造PA04动态执行数量；关联类型/原traits由公开PA01真实编译与断言覆盖，具体合同仍按原资产边界。

PA05的原测试实际经过保留真实prompt、实际wire frame读回和原oracle全根比较，v2检查大于32MiB，v1按既有选择语义回退，响应后pending_count为0。没有用孤立演示或开发自测替代本次独立叶。

## 关闭、首红和开放项

私有5均走完原Area.close：先QueueService.shutdown，再Storage.close，并断言report.joined；对应调用行323/364/447/491/734。PA05另外走完原IO owner.shutdown及closed.joined。成功结论仅及原断言，不能推出物理DB/sidecar释放、所有Storage所有者或Windows完整进程树证明。原Area.close还断言数据库保留，故5个自有合成SQLite文件继续保留在data/private，不打包为移植源码。

本独立沙箱首红和原DPAPI单项失败说明环境敏感；同源用户上下文143绿只说明本次可通过，不足以证明所有首红根因。Root随后交来的REGRESSION-RESULTS.json（本包inputs保留原字节和SHA）记录固定783/诊断副本的启动与首次物理area清理间歇失败。此为Root执行观测，未拿诊断绿代替独立oracle，也不声称自己复现了其cleanup失败。Q3-COMP-STARTUP001和Q3-COMP-CLEANUP001保持Open-intermittent，不做无依据归因/关闭，不再无关重跑碰绿。

strict603虽与固定候选完全相同，依然非绿；编译提前失败不代表所有未到达测试target的lint已干净。本轮未为消除旧债修改产品、资产或预期。独立执行器最初node --check曾发现自己脚本少一个引号，在任何Cargo执行前修正；产品和原验收叶无此修订。实际产品/验收首轮原日志均保留。

所有本轮命令已收到close退出事件。先记录本包tmp145个合成文件的路径/长度/SHA，再核绝对路径边界，清理29个自有临时子目录；tmp现空。未删Root、旧包或用户数据。5个私有合成DB及独占target保留；不宣称做过系统全进程审计。

## 交付和限制

asset-install.patch SHA2f310d341e30bb2392cf3f226c8ebb5b9dd1399ddf59d78c6079024ba697450d，仅两测试叶及lib局部挂载；在自有三路径重放副本实际git apply --check=0、apply=0，三个后像SHA精确匹配。此补丁前提是候选lib原SHAa881e2383ac331c25d88d7290943f8855a4a1ee240e3233f2c5339329f3385a0和两叶原不存在，Root若变源必须重新核对；重放副本不是另一份完整编译证据。

未签：Root合入后结果、普通Node工厂D、Node/WSS/upload/UI完整链路、真实Harness、物理OOM、完整进程树与DB物理释放、strict清零或整个产品最终交付。没有更改规范/旧包/Root/索引，也没有扩张原A授权范围。

SAFE_FINAL（证据包/有限A数据组件）=true；SAFE_FINAL（完整组合产品）=false。A12项失败0、未执行0；扩大回归与strict开放边界如上，不把本有限通过写成整体完成。
