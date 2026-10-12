fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        cc::Build::new()
            .file("vendor/everything/src/Everything.c")
            .warnings(false)
            .compile("everything_sdk");
        println!("cargo:rustc-link-lib=user32");
        println!("cargo:rustc-link-lib=shell32");
        println!("cargo:rerun-if-changed=vendor/everything");
    }
    tauri_build::build()
}
