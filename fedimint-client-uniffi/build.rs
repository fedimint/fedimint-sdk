fn main() {
    fedimint_build::set_code_version();

    // set_code_version() emits a `cargo:rerun-if-env-changed` directive, which
    // disables cargo's rerun-on-any-change default, so the stub needs its own
    // rerun directive or edits to it are silently ignored by stale builds.
    println!("cargo:rerun-if-changed=sdallocx_stub.c");

    // On Android, aws-lc declares `sdallocx` as a weak symbol and checks
    // `if (sdallocx)` before calling it. Android's linker resolves the GLOB_DAT
    // entry for weak undefined symbols to the PLT stub (non-NULL) while the
    // JUMP_SLOT remains 0, causing a SIGSEGV. Provide a strong stub that
    // delegates to free() so both GOT entries resolve correctly.
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "android" {
        cc::Build::new()
            .file("sdallocx_stub.c")
            .compile("sdallocx_stub");
    }
}
