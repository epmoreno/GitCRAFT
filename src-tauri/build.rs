fn main() {
    let mut attributes = tauri_build::Attributes::new();
    if building_for_windows_msvc() {
        // tauri-build embeds the application manifest (Common Controls v6)
        // only in the app binary, so the test binaries start without it and
        // Windows refuses to load them (STATUS_ENTRYPOINT_NOT_FOUND). The
        // same manifest goes in through the linker instead.
        //
        // `cargo:rustc-link-arg` (no suffix) reaches every linked target of
        // this crate, the library's unit-test binary (`gitcraft_lib`)
        // included; `rustc-link-arg-tests` only reaches `tests/*.rs`.
        //
        // Workaround given by a Tauri maintainer in
        // https://github.com/tauri-apps/tauri/issues/13419; fix in progress
        // in https://github.com/tauri-apps/tauri/pull/16320.
        //
        // TODO: remove when Tauri fixes it. When updating tauri-build, check
        // that its default manifest (src/windows-app-manifest.xml in the
        // tauri-build crate) has not changed; ours is a copy of the one in
        // tauri-build 2.7.4.
        attributes = attributes
            .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());
        embed_manifest();
    }
    tauri_build::try_build(attributes).expect("failed to run the Tauri build script");
}

/// The target, not the machine running this script.
fn building_for_windows_msvc() -> bool {
    std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
}

fn embed_manifest() {
    let manifest =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("windows-app-manifest.xml");
    println!("cargo:rerun-if-changed={}", manifest.display());
    println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
}
