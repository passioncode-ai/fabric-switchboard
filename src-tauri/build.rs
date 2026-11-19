fn main() {
    // SB-558: the app starts and stops its owner with `block_on` on the main thread, whose stack
    // is 1 MiB on Windows (8 MiB on macOS and Linux), and the owner's futures are large (a debug
    // test thread of 2 MiB overflowed in nightly 44533106888). The runtime's own threads get
    // 16 MiB in `main` (`RUNTIME_STACK_BYTES`), as the CLI's since 0.6.61 (`STACK_BYTES`); the
    // main thread gets the same from the linker (MSVC `link.exe` and `lld-link` both take it).
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        println!("cargo:rustc-link-arg-bins=/STACK:16777216");
    }
    tauri_build::build()
}
