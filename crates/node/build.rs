//! Builds the shell holder's screen (ADR-0078): `src/shells/vt.c` over libghostty-vt, linked
//! statically from `$LIBGHOSTTY_VT`, the flake's build of it for this machine.

fn main() {
    println!("cargo:rerun-if-changed=src/shells/vt.c");
    println!("cargo:rerun-if-env-changed=LIBGHOSTTY_VT");
    let lib = std::env::var("LIBGHOSTTY_VT")
        .expect("LIBGHOSTTY_VT names libghostty-vt's prefix: build in the flake's dev shell or package");
    cc::Build::new()
        .file("src/shells/vt.c")
        .include(format!("{lib}/include"))
        .flag_if_supported("-std=c11")
        .warnings(true)
        .compile("saarthi_vt");
    // The archive alone, in a directory of its own: beside the shared library, the macOS
    // linker takes that instead, whatever kind is asked for.
    let out = std::env::var("OUT_DIR").unwrap();
    // The store's copy is read-only, and so would this one be on the next build.
    let _ = std::fs::remove_file(format!("{out}/libghostty-vt.a"));
    std::fs::copy(format!("{lib}/lib/libghostty-vt.a"), format!("{out}/libghostty-vt.a")).expect("libghostty-vt.a");
    println!("cargo:rustc-link-search=native={out}");
    println!("cargo:rustc-link-lib=static=ghostty-vt");
}
