//! Pass link.x to the linker (rust-lld)

fn main() {
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    println!("cargo:rustc-link-search={dir}");
    println!("cargo:rustc-link-arg-bins=-Tlink.x");
    println!("cargo:rustc-link-arg-bins=--nmagic");
    println!("cargo:rerun-if-changed=link.x");
}
