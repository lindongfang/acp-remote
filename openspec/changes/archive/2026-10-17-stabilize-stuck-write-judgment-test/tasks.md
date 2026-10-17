# Tasks

## 1. 两条同构用例的余量（WP1）

- [x] 1.1 `crates/server/src/sync/conn/tests.rs` 的 `a_stuck_write_still_lets_the_session_judgments_run`：注入写窗口 `WRITE_WINDOW` 30s → 300s、断言预算 `SILENCE + Duration::from_millis(11_400)` → `SILENCE + Duration::from_millis(59_400)`，并同步改写那段记录数字与理由的注释（合法路径构成、本机 ≈3.5s、CI 慢跑器 7.54s、负载翻倍时 16.3s/29.7s、新余量 ≈2× 最差实测且与写窗口保持 5× 间距）——判据：`grep -n "59_400\|from_secs(300)" crates/server/src/sync/conn/tests.rs` 命中，且 `cargo test -p server --lib a_stuck_write_still_lets_the_session_judgments_run` 通过
- [x] 1.2 `crates/server/src/node_link/conn/tests.rs` 的同名用例：同样三处改动，常量、预算与注释逐字与 1.1 一致（两文件保持「逐条同构」）——判据：两个文件里这三处逐行相同（可直接 `diff` 对应的代码片段），且同一用例在该文件下通过
- [x] 1.3 对照实验（一次性，不留代码）：把两条用例的预算临时回退到 12s，在「并行跑两条完整 `cargo test -p server --lib`」的负载形态下复跑该用例，确认它变红；随后恢复 60s——判据：留下一次可读的红输出（含 `assert_eventually_within` 报出的实测耗时），且最终落盘的预算为 60s、写窗口为 300s
- [x] 1.4 门禁：`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test -p server`、`npm run check` 全绿——判据：四条命令的输出
