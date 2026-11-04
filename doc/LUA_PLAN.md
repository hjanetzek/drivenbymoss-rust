# Controllers in Lua: analysis and plan

Status: 2026-11-04, plan; nothing implemented. Decision: option 2, framework bindings for
controllers written in Lua. Lua is the one dependency allowed for it (decided by the project
owner, 2026-11-04).

**Goal.** Write new controllers, or variants of existing ones, in Lua on top of the Rust
framework, with no Rust compiler for whoever writes them. A controller editor can then produce a
Lua script plus a surface definition, and the library loads it at runtime.

## 1. Analysis

### What there is (Rust lines, 2026-11-04)

| Part | Lines |
| --- | --- |
| Framework (`src/framework`: views, modes, commands, surface, display, graphics) | 17,900 |
| DAW model (`src/daw`) | 13,600 |
| Config, scales, parser, app, FFI, MIDI | 12,100 |
| **Shared by all controllers** | **≈ 43,600** |
| Controllers: Akai 11.3k, Maschine 10.7k, Flexi 8.2k, Launchpad 6.9k, Mackie 5.8k, Launch Control XL 2.8k | ≈ 45,900 |
| Total | 90,200 |

### Option 1: a full Lua port (not chosen)

- **Scope:** port all 90k lines; Lua is terser, so a guess of 60–75k lines of Lua. That is the
  order of the Rust port itself, weeks of agent work.
- **Hosting:** a small shim embeds Lua and exports the same `dbm_*` C ABI. Then the Reaper and
  Bitwig extensions, the Python binding and the in-process conformance runner work unchanged, and
  the 195 scenarios check the port from the start.
- **What changes in testing:** the Rust replay test (`tests/conformance_replay.rs`) uses the Rust
  `App` directly and would need a variant through the C ABI.
- **Cost:** two implementations to maintain, or a decision to drop one. Speed is fine for 30 ms
  ticks and pad grids; graphics displays would be slower.
- **When it pays off:** only for a runtime without a native library. Otherwise option 2 gives the
  same hackability.

### Option 2: framework bindings (chosen)

- The framework (43.6k lines, tested by the conformance runs) stays one implementation in Rust.
  Only the controllers, where the per-device variety is, can be written in Lua.
- **What a controller uses is bounded:**
  - The context (`Ctx`): the Launch Control XL calls 24 distinct methods, all controllers
    together 63.
  - The `View` and `Mode` traits: 74 methods (`src/framework/feature.rs`), most with defaults.
  - The surface: buttons, lights, continuous controls, the pad grid, about 50 functions.
  - Reads of the model (tracks, channels, devices, clips) and the generic commands and modes.

  Roughly 400–600 functions in all, many of them plain getters.

### Testing tools that apply to both

The tests work on the protocol, not on the language behind it:

- The Python reference host runs the four setups' 195 scenarios in process, against the Java
  results on Bitwig (`bindings/python/conformance/`, PYTHON_HOST_PLAN.md).
- The replays of the recorded Bitwig sessions run in `cargo test`.
- A Lua controller with the same behaviour as a Rust one must give the same results. That is
  the oracle for the pilot (§4).

## 2. The Lua engine

| Engine | Kind | Integration | Maturity |
| --- | --- | --- | --- |
| **mlua** with `lua54` + `vendored` | Rust bindings to PUC-Rio Lua 5.4, which `lua-src` builds from C source with `cc` | Cargo only; a safe API (`UserData`, `Function`, `Table`, error conversion, `scope`), sandboxing by not loading libraries | Mature, widely used |
| piccolo | A Lua VM in pure Rust (no C compiler) | An arena-based API (gc-arena), unlike any other; partial standard library | Experimental, its README warns of frequent API breaks before 1.0 |
| Vendored Lua C source + own FFI | Lua 5.4 with hand-written `extern "C"` declarations | No crates, but `build.rs` drives the C compiler itself, and all of the binding is unsafe code we write | Lua is mature; the binding would be new |

**Choice: mlua (`lua54`, `vendored`).** It is the easiest to integrate. It needs a C compiler at
build time, which every target already has:

- The Reaper build uses CMake and a C++ compiler.
- The Bitwig dev build runs `cargo build` on Linux.
- Zynthian is cross-built for aarch64 with a cross C compiler.

The binding lives behind one module (`src/lua/`), so the engine can be swapped later, for
example for piccolo once it is stable. Lua code never sees mlua types.

- **Cargo feature `lua`, on by default.** A build without it has no new dependencies and no Lua
  controllers.
- **Size:** about 300 KB of Lua plus the binding code (to be measured).

## 3. Design

### Loading

- A `LuaControllerFactory` implements `ControllerFactory` for each script found at startup. It
  looks in `<ini folder>/DrivenByMoss/controllers/*.lua` and in the scripts built into the library
  (`include_str!`, for the pilot and the examples).
- The script returns a table:
  - `info`: vendor, model, UUID, Java-style class name for `CONTROLLER_INSTANCE`.
  - `discovery`: the MIDI port name pairs per OS.
  - `settings`: which generic settings to activate.
  - `create(ctx)`, `startup(ctx)`, `handle_changes(ctx)`, `flush(ctx)`, `exit(ctx)`.
