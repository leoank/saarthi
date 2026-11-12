// Links the prebuilt libtailscale.a (ADR-0015) and compiles the spike's proto.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::var("LIBTAILSCALE_LIB_DIR")
        .expect("LIBTAILSCALE_LIB_DIR must point at the directory holding libtailscale.a");
    println!("cargo:rerun-if-env-changed=LIBTAILSCALE_LIB_DIR");
    println!("cargo:rustc-link-search=native={dir}");
    println!("cargo:rustc-link-lib=static=tailscale");
    if std::env::var("TARGET")?.contains("apple") {
        println!("cargo:rustc-link-lib=framework=CoreFoundation");
        println!("cargo:rustc-link-lib=framework=Security");
    }
    tonic_prost_build::compile_protos("proto/echo.proto")?;
    Ok(())
}
