# 同一 Node 的 L6 浏览器阶段交接

此 helper 只接续 tester 已拥有的真实 Node，完成临时配对、阶段关联、证据收集和撤销。它不启动 Harness、不创建厂商身份、不改策略，也不凭 receipt 或文件摘要签 DOM 通过。即使所有输入完全匹配，返回值仍是 `status:"pending_independent_review"`、`acceptancePassed:false`；调用者必须保留独立浏览器分支的未通过状态，直到独立操作者实际采集且 reviewer 报告对应同一 phaseId。

## 接线

在 `node::tests::r3_source_wss_development` 的既有 feature 门下 include：

```rust
include!("node/session_services/capacity_l6_web_handoff.rs");
include!("node/session_services/capacity_l6_web_handoff_tests.rs");
```

开发测试文件只证明 helper 的合同与真实 Node 配对/撤销路径，使用惰性 DOM 文件和开发来源身份；它不证明真实浏览器、真实来源正文或 L6。

调用：

```rust
capacity_l6_web_phase(
    &mut fixture,
    &case_root,
    "closed-keeps-capacity",
    serde_json::json!({
        "nativeIds": [root_native],
        "operationIds": [original_operation],
        "occupied": 50,
        "mode": "replay_complete",
        "historyIncomplete": false
    }),
    Duration::from_secs(900),
).await
```

`expected` 仅允许 `nativeIds:string[]`、`operationIds:string[]`、`occupied:u32<=50`，可选 `mode`（`replay_complete/current_only`）和 `historyIncomplete:bool`。禁止正文或任意额外字段。来源/node 从原 fixture 实际读取；原生ID、原operation须由调用点实际取得，不得造数据。helper不替代调用点之后的真实SQL/容量/关闭验证。

`case_root` 必须是原 `TestArea.directory` 的直接父目录，即 tester 的 `<runRoot>/<cp>/node` 布局；路径全链不得为 symlink/junction/reparse。`phase` 只允许ASCII字母、数字、横线；`<case_root>/web-<phase>` 必须全新。目录先设当前Windows用户专属权限，再写私有info。预算必须大于0且不超过一天，限制整个配对/等待，不缩短产品空闲政策。

## 独立浏览器操作

1. helper 通过原Node的owner IPC开始临时配对，将私有 `info.json` 以 create_new 写入阶段目录；不会在stdout打印配对码。内容有规范URL、一次性phaseId/nonce、原node/source/expected绑定、selection、临时pairingCode、expiresAt和唯一displayName。
2. 独立浏览器操作者先读取一次info到其短时内存，使用专属browser space/context访问该URL，只对本次生成的localhost证书按既定流程确认。填写配对码及info中的displayName并实际点击claim，不把码或控制内容写截图/永久日志。helper只批准匹配当前pairingId/displayName的真实claim，且拒绝复用原controller身份。
3. 原批准持久确认后，helper删除info，并写 `paired.json`（只有phaseId、实际browserClientId）。浏览器脚本应保留短时绑定数据，不依赖批准后重新读取info。按原页面完成真实capacity/root/child等操作，保持真实DOM、动作序列及执行源码可追溯。
4. 在本阶段私有目录写三个**实际采集**文件，先完整落文件再原子写入 `receipt.json`。固定文件名拒绝外部路径或链接：
   - `capture.json`：`{"kind":"independent-browser-dom-capture","binding":<info.binding>,"browserClientId":<paired实际ID>,"dom":<实际DOM捕获>}`。
   - `actions.json`：`{"kind":"independent-browser-actions","binding":<同绑定>,"browserClientId":<实际ID>,"actions":<实际执行动作记录>}`。
   - `capture-source.txt`：产生上述证据的实际脚本/工具调用来源；不得内嵌配对码、私钥、token或原生敏感正文。
5. receipt仅含format1、同一binding、实际browserClientId、上述三个文件各自的原字节SHA256（小写hex），字段名依次为 `captureSha256/actionsSha256/captureSourceSha256`。不得增加 `passed`、自签角色或随意外部证据路径。随机nonce只用于这一阶段相关性，不是浏览器执行证明。
6. helper核同一次Node/source/native/operation/phase/URL/nonce绑定、同一新配对身份、文件摘要、capture/actions内部绑定，并create_new `receipt-consumed.json`，重复receipt拒绝。receipt内容或SHA相同并不证明浏览器运行；返回的只是待独立检视引用。手写惰性文件在开发测试中也只能返回pending，不能关闭DOM分支。
7. helper通过原owner IPC撤销本次browserClientId并核实际Revoked，写 `revoked.json`；浏览器可据此再实际采集配对失效的DOM，并在独立报告中关联同phaseId。只清本次browser身份对应origin的本轮状态，关闭本次专属space；不要触用户其他空间/身份。

## 结果、保密与关闭

返回摘要保留phaseId、node/source/URL、必要expected、browserClientId及三个证据hash，**不含nonce、pairingCode、DOM或动作正文**。独立报告必须引用实际browser工具采集记录、执行来源和动作；仅写“ego-browser”或文件SHA不是通过依据。

原始capture/actions/source保持本阶段私有目录，只有审查后的脱敏版本/必要摘要可进入永久证据。helper不自动归档。info与receipt在正常、失败、panic或future被取消时由RAII删除；不递归删除目录或任何认证历史。

配对/等待有同一个总deadline。批准后正常或失败均尝试撤销该新身份，撤销额外最多30秒；未确认撤销则报错。若等待失败、panic或调用future被取消，RAII同步请求**原Node owner**停止，不生成游离cleanup task；原调用方仍必须finally调用 `R3WssCase::close()` 并join原owner。关闭请求不等于清理成功；Node/Job/端口实证由原case记录。缺receipt或独立报告保持blocked/pending。
