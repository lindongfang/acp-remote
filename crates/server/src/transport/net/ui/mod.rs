//! 前端构建产物的**嵌入资源表**：入口文档、按路径查资源、枚举资源与构建版本，以及发布构建的失败关闭
//! （`openspec/changes/sync-ui-hosting/design.md` D2/D3，`specs/ui-asset-hosting/spec.md` 的
//! 「根目录入口与单页回退」与「前端构建版本与 Daemon 可检查关联」）。
//!
//! **为什么嵌入而不是运行期读磁盘**：产物随二进制一起发布，运行期零 IO、零路径解析。更关键的是安全面：
//! 表是编译期生成的**精确键集合**，不存在的路径根本查不到，因此静态面**没有路径穿越面**，本模块也就不
//! 需要（更**不得**）做 `..` 归一化之类的路径处理——那种处理恰好是「未命中」被改写成「可能撞上别的
//! 条目」的常见来源。
//!
//! **两份资源表、编译期二选一**（见 `crates/server/build.rs`）：前端产物不入库（根 `.gitignore` 的
//! `dist/`），而 `#[folder = "..."]` 在目录不存在时是编译错误。因此 `build.rs` 在「真实产物」与「树内
//! 占位入口」两条**字面**路径之间发 `cargo::rustc-cfg` 选一条——两条路径都永远存在，`.gitignore` 与
//! 构建顺序不会再影响 Rust 侧能否编译。
//! 开发/测试（debug 构建）走占位入口不阻断 `cargo test`；**发布构建**检测到占位入口即失败关闭
//! （[`check_release_build`]，调用点在组合根，按 `!cfg!(debug_assertions)` 传入）。
//!
//! **构建版本**（D3）：前端构建钩子把 `build-version.json` 写进产物根目录
//! （`clients/app/scripts/write-build-version.mjs`），Daemon 从**同一份嵌入资源**里读它，因此版本与资源
//! 必然出自同一次构建。不用「Daemon 自己算产物哈希」：那只能证明资源没被改，回答不了「这是哪一次构建」
//! ——而「刷新后仍拿到旧 JS 去连新 Daemon」这类诊断要的正是后者。版本文件缺失或不可用时返回 `None`
//! （明确缺失语义），**不编造版本**。

use std::borrow::Cow;

use axum::body::Bytes;

use crate::transport::net::static_ui::UiAssetSource;

/// 入口文档在资源表里的键（构建产物根目录下的固定文件名）。
///
/// **唯一来源是构建脚本**（`crates/server/build.rs` 经 `cargo::rustc-env` 传入，见那里的说明）：构建
/// 脚本先于本 crate 编译、读不到 crate 内的常量，因此该名字的字面量只写在那一侧；这里用 `env!` 取值，
/// 两侧口径不可能漂移。改名只需改 `build.rs` 一处——而占位目录缺新名字会让构建**直接失败**，不会静默
/// 编出一份取不到入口文档的资源表。
pub const ENTRY_DOCUMENT: &str = env!("UI_ENTRY_DOCUMENT");

/// 构建版本文件的键（前端构建钩子写在产物根目录）。
pub const BUILD_VERSION_FILE: &str = "build-version.json";

/// 真实前端产物的资源表（`clients/app/dist` 存在且含入口文档时由 `build.rs` 选中）。
///
/// 不 `use rust_embed::RustEmbed`（也不 `use rust_embed::Embed`）：`get`/`iter` 由 derive 生成为
/// **固有**方法（见 `rust-embed-impl` 的展开），把 trait 导入作用域只会多一个未使用导入，
/// 而 `clippy -D warnings` 会把未使用导入当门禁失败。
#[cfg(real_ui_dist)]
#[derive(rust_embed::RustEmbed)]
#[folder = "../../clients/app/dist/"]
pub struct EmbeddedUi;

