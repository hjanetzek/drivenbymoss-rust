# Controller help in the simulators

Status: phase 1 and phase 2 done for the Maschine JAM (see §6); round 2: the JAM's texts as one
declarative table (§9, with a comparison of the two approaches), boxes coloured by scope, one box
per row of buttons, the global boxes hideable (§10). The mirror (§4) is the next phase.

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
the template's CC, `maschine_simulator/templates.py`). The JAM's Shift has no MIDI binding (NI's
SysEx), it is matched by its id `Shift`; `grid` goes to the pads and `strips` to the touch
strips, the encoder's entry `MasterKnob` to the encoder (CC 57).

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
- **Expanded:** drawn on top of its neighbours (wider and taller), not pushing them: the layout
  of the others stays still, which reads better than boxes jumping around on every press.
- A control without help in the current state gets no box.
- Boxes update live (the host's changes come in on every tick).

## 3. Texts

Sources: the JAM manual (DrivenByMoss-Documentation), the Java original, and the port's own code,
including its additions (the host page strips on CONTROL's second slot, Shift + CONTROL to the
first remote page, Select + CONTROL / Select + SONG for the page surface, doc/HOST_PAGE_PLAN.md
§5.1, doc/SURFACE_LINK.md). **The code wins** where they differ; the differences are in §7.

## 4. Mirror

The simulator as a mirror of a real device which the core drives: its LEDs and strips, and the
boxes highlighted and expanded on hardware presses.

- **Presses** come with the help entries in every case: `pressed` is part of an entry, so the
  simulator expands the box of a button which became pressed, whatever the transport.
- **ALSA (dbm-nihost, VirMIDI):** the simulator subscribes passively. The device's input
  (presses) is dbm-nihost's readable port `Maschine Jam - 1 Input`; the DAW's output (LEDs) is not
  readable at dbm-nihost's write port, but at the DAW's own source port (dbm_host's client, the
  VirMIDI client which Bitwig writes to). Needs a read-only simulator (no hello, no answers, no
  sends) with two subscriptions found by the user (`--mirror-from`).
- **In-process HID (libdbm_nihost, `src/nihost`):** there is no ALSA port. The core already has
  the template MIDI in both directions (`DeviceIo::send` / `read`), the stream the simulator
  speaks. A tap there (a queue the host fetches, e.g. `dbm_take_device_midi`) sent over the help
  socket as `{"midi": "out"|"in", "data": "hex"}` lets the simulator draw the LEDs from "out" and
  the presses from "in".
- **Recommendation:** the in-process tap over the existing help socket. It is the default path
  on Linux now, needs no port juggling and one socket carries everything; the ALSA mirror only
  serves the daemon setups. Not implemented yet (the next phase).

## 5. Phases

1. The framework end to end with the JAM's global buttons: core trait and collection, C ABI,
   Python binding, socket, simulator mapping and drawing.
2. The views and modes: Play, Chords, Piano, Drum 64, Drum, Drum 4/8, the sequencers, Session,
   the Shift view, note repeat, Tempo/Shuffle, the browser, the accent view, the strip modes,
   the page surface and the host page strips.
3. The mirror (§4).
4. The other simulators (Mk2, Mk3, the MIDI simulators, the Push): their commands' texts; the QML
   is shared already.

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

- The mirror (§4): the next phase. Presses on the hardware already expand the boxes (the
  entries carry `pressed` / `presses` from the core, whatever the transport); the LEDs need the
  tap.
- The other simulators (Mk2, Mk3, Mikro Mk3, the MIDI simulators, the Push): the QML and the
  socket are shared; they need their commands' texts and a resolver (`enable_help`).
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
