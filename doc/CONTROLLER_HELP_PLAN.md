# Controller help in the simulators

Status: phase 1 and phase 2 done for the Maschine JAM (see §6); round 2: the JAM's texts as one
declarative table (§9, with a comparison of the two approaches), boxes coloured by scope, one box
per row of buttons, the global boxes hideable (§10). Phase 3, the mirror (§4): done for the devices
the core drives in-process (the JAM and the Mk2). Phase 4 started: the Maschine Mk2's table and its
layout from a photo (§12).

## 1. Goal

The controller simulators mirror the hardware and serve as a context reference while one learns
a controller. With help switched on, every button gets a help box around the panel which says
what the button does **in the current state**: the active view and mode, the held (or latched)
Shift, Select and the other modifiers. Each box sits near its button and has a leader line to
it.

- The text of a box has at most 2 lines.
- Clicking a box, or pressing its button (in the simulator, or on the hardware when the simulator
  mirrors it), expands that box to the full text. One box is expanded at a time.
- First target: the Maschine JAM.

## 2. Design: the core describes its commands

The texts come from the core, next to the code they describe, so they follow the state the code
sees (the user chose this over texts in the simulator, design "A").

### 2.1 Core

- `src/framework/help.rs`: `Help { short, long }` (short: ≤ 2 lines, about 60-80 characters; long:
  optional, the full text, typically all combinations of the control) and the collection.
- `TriggerCommand::help(&self, ctx: &Ctx) -> Option<Help>` (default `None`) describes what
  pressing the button would do now. `ContinuousCommand::help` does the same for a knob (the JAM's
  encoder).
- Commands which hand the button to the active view or mode forward it: `View::button_help(ctx,
  button)`, `Mode::button_help(ctx, button)`; the pads are one entry, `View::grid_help(ctx)`, and
  the knobs or strips of the active mode are one entry, `Mode::controls_help(ctx)` (JAM: the
  touch strips). Closures get a text with `framework::help::WithHelp` (a command and a function
  for its help).
- `help::collect(ctx)` collects the entries of all buttons and continuous controls of the main
  surface (`HelpEntry`: id = the `ButtonId`/`ContinuousId` name, the label, the MIDI binding,
  whether the button is pressed, short and long text), plus `grid` (the active view) and the
  mode's entry (`Ctx::help_controls_id`, default `knobs`, the JAM: `strips`).
- `App::take_control_help` keeps the last line of every entry and returns only the changed ones.

### 2.2 C ABI and Python

- `char *dbm_take_control_help(DbmApp *app)`: the entries which changed since the last call, one
  JSON object per line (JSON Lines), of the first running controller (as the settings
  functions). Release with `dbm_free_string`. Fields: `id`, `label`, `bind` (`cc`, `note`,
  `pitchbend`, `program` or `null`), `channel`, `control` (-1 without a binding), `pressed`,
  `presses` (the number of presses so far: a press released between two calls still shows),
  `short`, `long`. An entry whose help went away has empty `short` and `long`; an entry seen
  for the first time is only returned with a help. `dbm_reset_control_help(app)` forgets the
  cache: the next call returns all entries (a simulator which connected again).
