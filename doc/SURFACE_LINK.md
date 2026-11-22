# The surface link

A generic link between two controller instances of the core (not part of Java): a **provider**
publishes a 6x6 page model and executes input; **surfaces** (any controller with a grid of at
least 6x6 and some encoders) render it and send input back. The first provider is the Electra
One (its DrivenByMoss pages), the first surface the Maschine JAM. Nothing in the link knows a
concrete controller.

The design was written in the emulator project: electra-one `docs/surface-link.md` (option A,
made generic; §6a the standard SysEx API, §6b the rows/selection contract). This document is the
design as built.

## 1. Roles and states

- One provider per link, any number of surfaces. Either may start first.
- A provider without surfaces works as before; a surface without a provider shows "no provider"
  and keeps its own modes.
- The surface side has explicit states (`LinkState`):

| State | Meaning |
| --- | --- |
| `Missing` | no provider (yet), or the provider runs on another copy of the library (§5) |
| `Attached` | the provider welcomed the surface; the model is valid |
| `Detached` | the provider left (unloaded, restarted); a new provider welcomes the surface again |
| `Incompatible` | the provider has another major protocol version |

A detach (or an incompatible provider) makes the surface fall back to its own modes.

## 2. Messages (`src/framework/surface_link/model.rs`)

Typed Rust enums, no strings.

**The model** (`PageModel`, provider → surfaces):

- `page`, `pages` (names);
- `slots[36]`, row by row, the ID is the index. Each `Slot` has:
  - `kind`: `Fader`, `List`, `Pad`, `Relative`, `Empty`;
  - `name`, `color` (`Rgb`);
  - `value` (0..1), `value_text`;
  - `steps` (lists), `on` (pads), `enabled`;
- `rows_per_set` (2: rows 1/2, 3/4, 5/6 are the control sets), `active_row`, `selected_slot`;
- `extras` (optional values for encoders beyond 6).

**The changes** (`ModelChange`): `Snapshot` (on attach and on a page switch), `Slot`, `Value`
(only the value and its text), `ActiveRow`, `SelectedSlot`, `Extra`.
`PageModel::diff` / `apply` make and consume them.

**The input** (`Input`, surface → provider; rows and slots count from 0):

- `SlotTurn { id, delta, fine }`, `SlotPress`, `SlotRelease`, `SlotTouch { id, on }`;
- `RowSelect`, `PageSelect`, `PageNext`, `PagePrev`;
- `SelectedTurn { delta, fine }`, `SelectedReset`;
- `ExtraTurn { index, delta, fine }`.

Deltas are steps of the provider: the provider applies its ranges and its acceleration, so that
every surface behaves the same. `fine` asks for its fine resolution.

**Capabilities** (surface → provider, with the attach): grid size, number of encoders,
touch-sensitive or not, a main encoder or not, color depth (RGB, palette, mono).

**Versioning:** `PROTOCOL_VERSION` (1.0) goes with the attach and the welcome. The same major
version works together; a newer minor version only adds messages or fields which the other
side ignores. Another major version: the provider refuses the surface (`Incompatible`).

## 3. Mechanics (`src/framework/surface_link/mod.rs`)

- **The registry:** a static of the library, keyed by the link name.
  - `provide(link, name)` fails if the link has a provider already.
  - `attach(link, capabilities)` always works.
  - The controllers of this crate use the link `DEFAULT_LINK` ("pages").
- **Two queues per surface, one per direction**, each with its own `Mutex`:
  - The provider drains "up" (the input) at its tick and pushes to "down".
  - The surface drains "down" (the welcome, the model, the detach) at its tick and pushes to
    "up".
  - Neither side ever calls into the other instance, so two hosts' threads cannot deadlock.
- **The registry lock** is only held to add or remove a side.
  - Dropping a provider pushes its "gone" to the surfaces under this lock, so a new provider
    cannot welcome a surface before that.
  - The lock order is always registry, then a queue.
- **Stale messages:** every message to a surface carries the provider's ID. Messages of an
  older provider are ignored.
