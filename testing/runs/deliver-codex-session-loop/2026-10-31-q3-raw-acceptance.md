# Raw JSON 基础层候选 v1 独立验收

2026-10-31。结论：**限定范围 PASS，RJ01–RJ30 全部通过**。未发现产品阻断问题；没有修改产品源、原验收定义或旧资产。此结果不能关闭 CL-095/103 整体验收，也不能作为整个 deliver-codex-session-loop 已完成的依据。

## 独立性、版本与来源

本 tester 无 coder 对话继承。先读 AGENTS、OpenSpec config、tester/reviewer 角色、plan Q3 Raw 段、当前 acp-platform-constraints/structured-history 增量规格及 CL-095/103，独立实现原编码前固定30项定义。没有以开发8项或开发自评代签。

- 合同 SHA256：9f2b236f325a0cceed3c1c34274d73ca953d9c7dfec571d22745b2b7ef70cce6。
- 编码前独立定义：d172289c8018fba5d807896a3c33dd845276d10d9fd38e2e8c748c44cca1e400；未改。
- 编码前限定预审：548c7be744601b743b3f9dd7503717f525162e3055887d845fb81f343bac3204。
- 候选 source-manifest：7b1a6d69f0c931adf5d3e3bf1010f5fe85bd5877dfac0d374e67d80a30e65287；127项自行核对全部匹配。
- 候选 delivery.json：b8cd7671f7c6c34473ea68956ebb8184d46270d889ebc77f40892389a857fc75。另核其5个实际构建输入：原rust-toolchain、真实Web dist三文件及JWS fixture。没有伪造 index、安装工具链或下载依赖。127与5单列，不冒认忽略构建资产已包含在127清单中。
- Cargo.lock 始终原字节 87f27b619281b37015c79ceae8d95471c8a4de50f202f4dd8a6d99fa0d27e324；冻结前后132条全部匹配，见 source-before.json/source-after.json。

独立工作区为本报告同目录 workspace，独占 target D:/Project/acp-remote/.tmp/q3-raw-tester-target。Windows NT 10.0.26300.0，x86_64-pc-windows-msvc，原文件指定 rustc/cargo 1.98.1；详情 environment.json。CIM OS查询无权限，OS字符串改从 System.Environment 取得，不影响测试。

## 资产和执行

唯一新增资产是可直接原字节入仓的 delivery/app/tests/raw_json_acceptance.rs，目标 app/tests/raw_json_acceptance.rs；无外部绝对路径或新增依赖。SHA256 **1853768ef419f0838f543cc6c050c0ecb932f961e5f86638bbf3f5b81f2ce43a**，assets-before/after.json证实冻结后不变。

该 integration test 用相对 #[path = "../src/strict_json.rs"] 编译原字节管理私有模块，直接运行实际 parse_object 8/9层分支；这是原模块源码在测试crate复用，不宣称其成为公开lib接口。其原内置3项cfg(test)也实际执行，完整保留。strict-json-callpoints.txt列出原管理模块/调用点，所有对应源都在132项前后匹配内；未向产品模块注入或修改cfg(test)。没有source overlay变化，只有新增测试文件。

acceptance-frozen.txt为冻结资产正式重复执行的原字节日志，命令退出0：

`cargo test --offline --locked --test raw_json_acceptance -- --test-threads=1`

主harness真实33/33：固定RJ函数30 + 原管理模块内置3。RJ23/24/25/26各自重新运行同一test executable的精确单项，额外4个子进程各1/1；不把它们计成34个新定义用例。每个子进程内线程显式256KiB栈，正常返回、join、子进程exit0三项均必要。设120s仅为tester收敛自有子进程的截止，不是产品性能或容量门槛。本轮均正常结束，无强制终止。

所有大内容均合成非敏感数据。大正文32*1024*1024+257个ASCII字符，另尾Unicode标记，完整比较原bytes、decoded string、fragment和compact；未只比摘要。262145叶数组和131073键对象完整遍历及逐序比较。20000层纯数组/混合对象，64组document/root-owned/深owned共享clone与交错drop，以及深重复键/Unicode/缺闭括号/尾逗号失败清理均在小栈真实执行。oracle是确定性构造文本和逐层get，不使用递归serde_json::Value。

## 固定ID到真实函数

逐项操作和固定预期不变，case-results.json保留原定义行、真实函数、行号和PASS。以下行号对应最终SHA资产。

