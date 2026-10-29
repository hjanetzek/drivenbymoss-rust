# Language Bindings Plan

Goal: make DrivenByMoss (Rust) usable from several DAWs and projects, e.g. Reaper (C++),
Zynthian (Python) and Java hosts, through one stable native library.

## Decision

Keep a **hand-written, stable C ABI** as the only primitive. Generate its header with
**cbindgen**. Add a **thin hand-written wrapper per language** on top of it.

SWIG and the other generators are not used:

- **The interface is small.** It has 16 functions and a callback table of 18 entries. Every
  argument is a string, an int, a double or a byte buffer. The DAW-specific meaning lives in
  the text protocol (`track/0/volume 0.7`, processor/command pairs), not in the API types.
- **Callbacks are the hard part, and SWIG is weakest there.** It needs "directors" (C++ wrapper
  classes with virtual methods), typemaps for buffers and string arrays, and care with threads:
  the Python GIL and JNI thread attachment.
- **SWIG adds build cost.** It needs a generator in the build and a compiled glue library per
  language. The plain C ABI needs neither: hosts ship the one `.so`/`.dll`/`.dylib`.

## Options considered

| Option | C++ | Python | Java | Callbacks | Cost |
| --- | --- | --- | --- | --- | --- |
| SWIG on the C header | yes | yes | yes | Directors, typemaps, thread care | Generator plus compiled glue per language |
| **C ABI + thin wrappers (chosen)** | Header-only RAII class | `cffi` (ABI mode) or `ctypes`, pure Python | FFM/Panama (Java 22+, `jextract` from the header) or JNA | Natural; ctypes/cffi take the GIL, FFM upcalls attach the thread | Only the native library |
| UniFFI (Mozilla) | Community generator | Official | Community generators | Good (callback interfaces) | Dependency plus its own wire format; the API has to be reshaped |
| Diplomat (ICU4X) | yes | yes (nanobind) | Partial (Kotlin) | Limited | Pays off for large object-oriented APIs |
| PyO3/maturin, `jni` crate, `cxx` | `cxx` | PyO3 | `jni` | Good | One idiomatic binding per language, the most code to maintain |

Revisit UniFFI if the API becomes object-oriented (many instance types, iterators, rich types
across the boundary).

## API changes before bindings

These come from the shape of the API, not from the binding tool. Every binding would bake them
in, so they are done first.

1. **Handle-based API (done).**
   - The global singleton is replaced by `DbmApp* dbm_create(...)` / `dbm_destroy(DbmApp*)`.
   - All other functions take the handle.
   - Several independent instances can exist in one process: several projects, or a host that
     loads the library more than once.
2. **Optional host-driven ticking (done).**
   - `DBM_FLAG_HOST_TICK` turns off the internal 30 ms timer thread.
   - The host then calls `dbm_tick()` from its own loop. This suits Python (GIL) and hosts with
     an event loop.
   - Without the flag, the internal thread is kept; this is the default for Reaper.
3. **Header generation with cbindgen (done).**
   - `build.rs` writes `include/drivenbymoss.h`, configured in `cbindgen.toml`.
   - The header is committed, so hosts don't need Rust tooling. A CI check can verify it is up
     to date.
4. **Documented ownership and threading rules, in the header (done).**
   - All strings are NUL-terminated UTF-8.
   - Strings returned by the library are released with `dbm_free_string`.
   - Buffers passed in either direction are only valid during the call.
   - Calls on one handle are serialized internally.
   - Callbacks run on the calling thread or on the timer thread.
   - Panics never cross the boundary. The release profile must not set `panic = "abort"`.
   - Every callback may be NULL, which means it is ignored. `open_*` then reports failure and
     `get_midi_*` an empty list. Bindings only have to implement what their host needs.
5. **Backend protocol specification (todo).**
   - The processor/command names and the model update lines are defined only by the C++
     processors in `backends/reaper`. Write them down as the real host interface.
   - Make Reaper-only parts optional capabilities that a backend declares: action IDs, the INI
     file processor, and so on.

## Language wrappers (todo, in this order)

1. **C++.**
   - A header-only wrapper `include/drivenbymoss.hpp`: an RAII `dbm::App` class and an abstract
     `dbm::Backend` class with virtual methods, mapped onto `DbmCallbacks`.
   - Switch the Reaper extension (`backends/reaper`, currently `JvmManager` + JNI
     `RegisterNatives`) to it.
2. **Python (Zynthian).**
   - A pure-Python package (`cffi` in ABI mode, or `ctypes`) that loads the prebuilt library.
   - Nothing is compiled on the device; the library is cross-built for aarch64.
   - The package uses `DBM_FLAG_HOST_TICK` and ticks from its event loop.
3. **Java.**
   - FFM (Java 22+) with `jextract` from the header, or a hand-written wrapper.
   - JNA as a fallback for older JDKs.
   - A single wrapper class, with no JNI C code.

## Testing

- Rust tests call the C ABI through `extern "C"` callbacks. They check create/destroy, several
  instances side by side, host ticking, and string ownership.
- A C test compiles against the generated header (`tests/c/`) to catch header problems.
- These are in place: `tests/ffi.rs`, 7 tests including the C and C++ compile check.
- Each language wrapper gets a smoke test against a recording backend implemented in that
  language.