- **Dropping a handle** (the instance stops) detaches it. Its counterpart notices at its next
  tick.

## 4. One library per process (the assumption)

The registry only connects instances which run on the same copy of the library in one process:

| Host | Shared? |
| --- | --- |
| Reaper (`App` with several instances) | yes, one `App` |
| Bitwig | yes: all extensions of the JVM load the same file. `NativeLibraryLoader` copies the library to the cache folder, named by the hash of its content, and `SymbolLookup.libraryLookup` (dlopen) of the same path returns the loaded library |
| Python reference host (`dbm_host_alsa.py --model a --model b`), the Python tests | yes: `drivenbymoss._ffi` loads the library once per process (`ctypes.CDLL`) |
| Zynthian | yes: `dbm_zynthian.nihost.load_library` keeps one `CDLL`. Each driver ticks on its own thread, which the locks allow |

**Two different copies** have two registries and cannot find each other. Examples:
- a Bitwig with `DBM_DEV_LIBRARY` and an extension which still runs the bundled library;
- in dev mode, one extension which reloaded a new build while the other did not yet.

The surface then stays in `Missing` and shows "no provider". Restarting both extensions (the
same library) connects them.

## 5. API for future controllers

### A provider (`src/framework/surface_link/provider.rs`)

```rust
let mut provider = LinkProvider::new(DEFAULT_LINK, "My Controller")?; // or LinkProvider::inactive()
// handle_changes:
for event in provider.poll() { if let ProviderEvent::Input { input, .. } = event { /* execute */ } }
// flush, after the controller updated its own state:
*provider.model_mut() = build_model(ctx);
provider.publish(); // snapshot on page switches, changes otherwise; new surfaces get a snapshot
```

Execute the input the way the controller's own controls would, so that the device shows the
change too.

### A surface (`client.rs`, `view.rs`)

The controller supplies only the mapping (`SurfaceMapping`):

```rust
SurfaceMapping::new(8, 8)
    .slots_at(1, 1)                                   // the 6x6 slots: top left pad
    .row_selectors_at(0, 1)                           // 6 row selectors in a column
    .page_list_at(7, 0, 8)                            // a page list
    .pad(0, 0, PadFunction::PagePrev).pad(0, 7, PadFunction::PageNext)
    .encoders(EncoderKind::Absolute, vec![RowSlot(0), …, RowSlot(5), Selected { fine: true }, Extra(0)])
    .main_encoder(RelativeEncoding::TwosComplement)
```

Plus, optionally, `touch_encoders` and `ui_colors`: palette indices for the row selectors,
pages, navigation, selection and the "no provider" cross. Without them, the closest palette
color of a default RGB color is used, which is too dark on fixed palettes.

Then:

1. Create a `SurfaceLinkClient` (shared, attaches at once) and register
   `SurfaceLinkView::new(client)` (`Views::SurfaceLink`) and `SurfaceLinkMode::new(client)`
   (`Modes::SurfaceLink`).
2. Call `client.poll()` in `handle_changes`; it returns the state changes.
3. Wire what is not a grid pad or a mode knob:
   - the main encoder → `on_main_encoder`;
   - row and page buttons → `step_row`, `send(Input::PageNext)`;
   - encoder touches → `on_encoder_touch`;
   - LEDs → `encoder_value(index)`.
4. Decide the entry gesture, and leave the surface on `Detached`.

The view draws the slots in their colors (RGB → the controller's palette through its color
manager, via the closest DAW color, as for track colors). It also draws the row and page
selectors, the selected slot blinking, and a cross without a provider. The mode turns the slots
of the active row: relative encoders by their encoding; absolute faders and strips by the
distance between two positions, anchored on a touch or after 300 ms without movement. Shift
makes it fine. `state_text` gives the texts to notify.

The JAM's part (`src/controller/maschine/jam/surface_link.rs`) is about 280 lines with the
command wrappers. That is the pattern for a Launchpad, Push or APC surface.

## 6. The Electra One as the provider (`src/controller/electra/link.rs`)

- **Registration:** the normal model (Mk1/Mk2) registers at creation. A second Electra in the
  same process logs that the pages are provided already and works without. The Mini (2x8) does
  not provide.
