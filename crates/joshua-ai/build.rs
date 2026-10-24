// ONNX Runtime's prebuilt macOS library calls `__isPlatformVersionAtLeast`
// (from `@available`), which lives in clang's compiler-rt and is not linked
// automatically by rustc.
fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    println!("cargo:rustc-link-lib=framework=Foundation");
    let resource_dir = std::process::Command::new("xcrun")
        .args(["clang", "--print-resource-dir"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok());
    if let Some(dir) = resource_dir {
        let lib_dir = std::path::Path::new(dir.trim()).join("lib").join("darwin");
        if lib_dir.join("libclang_rt.osx.a").is_file() {
            println!("cargo:rustc-link-search=native={}", lib_dir.display());
            println!("cargo:rustc-link-lib=static=clang_rt.osx");
        }
    }
}
