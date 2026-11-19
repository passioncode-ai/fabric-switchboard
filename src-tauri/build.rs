fn main() {
    // SB-558: the app starts its owner with `block_on` on the main thread, whose stack is 1 MiB
    // on Windows (8 MiB on macOS and Linux). The owner's futures outgrew that in a debug build
    // (nightly 44533106888); the CLI already runs on 16 MiB (`STACK_BYTES`, 0.6.60), and the app's
    // main thread gets the same from the linker (MSVC `link.exe` and `lld-link` both take it).
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        println!("cargo:rustc-link-arg-bins=/STACK:16777216");
    }
    tauri_build::build()
}
