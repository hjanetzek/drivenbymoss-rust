# Several surfaces per controller instance

Status: 2026-11-02, implemented (used by the Mackie MCU and HUI with extenders).

## The Java design

`AbstractControllerSetup` keeps a list of surfaces (`this.surfaces`, `getSurface ()` is the
first, `getSurface (index)` any). Each surface has its own MIDI input and output, buttons,
lights, continuous controls, text displays, view manager and mode manager. Views, modes and
commands are created per surface and keep a reference to "their" surface; the model and the
configuration are shared. The MCU/HUI setups loop over the surfaces in `createModes`,
`registerTriggerCommands`, `flush`, `startup`, and so on.

Three cross-surface features:

- `MCUControlSurface.isPressed` / `HUIControlSurface.isPressed` check the buttons of all
  surfaces, so that a mode on an extender sees Shift, Select, Control of the main device.
- `FeatureGroupManager.addConnectedManagerListener`: the mode managers of all surfaces follow a
  mode change of one of them.
- The extender offset (8 tracks per surface) is a property of the surface.

The Reaper instance (`MCU1..4ControllerInstance`, `HUI1..3ControllerInstance`) declares
`numMidiInPorts = numMidiOutPorts = number of surfaces`; `MidiAccessImpl.createInput (i)` and
`createOutput (i)` open port pair `i`, configured in the controller's `.config` file as the
indexed MIDI input/output names.

## Decision: several surfaces in one `Ctx`, swapped in

The two options were (a) one `Ctx` per surface with a shared model, or (b) several surfaces in
`Ctx` with a surface index in the call paths. (a) needs the model and the configuration behind
shared references (`Rc<RefCell<..>>`), which changes every call path of the existing
controllers. (b) as "an index in every call path" also touches all views, modes and commands.

Chosen: a variant of (b) which changes no existing call path. The fields of `Ctx` hold the
**current** surface: `surface`, `views`, `modes`, `extra_modes`, `text_display`, `ext`,
`midi_handler`, `sysex_handler` (and the notification timer). The other surfaces are parked
in `ctx.surfaces` (`framework::multi_surface::SurfaceSet`). `ctx.select_surface(i)` swaps the
parts of surface `i` in (`std::mem::swap` of the fields, no copies);
`ctx.with_surface(i, |ctx| ...)` runs code with surface `i` and swaps back. Existing code keeps
working on "the" surface, a controller with one surface never swaps anything, and the Java
per-surface objects map one to one: a mode registered while surface `i` is current is in the
mode manager of surface `i`, a button command executes while its surface is current.

Rules:

- Surface 0 is the resting state: the app selects the surface of the receiving MIDI input
  around `handle_midi`/`handle_sysex`, everything else (`tick`, `handle_changes`, `flush`,
  parser events, tests via `instance_ctx`) sees surface 0 unless it selects another.
- `ctx.schedule` remembers the current surface; the task runs with it (button long press, the
  notification timer, delayed displays).
- Shared: host, sender, model, configuration, color manager, scheduler.
- Controller state per surface goes into `ctx.ext` (swapped with the surface), state shared by
  all surfaces into the setup struct or into an `Rc` held by all surface states.

## Changes

### `src/framework/multi_surface.rs` (new)

`SurfaceParts`, `SurfaceSet` and on `Ctx`: `add_surface`, `surface_count`, `surface_index`,
`select_surface`, `with_surface`, `for_each_surface`, `parked_surface(_mut)`,
`share_button_states` (Java `isPressed` over all surfaces), `connect_surface_mode_managers`
(Java connected mode managers, via a change listener on each main mode manager),
`set_previous_mode_on_all_surfaces`.

### `src/framework/ctx.rs`

- New field `pub surfaces: SurfaceSet` (initialized in `Ctx::new`).
- `notification_end` is `pub(crate)` (swapped with the surface).
- `schedule`: with several surfaces the task is wrapped in `with_surface(current, ..)`.

### `src/framework/surface.rs`

- New field `pressed_on_other_surfaces` (empty unless `share_button_states` is enabled; set when
  a surface is selected; the parked surfaces cannot change while another one is current, so
  the snapshot is exact).
- `is_pressed` also returns true for a button in that set.

### `src/controller/mod.rs`

`ControllerFactory` got two methods with defaults: `num_port_pairs()` (1) and
`create_with_ports(host, sender, &[MidiPorts], configuration)` (calls `create` with the first
pair). The MCU/HUI factories return the number of surfaces and create one surface per pair.

### `src/app.rs`

- `Running.ports` is a `Vec<MidiPorts>` (one pair per surface).
- `resolve_ports` resolves `num_port_pairs()` pairs: the indexed MIDI input/output names of
  the controller configuration file (all must be configured and present), or a discovery pair
  with that many names (all must be present). One pair behaves exactly as before. In a single
  controller host (Bitwig) the ports are `0..n`.
- `start_instance`/`stop_instance` open and close all ports; `create_with_ports` instead of
  `create`.
- `on_midi_event` finds the surface by the input port and handles the message in
  `with_surface`.
- The parser event `RebindKnobs` re-activates the active mode of every surface.

### C ABI and Reaper backend

No change. The backend calls are already per device ID (`open_midi_input (id)`,
`send_midi_data (id, ..)`, `on_midi_event (id, ..)`), and the Reaper extension keeps sets of
open inputs and outputs (`activeMidiInputs`/`activeMidiOutputs` in `dllmain.cpp`).

### Bitwig extension

- `DbmExtension` opens `getNumMidiInPorts ()` inputs and `getNumMidiOutPorts ()` outputs;
  the device ID is the port index. Note inputs are kept per port.
- The protocol recorder writes the MIDI of port `n > 0` as `in<n>` / `out<n>` (and `raw<n>`,
  `filters<n>`, `keys<n>`, `velocities<n>`); port 0 keeps its kinds, so the existing
  recordings are unchanged. `tests/conformance_replay.rs` replays `in<n>` and compares
  `out<n>`.
- The definitions with their port counts come from the catalog (`tools/gen_definitions.py`).

## Not done / open

- The connected mode managers sync the siblings with `set_active_mode` also for a temporary
  mode and a restore (Java: `restore` on the siblings); the active mode is the same, only the
  previous mode of a sibling may differ.
- The parser events other than `RebindKnobs` (note repeat, note mapping) only reach surface 0;
  no multi-surface controller uses them.
