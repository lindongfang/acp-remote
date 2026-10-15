//! `server` 的构建脚本：在**编译期**判定本次构建嵌入真实前端产物还是树内占位入口，并把选择以 `cfg`
//! 传进 crate（`openspec/changes/sync-ui-hosting/design.md` D2；消费方是
//! `crate::transport::net::ui`）。
//!
//! **为什么在这里选，而不是让 `#[folder]` 直接指向 `clients/app/dist`**：`#[derive(RustEmbed)]` 的
//! `#[folder = "..."]` 在目录不存在时是**编译错误**，而前端产物不入库（根 `.gitignore` 的 `dist/`）、
//! CI 的 Rust 作业也不构建前端。直接指向它会把「编译 Rust」与「先构建前端」绑死。因此两条路径都是
//! **固定的字面量**（一条树外产物目录、一条树内占位目录），由本脚本二选一：字面路径永远存在，
//! 上面那个编译错误不会触发，而 `cargo test` 在没有产物的机器上照常能跑。
//!
//! **为什么不用 `#[folder = "$OUT_DIR/..."]` 或任何环境变量插值**：那需要 `interpolate-folder-path`
//! 特性，它引入 `shellexpand → dirs → dirs-sys → option-ext`，而 `option-ext` 是 MPL-2.0，不在
//! `deny.toml` 的 `[licenses] allow` 白名单里（CI 的「依赖许可证与来源」判定会因此变红）。
//! **也不用**「手写 build script 生成 `include_bytes!` 表」：那是把 `rust-embed` 的活（遍历目录、
//! 生成键值表、二分查找、元数据）自己再写一遍。
//!
//! **失败关闭的边界**（`design.md` D2 与 Risks 一节）：本脚本**不**因为缺产物让编译失败——那同样会把
//! `cargo test` 交给构建顺序。它只负责「编进去的是哪一份」；「发布构建不得携带占位入口」由运行期入口
//! [`crate::transport::net::ui::check_release_build`] 判定，是否发布构建由**调用方**传入
//! （组合根用 `!cfg!(debug_assertions)`）。

use std::env;
use std::error::Error;
use std::path::PathBuf;

/// 入口文档名（构建产物根目录下的固定文件名）。
///
/// **这里是该名字作为「资源表键」的唯一定义处**（诊断文案与测试夹具按字面另计，不是键的来源）：构建
/// 脚本先于 crate 编译、读不到 crate 内的常量，因此由本脚本经 `cargo::rustc-env` 传给 crate
/// （`transport::net::ui::ENTRY_DOCUMENT` 用 `env!` 取值），两侧口径因此不可能漂移。改名时随之要改的是
/// **文件名本身**：占位目录缺该文件会让本脚本直接失败，`clients/app/dist` 的可用性也按下行判定——
/// 两种失败都是响的，不会编出「有 dist 但入口文档取不到」的资源表。
const ENTRY_DOCUMENT: &str = "index.html";

/// 树内占位入口目录（相对本 crate 的清单目录；与 `ui` 模块里 `#[folder]` 的字面量一致）。
const PLACEHOLDER_DIR: &str = "src/transport/net/ui/placeholder";

/// 真实前端产物目录（`clients/app` 是独立 npm 工程，相对本 crate 的清单目录向上两级）。
const DIST_DIR: &str = "../../clients/app/dist";

fn main() -> Result<(), Box<dyn Error>> {
    // 没有这两行时 `cfg(real_ui_dist)`/`cfg(placeholder_ui)` 在 rustc 眼里是未知 cfg，`unexpected_cfgs`
    // 警告会在 CI 的 `clippy -D warnings` 下把门禁弄红。
    println!("cargo::rustc-check-cfg=cfg(real_ui_dist)");
    println!("cargo::rustc-check-cfg=cfg(placeholder_ui)");

    // 入口文档名从这里传给 crate（`ui::ENTRY_DOCUMENT` 用 `env!` 取）：名字只有上面那一处字面量，
    // 两侧口径因此不会漂移——改名的后果是「占位目录缺新文件 ⇒ 本脚本失败」，而不是静默的不一致。
    println!("cargo::rustc-env=UI_ENTRY_DOCUMENT={ENTRY_DOCUMENT}");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let dist = manifest_dir.join(DIST_DIR);
    let placeholder = manifest_dir.join(PLACEHOLDER_DIR);

    // 两处都要跟踪：
    // - 产物目录的**出现/消失**必须触发重建，否则会一直沿用上一次构建的 cfg（危险方向是「产物被删掉后
    //   仍按真实产物编译」，那会让发布路径的失败关闭放过一个已经过时的资源表）；
    // - 占位目录里**新增/删除文件**也要重跑：`rust-embed` 的目录展开发生在编译期，Cargo 不认识新增文件。
    // 代价是产物目录不存在时（CI）cargo 会把「路径缺失」当作已变更、每次重跑本脚本——它只做两次
    // `is_file()` 判定，为正确性接受这点开销。
    println!("cargo::rerun-if-changed={}", dist.display());
    println!("cargo::rerun-if-changed={}", placeholder.display());

    let placeholder_entry = placeholder.join(ENTRY_DOCUMENT);
    if !placeholder_entry.is_file() {
        // 占位目录是**树内**文件：它缺入口文档是本仓库的缺陷，不该被掩盖成「能编译但没有页面」。
        // 用构建脚本的错误返回让 `cargo build` 明确失败（正常运行路径不得 panic，这里也不 panic）。
        return Err(format!(
            "占位入口目录缺少 {ENTRY_DOCUMENT}（{}）",
            placeholder_entry.display()
        )
        .into());
    }

    // 只有「产物目录存在**且**含入口文档」才算可用产物：一个没有入口文档的 dist（构建中断留下的半成品、
    // 或只写了诊断文件）能编出资源表，但入口文档与单页回退都会落空——那正是 D2 禁止的「静默提供空页面」。
    // 判成占位入口后，发布构建的失败关闭会把它连同「请先构建前端」的提示一起拦下来。
    if dist.join(ENTRY_DOCUMENT).is_file() {
        println!("cargo:rustc-cfg=real_ui_dist");
    } else {
        println!("cargo:rustc-cfg=placeholder_ui");
    }
    Ok(())
}