/// 占位入口的资源表（产物缺失或不可用时由 `build.rs` 选中）。
///
/// 目录是**树内**文件（`src/transport/net/ui/placeholder/`）：`#[folder]` 的字面量在任何环境下都存在，
/// 这正是「两条字面路径二选一」要解决的问题。占位入口必须能被**程序**识别（[`EmbeddedUi::is_placeholder`]
/// 读的是 `build.rs` 的编译期标记），而不是靠读入口文档的内容猜。
#[cfg(placeholder_ui)]
#[derive(rust_embed::RustEmbed)]
#[folder = "src/transport/net/ui/placeholder/"]
pub struct EmbeddedUi;

/// 资源表键 → 字节的统一取用。
///
/// 泛型参数只为了让**测试夹具表**走同一条代码路径（`tests` 里的 `WithVersion`/`WithoutVersion`）：
/// 生产表与夹具表都只经这里查字节，因此单测覆盖的是真实实现，而不是它的复制品。
fn table_file<T: rust_embed::RustEmbed>(key: &str) -> Option<Bytes> {
    let file = T::get(key)?;
    // 嵌入形态下 `data` 是 `Cow::Borrowed(&'static [u8])`（`debug-embed` 在 debug 与 release 都生效），
    // 因此正常路径零拷贝；`Cow::Owned` 分支只为「将来关掉 `debug-embed`」保留（那时它才可能出现）。
    Some(match file.data {
        Cow::Borrowed(bytes) => Bytes::from_static(bytes),
        Cow::Owned(bytes) => Bytes::from(bytes),
    })
}

/// 资源表里的全部路径，口径与 [`UiAssetSource::asset`] 一致（以 `/` 开头的根相对绝对路径）。
///
/// derive 生成的键表按路径字典序（`BTreeMap`），补前导 `/` 不改变顺序，因此返回**升序**且稳定。
fn table_paths<T: rust_embed::RustEmbed>() -> Vec<String> {
    T::iter().map(|path| format!("/{path}")).collect()
}

impl EmbeddedUi {
    /// 组装资源源（组合根用）。无状态，`Arc::new(EmbeddedUi::new())` 即可跨任务共享。
    pub const fn new() -> Self {
        Self
    }

    /// 本次构建嵌入的是不是**占位入口**。
    ///
    /// 读的是 `build.rs` 传进来的编译期标记（`cfg(placeholder_ui)`），**不是**入口文档的内容：内容判定
    /// 会在前端改文案时失效，而「这份二进制有没有真实前端产物」是编译期事实。
    pub fn is_placeholder(&self) -> bool {
        cfg!(placeholder_ui)
    }

    /// 资源表里的全部路径（升序，以 `/` 开头，可直接送回 [`UiAssetSource::asset`]）。
    pub fn paths(&self) -> Vec<String> {
        table_paths::<Self>()
    }

    /// 本次托管前端的构建版本；没有可识别的版本信息时返回 `None`。
    ///
    /// 版本来自**同一份嵌入资源**里的 [`BUILD_VERSION_FILE`]，因此它必然与当前托管的资源出自同一次
    /// 构建；`None` 是**显式**的缺失语义（调用方如实上报缺失，不编造版本）。
    pub fn build_version(&self) -> Option<String> {
        parse_build_version(table_file::<Self>(BUILD_VERSION_FILE).as_deref())
    }
}

impl Default for EmbeddedUi {
    fn default() -> Self {
        Self::new()
    }
}

impl UiAssetSource for EmbeddedUi {
    fn entry_document(&self) -> Bytes {
        table_file::<Self>(ENTRY_DOCUMENT).unwrap_or_else(fallback_entry_document)
    }

    fn asset(&self, path: &str) -> Option<Bytes> {
        // 未命中就返回 `None`：是否回退到入口文档由静态面的解析顺序决定，资源表不参与那个判断。
        embed_key(path).and_then(table_file::<Self>)
    }
}