- **The model:** the page of the active mode.
  - From its page cache: names, colors, visibility, values (0..127 → 0..1), value texts.
  - From the preset (`preset_layout.rs`, generated from `DrivenByMoss.eproj`): the kind of each
    slot, and the names and colors the modes do not set.
  - While offline (the DrivenByMoss preset not selected): one page "Offline" with empty slots.
- **The input**, as from the Electra's own controls:

| Input | What the provider does |
| --- | --- |
| `SlotTurn` | the knob's relative CC (`10 + 10·row + column`, channel 16, two's complement), through the same bound parameter as the Electra's knob. `fine`: single steps and the slow sensitivity (as the emulated Shift) |
| `SlotPress` / `SlotRelease` | for a pad, its CC `70 + 10·row + column` with 127 / 0. For a knob, it selects the slot and its row (a list also steps once) |
| `SlotTouch` | the pot touch (touches the parameter) |
| `PageSelect`, `PageNext`, `PagePrev` | the mode at once, and the device with `09 0A [page]` |
| `SelectedTurn` | as `SlotTurn` on the selected slot |
| `SelectedReset` | resets the parameter of the selected slot (the Electra has no gesture for it) |

- **Device-side changes** reach the surfaces: a page switch on the device (`7E 06`) is the next
  snapshot; turning a knob changes the DAW and with it the model.
- **Acceleration:** the provider passes the delta to the Electra's own relative-knob path (its
  value changer, 128 steps over the range). A JAM strip swipe across its length is about the
  full range. The provider adds no further acceleration.

### 6.1 The Electra-side contract (`src/controller/electra/surface_ext.rs`)

The active row and the selected slot on the device. This is electra-one `docs/surface-link.md`
§6b (agreed); all its bytes are in `surface_ext.rs` only.

**Capability.** The device information (`02 7F` → `01 7F` JSON) has
`"surface": {"rows": true, "selection": true}` on the emulator.

| | With the capability | Without it (a real Electra) |
| --- | --- | --- |
| Subscription | `14 79 [0x39 \| 0x02]` (the control set events, which carry the row and selection events) | Java's `14 79 39` |
| Online | reads the row (`02 50`) and the selection (`02 51`) | — |
| Row r (1..6) | `F0 00 21 45 09 50 [r] F7` (control set ceil(r/2), odd = upper half) | `09 0B [set]` (set 0..2) only when the set changes; it selects the upper row of the set |
| Selection | `F0 00 21 45 14 51 [id lsb] [id msb] F7`: a control ID of the current page (`page·36 + slot + 1`), 0 = none | no highlight |

**Answers and events:**

| Message | What the provider does |
| --- | --- |
| `01 50 [r]` | the row |
| `01 51 [lsb] [msb]` | the selection |
| `7E 60 [r] [origin]` | row changed |
| `7E 61 [lsb] [msb] [origin]` | selection changed |
| `7E 07 [set]` | the control set (without the capability: the upper row of a new set) |

The origin of the two events:
- 0 = the device (panel, keys, touch) and 2 = Lua: adopted and sent to the surfaces;
- 1 = SysEx: the provider's own changes (DrivenByMoss is the only SysEx sender on CTRL),
  including the row reset after its own `09 0A`. They only confirm a pending change.

**Page switch:** the device resets the row to the upper row of the page's default control set
and the selection to none, and reports both. The provider resets to row 1 and no selection on
its own page switch and on `7E 06`, and follows the events.

**NACK:** ACK and NACK do not name the request, and nearly every command is acknowledged. So a
pending own row or selection change is confirmed by its origin-1 event, and only a NACK while
one is pending undoes it:
- a NACK for `09 50`: the device does not have the rows after all; back to `09 0B`;
- a NACK for `14 51`: the selection is unchanged.

A NACK of another command while a change is pending would undo it too. This is rare; the
contract only NACKs IDs which are not on the current page, and the provider never sends those.

