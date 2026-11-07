# DrivenByMoss on Zynthian

zynthian controller drivers (ctrldev) which run DrivenByMoss controllers on zynthian: the core runs
the controller, zynthian is its DAW. Plan and state: [doc/ZYNTHIAN_PLAN.md](../../doc/ZYNTHIAN_PLAN.md),
analysis: [doc/ZYNTHIAN_INTEGRATION.md](../../doc/ZYNTHIAN_INTEGRATION.md).

These files are licensed under the GPL (version 2 or later, [LICENSE.txt](LICENSE.txt)), as
zynthian, whose classes they extend; the rest of the repository is LGPLv3.

## Layout

| Path | Content |
| --- | --- |
| `ctrldev/` | The files as they are installed into zynthian's `zyngine/ctrldev/`: the driver base `zynthian_ctrldev_base_dbm.py`, one module per controller (`zynthian_ctrldev_dbm_*.py`), the model adapter package `dbm_zynthian/` |
| `tests/` | The tests without the zynthian UI (`run_tests.py`), the conformance scenarios against the adapter (`zyn_conformance.py`) |

Controllers: the Novation Launch Control XL; the Native Instruments Maschine Mk2, which needs
dbm-nihost (`support/linux/ni_host`) for its MIDI ports, displays and Shift button; the Native
Instruments Maschine JAM, also through dbm-nihost (`Maschine Jam - 1 IN 1`); the Novation
Launchpad X, Mini MkIII, Pro Mk3, Pro and MkII on their MIDI ports. zynthian's own Launchpad
drivers (on the DAW ports) autoload; pick "DrivenByMoss Launchpad …" in the MIDI device options
of the MIDI port (`Launchpad X IN 2`), which then unloads zynthian's driver of that device
([plan §7b](../../doc/ZYNTHIAN_PLAN.md)). The MkII has no zynthian driver and autoloads.

The core's browser browses the banks and presets of the active chain's current processor (the
Mk2's BROWSE: knob 1 the bank, knob 8 the preset, ENCODER PUSH loads, BROWSE again cancels);
meanwhile zynthian shows its preset view of that processor ([plan §3.7](../../doc/ZYNTHIAN_PLAN.md)).
With zynthian's Preset Preload on, the preset under the cursor sounds, also after a bank change;
cancel brings back the bank and preset loaded before. The Mk2's knobs move one entry per knob
event (the core's browse mode), fast on knobs without detents; the encoder steps one per click.

On the Mk2 the group button of the "+" track (after the last chain) opens zynthian's Add Chain
view and the browse mode on its entries (the encoder moves, zynthian's grid follows; ENCODER PUSH
picks the chain type, BROWSE cancels); then the browse mode continues with zynthian's engine
screen (knob 1 the category, the encoder the engine; PUSH picks it, BROWSE goes back to the chain
types). zynthian then asks for the MIDI channel of a MIDI chain, or adds the chain at once
([plan §3.7b](../../doc/ZYNTHIAN_PLAN.md)).

Knobs (relative changes) move stepped parameters (on/off switches, labels, integers) by whole
steps: a short turn switches a toggle (right on, left off) or moves one label
([plan §7](../../doc/ZYNTHIAN_PLAN.md)).

The drivers need zynthian-ui's `dbm` branch (from `nura`): it has the PipeWire port names, the
`dev_in_2_dev_out` fix, `getTransportState` in `zynseq.h` and `ZYNTHIAN_CTRLDEV_PATH`.

## Running zynthian with the drivers of this checkout

Build the library (`cargo build`), then start zynthian's desktop mode with this folder as an extra
driver folder:

```sh
ZYNTHIAN_CTRLDEV_PATH=<this repository>/backends/zynthian/ctrldev <zynthian-ui>/desktop/zynthian_desktop.sh
```

`ZYNTHIAN_CTRLDEV_PATH` (folders separated by `:`) adds folders to zynthian's package
`zyngine.ctrldev`; the drivers then import as if they were installed. The drivers find
`bindings/python` and `target/debug` of this repository themselves; `DBM_PYTHON_DIR` and
`DBM_LIBRARY` override them.

## Tests

```sh
cargo build
python3 backends/zynthian/tests/run_tests.py [-v]
python3 backends/zynthian/tests/zyn_conformance.py launchcontrol_xl track_mix [phase|all]
python3 backends/zynthian/tests/zyn_conformance.py maschine_mk2 global 2
python3 backends/zynthian/tests/zyn_conformance.py maschine_jam session 3
```

They need a zynthian-ui checkout of the `dbm` branch: `$ZYNTHIAN_UI_DIR`, else `../zynthian-dbm`
or `../zynthian-ui` next to this repository (`run_tests.py` exits with 77 without one), and plain
Python 3.11+ (no JACK, no zynthian dependencies: `tests/zynstubs.py` stands in for them). The
conformance tests also need the profiles of the Bitwig setups
(`~/.local/share/dbm-conformance/profiles/<setup>`, or `DBM_PROFILE_DIR`). Because of the
zynthian checkout they are not part of `ci/linux.sh`.
