// Links the prebuilt libtailscale.a (ADR-0015, ADR-0048); the flake builds it and sets the
// variable for the packages, the checks and the dev shell. Nothing here runs `go`.
fn main() {
    println!("cargo:rerun-if-env-changed=LIBTAILSCALE_LIB_DIR");
    let Ok(dir) = std::env::var("LIBTAILSCALE_LIB_DIR") else {
        panic!(
            "LIBTAILSCALE_LIB_DIR must name the directory holding libtailscale.a; \
             build inside `nix develop`, which sets it"
        );
    };
    println!("cargo:rustc-link-search=native={dir}");
    println!("cargo:rustc-link-lib=static=tailscale");
    if std::env::var("TARGET").unwrap_or_default().contains("apple") {
        // Go's runtime and net package on darwin need these.
        println!("cargo:rustc-link-lib=framework=CoreFoundation");
        println!("cargo:rustc-link-lib=framework=Security");
    }
}
