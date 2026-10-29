// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Generates the C header `include/drivenbymoss.h` from `src/ffi.rs` (see `cbindgen.toml`).
//! The header is committed, so hosts do not need Rust tooling. It is only rewritten if its
//! content changed.

fn main() {
    println!("cargo:rerun-if-changed=src/ffi.rs");
    println!("cargo:rerun-if-changed=cbindgen.toml");

    let crate_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set by cargo");
    let config = cbindgen::Config::from_file(format!("{crate_dir}/cbindgen.toml")).expect("cbindgen.toml is valid");
    // Only the C interface module (with_crate would export all public constants of the crate)
    match cbindgen::Builder::new().with_src(format!("{crate_dir}/src/ffi.rs")).with_config(config).generate() {
        Ok(bindings) => {
            bindings.write_to_file(format!("{crate_dir}/include/drivenbymoss.h"));
        }
        // Do not fail the build (e.g. while ffi.rs is edited); the committed header is kept
        Err(error) => println!("cargo:warning=Could not generate the C header: {error}"),
    }
}