**Not used:** the `02 41` parameter map dump and the value-change event `7E 0D` (subscribed with
a second flags byte of `14 79`; electra-one §6a). This provider's values come from the DAW
through its own page cache, and the device's knob turns already arrive as relative CCs on Port
1. They are the way for a provider over the standard SysEx API, for any preset:
- `02 41` → `01 41 {"version":1,"parameters":[…]}` (the emulator's format);
- `7E 0D {"controlId":…,"value":…,"origin":"internal|midi|lua|modulation|file"}`, 20 ms rate
  limit.

`initial_state_requests` is the hook for such an initial state after going online.

## 7. The JAM as a surface (`src/controller/maschine/jam/surface_link.rs`)

**Entry:** **Select + SONG** switches the surface on and off.
- Select + X is the JAM's pattern for second functions (Select + CONTROL, AUX, LEVEL, MACRO), and
  Select + SONG was free.
- Any other view or mode button (SONG, STEP, PAD MODE, MACRO, LEVEL, AUX, CONTROL) leaves the
  surface for its view or mode.
- A detach of the provider leaves it too. The view and the mode from before come back.
- A temporary view (the Shift view while Shift is held) keeps the surface.

**Mapping:**

| JAM | Surface |
| --- | --- |
| Grid centre 6x6 (columns 2-7, rows 2-7) | the slots in their colors; a press selects a knob slot or presses a pad slot; the selected slot blinks white |
| Column 1, rows 2-7 | the row selectors: the active row white, the control sets blue and turquoise |
| Column 1, top / bottom pad | previous / next page |
| Column 8 | the page list (up to 8 pages), the current one orange |
| Strips 1-6 | the slots of the active row; the LEDs show their values in the slot colors. A swipe turns by its distance (no jumps), with Shift in single steps; the strip touch is the slot touch |
| Strip 7 | the selected slot, always fine |
| Strip 8 | the first extra of the provider (the Electra has none) |
| Main encoder | `selected.turn`, Shift for fine; its push: `selected.reset` |
| PAGE LEFT / RIGHT | previous / next page |
| Up / down | previous / next row |
| No provider | a red cross over the slots, notification "Surface: no provider" |

The JAM attaches at its start (so the surface is there at once when it is switched on) and polls
the link at every tick, also while its own modes are active.

## 8. Using it

**Bitwig:** add both controllers, the Electra One and the Maschine JAM, as two DrivenByMoss
(Rust) extensions in one Bitwig. Both must run the same library (see §4); with
`DBM_DEV_LIBRARY`, restart both after a rebuild. Select + SONG on the JAM.

**The reference host:** several `--model`s run several controllers in one process, on one
simulated DAW:

```sh
cargo build
~/bastel/github/electra-one/build/electra --headless --port 5497 --card /tmp/e1/card \
    --eeprom /tmp/e1/card.eeprom --load ~/bastel/github/electra-one/data/DrivenByMoss.eproj --slot 1 &
support/controller_simulator/run.sh --model jam --suffix " (sim)" &
support/controller_simulator/tools/dbm_host_alsa.py --model electra --model jam --suffix " (sim)" \
    --library target/debug/libdrivenbymoss.so
```

**Tests:**
- `cargo test --lib surface_link`: the registry, queues, states, start orders, several surfaces,
  versions, threads, the client and the provider helper;
- `tests/surface_link.rs`: the Electra and the JAM as two `App`s in one test, both start orders,
  the snapshot and changes, the JAM's input as typed `Cmd`s and as MIDI/SysEx to the Electra,
  pages and rows, detach/re-attach, and the capability present/absent/NACK with the events of
  each origin and the page-switch reset;
- `surface_ext.rs` unit tests: the bytes.

## 9. Open questions

- Transport outside one process (the emulator as a provider, a JAM on another host): OSC or UDP
  with the same messages. `Version` is there for it.
- Acceleration: the provider passes the deltas to the Electra's knob path. A surface with
  detented encoders may want acceleration; then the provider (or `LinkProvider`) should add it,
  not the surface.
- The 12-encoder layout (two rows per control set) for surfaces with 12+ encoders, and the Mini
  (2x8) as a provider.