- `dbm_list_controllers` lists the Lua controllers too. The Bitwig extension and the Python
  binding then find them unchanged.
- Each controller instance gets its own `mlua::Lua` state: instances share nothing, and an error
  in one script cannot reach another instance.

### What Lua sees

Lua sees handles, never references. As with `ParamRef`, every call resolves its target through
the context when it runs:

- `ctx` is a userdata valid only during the call it is passed to. Any other access raises a Lua
  error, never undefined behaviour; `mlua::Lua::scope` gives this.
- **The model:** `ctx.model:track(i):volume()` and `:set_volume(v)`, `ctx.model:cursor_device()`,
  … Each returns plain values or a small handle (bank + index) that is resolved again on the next
  call.
- **The surface:**
  - `ctx.surface:button(id, {midi = …, command = fn, light = fn})`.
  - `ctx.surface:continuous(id, {…})`, `:grid()`, `:send_sysex(bytes)`, `:send_midi(…)`.
  - The IDs are the existing `ButtonId`/`ContinuousId` names, as strings or constants.
- **Views and modes:**
  - Register built-in ones by name with options: `ctx:register_mode("Volume",
    "TrackVolumeMode", {controls = …})`.
  - Or Lua tables with any of the trait methods (`on_activate`, `on_knob_value`, `button_color`,
    `draw_grid`, …).
  - The adapters `LuaView`/`LuaMode` implement `View`/`Mode` and forward each method to the
    table, falling back to the trait default.
  - The rule that a view or mode is borrowed while its methods run stays. A deferred action
    goes through `ctx:schedule(ms, fn)`.
- **Commands:** the generic commands by name (`ctx:command("Play")`) or Lua functions.
- **Bases with hooks** (`SessionViewBase` + `SessionHooks`, `ParameterModeBase`, the
  sequencers): exposed as a built-in view whose hooks a Lua table overrides. This comes after
  the pilot, as the first non-pilot controller needs it.

### Errors and safety

- Only the safe standard libraries are loaded: no `io`, `os` or `debug`, and `package` is
  limited to the controller folder.
- A Lua error is logged with the script name and line through `Host::println`. The call it
  happened in has no effect, and three errors in a row from the same function deactivate it, so
  a broken script cannot flood the log.
- No Lua call can panic across the C ABI. The FFI already catches panics.
- A memory limit per instance (`Lua::set_memory_limit`), and an instruction-count hook against
  endless loops.

### Generated bindings

Most model getters and setters are mechanical. A small table in `src/lua/model.rs` (a macro, no
code generator) lists name → Rust accessor, and the Lua API reference is generated from the same
table into `doc/LUA_API.md`.

## 4. Pilot: the Launch Control XL in Lua

1. **Engine and loader:**
   - The `lua` feature and `src/lua/`: the factory, the per-instance state, error handling.
   - A test script that registers one button and lights it.
2. **The bindings the Launch Control XL needs:**
   - The 24 context methods.
   - Its modes, built in: track mute/solo/arm/select, volume, pan, send, device, the
     sequencers through their built-in views.
   - Its surface: buttons, knobs, faders, the template sysex.
3. **`controllers/launch_control_xl.lua`**, a port of `src/controller/launchcontrol/` (2.8k Rust
   lines):
   - Registered as "Launch Control XL (Lua)" with its own UUID.
   - It uses the same MIDI discovery, so a configuration picks one or the other.
4. **The oracle.**
   - The Python reference host runs the 27 Launch Control XL scenarios with the Lua controller in
     place of the Rust one: a runner option `--controller-id` with the same profile and
     references. It must give the same results.
   - Also: the Rust LCXL tests (`tests/launchcontrol_xl.rs`) against the Lua controller, where
     they do not test internals.
5. **Report:** how many bindings were needed, what was awkward, size and speed (tick time of the
   Rust and the Lua controller on the same scenario), and the API changes the next controller
   should get.

Estimate: steps 1–2 two days, step 3 one to two days, step 4 half a day.

## 5. After the pilot

1. **The bases with hooks** (session, play, drum and sequencer views, parameter modes), so that
   pad controllers can be written in Lua.
2. **A first new controller in Lua** that is not ported yet: from PORTING_GUIDE.md §1, e.g. the
   ESI Xjam or the Arturia Beatstep (pure MIDI, small).
3. **`doc/LUA_API.md`**, generated, and a short guide for writing a Lua controller.
4. **The controller editor:** a surface definition plus Lua, decided after the pilot.

## 6. Risks

- **The binding surface grows with each controller.** Bind what a controller needs, generate
  the mechanical part, and review the API after the pilot before binding more.
- **Re-entrancy.** A Lua callback that calls back into the view or mode it runs in hits the
  existing borrow rule. The adapters must return a clear Lua error instead of panicking or
  silently skipping.
- **Speed of pad grids in Lua** (64–128 pads per flush). To be measured in the pilot. If it is
  too slow, the built-in views draw the grid and Lua only supplies colors per pad.
- **Build:** the vendored C build has to work in the Reaper CMake build on Windows and macOS and
  in the Zynthian cross build. Check this early, in step 1, with `ci/linux.sh` and one cross
  build to aarch64.
- **Two implementations of the Launch Control XL** after the pilot. Keep the Rust one; the Lua
  one stays as the example and test of the bindings.