/// 静态面的路径（`/…`）→ 资源表键（相对产物根目录）。
///
/// **这不是路径处理**：只去掉一个前导 `/`。`..`、重复斜杠、`\` 一律**原样**作为键交给表查找——表是编译期
/// 生成的精确键集合，表外的一切键都不可能命中，穿越入口在类型上就不存在。
///
/// 不以 `/` 开头的入参不是静态面的口径（处理器传的是 `request.uri().path()`），返回 `None` 而不是顺手
/// 补一个分隔符：补出来的键只会撞上无关条目。
fn embed_key(path: &str) -> Option<&str> {
    path.strip_prefix('/')
}

/// [`BUILD_VERSION_FILE`] 的形状。
///
/// 只取 `version`：钩子还会写「哪一次构建」的辅助字段（`buildIdentifier`/`source`/`builtAt`），它们帮助
/// 人排查但不是判据，因此**允许存在并被忽略**——不写 `deny_unknown_fields`，那会让前端加一个诊断字段
/// 变成破坏性改动。
#[derive(serde::Deserialize)]
struct BuildVersionFile {
    version: String,
}

/// 从构建版本文件的原始字节取版本串。
///
/// 四种情况都归一到 `None`：文件不存在、JSON 不可解析、缺 `version` 键、值只有空白。它们都等于「没有
/// 任何可信的构建标识」，而 spec 的要求是 MUST NOT 悄悄返回一个编造的版本——`None` 就是显式的缺失语义。
/// 不为「文件在但解析失败」另造一个可查询状态：该文件由本仓库的构建钩子产出，出现却没写对属构建期缺陷，
/// 不是运维要区分的运行期状态。
fn parse_build_version(raw: Option<&[u8]>) -> Option<String> {
    let parsed: BuildVersionFile = serde_json::from_slice(raw?).ok()?;
    let version = parsed.version.trim();
    if version.is_empty() {
        return None;
    }
    Some(version.to_owned())
}

/// 入口文档缺失时的兜底：一份写明「资源表与构建脚本不一致」的最小 HTML。
///
/// 这条分支**不可达**：`build.rs` 只在产物含入口文档时选真实表，占位目录缺入口文档时它直接让构建失败
/// （单测另有一条断言锁定「两张资源表都含入口文档」）。保留它是因为 `entry_document()` 的签名返回
/// `Bytes` 而不是 `Option`，而正常运行路径不得 panic：这里给出的是一份说明问题的页面，不是空页面，
/// 也不是「200 的沉默」。
///
/// 内容遵守本模块自己的 CSP（无内联样式/脚本），否则这份诊断页面会被静态面下发的响应头拒绝渲染。
fn fallback_entry_document() -> Bytes {
    Bytes::from_static(MISSING_ENTRY_DOCUMENT.as_bytes())
}

/// [`fallback_entry_document`] 的正文。
const MISSING_ENTRY_DOCUMENT: &str = "<!doctype html><html lang=\"zh-CN\"><head><meta charset=\"utf-8\"><title>ACP Remote 前端资源表不完整</title></head><body><h1>前端资源表不完整</h1><p>本二进制嵌入的资源表里没有入口文档（index.html）：构建脚本与资源表不一致。请重新构建；开发环境请先在 clients/app 运行 npm run export:web。</p></body></html>";

/// 发布构建携带占位入口（即没有真实前端产物）时返回的错误。
///
/// 错误信息必须**指向下一步**：运维看到的是「先构建前端」，而不是一个内部错误码。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "发布构建缺少前端产物：本次二进制嵌入的是占位入口（构建时 clients/app/dist/index.html 不存在）。请先在 clients/app 运行 npm run export:web 产出 dist/，再重新构建 Daemon；发布构建（daemon start）不会提供占位页面。"
)]
pub struct MissingFrontendBuild;

