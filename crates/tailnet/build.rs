// Links libtailscale (ADR-0015, ADR-0048): the prebuilt static archive on the host, the
// shared library for Android, where Go builds no archive (ADR-0016). The flake builds both
// and sets the variables for the packages, the checks and the dev shells. Nothing here
// runs `go`.
fn main() {
    let android = std::env::var("CARGO_CFG_TARGET_OS").is_ok_and(|os| os == "android");
    let var = if android { "LIBTAILSCALE_ANDROID_LIB_DIR" } else { "LIBTAILSCALE_LIB_DIR" };
    println!("cargo:rerun-if-env-changed={var}");
    let Ok(dir) = std::env::var(var) else {
        panic!("{var} must name the directory holding libtailscale; build inside the flake's dev shell, which sets it");
    };
    println!("cargo:rustc-link-search=native={dir}");
    if android {
        // The app ships libtailscale.so beside the core in its jniLibs.
        println!("cargo:rustc-link-lib=dylib=tailscale");
        return;
    }
    println!("cargo:rustc-link-lib=static=tailscale");
    if std::env::var("TARGET").unwrap_or_default().contains("apple") {
        // Go's runtime and net package on darwin need these.
        println!("cargo:rustc-link-lib=framework=CoreFoundation");
        println!("cargo:rustc-link-lib=framework=Security");
    }
}
