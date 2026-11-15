# P1/P2私有Received引起的测试迁移补充复核

2026-10-31。**限定准许下述输入构造迁移，实际候选逐hunk及动态证据仍待后验。** 不新增测试grant/hook，不改编码门禁范围。

已亲读原P1 `r7_inconsistent_public_fields_foreign_windows_and_read_lifetime`及P2 `g_ignored_invalid_and_foreign_scope_do_not_double_observe`完整函数。原P1文件SHA256 `00146db42db0c6a78fe9f888e0d9cf68af0e43606ae5d95ed546656ffd6f9d0f`；P2文件`a4a85b41a00a4b7a046ac7a0006504d9ee3b79e74b993e4e3a47da9f3ddc1382`，均保持原字节。原36执行函数数目不变，新增两个函数的逐分支迁移映射，不另冒计新通过。

| 原攻击输入 | 准许的新公开可达输入 | 必保oracle |
| --- | --- | --- |
| P1 r7：修改Incoming.Other.kind，使其与raw不符 | 实际别连接、同版本/同session有效Received，交给原ObservationRead | InvalidObservation、stats/usage不变；后续foreign windows Stale、其Reading状态、Ignored普通通知、原连接真正记录、raw借用存活及最终ROOT_BYTES释放全部原样保留 |
| P2 g首段：修改Tool.session=B而raw仍A | 原连接真正读取session B的wire及Received | raw/anchor均Ignored、stats/usage不变 |
| P2 g第二段：修改Tool.tool而raw仍原值 | 实际别连接、同版本/同session/tool一致的有效Received | ProjectionError::Observation(InvalidObservation)、原stats不变；后续foreign windows Stale、created position=2、raw sequence=1及no_wire全部保留 |

这是新的来源校验边界，不是字面等价于旧raw/event不一致攻击。revision2主合同64cc使Received私有、仅真实reader构造、incoming()只借&Incoming；旧外部突变路径不可达。故与已准许的P1 ordering三个导入例一样，旧攻击面另通过API05的正对照及compile-fail/静态证明：相同imports只读调用先编译0；负例只改一攻击操作，核对应不可变借用/类型不匹配/私字段等正确错误位置；clone Incoming不能传入observe/apply(&Received)或重建Received；盘点无public factory/From/Deserialize/可写字段。不能以坏import或产品未编译的任意非0充数。

原wire坏shape、跨scope、stale、原文与布局/计费动态规则继续跑。不得为延续旧伪造输入增加fake Received品牌、生产yield或私有setter。新测试报告须分清“公开攻击入口消失的编译/静态证据”与“真实异scope拒绝的动态证据”，旧函数/原件/旧通过报告不回写。