/// 发布构建的失败关闭判定：占位入口 + 发布构建 → 明确失败（MUST NOT 静默提供一个空页面）。
///
/// **两个输入都是参数，不在函数体里读 `cfg!`**：「是否占位」是编译期事实
/// （[`EmbeddedUi::is_placeholder`]），「是否发布构建」由调用方给出（生产调用点是组合根，传
/// `!cfg!(debug_assertions)`）。写死 `cfg!` 会让两条分支各自只在一种环境里可测，而这两条分支恰好都
/// **必须**锁住：发布路径不得静默发空页面，测试路径不得被「先构建前端」绑死（design.md D2 与 Risks 一节
/// ——产物不入库，CI 的 Rust 作业不构建前端，因此那里永远面对占位入口，无条件失败关闭会把整片
/// `cargo test --workspace` 弄红）。
pub fn check_release_build(
    is_placeholder: bool,
    release_build: bool,
) -> Result<(), MissingFrontendBuild> {
    if is_placeholder && release_build {
        return Err(MissingFrontendBuild);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::net::static_ui::ENTRY_CONTENT_TYPE;
    use crate::transport::net::static_ui::content_type_for_path;

    /// 测试夹具：**有**构建版本文件的资源表。
    ///
    /// 为什么用夹具目录而不是断言主检出里 `clients/app/dist` 的实际内容：产物不入库，CI 的 Rust 作业
    /// 根本没有它。夹具目录入库，因此在任何环境下都能驱动「有版本 / 无版本」两条分支。
    #[derive(rust_embed::RustEmbed)]
    #[folder = "src/transport/net/ui/testdata/with-version/"]
    struct WithVersion;

    /// 测试夹具：**没有**构建版本文件的资源表（「版本缺失」分支）。
    #[derive(rust_embed::RustEmbed)]
    #[folder = "src/transport/net/ui/testdata/without-version/"]
    struct WithoutVersion;

    #[test]
    fn fixture_assets_resolve_by_path_and_misses_are_none() {
        // 夹具表基于**与生产表同一份** `table_file`/`table_paths`，因此这里覆盖的不是复制品。
        let entry = table_file::<WithVersion>("index.html").expect("夹具表必须含入口文档");
        assert!(entry.starts_with(b"<!doctype html>"));
        assert!(table_file::<WithVersion>("assets/app.js").is_some());
        // 未命中即 `None`：包括相邻的子路径、`..` 形态（不归一化后去撞表）与空键。
        assert_eq!(table_file::<WithVersion>("assets/missing.js"), None);
        assert_eq!(table_file::<WithVersion>("assets/../index.html"), None);
        assert_eq!(table_file::<WithVersion>(""), None);
    }

    #[test]
    fn asset_and_entry_document_share_one_path_convention() {
        // 静态处理器按 `request.uri().path()` 原样传路径（`/…` 口径），因此 `/index.html` 必须命中与
        // `entry_document()` 相同的字节。这条在**两种** cfg 下都成立（两张表都含入口文档）。
        let ui = EmbeddedUi::new();
        assert_eq!(ui.asset("/index.html"), Some(ui.entry_document()));
        // 不是静态面口径的入参、以及表外的一切路径：未命中，**不**回退入口文档（回退由静态面的解析顺序
        // 决定，资源表只回答「表里有没有」）。
        for path in [
            "index.html",
            "",
            "/",
            "//index.html",
            "/assets/../../secret",
            "/../clients/app/package.json",
            "/definitely-not-an-asset.wp2",
        ] {
            assert_eq!(ui.asset(path), None, "路径 {path}");
        }
    }

    #[test]
    fn enumeration_is_sorted_and_uses_the_asset_convention() {
        let paths = EmbeddedUi::new().paths();
        assert!(!paths.is_empty());
        assert!(
            paths.iter().all(|path| path.starts_with('/')),
            "枚举口径必须与 asset() 一致：{paths:?}"
        );
        assert!(paths.iter().any(|path| path == "/index.html"), "{paths:?}");
        let mut sorted = paths.clone();
        sorted.sort();
        assert_eq!(paths, sorted, "枚举顺序必须稳定");
        // 夹具表的枚举逐项锁定；生产表的条目取决于 dist 内容（不可断言具体集合），只断言上面那两条形状。
        assert_eq!(
            table_paths::<WithVersion>(),
            vec![
                "/assets/app.js".to_owned(),
                "/build-version.json".to_owned(),
                "/index.html".to_owned(),
            ]
        );
        assert_eq!(
            table_paths::<WithoutVersion>(),
            vec!["/assets/app.js".to_owned(), "/index.html".to_owned()]
        );
    }

    #[test]
    fn build_version_is_read_from_the_same_embedded_table() {
        // 「有」：夹具表里的版本文件被解析出来（形状见 clients/app/scripts/write-build-version.mjs）。
        assert_eq!(
            parse_build_version(table_file::<WithVersion>(BUILD_VERSION_FILE).as_deref()),
            Some("9.9.9+fixture".to_owned())
        );
        // 「无」：表里没有该文件 → 显式缺失语义。
        assert_eq!(
            parse_build_version(table_file::<WithoutVersion>(BUILD_VERSION_FILE).as_deref()),
            None
        );
        // 「有文件但不可用」同样归一到缺失语义，且**不编造**版本。
        assert_eq!(parse_build_version(None), None);
        assert_eq!(parse_build_version(Some(b"{}")), None);
        assert_eq!(parse_build_version(Some(b"not json")), None);
        assert_eq!(parse_build_version(Some(br#"{"version":"   "}"#)), None);
        assert_eq!(parse_build_version(Some(br#"{"version":42}"#)), None);
        // 钩子写的诊断字段被忽略（不 `deny_unknown_fields`），前端可以加字段而不破坏 Daemon。
        assert_eq!(
            parse_build_version(Some(
                br#"{"version":"1.2.3+abc","source":"git-describe","builtAt":"2026-01-01T00:00:00.000Z"}"#
            )),
            Some("1.2.3+abc".to_owned())
        );
    }

    #[test]
    fn placeholder_never_reports_a_version() {
        // 占位入口**不是**任何一次构建：它的版本必须是缺失语义（spec「版本缺失时不静默通过」）。
        // 这条只在占位环境（CI 的 Rust 作业、没有产物的开发机）里有意义；真实产物环境下版本由前端构建
        // 钩子写入、取决于 dist 内容，因此不做断言。
        if EmbeddedUi::new().is_placeholder() {
            assert_eq!(EmbeddedUi::new().build_version(), None);
            assert_eq!(table_file::<EmbeddedUi>(BUILD_VERSION_FILE), None);
        }
    }

    #[test]
    fn the_embedded_table_always_carries_an_entry_document() {
        // 两种 cfg 的共同不变量：真实表只在产物含入口文档时才被选中，占位表缺它时构建脚本直接失败
        // （build.rs）。它也是 `entry_document()` 兜底分支不可达的依据。
        assert!(table_file::<EmbeddedUi>(ENTRY_DOCUMENT).is_some());
    }

    #[test]
    fn release_build_fails_closed_on_a_placeholder_entry() {
        let error = check_release_build(true, true).expect_err("占位入口 + 发布构建必须失败关闭");
        let message = error.to_string();
        for expected in ["clients/app", "npm run export:web"] {
            assert!(
                message.contains(expected),
                "错误信息必须指向「先构建前端」，实际：{message}"
            );
        }
        // 非发布构建（测试/开发环境）放行：否则 CI 的 Rust 作业会被前端构建顺序绑死。
        assert_eq!(check_release_build(true, false), Ok(()));
        // 真实产物：两种形态都放行。
        assert_eq!(check_release_build(false, true), Ok(()));
        assert_eq!(check_release_build(false, false), Ok(()));
    }

    #[test]
    fn the_entry_document_key_is_html_for_the_static_layer() {
        // 内容类型映射**只有一份**（`static_ui`）：本模块只给字节，不给类型。这条断言把两边的口径绑在
        // 一起——入口文档的键必须落在静态面的 HTML 类型上（改名成 `.htm` 或无扩展名会让它回落成
        // `application/octet-stream`，配合 `nosniff` 后页面会被浏览器拒绝渲染）。
        assert_eq!(
            content_type_for_path(&format!("/{ENTRY_DOCUMENT}")),
            ENTRY_CONTENT_TYPE
        );
    }
}
