//! `cfg(ci)` when the `CI` environment variable is set (GitHub Actions sets
//! it): tests that touch real system state, such as the Windows Recycle Bin,
//! always run there, and on a personal computer only with `--ignored`.

fn main() {
    println!("cargo:rustc-check-cfg=cfg(ci)");
    println!("cargo:rerun-if-env-changed=CI");
    let ci = std::env::var("CI").is_ok_and(|v| !v.is_empty() && v != "false" && v != "0");
    if ci {
        println!("cargo:rustc-cfg=ci");
    }
}