| ID | 实际函数 | 行号 | 结果 |
| --- | --- | --- | --- |
| RJ01 | `rj01_six_roots_and_exact_original_bytes` | 89 | PASS |
| RJ02 | `rj02_typed_accessors_never_coerce` | 106 | PASS |
| RJ03 | `rj03_invalid_syntax_utf8_and_trailing_data` | 133 | PASS |
| RJ04 | `rj04_full_syntax_validation_precedes_scan_errors` | 173 | PASS |
| RJ05 | `rj05_ordered_decoded_object_and_exact_double_ended_iter` | 189 | PASS |
| RJ06 | `rj06_ordered_array_bounds_and_exact_double_ended_iter` | 223 | PASS |
| RJ07 | `rj07_document_clone_and_root_owned_share_storage` | 252 | PASS |
| RJ08 | `rj08_nested_owned_outlives_every_original_handle` | 270 | PASS |
| RJ09 | `rj09_compact_only_four_json_whitespaces_outside_strings` | 308 | PASS |
| RJ10 | `rj10_compact_nested_fragment_roundtrip_keeps_input` | 318 | PASS |
| RJ11 | `rj11_strings_all_escapes_and_scalar_pairs` | 344 | PASS |
| RJ12 | `rj12_unknown_numbers_keep_arbitrary_lexical_size` | 361 | PASS |
| RJ13 | `rj13_signed_integer_exact_boundaries` | 382 | PASS |
| RJ14 | `rj14_unsigned_exact_boundaries_and_negative_zero` | 403 | PASS |
| RJ15 | `rj15_invalid_unicode_values_and_unknown_descendants` | 419 | PASS |
| RJ16 | `rj16_invalid_unicode_keys_and_pair_boundaries` | 438 | PASS |
| RJ17 | `rj17_first_scanner_error_follows_source_order` | 449 | PASS |
| RJ18 | `rj18_duplicate_decoded_keys_at_every_depth` | 462 | PASS |
| RJ19 | `rj19_sibling_keys_and_no_unicode_normalization` | 479 | PASS |
| RJ20 | `rj20_escaped_delimiters_and_quotes_in_keys` | 498 | PASS |
| RJ21 | `rj21_exceed_old_structural_token_budget` | 509 | PASS |
| RJ22 | `rj22_body_above_thirty_two_mib` | 539 | PASS |
| RJ23 | `rj23_twenty_thousand_arrays` | 554 | PASS |
| RJ24 | `rj24_twenty_thousand_mixed_objects` | 565 | PASS |
| RJ25 | `rj25_deep_clone_owned_and_drop` | 574 | PASS |
| RJ26 | `rj26_deep_failure_drop_is_nonrecursive` | 604 | PASS |
| RJ27 | `rj27_errors_never_include_sensitive_payload` | 633 | PASS |
| RJ28 | `rj28_all_public_handle_debug_redacted` | 676 | PASS |
| RJ29 | `rj29_public_traits_and_temporary_lifetimes` | 713 | PASS |
| RJ30 | `rj30_management_eight_layers_and_old_assets_unchanged` | 752 | PASS |

## 其他实际检查

全部命令均在独立workspace/app内，CARGO_TARGET_DIR为上述独占目录；实际命令、时间、退出码在 final-command-results.json 及各独立exit.json。

| 检查 | 命令要点 | 实际结果 |
| --- | --- | --- |
| 原公开协议/JWS | cargo test --offline --locked --test jws_vectors --test acp_message_id_acceptance --test acp_tool_projection_acceptance --test acp_view_observations_acceptance | exit0；4+6+19+11=40/40 |
| 原管理库 | cargo test --offline --locked --lib strict_json::tests | exit0；3/3 |
| ACP库回归 | cargo test --offline --locked --lib adapters::acp | exit0；69/69，其中包含开发raw8项，单列不充独立30项 |
| 严格Clippy | cargo clippy --offline --locked --all-targets -- -W clippy::all -W clippy::pedantic -W clippy::nursery -D warnings | exit0，包含新独立资产 |
| 格式 | cargo fmt --all -- --check | exit0 |

acceptance-initial/final及clippy-initial原日志保留。第一次Clippy exit101只报告独立资产刻意Clone/显式生命周期检查的3项提示；用两处函数级、附理由allow保留验收分支，其余all/pedantic/nursery仍启用。之后最终Clippy和冻结30项重跑全部通过。没有产品修复，因此未产生需coder关闭的产品问题ID。cargo fmt规范化新增资产后，原132项仍全部原字节。

公开trait和借用签名经真实编译检查，临时object/array句柄迭代器可维持文档'a，双端len/size_hint精确。全部五类句柄Debug/clone与所有错误Display/Debug/to_string/source均执行canary断言；所有五种公开错误变体均覆盖。Allocation仅构造变体/文案，**真实OOM not-run**，不制造耗尽内存来虚报其失败路径。静态平面索引/Arc共享、checked/try_reserve、语法优先和无递归Value回退由同版本独立reviewer的 source-v1-static-review.md（cf31726a14235d5fb5c776d1e61de96c09cc18bf4b80f2763964a1a338676f36）另签；动态耗时不被用作线性复杂度证明。

## 未执行与结论边界

此阶段仅新增基础层，未接现有RawMessage/Strategy/transport/P1/P2；旧32MiB帧、96层/131072-token门槛及旧缓存上限不因本报告删除。原ACP库中旧容量拒绝测试仍通过，与本阶段范围一致。真实NDJSON/outbox/writer/reader、无上限共享缓存、真实Harness/WSS/浏览器、完整产品CL-095/103及独立安全/供应链审查均不由本报告证明。

未运行Web npm audit（用户明确拒绝元数据外发），未安装/联网/提交/推送/发布/删私人数据。未重跑与此基础层无关的Windows全库场景。实际R3先合入后再合raw的完整工程 check/build/check入口应由主执行在真实集成结果运行，不能用此隔离副本替代。

主执行可同源纳入上述唯一portable资产和本证据；此限定PASS只适用于同127源码摘要及同资产SHA。新版本须判断影响并独立重验。tester尚待reviewer亲读本资产/日志作最终交付检视，不能将tester结论代签reviewer。