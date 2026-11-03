# DrivenByMoss in Rust

A Rust port of [DrivenByMoss](https://github.com/git-moss/DrivenByMoss) by Jürgen Moßgraber: the
controller framework and controller implementations, as one library with a C ABI which DAW
extensions load. Work in progress.

## What is there

- **The core** (`src/`): the framework of DrivenByMoss (views, modes, commands, scales, the model
  of the DAW) and the ported controllers:
  - Novation Launchpad (MkII, Pro, Pro Mk3, X, Mini MkIII) and Launch Control XL.
  - Native Instruments Maschine Mk2 and Maschine JAM.
  - Mackie MCU and HUI, each with extenders.
  - Akai APC mini Mk1/Mk2, APC40 Mk1/Mk2, MPC/Force (ACVS) and Fire (with its OLED display).
  - Generic Flexi.

  It builds `libdrivenbymoss` with a handle-based C ABI (`include/`, `doc/BACKEND_PROTOCOL.md`).
- **Backends** (`backends/`):
  - `reaper`: the Reaper extension (C++, through the C++ wrapper in `bindings/cpp`); tested on
    Linux.
  - `bitwig`: a Bitwig Studio extension (Java, Foreign Function & Memory API, JDK 25) which
    loads the library.
- **Bindings** (`bindings/`):
  - `cpp`: a header-only C++ wrapper.
  - `python`: a Python binding, ctypes over the C ABI, standard library only, Python 3.11+.
    On top of it:
    - `dbm_host`: a reference host, a complete bridge on a simulated DAW, as the readable
      example for connecting another DAW.
    - `conformance`: the conformance scenarios in process. All 195 scenarios compare the same
      as the Java version on Bitwig, without Bitwig.

    See `bindings/python/README.md`.
- **dbm-nihost** (`support/linux/ni_host`): a Linux user-space host for the Maschine Mk2 and
  JAM, which have no MIDI driver on Linux: it reads them over hidraw and creates MIDI ports as
  NI's driver does.
- **Conformance tests** (`tests/conformance/`, `backends/bitwig/tools/`): the same MIDI played
  into the Java DrivenByMoss and this port on Bitwig, LEDs, displays and Bitwig's state compared;
  recorded sessions are replayed in `cargo test`.

## Build

```
cargo build --release          # the library
cargo test                     # the tests, including the replays of recorded Bitwig sessions
ci/linux.sh                    # everything that builds on Linux (crate, C++ wrapper, Python
                               # binding and reference host, dbm-nihost, Reaper and Bitwig
                               # extensions)
python3 -m unittest discover -s bindings/python/tests          # the Python binding and host
(cd bindings/python && python3 -m conformance --all)           # the conformance scenarios on
                                                               # the Python reference host
```

The Reaper extension: `backends/reaper` (CMake, see `doc/STATUS.md`). The Bitwig extension:
`backends/bitwig` (Maven). dbm-nihost: `support/linux/ni_host/README.md`.

## Documentation

- `doc/STATUS.md`: the state of the port and the open items.
- `doc/PORTING_GUIDE.md`: how the Java code maps to Rust.
- `doc/BACKEND_PROTOCOL.md`, `doc/HOST_BRIDGE_GUIDE.md`: the protocol between the core and a DAW,
  and how to write a bridge for another DAW.
- `doc/BINDINGS_PLAN.md`, `doc/PYTHON_HOST_PLAN.md`: the language bindings, and the Python
  binding with its reference host.
- `doc/BITWIG_DIVERGENCES.md`: where the Bitwig backend differs from the Java version, and why.
- `doc/CONFORMANCE_TESTING_PLAN.md`, `doc/CONFORMANCE_AUTOMATION_PLAN.md`: the conformance tests
  and their unattended runs.
- `doc/UPSTREAM_CANDIDATES.md`: fixes found in the Java DrivenByMoss.

## Origin and license

Split out of [DrivenByMoss4Reaper](https://github.com/git-moss/DrivenByMoss4Reaper) (branch
`rust-port`, folder `rust/`) with its history. LGPL-3.0, see `LICENSE`; the parts of dbm-nihost
taken from cabl are MIT (`support/linux/ni_host/LICENSE-cabl.txt`).
