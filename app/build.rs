use std::{env, fs, path::PathBuf, process};

fn main() {
    let manifest_dir = env::var_os("CARGO_MANIFEST_DIR").map_or_else(
        || {
            eprintln!("CARGO_MANIFEST_DIR is unavailable");
            process::exit(1);
        },
        PathBuf::from,
    );
    let index = manifest_dir.join("../clients/web/dist/index.html");
    println!("cargo:rerun-if-changed={}", index.display());
    if !fs::metadata(&index).is_ok_and(|metadata| metadata.is_file()) {
        eprintln!(
            "missing embedded Web build at {}; run `npm --prefix clients/web run build` first",
            index.display()
        );
        process::exit(1);
    }
}
