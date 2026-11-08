// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Writes the bindings of the typed protocol from the tables in `src/protocol/`
//! (TYPED_PROTOCOL_PLAN §2.1): `include/dbm_protocol.h`, `bindings/cpp/include/dbm_protocol.hpp`,
//! … (see `drivenbymoss::protocol::gen::OUTPUTS`). A file is only written if it changed. The
//! test `the_generated_bindings_are_up_to_date` fails while a committed file is stale.
//!
//! Usage: `cargo run --bin dbm-protocol-gen [<crate root>]`

use std::path::PathBuf;

use drivenbymoss::protocol::gen::OUTPUTS;

fn main() {
    let root = std::env::args().nth(1).map_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")), PathBuf::from);
    for (path, generate) in OUTPUTS {
        let file = root.join(path);
        let content = generate();
        if std::fs::read_to_string(&file).ok().as_deref() == Some(content.as_str()) {
            continue;
        }
        if let Err(error) = std::fs::write(&file, content) {
            eprintln!("{}: {error}", file.display());
            std::process::exit(1);
        }
        println!("Written: {path}");
    }
}
