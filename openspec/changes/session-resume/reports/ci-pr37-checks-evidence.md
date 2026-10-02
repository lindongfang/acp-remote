# PR #37 CI 实测证据（Linux runner）

- **工作流运行**：**两次运行都落在同一提交 `39d4b9b`**（一次 `pull_request`、一次 `push`）——`36973886861`（head `39d4b9b`，事件 **`pull_request`**）与 `36973882474`（head `39d4b9b`，事件 `push`）。**DR1-F103/F109 实测订正**：原写「均为 `push` 触发」不实；订正时又误把第二次的 head 写成 `56de6bd`（那是 `39d4b9b` 的**父提交**、即当时的 `refs/heads/main`），已按 `gh run view 36973882474 --json event,headSha,conclusion` 的实测原文更正。
- **结论**：五个 job 全部 `pass` —— `合同门禁 + Rust 检查`（7m5s / 6m43s）、`提交信息规范`、`依赖许可证与来源`、`依赖安全公告`、`密钥扫描`。

## 一、关闭「`#[cfg(unix)]` 两条用例 PENDING」

此前登记的未闭合项 ①：这两族用例在开发机（Windows）**从未编译、从未执行**，交叉编译缺 `x86_64-linux-gnu-gcc`，证据为零。CI 首次在 Linux 上编译执行，实测**通过**：

```
test a_persisted_directory_replaced_by_a_symlink_is_refused_before_any_backend_call ... ok
test an_inaccessible_persisted_directory_is_refused_before_any_backend_call ... ok
```

同批通过、与 unix/权限相关（或名称相关）的用例（节选）。**DR1-F103 订正口径**：本节原标题写「`#[cfg(unix)]` 用例」，但下列清单中 `unix_mode_bits_map_to_the_expected_verdicts`（`crates/storage-sqlite/tests/permissions.rs`）与 `params_keep_integer_fidelity`（`crates/server/src/local_admin/envelope.rs`）按源码**不是** `#[cfg(unix)]` 门控，只是同批通过；真正的 unix 门控清单见本节下方注记。

```
test unix_mode_bits_map_to_the_expected_verdicts ... ok
test unix_endpoint_creates_socket_with_private_permissions ... ok
test unix_endpoint_tightens_a_pre_existing_directory ... ok
test unix_endpoint_refuses_to_replace_a_live_socket ... ok
test unix_endpoint_replaces_a_stale_socket_and_accepts_same_user_clients ... ok
test unix_endpoint_uses_xdg_runtime_dir_then_data_dir_run ... ok
test transport::net::permissions::tests::unix_inspection_reports_relaxed_for_world_readable_file ... ok
test transport::local::platform::unix::tests::peer_user_id_matches_current_user_for_a_local_pair ... ok
test local_admin::envelope::tests::params_keep_integer_fidelity ... ok
```

⇒ **该项由 PENDING 转为 PASS（有执行证据）**。

## 二、关闭「`cargo-deny` 与 `gitleaks` 本机不可执行」

此前登记的未闭合项 ②：这两类判定**没有本地等价物**，只能由 CI 判定。实测：

- `依赖许可证与来源`（`cargo-deny` bans / licenses / sources）= **pass**（39s / 40s）
- `依赖安全公告`（`cargo-deny` advisories）= **pass**（47s / 53s）
- `密钥扫描`（`gitleaks`）= **pass**（6s / 10s）

⇒ **该项由「未判定」转为 PASS（有 CI 证据）**。

## 三、CI 抓到的一处本地门禁盲区（已修）

`checks` job 首次运行时 `check:docs` 报红：

```
error: openspec/changes/session-resume/reports/wp5-coder-fix-cr5f1.md:82: 链接目标不存在 -> wp5-coder-fix-cr5f1-PV1.log
```

根因：该行用 **markdown 链接**指向 `reports/wp5-coder-fix-cr5f1-PV1.log`，而 `reports/**/*.log` 按 `.gitignore` 排除 ⇒ **本地磁盘存在该文件故通过，CI 干净克隆中不存在故失败**。已改为行内路径并注明不随仓库分发，并全量复核「markdown 相对链接是否指向未跟踪文件」＝ **0**。

⇒ 这是「本地绿 ≠ 交付绿」的一个具体实例，已记入 `verification.md` 的 `## Check Plan Changes`。

## 三·补、两条 PENDING 用例的精确名（DR1-F101 订正）

被登记为 PENDING 的两条，源码位置与 CI 实测结果如下（**两条均在本次 CI 上 `ok`，非 skip、非 ignored**）：

```
test a_persisted_directory_replaced_by_a_symlink_is_refused_before_any_backend_call ... ok   # crates/app/tests/session_resume_e2e.rs:1408（符号链接改指）
test an_inaccessible_persisted_directory_is_refused_before_any_backend_call ... ok          # crates/app/tests/session_resume_e2e.rs:1514（chmod 000 打在父目录）
```

> **DR1-F101 更正留痕**：本文件 **§一 的代码块**与 `tasks.md`、`verification.md` 原把第二条记成 `store::unix_modes::private_directory_and_entry_file_modes_are_restrictive`（`crates/identity-keystore/src/store.rs`），那是**另一条** unix 门控用例——它也通过，但不是 PENDING 清单要关闭的对象。

## 四、仍未闭合

validator 登记的 **5 处 MINOR 测试覆盖盲区**（详见 `reports/validation-session-resume.md`）：属既知覆盖面缺口，非本次 CI 可判定项，仍然开放。