- Python: `App.take_control_help() -> list[dict]`, `App.take_control_help_lines() -> list[str]`
  (the JSON lines as they are, for the host's socket) and `App.reset_control_help()`
  (`bindings/python/drivenbymoss/__init__.py`, `_ffi.py`; optional symbols, an older library
  gives empty lists).

### 2.3 Transport

`support/push_simulator/tools/dbm_host_alsa.py` (also run as
`support/controller_simulator/tools/dbm_host_alsa.py`) sends the changes to the simulator over a
UNIX stream socket, the pattern of the Push display socket: the simulator listens, the host
connects lazily and reconnects; after a connect it resets the cache, so the simulator gets all
entries. The path: `--help-socket PATH`, else `$DBM_SIM_HELP_SOCKET`; without either no help is
sent. The simulator listens on `--help-socket` or `$DBM_SIM_HELP_SOCKET`, default
`$XDG_RUNTIME_DIR/dbm-sim-help.sock`. One JSON object per line, as `dbm_take_control_help` returns
them.

The simulator maps an entry to its control by the MIDI binding (the template id of the control →
the template's CC, `maschine_simulator/templates.py`). The Shift of the JAM and the Mk2 has no MIDI
binding (NI's or DrivenByMoss' SysEx), it is matched by its id `Shift`; `grid` goes to the pads,
`strips` to the JAM's touch strips and `knobs` to the Mk2's 8 knobs, the encoder's entry
`MasterKnob` to the encoder (JAM CC 57, Mk2 CC 7).

### 2.4 Drawing

QML in `sim_common/qml/SimCommon` (`HelpLayer.qml`, `HelpBox.qml`), so every simulator can use
it:

- **Toggle:** key H and a checkbox in the status bar. The window then gets a margin around the
  panel for the boxes.
- **A box:** a rectangle with the text (2 lines, elided), a leader line (a `Canvas`,
  `HelpLines.qml`) from the box edge to the control's centre; the lines are drawn inside the
  panel under the controls, so a line which passes a button is hidden by it instead of crossing
  its label. Hovering highlights the box and the control.
- **Placement:** each control goes to the side of the central area (the pads) it is on: left
  column → left, right column → right, above → top, below → bottom; a control can name its side
  (`Control.help_side`: the JAM's Song → top; the pads' box → top, the strips' → bottom). Along a
  side the boxes keep the order of their controls; their positions are the least squares fit to
  the controls' positions at a minimum distance ("pool adjacent violators", `spread` in
  `sim_common/help.py` and the same in QML): a crowded stretch spreads up and down evenly, so the
  lines stay short and don't cross.
- **Rows:** top and bottom have two staggered rows. All boxes of a side are spread together at
  half a box apart and alternate between the rows, so an outer box's middle lies above the gap
  between its inner neighbours; its line goes straight through that gap (an elbow) and then to the
  control.
- **Expanded:** the box stays in its place, highlighted (white frame and line); the full text is
  shown in one fixed place, a panel centred over the pads (`HelpDetail.qml`) with the name and the
  scope's colour. It doesn't hide the control it describes: a control in the top part of the pads
  moves it to their bottom edge, the pads' own box puts it above them. A click on the panel or on
  the box again closes it. (Round 1 grew the box in place, over the buttons.)
- **Tight to the panel:** the columns and rows keep a fixed gap of a few millimetres from the
  panel; a wider window leaves its free space outside the boxes, so the leader lines stay short.
- A control without help in the current state gets no box.
- Boxes update live (the host's changes come in on every tick).

## 3. Texts

Sources: the JAM manual (DrivenByMoss-Documentation), the Java original, and the port's own code,
including its additions (the host page strips on CONTROL's second slot, Shift + CONTROL to the
first remote page, Select + CONTROL / Select + SONG for the page surface, doc/HOST_PAGE_PLAN.md
§5.1, doc/SURFACE_LINK.md). **The code wins** where they differ; the differences are in §7.

## 4. Mirror

The simulator as a mirror of a real device which the core drives: its LEDs, strips, level meters
and displays, and the controls highlighted (their boxes expanded) on hardware presses.

### 4.1 The options (analysed before the implementation)

- **Presses** come with the help entries in every case: `pressed` is part of an entry, so the
  simulator expands the box of a button which became pressed, whatever the transport.
- **ALSA (dbm-nihost, VirMIDI):** the simulator subscribes passively. The device's input
  (presses) is dbm-nihost's readable port `Maschine Jam - 1 Input`; the DAW's output (LEDs) is not
  readable at dbm-nihost's write port, but at the DAW's own source port (dbm_host's client, the
  VirMIDI client which Bitwig writes to). Needs a read-only simulator (no hello, no answers, no
  sends) with two subscriptions found by the user (`--mirror-from`). Not implemented: only the
  daemon setups need it.
- **In-process HID (libdbm_nihost, `src/nihost`):** there is no ALSA port. The core has the
  template MIDI in both directions (`DeviceIo::send` / `read`), the stream the simulator speaks:
  a tap there, sent over the help socket. **Chosen** (verified: every message to the device goes
  through `DeviceBackend::send_midi_data`, every message of the device through
  `DeviceLink::take_input`, for any controller with a `hid_device`; the help socket already
  connects the host and the simulator).

### 4.2 Core: the tap (`nihost::MirrorTap`)

- `DeviceBackend::send_midi_data` taps the messages to the device ("out"), `DeviceLink::take_input`
  the messages of the device ("in"; not the hello and "gone").
- **No cost without a consumer:** inactive, a push is one flag check; nothing is queued.
- **Bounded:** at most 4096 messages between two takes. More (a host which stopped taking) drop
  the queue and stop queueing; the next take refreshes the device's state instead, so the mirror
  is right again without unbounded memory.
- **Initial state:** starting the mirror calls the controller's refresh path
  (`ControllerSetup::refresh_device`, also used when an in-process device comes back): the next
  flush sends the complete state, which the tap queues. The default forces all lights and the
  text display; the JAM also forces its touch strips (setup, values, dual values: sent on change
  only), the Mk2 its button labels. The device gets the same messages, but the library only
  writes what changed, so the hardware sees nothing. No per-key cache is needed (and none costs
  memory or time while nobody mirrors).
- C ABI (`src/ffi.rs`, `include/drivenbymoss.h`):
  - `int dbm_mirror_device_midi(DbmApp *app, int on)`: start / stop the tap of the first instance
    which drives a device in-process; starting refreshes its state. 1 if there is such a device.
  - `char *dbm_take_device_midi(DbmApp *app)`: the queued messages, the oldest first, as JSON
    lines `{"midi":"out","data":"B0083F"}` / `{"midi":"in","data":"903664"}` (release with
    `dbm_free_string`).
  - `int dbm_inject_device_midi(DbmApp *app, const uint8_t *data, int length)`: a message as if
    the device sent it (a 3-byte channel message or a complete SysEx; not the hello or "gone").
    The notes go to the host's note input as the device's. Not mirrored back. 1 if taken.
- Python: `App.mirror_device_midi(on) -> bool`, `App.take_device_midi_lines() -> list[str]`,
  `App.take_device_midi() -> list[(direction, bytes)]`, `App.inject_device_midi(data) -> bool`
  (optional symbols).

### 4.3 Socket

The help socket carries the mirror too (`dbm_host_alsa.py` → `sim_common`):

- On every (re)connect the host resets the help cache and, if the core drives the device
  in-process, starts the mirror: it writes `{"mirror":"Maschine JAM","source":"hardware",
  "input":true}`, the core refreshes, and the complete state follows with the next tick. When the
  simulator goes away (a failed write, or the peer closed: the host now reads the socket too) the
  host stops the mirror, so the core queues nothing.
- `{"midi":"out"}` lines go to the simulator's device model exactly as the MIDI of its ALSA port
  (`device.receive`): the LEDs, the JAM's strips and meters, the Mk2's displays and button labels
  are decoded by the existing code.
- `{"midi":"in"}` lines go to `device.mirror_input`, which only names the controls (a press /
  touch held until its release; an encoder turn or strip move flashes for 0.2 s): drawn with a
  white frame (`sim.controlPressed(key)`), their help box expanded (`HelpModel.expand_control`:
  the control's entry, its row or the area it is in). They never go anywhere (no LED changes,
  nothing sent). The Shift SysEx lights the Shift LED as the host does.
- **The simulator's own input while mirroring:** injected. Its MIDI (clicks, pads, strips) goes
  back over the socket as `{"midi":"in"}` and the host plays it in with `dbm_inject_device_midi`:
  the simulator is a second surface of the same instance, which is useful while learning (try a
  combination with the mouse, see the device answer). It is safe: only complete messages are
  taken, the handshake is filtered (in the core) and a mirroring simulator answers no query; the
  LEDs it causes come back through the mirror like the hardware's. `--no-mirror-input` on the
  simulator (`run_with_core.sh --no-mirror-input`) drops them instead; the host's
  `--no-mirror-input` ignores them. While mirroring, nothing goes to the simulator's ALSA port.
- **Latency:** the "out" messages are produced by the tick's flush (every 30 ms, as for the
  hardware); the host forwards them right after the tick. The "in" messages and the simulator's
  input are forwarded after each `poll_devices` (every 3 ms, the existing poll of the in-process
  device; no extra loop). The host's writes never block: they are buffered, a simulator which
  stops reading (1 MB pending) is dropped and connected again later (with the complete state).
  The simulator draws on its 33 ms timer. Meters (sent with every flush) and strips move smoothly.
- **Status bar:** "Mirror: Maschine JAM (hardware), clicks act on the core" (or "clicks ignored").
- `run_with_core.sh --hw` (the default while the model's device is plugged in): the simulator
  without ALSA ports (`--no-alsa --detached`) mirrors the device over the help socket.

### 4.4 Controllers

Generic for any controller the core drives in-process (`ControllerFactory::hid_device`): today
the Maschine JAM and Mk2 over libdbm_nihost. The simulator side works for every device model
with `receive` (all simulators) and `mirror_input` (the Maschine simulators); a controller added
to the in-process transport needs a `refresh_device` override only if it sends some state on
change only outside the surface's lights and text display.

## 5. Phases

1. The framework end to end with the JAM's global buttons: core trait and collection, C ABI,
   Python binding, socket, simulator mapping and drawing.
2. The views and modes: Play, Chords, Piano, Drum 64, Drum, Drum 4/8, the sequencers, Session,
   the Shift view, note repeat, Tempo/Shuffle, the browser, the accent view, the strip modes,
   the page surface and the host page strips.
3. The mirror (§4): done (in-process devices).
4. The other simulators (Mk2, Mk3, the MIDI simulators, the Push): their commands' texts; the QML
   is shared already. The Mk2: done (§12).

## 6. State

See the end of the work's report and the commits; kept up to date here:

- Phase 1: done: the core (`src/framework/help.rs`, the traits), the C ABI and the Python
  binding, the socket (`dbm_host_alsa.py` → `sim_common/help.py`), the mapping (Maschine
  simulators by the template's MIDI) and the drawing (`HelpLayer.qml`, `HelpBox.qml`).
- The JAM's global buttons (all of them; `src/controller/maschine/jam/help.rs`): Shift, Select,
  Clear, Duplicate, Play, Rec, Page ◂▸, Solo, Mute, Note Repeat, Macro, Level, Aux, Control, Auto,
  the group buttons A-H, the scenes 1-8, Song, Step, Pad Mode, the arrows, Browse, MST / GRP / IN 1
  / HEADPHONE, the encoder (turn, push), Tempo, Swing, Grid, Perform / Notes / Lock / Tune, each
  with the state's text (Shift, Select, the other held buttons, the view, the mode, the page
  surface and the host page strips) and a long text with all combinations.
- Phase 2: the pads of Play, Piano, Chords, Drum 64, Drum, Drum 4/8, Sequencer, Poly Sequencer,
  Raindrops, Session (also the held modifiers and the birds-eye view), the Shift view, the note
  repeat configuration, Tempo / Shuffle, the browser, the accent view and the page surface; the
  arrows of every view; the strips of Volume, Pan, Send 1-8, Macro (track), the device
  parameters, the user (project / track) parameters, the host page strips and the page surface;
  PAGE LEFT/RIGHT of every mode.
- Tests: `tests/controller_help.rs` (the core: changes only, the modifiers, every view and mode
  has a text of at most 90 characters and one line), `bindings/python/tests/test_binding.py`
  (`test_control_help`), `support/controller_simulator/tests/test_help.py` (layout, model, socket,
  the real core's entries all map to JAM controls in the states every button makes, the boxes
  drawn offscreen, the host's writer).
- Phase 3, the mirror (§4): the tap in the core (`src/nihost/mod.rs` `MirrorTap`,
  `ControllerSetup::refresh_device`), the C ABI and the binding, the host (`Mirror`, the
  non-blocking `HelpWriter`), the simulator (`SimulatorBase.mirror_midi` / `mirror_output`,
  `MaschineDevice.mirror_input`, `HelpModel.expand_control`, the white frames in the QML),
  `run_with_core.sh --hw`. Tests: `src/nihost/mod.rs` (the tap, the link, the complete state for a
  mirror of the JAM and the Mk2, injection, the bound), `tests/ffi.rs`,
  `bindings/python/tests/test_binding.py` (`test_device_mirror`),
  `support/controller_simulator/tests/test_mirror.py` (the real core's stream in a mirroring JAM
  and Mk2 simulator equals a simulator on the port, the presses, the simulator's input, a
  reconnect over the real socket, a simulator which doesn't read).
- Round 5 (both controllers): the expanded text in a fixed panel over the pads, the columns and
  rows tight to the panel (§2.4); the Mk2's upper transport row moved to the left column (the
  bottom row had 9 boxes fanning out).
- Phase 4, the Mk2 (§12): its table (`src/controller/maschine/mk/help.rs`), the layout from a photo,
  the boxes' sides and the uncrossing of lines. Tests: `tests/controller_help.rs` (the Mk2's
  first call, modifiers with the emulated and the real Shift, views and modes, coverage),
  `support/controller_simulator/tests/test_help.py` (`UncrossTest`, `Mk2MappingTest`,
  `Mk2CoreHelpTest`).
- Open: §8.

## 7. Manual and code

The differences found while writing the texts (the texts follow the code):

- **GRID + encoder** (manual: moves the play position): not implemented, neither in Java nor in the
  port; GRP selects the play position for the encoder.
- **SHIFT + Scene** (manual: a new scene from the playing clips): launches the scene with the
  alternative launch (Java and the port; `StartSceneCommand`'s own Shift function is overridden).
- **Note repeat configuration, bottom row** (manual: the octave range): the port's settings have no
  note repeat octave (`SUPPORTS_OCTAVES = false`), the row is off.
- **SELECT + CONTROL** (manual: project/track parameters): with the host's pages it toggles the page
  surface (as Select + SONG); without them as the manual.
- **CONTROL** cycles device parameters ↔ host page strips with the host's pages (manual: device ↔
  project/track parameters); **SHIFT + CONTROL** goes to the first remote control page with the
  host's pages (manual: the device window).
- **SELECT + SONG**, the arrows and PAGE LEFT/RIGHT on the page surface / host page strips: port
  additions.
- **SHIFT + PLAY** with SELECT held: punch out (manual: SELECT+SHIFT+PLAY, same).
- **Piano: left/right** change the scale layout as in the Play view, which the piano doesn't use.
- **DUPLICATE + scene** (manual: duplicate the scene): the JAM's scene buttons don't look at
  Duplicate (`StartSceneCommand` handles Clear only); the scene is launched.
- **Session arrows** (manual: blocks of 8, Shift: 1): they scroll by 1 track / scene; with the
  Shift view (a tap of Shift, or Shift held with the setting) by 8, as Java (Bitwig's
  `scrollForwards` is one step).
- **SHIFT + MACRO / STEP / PAD MODE / PERFORM / NOTES / LOCK / TUNE** do nothing (no box while Shift
  is held).
- **Shift view**: the manual names the combinations "Stop+Pad n"; the JAM has no Stop: tap Shift
  (the view stays open), or hold it while the setting "Show the Shift view while Shift is held"
  is on.

## 8. Open points

- The mirror of the daemon setups (dbm-nihost over ALSA, §4.1): not implemented.
- The other simulators (Mk3, Mikro Mk3, the MIDI simulators, the Push): the QML and the socket are
  shared; they need their commands' texts and a resolver (`enable_help`). The Mk3 and the Mikro Mk3
  share the Mk2's setup: their tables can start from the Mk2's (their own buttons instead of the
  `none` rows; the Mk3's touch strip, PAGE ◂▸, the 4-D encoder) and their layouts from photos.
- The texts of the sequencers' finer grid functions (the base views' combinations: loop, page
  copy, velocity with Lock) are summaries, not state dependent per pad.
- The Footswitches have no text (the simulator has no footswitch).
- A box for a control without help in the current state is hidden, not dimmed.

## 9. Round 2: the texts as a table (`help!`)

The JAM's texts are now one declarative table, `src/controller/maschine/jam/help.rs`, written
with `crate::help!` (`src/framework/help_table.rs`, `macro_rules!` only):

```rust
crate::help! {
    global {
        Play                  => |ctx| play_button(ctx), long PLAY_LONG,
        Shift + Play          => "Restart playback from the beginning",
        Select + Shift + Play => |ctx| format!("Punch out: turn it {}", ...),
        Shift + Track         => none,                       // explicitly no help
        Volume                => command, long LEVEL_LONG,   // the escape hatch: the command's help
        Solo + RowSelect1 .. RowSelect8 => "Solo / unsolo the track",   // one box for the row
        Footswitch1 | Footswitch2 => none,
    }
    view Play | Piano { Delete + grid => "...", grid => |ctx| grid_play(ctx), ArrowUp => |ctx| ... }
    mode HostStrips { strips => "..." long STRIPS_LONG, Select + PageLeft => "...", PageLeft => "..." }
}
```

- Keys: `ButtonId` / `ContinuousId` names (looked up by a `const fn` while the `static` is built:
  a typo stops the build), `grid`, `strips`, `A | B`, `First .. Last` (a row of buttons, one box).
  Section names (`view Play | Piano`, `mode Send1 | ...`) are `Views::` / `Modes::` paths.
- Texts: a literal (`long ...` optional), `none`, `command`, or `|ctx| ...` / `|ctx, key| ...`
  returning `&str`, `String`, `Help`, an `Option` (None: the row doesn't apply, look further) or
  `NoHelp`.
- Resolution: rows with held modifiers (more modifiers first; mode, view, global sections), then
  the plain rows (mode, view, global). A text without a long text takes the key's first long text.
- `TriggerCommand::help`, `View::grid_help` / `button_help`, `Mode::controls_help` / `button_help`
  stay as the escape hatch. The JAM uses it for LEVEL, AUX and CONTROL only (`command`): which
  mode they select next is state of the command (`ModeMultiSelectCommand::current_mode_id`), not
  of the `Ctx`. All other per-command, per-view and per-mode help code of the JAM is gone.
- Coverage: `tests/controller_help.rs` `the_table_covers_every_button` checks that every button
  with a command (or its row) has a row in every view × mode of the JAM (an explicit `none` or
  `command` counts), and the pads and the strips too.
- The macro munches its rows one by one; the crate sets `#![recursion_limit = "512"]` (the JAM's
  global section has about 100 rows, two levels each).

### Comparison (the user decides which to keep)

| | Trait methods (round 1) | Table (round 2) |
| --- | --- | --- |
| JAM code | about 1,340 lines in 13 files (`help.rs` 1,006 + `help` in 22 commands, the view and mode hooks, wrappers in `setup.rs`) | 781 lines in one file (the table about 300, the long texts and the state functions the rest) + 3 escape hatch methods |
| Framework | the hooks (traits), `WithHelp`, `collect` | + `help_table.rs`, 526 lines with docs and tests (once, for all controllers) |
| Reading | a button's text is spread over its command, the view's and the mode's hook; modifiers are `if`s inside functions | one place; each combination is a row (`Shift + Play => ...`), the view/mode sections show what changes with them |
| What got easier | the scope (global / view / mode / modifier) follows from the section, the rows of buttons are one key, coverage is testable (`has_entry`), a new controller's texts are written without touching its commands | |
| What got harder | | state only a command has needs the escape hatch (`command`); the precedence is the table's rule (modifiers first, then mode, view, global), not the command's own order: where a command checks Select before Shift, the rows must be ordered so (and a two-modifier row added where both held differ); closures still hold the logic for texts which name state (track names, tempo) |
| Drift risk | low per text (next to the code it describes), but nothing checks that every state is described | higher in principle (the table is away from the commands: a changed command doesn't change its row), lowered by the coverage test and by the closures reading the same state the commands read; the precedence of modifiers is a second copy of the commands' `if` order |
| Build | normal | a macro error points at the macro; unknown names give a const evaluation error naming the macro's panic message |

## 10. Round 2: scopes, rows, hiding the globals

- **Scope** per entry (`scope` in the JSON line): `global` (independent of view and mode),
  `view` (a row of a `view` section: the pads, the arrows, ...), `mode` (a `mode` section: the
  strips, PAGE ◂▸), `modifier` (a row with modifiers: it exists because Shift / Select / another
  button is held). The escape hatch takes the scope of its `command` row.
- **The active view and mode**: the entries `@view` and `@mode` (their names in `short`); the
  simulator shows them in the legend.
- **Palette** (Okabe–Ito hues, told apart with the common colour vision deficiencies, on the dark
  background): global `#9aa0ab` (neutral grey), view `#56b4e9` (sky blue), mode `#e69f00`
  (orange), modifier `#cc79a7` (reddish purple). A box has a 4 px bar in its colour, a 55 % frame
  and a 10 % tint of the background; its line has the colour (75 %). Hover, press and the expanded
  box use white (it was yellow, too close to the orange). The status bar shows the legend: "View:
  Play", "Mode: Volume", "Shift / Select / held", "Global".
- **Rows of buttons**: a table key `First .. Last` describes a row with one text; the core sends
  one entry `First..Last` with `members` (their bindings) instead of 8 entries, when the row's
  text applies (a closure returning None gives the buttons their own entries back). The JAM: the
  scenes (plain, Shift, Select, Clear, Step, Pad Mode) and the group buttons (plain, Aux, Grid,
  Rec, Solo, Mute, Clear, Duplicate); the per-button details (track, scene, send names,
  resolutions) are in the long text. The simulator draws one box around the row with a bracket
  along it, its line from the bracket's middle; hovering highlights all buttons. The other
  candidate rows (the encoder buttons, Perform/Notes/Lock/Tune) have different texts per button
  and stay single, also with Shift and Select held.
- **Globals off**: key G and the "Globals" checkbox hide the `global` boxes; the others re-spread.
  `--hide-global` starts so.
- The scene texts no longer repeat the name ("Launch the scene Scene 1"): "Launch the scene", the
  names in the long text.

## 11. Round 3: the JAM's layout as the hardware, small fixes

- **Layout** (`maschine_simulator/layout.py`, `_jam()`): placed from a top view photo of the
  device (positions and sizes in the photo's pixels, 0.264 mm per pixel; the surface is now
  370 × 312 units, about mm). Upper left column Song, Step, Pad Mode, Clear, Duplicate, the d-pad,
  Note Repeat on the group row; the block left of the strips Macro, Level, Aux, Control, Auto; the
  block right of them Perform, Notes, Lock, Tune, Swing; the bottom row Shift, Play, Rec, ◂, ▸,
  Tempo, Grid, Solo, Mute, Select (Play … Mute in the columns of the pads); upper right the level
  meters, MST GRP, IN 1 and the headphones (BtnCue), the encoder with its push, Browse on the group
  row. The pads have the photo's proportions (wider than tall); the strips are four pad rows long.
  The template ids and the keys are unchanged (a check compared them with the old layout); NI's
  printed Shift labels are not copied.
- **Sides** follow from the positions: the left column and the lower left block → left, the right
  column and block → right, the bottom row → bottom, Song → top.
- **Areas** (the pads, the strips, the rows of buttons): the line goes to the panel's edge, along it
  to an anchor and straight in; the JAM puts the anchors into the gaps between the columns
  (`HelpModel.area_anchors`): the strips between C and D, the group row between E and F (through
  the gaps of the bottom row and the strips, so its bracket under the group row is reached
  without crossing a strip), the scenes between 3 and 4 (the pads' line passes between 4 and 5).
- **Texts:** the pads' and the strips' texts no longer repeat the box title ("Pads · Play C
  Major…", "Strips · The volume of 8 tracks"). The mode and view toggles say what a press does:
  "Press: the strips back to the device parameters", "Press: the strips to the panorama of 8
  tracks", "Press: the Chords view (hold + scene 1-4: pick one)"; MACRO while active: "Active: …".

## 12. Phase 4: the Maschine Mk2

### 12.1 The table

`src/controller/maschine/mk/help.rs`, installed by the shared Maschine setup for the Mk2 only
(`ctx.help_table` in `mk/setup.rs`). The Mk2's buttons have the shared setup's `ButtonId`s (the
template's CCs): CONTROL `Drum`, STEP `Row14`, SAMPLING `DeviceOnOff`, ALL `AddTrack`, AUTO
`Automation`, ENTER `Overdub`, the encoder's ◂ ▸ `ArrowLeft`/`ArrowRight`, its push `FaderTouch1`,
RESTART `Loop`, the transport's ◂ `Stop` and ▸ `TapTempo`, GRID `Flip`, ERASE `Delete`, SCENE
`Scene1`, PATTERN `Clip`, PAD MODE `Row11`, NAVIGATE `ToggleDevice`, the buttons above the displays
`Row21`-`Row28`, the group buttons `Row31`-`Row38`.

- **Global buttons** with Shift, Select and the held buttons the code looks at (Erase, Duplicate,
  Rec, Solo, Mute, the transport's ◂ = Stop for CONTROL, SCENE / PATTERN / SOLO / MUTE for the
  encoder). Shift is `is_shift_pressed`: dbm-nihost's Shift, or the transport's ◂ with NI's driver
  (Stop's own text says which).
- **Rows** (one box each, the details in the long text): the buttons above the displays (the modes
  of the knobs; Pin / Active the track or, in the device mode, the device; with Shift the previous
  send), the group buttons (plain, Rec, Solo, Mute, Erase, Duplicate), the encoder's ◂ ▸ (per mode:
  the track, the parameter, the column, the setting, the note attribute; with Shift the page).
- **The knobs** (`knobs`, one box): every mode: Volume, Pan, Send 1-8 (the track names in the long
  text), the device parameters (their names), User, the browser, Tempo, Position, Loop Start /
  Length, the note repeat configuration, the scale and drum settings, the edit note mode (by its
  page). The single knobs are `none`.
- **The encoder**: by the mode (the selected track, parameter, column, setting), the held buttons
  (pages of scenes, clips, tracks) and "last touched parameter" (the push).
- **The pads** of every view: Scene (+ Scene, Duplicate, Erase, Select, Shift), Clip (+ Duplicate,
  Pattern, Browse, Erase, Select, Shift), Play and Drum (the notes / drum pads, the step sequencer,
  the grid editor, the chords; Mute, Duplicate, Select, Erase, Solo, Browse), the parameter view
  (NAVIGATE), the note repeat view (not used by the Mk2, which configures it on the knobs) and the
  Shift view.
- **The Mk3's buttons** the Mk2 doesn't have (MACRO, PITCH / MOD / PERFORM / NOTES, PLUG-IN,
  SETTINGS, FIXED VEL, EVENTS, KEYBOARD, CHORDS, the 4-D encoder's up / down, PAGE ◂▸, ARRANGER,
  MIXER, the footswitches, the touch strip): `none`, so the coverage test sees them.
- **Escape hatch**: not needed. VOLUME and SWING (`ModeMultiSelectCommand`) name the next mode when
  one of theirs is active, else "the last used" (the command's own state).
- Left: the finer combinations of the step sequencer (long press + step: length) are in the long
  texts, not per pad; the edit note mode's pages show the Bitwig attributes also where the host has
  no step grid (Reaper: only length and velocity).

### 12.2 Generalisations

- `framework/help.rs`: the texts the shared commands need: `on_off`, `view_name`, `browsing`,
  `play_button` (PlayCommand), `automation` (AutomationCommand), `shift_button(ctx, setting)`
  (ToggleShiftViewCommand with its hold setting), `group_track_names`, `scene_names`, `next_mode`
  (ModeMultiSelectCommand). The JAM's table uses them (its copies are gone).
- `help_table.rs`: the key `knobs` (as `strips`) for the active mode's controls.
- The JAM's table names its strips' faders (`Fader1`-`Fader8 => none`); the coverage test now also
  checks the continuous controls (`check_coverage(app, views, modes)`, for both controllers).
- Simulator (`HelpLayer.qml`, `sim_common/help.py`): **neighbours whose lines cross change places**
  (`uncross`: columns and the staggered rows, with the real polyline of an area's line along the
  edge), and **the rows above / below the panel use the corners** when the columns' boxes leave
  them free. `qt_bridge.py`: the area `knobs` (the encoders of the template's `Knob1`-`Knob8`),
  the pads' side by model, the Mk2's area anchors.

### 12.3 Manual and code (Mk2)

The texts follow the code; the manual (`Native-Instruments-Maschine-Mk3.md`, its Mk2 section and
the common parts) differs here:

- **Shift**: the manual says Shift doesn't work in the MIDI mode and STEP LEFT (the transport's ◂)
  is Stop and Shift. With dbm-nihost (which sends the Shift button as a SysEx) Shift is Shift and
  the ◂ is Stop; with NI's driver it is as in the manual.
- **BROWSE while browsing**: the manual's first sentence says it accepts the selection; the code
  discards it (as the manual's later line); the encoder's push confirms.
- **SELECT + group button** (manual, "Pad Modes": rec arm): the code selects the track; REC + group
  button arms it (as the manual's "Group buttons").
- **User Params**: the project / track switch is on PAGE ◂▸, which the Mk2 doesn't send in the
  MIDI mode: the mode stays on the project parameters. The pages of the scenes and clips are on
  SCENE / PATTERN + encoder (not in the manual), not on PAGE ◂▸.
- **Drum configuration** (manual: PAD MODE again, pads 15/16 transpose): on the Mk2 (a display)
  Shift + PAD MODE puts the drum pads' offset on the knobs 7-8; PAD MODE switches Drum → Play →
  chords. PAD MODE closes the settings opened with Shift + PAD MODE (a port addition).
- **TEMPO** (manual: press again for fine tune): the tempo mode is selected; fine changes with
  Shift held.
- **Events** (manual: STEP LEFT + Select): Shift + Select (dbm-nihost's Shift).
- **VOLUME / SWING / TEMPO in the edit note mode** switch the encoder's attribute (velocity /
  gain, panorama / duration, pitch / pressure); on the Mk2 the knobs edit the attributes and the
  encoder the selected one, so this only notifies. Not in the manual.
- **CONTROL** (manual: a new instrument channel): with the transport's ◂ (Stop) held an audio
  track.
- Not in the manual: Select + PLAY punch in (+ Shift: punch out), Select + AUTO the automation write
  mode, Select + ENTER clip launcher automation writing, Shift + NOTE REPEAT the configuration,
  SOLO / MUTE + encoder the page of 8 tracks, DUPLICATE + pad in the scene view (duplicate the
  scene).
- **Note repeat octave**: not supported (the knobs 7-8 do nothing in the configuration).

### 12.4 The layout

`_mk2()` is redone from a top view photo (1864 × 1521 px): the device is 32 cm wide, so 0.172 mm per
pixel; the photo starts below the logo strip, the surface is 321 × 262 units. Positions and sizes
are the photo's (button centres measured by brightness segmentation). The template ids, kinds and
keys are unchanged (compared with the old drawing; `Mk2MappingTest.test_layout_keeps_the_templates`).
Labels: the hardware's main labels (Control, Step, Browse, Sampling, All, Auto, Volume, Swing, Tempo,
Note Repeat, Enter, Restart, Grid, Play, Rec, Erase, Shift, Scene, Pattern, Pad Mode, Navigate,
Duplicate, Select, Solo, Mute, A-H), not NI's secondary labels. Deviations from the photo:

- the encoder's push is a small "Push" button under the encoder (the hardware presses the encoder);
- the arrows under BROWSE are drawn as buttons without a template (they send nothing in the MIDI
  mode), added at the end so the keys stay;
- the displays are a little larger than the screens (105 × 29 mm, room for 28 characters);
- the buttons above the displays keep the numbers 1-8 (unlabelled on the hardware; the display
  shows their labels).

Help sides: the upper left block (Browse, Sampling, All, Auto), the master section and the groups
left; Control, Step and the buttons above the displays top; the knobs right (the line along their
row, not across the displays); the column left of the pads right (on the left their lines crossed
the master section's; now they pass under the pads); Restart, the transport's ◂ ▸ and Grid left, Play, Rec, Erase, Shift and the pads below (the pads'
line between the pad columns 2 and 3).

