# P0 v1可空messageId独立验收

2026-10-31。限定P0通过：**新公开Connection测试6项+原发现17项+原权限12项=35/35，0failed/0ignored**；另实际重跑根ACP开发回归36/36。外部普通依赖all-targets严格clippy（all/pedantic/nursery，-D warnings）与新资产fmt检查exit0。没有发现P0产品缺陷，生产源码本轮只读。

依据833c1124...合同和4a16aa62...七组定义；P1/P2、完整结构投影、可信页、Node/WSS及真实Harness未因此通过。特别是v2省略messageId仍只是当前粗投影None，本包不签完整v2必需ID规则或提前收紧。

## 同版与资产

冻结前后7项逐一匹配，见p0-before.json/p0-after.json：strategy `72ad187e25cf6ba4c530be4fb8eaa6c482fcca9bab32c3a3220b4fdd42842e4c`、测试声明b5b2c180...、开发新测试0f257a39...、model a14f33cf...、transport93bd37e4...、锁87f27b61...及合同833c1124...未变。独立在内存撤回唯一v1 null filter/对应换行后，整Strategy准确恢复原SHA `cb91ed1d4395ac70af35fb422a8c97e1b2edf93f76feb39b10f638f3cf43debf`；未改实际文件。p0-scope-and-lock.json保留该窄差异证明。

可持续公开API新资产：`p0-message-probe/tests/message_id.rs`，SHA256 `73580d598dd79b7b43c7a33262511e2df57c691240198debfe5b1a458ef1e8c4`。原17与12逐字复制，分别仍48ac5cb9.../cccda47e...。完整资产SHA为p0-acceptance-assets.json。普通path依赖只增加probe自身，第三方name/version/source来自产品锁，无产品锁修改或新依赖版本；Cargo.toml仅声明产品已有tokio/serde_json，独立workspace，没有强制依赖cfg(test)。

## 七组实际结果

| 组 | 经真实initialize与Connection::next执行的结果 |
| --- | --- |
| A | 三个v1合法chunk tag均省略/null→None，正常字符串/空串→Some原值；同连接后续通知继续读取，不聚合/造轮次 |
| B | 各tag ASCII4096、UTF8共4096字节均Some；ASCII/UTF8共4097均InvalidMessage，不截断 |
| C | 各tag数字（整数/小数）、布尔、数组、对象均在独立连接InvalidMessage，原错误关闭及rejected raw保留；未被null分支吞入 |
| D | 真实preferred offer2且合法selected1时，三tag null/字符串按selected v1解释；无第二initialize或额外客户端wire |
| E | Live、真实同session replay pending、普通resume pending三窗口的原raw/字段/未知内容/顺序及origin保持；另一session Live；null后标准响应完成、再读正常字符串不受影响。v1 replay严格session/load，广告resume的普通resume严格session/resume，不将DuringReplay当每条历史证明 |
| F | selected v2三tag省略仍None，正常/空/4096字节字符串Some，null/错误形状/超长仍InvalidMessage；明确保留粗投影现状、不扩成P2 |
| G | 原17发现和12权限同版实际通过，根ACP开发36同版实际通过，严格lint/fmt通过；窄源码差异与7摘要保持 |

6个新测试函数内有矩阵循环，七组中的G是上述回归，并非七个新函数。原始输入为合成JSON-RPC完整帧，bytes比较去掉帧分隔换行后逐字一致；没有仅绕过Connection调用strategy的替代验收。共正常57条update与42条定向错误update矩阵，初始化/标准响应另计。

## 命令、退出与失败保留

Windows、rustc/cargo1.98.1，独占tester/activity-target，本次进程CARGO_INCREMENTAL=0；无浏览器/Web audit/网络外发。根工作目录执行，probe绝对路径为原tester目录下p0-message-probe：

```powershell
cargo test --manifest-path <probe>/Cargo.toml --locked --offline --tests -- --nocapture
cargo clippy --manifest-path <probe>/Cargo.toml --locked --offline --all-targets -- -D warnings
cargo test --manifest-path app/Cargo.toml --locked --offline --lib adapters::acp:: -- --nocapture
rustfmt --check --edition 2024 <probe>/tests/message_id.rs
```

完整绝对命令与同shell采集exit0在p0-message-test-final-exit.json、p0-clippy-final-exit.json、p0-acp-development-exit.json；fmt exit0在p0-fmt-exit.json。原始最终日志p0-message-tests-final.log（35）、p0-clippy-final.log、p0-acp-development36.log（36）。空probe lib的0测试不计通过数；开发36不是新增独立用例，不把两者相加冒签71独立项。PowerShell stderr进度包装的NativeCommandError外观不替代真实退出码。

首两轮P0-E失败分别为tester夹具漏广告标准resume能力、误将普通resume预期写为session/load；按原§6A/实际已批准标准修正为resume广告及分支精确wire oracle，raw/origin/order断言未放宽。失败与101退出保留p0-message-tests-attempt1.log、attempt2.log及对应exit文件；没有把夹具错误记产品失败，也没有删除。其后35全绿保留；自有clippy一项冗余closure改为同方法引用，原告警日志p0-clippy-attempt1.log保留，最终35和lint均在最终资产上重跑。

## 限制与交付

本轮不修改root产品/旧验收预期，不建立Node或WSS、不读数据库/凭据/私有路径、不访问真实Harness。仅合成双工peer及自有临时编译资产，任务/连接随测试结束清理，无后台服务。probe/锁/日志有意保留用于持续回归；原budget formal/集成证据不改。

完整CL标题/ID不变，仅签定义所列纯协议子范围；没有签v2必需ID收紧、P1/P2、无损实体聚合、预算适配或L5/L6。后续root可将单个公开API新测试资产按独占路径集成，需同版检查实际数量/严格lint，不覆盖生产源。
