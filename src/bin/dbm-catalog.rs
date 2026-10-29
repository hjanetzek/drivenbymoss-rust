// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Prints the supported controllers as JSON (see `dbm_list_controllers`). Used to generate the
//! controller definitions of the Bitwig extension (`backends/bitwig/tools/gen_definitions.py`).

fn main() {
    println!("{}", drivenbymoss::controller::catalog_json());
}
