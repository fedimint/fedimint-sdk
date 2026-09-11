fn main() {
    println!("cargo:rerun-if-changed=sdallocx_stub.c");

    // On Android, aws-lc (reached through rustls, via iroh, via
    // fedimint-connectors) declares `sdallocx` as a weak symbol and checks
    // `if (sdallocx)` before calling it. Android's linker resolves the GLOB_DAT
    // entry for a weak undefined symbol to the PLT stub, which is non-NULL,
    // while the JUMP_SLOT stays 0 — so the null check passes and the call
    // jumps to address 0. A strong definition delegating to free() makes both
    // entries resolve. Same fix, same reason, as rust/fedimint-client-uniffi.
    //
    // `-u sdallocx` is what keeps it: the stub is in its own archive member
    // that nothing references, so the linker would otherwise drop it. Forcing
    // the symbol undefined at link time pulls the member in. Doing it here
    // rather than with a `#[used]` static in Rust keeps the crate's
    // `#![forbid(unsafe_code)]` intact, since that would need an
    // `unsafe extern` block.
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "android" {
        cc::Build::new()
            .file("sdallocx_stub.c")
            .compile("sdallocx_stub");
        println!("cargo:rustc-link-arg=-Wl,-u,sdallocx");
    }
}
