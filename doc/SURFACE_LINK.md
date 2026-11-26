# The surface link

A generic link between two controller instances of the core (not part of Java): a **provider**
publishes a 6x6 page model and executes input; **surfaces** (any controller with a grid of at
least 6x6 and some encoders) render it and send input back. The first provider is the Electra
One (its DrivenByMoss pages, and on the Mk2 emulator the pages of any other preset), the first
surface the Maschine JAM. Nothing in the link knows a concrete controller.

The design was written in the emulator project: electra-one `docs/surface-link.md` (option A,
made generic; §6a the standard SysEx API, §6b the rows/selection contract, §6c the generic
provider). This document is the design as built.

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

## 2. Messages (`src/framework/page_surface/model.rs`)

The page model, the generic view and mode are in `framework::page_surface` (shared with the
host's pages, HOST_PAGE_PLAN.md); the link is one of the page sources (`surface_link::LinkSource`).
Model 1.2 adds sizes up to 8x8, control sets, sections, turn units and `Shown` (HOST_PAGE_PLAN.md §3.3).

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
- `ValueNext`, `ValuePrev` (1.1): the next / previous value of the selected slot's control, for a
  control with several values on one knob (an envelope's or XY pad's handle);
- `ExtraTurn { index, delta, fine }`.

Deltas are steps of the provider: the provider applies its ranges and its acceleration, so that
every surface behaves the same. `fine` asks for its fine resolution.

**Capabilities** (surface → provider, with the attach): grid size, number of encoders,
touch-sensitive or not, a main encoder or not, color depth (RGB, palette, mono).

**Versioning:** `PROTOCOL_VERSION` (1.1; 1.0 without `ValueNext`/`ValuePrev`) goes with the attach and the welcome. The same major
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

### A surface (`page_surface/client.rs`, `page_surface/view.rs`)

The controller supplies only the mapping (`SurfaceMapping`):

```rust
SurfaceMapping::new(8, 8)                             // the JAM's
    .page_list_row_at(0, 0, 6)                        // a page list in row 1
    .pad(6, 0, PadFunction::PagePrev).pad(7, 0, PadFunction::PageNext)
    .cells_at(0, 1, 8, 6)                             // up to 8x6 cells: top left pad
    .section_list_row_at(0, 7, 8)                     // the sections of a source
    .encoders(EncoderKind::Absolute, vec![RowSlot(0), …, RowSlot(5),
        RowSlotOrSelected { column: 6, fine: true }, RowSlotOrExtra { column: 7, index: 0 }])
    .main_encoder(RelativeEncoding::TwosComplement)
```

Plus, optionally, `touch_encoders` and `ui_colors`: palette indices for the row selectors,
pages, navigation, selection and the "no provider" cross. Without them, the closest palette
color of a default RGB color is used, which is too dark on fixed palettes.

`row_selectors_at` and `page_list_at` (a column) exist for other layouts. **Row selectors:** the
active row is white (`row_active`). The inactive rows alternate between
`row_set_a` and `row_set_b` per control set (rows 1/2, 3/4, 5/6) only if the surface's slot
encoders (`RowSlot`) cover a whole control set, i.e. 12 or more for the 2 rows of a set
(`SurfaceMapping::covers_control_set`): the colors then show which two rows the encoders turn
together. A surface with one row of encoders (the JAM's 6 strips) selects each row on its own,
so all its inactive rows have the `row_set_a` color (`view::row_color`).

Then:

1. Create a `SurfaceClient` with its sources (`SurfaceClient::shared(mapping, sources)`, e.g. a
   `LinkSource`, which attaches at once) and register `PageSurfaceView::new(client)`
   (`Views::SurfaceLink`) and `PageSurfaceMode::new(client)` (`Modes::SurfaceLink`).
2. Call `client.poll(&ctx.model)` in `handle_changes`; it returns the state changes of the
   current source. `set_shown` on entering and leaving, `select_source` / `next_available` for
   several sources.
3. Wire what is not a grid pad or a mode knob:
   - the main encoder → `on_main_encoder`;
   - row and page buttons → `step_row`, `send(Input::PageNext)`;
   - buttons for the value of the selected slot → `step_value(±1)`;
   - encoder touches → `on_encoder_touch`;
   - LEDs → `encoder_value(index)`.
4. Decide the entry gesture, and leave the surface on `Detached`.

The view draws the slots in their colors (RGB → the controller's palette through its color
manager, always on the direct path, `ColorManager::direct_color_id_for_role`, since these colors
never come from Bitwig. On a palette made of hues × levels (NI's) the slot gives the hue and the
view the level by role: a normal slot full, the selected slot pale (blinking white), a disabled
(hidden) or covered slot dim, and the default colors of the inactive row and page selectors
medium; other palettes take the nearest entry and leave disabled and covered slots off; before, they went
over the closest of Bitwig's track colors and the controller's table for them, which turned the
Electra's orange F49500 lime green on the JAM. A controller without a palette table keeps the
closest DAW color. See FRAMEWORK_MAP.md, "framework/controller/color"). It also draws the row and page
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

**Not used for the DrivenByMoss pages:** the `02 41` parameter map dump and the value-change
event `7E 63`. This provider's values come from the DAW through its own page cache, and the
device's knob turns already arrive as relative CCs on Port 1. The generic provider (§6.2) uses
`7E 63`.

## 6.2 The generic provider (`src/controller/electra/generic.rs`, `page_view.rs`)

For any preset other than DrivenByMoss's, on the Mk2 emulator: electra-one `docs/surface-link.md`
§6c and `firmware-mk1/SYSEX-API.md` "The generic provider" (master a59929d, firmware 5f425dc).
The firmware does all the value math (ranges, steps, overlays, relative messages, formatters);
DrivenByMoss only maps cells to knobs and the input to messages. All bytes are in
`surface_ext.rs`.

**The mode switch** (in the Electra controller, the normal model):

| Active preset (`7E 02`) | Device | The Electra |
| --- | --- | --- |
| DrivenByMoss | any | online, the DrivenByMoss pages as before (§6) |
| another | `"surface":{"rows","selection","view","input"}` all true | offline for the DAW (the Dummy mode, no MIDI to the pads, the touch combinations as in Java), the generic provider for the preset |
| another | without them (a real Electra, an older emulator) | offline, the page "Offline" as before |

- Entering: `14 79 [flags] 01` (the value changes) and `02 52`. Leaving (the DrivenByMoss preset
  again): `14 79 [flags] 00`, then online as usual.
- Without the DrivenByMoss preset in the preset list, a capable device goes straight to the
  generic provider (the error is logged as before).
- **Bitwig meanwhile:** the Electra controller does nothing for the DAW: the Dummy mode is active,
  the knobs of the other preset go to the synth (as without the link), and the knob touch
  combinations work as in Java. Selecting the DrivenByMoss preset (on the device, or with a
  combination) brings the DAW pages back.

**The model:** `02 52` → `01 52 <JSON>` on entering, on `7E 62` (Lua or SysEx changed the page,
a handle, the control set, the page or the preset; a page renamed or hidden), on `7E 06` and
`7E 02`. One request at a time: a change while one is open asks again after the answer; an
unanswered request is sent again after a second, up to three times.

- **One slot per cell, a cell is a knob:** cell (row r, column c), counted from 1, is knob c
  (odd r) or 6 + c (even r) of the control set ceil(r/2).
  - The active set: from the device's knob map (`knobs`).
  - The other sets, and knobs the map has none for: from the controls' `inputs`. A Custom control
    listens to all its inputs (each input a slot of its own value); every other control only to
    its first input, which turns its active value.
  - A visible control wins over a hidden one.
- **The slot:**

| Field | From |
| --- | --- |
| `kind` | `list` (or a value with `items`) → `List`, `pad` → `Pad`, `relative` → `Relative`, `textBox`/`none` → `Empty`, everything else (faders, knobs, envelopes, XY pads, Custom inputs) → `Fader` |
| `name` | the control's name, plus " · " and the value id (capitalized) if the control has several values: "ENV · Attack"; a Custom control with several inputs: "GRID · B" |
| `color` | `color` |
| `value` | (value - min) / (max - min); a list: index / (items - 1) |
| `value_text` | `text` (what the panel shows: formatter, override, overlay label) |
| `steps` | the number of items of a list |
| `on` | a pad's value above its minimum |
| `enabled` | `visible` |

- **Covered cells:** a cell which is no knob, but which a visible control covers (`cell` and
  `span`: an envelope's other three cells, a wide Custom control): an `Empty` slot with that
  control's name and color, not enabled. The generic view draws it dim on a palette with levels (NI's: the
  dim level of the control's hue, so the knob pads stand out in full) and off on other palettes
  (one lit pad per knob, which is how many knobs the control has); a press selects the control's knob slot. Other cells
  are empty.
- **Pages:** the `pages` of the page view without the hidden ones (the current page always);
  `page` is its index in that list.
- **Values:** `7E 63` updates the value and its text (also of values no slot shows, e.g. an
  envelope's other handles, for when they become active).
- **Row and selection:** from the page view (`row`, `selectedControlId`) and the §6.1 events of
  origin 0 and 2. The selection is the knob slot of the selected control (of its active value).

**The input:**

| Input | Message |
| --- | --- |
| `SlotTurn` | `14 53 [knob] [delta] [01]` for the cell's knob. If the cell's control set is not the active one, `09 50 [row]` first: the device then turns the right knob, and the surfaces only turn the active row anyway. One link step is half a panel step, sent as 4 fine steps (a JAM strip across its length is about the whole range of a fader, as on the DrivenByMoss pages; a list moves an item per 16 link steps); `fine` sends one fine step (1/8 panel step) per link step. The firmware carries the fractions until a touch end |
| `SlotTouch` | `14 54 [knob] [0\|1]` (after `09 50` as above) |
| `SlotPress` / `SlotRelease` | a pad: `14 55 {"value": max}`, on the release `min` (momentary), or the other one of min/max on the press (toggle); a list: selects the slot and steps to the next item (`14 55 {"step":1}`, after the last item the first item's value); a fader or relative slot: selects it (`09 50` if needed, `14 51 [control]`); a covered cell: selects its control's knob slot |
| `SelectedTurn` | as `SlotTurn` for the selected slot |
| `SelectedReset` | `14 55 {"default":true}` for a fader or list |
| `ValueNext` / `ValuePrev` | `14 56 {"controlId", "next"\|"prev": true}` for the selected slot's control, if it has several values and is not a Custom control. The device answers with `7E 62`, the new page view names the new handle |
| `RowSelect` | `09 50 [row]` |
| `PageSelect`, `PageNext`, `PagePrev` | `09 0A [page id - 1]` |

`14 55` always names the slot's value (`valueId`); lists take the overlay item's value, as the
firmware expects.

**The test environment:** the emulator answers `02 52` with one SysEx of 10-30 KB. An ALSA
sequencer client receives it only with a larger input pool than the default (200 cells, about
5 KB): `sim_common/alsa_seq.py` sets the largest one (2000), so `dbm_host_alsa.py` gets it.

## 7. The JAM as a surface (`src/controller/maschine/jam/surface_link.rs`)

**Entry:** **Select + CONTROL** cycles through the available sources: off → the next available
one → … → off (the Electra's pages over the link, then the host's pages, HOST_PAGE_PLAN.md; with
one source it switches the surface on and off), on every host.
- Select + X is the JAM's pattern for second functions (Select + AUX, LEVEL, MACRO). Select +
  CONTROL replaces Java's project / track toggle of the user mode; until 2026-11-26 the surface was
  on Select + SONG, which is now the host's snapshots (HOST_BRIDGE_GUIDE.md §4.14; it leaves the
  surface for the snapshot view).
- Any other view or mode button (SONG, STEP, PAD MODE, MACRO, LEVEL, AUX, CONTROL) leaves the
  surface for its view or mode.
- A detach of the source leaves it too. The view and the mode from before come back.
- A temporary view (the Shift view while Shift is held) keeps the surface.
- From the **host page strips** (CONTROL's second slot with host pages, HOST_PAGE_PLAN.md §5.1)
  Select + CONTROL starts with the host source (then the Electra, then off); off returns to the
  strips, and CONTROL from the surface goes to the strips too (the last of CONTROL's modes).

**Mapping** (one layout for all sources, HOST_PAGE_PLAN.md §5):

| JAM | Surface |
| --- | --- |
| Grid row 1, pads 1-6 | the page list (the window of 6 pages around the current one), the current one orange |
| Grid row 1, pads 7-8 | previous / next page |
| Grid rows 2-7 | the cells, left-aligned: 6 columns for the Electra (columns 7-8 dark), up to 8 for the host; in the hue of their colors at full level; covered and hidden cells dim; outside the active control set (with sets) medium; a press selects a knob cell (and its row) or presses a pad cell; the selected cell is pale and blinks white |
| Grid row 8 | the sections of the source (the host's), the active one full |
| Strips 1-6 (1-8 with an 8-wide page) | the cells of the active row; the LEDs show their values in the cell colors. A swipe turns by its distance (no jumps), with Shift in single steps; the strip touch is the slot touch |
| Strip 7 (6-wide page) | the selected slot, always fine |
| Strip 8 (6-wide page) | the first extra of the provider (the Electra has none) |
| Main encoder | `selected.turn`, Shift for fine; its push: `selected.reset` |
| PAGE LEFT / RIGHT | previous / next page |
| Up / down | previous / next row (there is no row-selector column) |
| Left / right | previous / next control set (with sets), else the previous / next value of the selected slot (an envelope's handle) |
| No source | a red cross over the cells, notification "Surface: no provider" |

The JAM attaches at its start (so the surface is there at once when it is switched on) and polls
the link at every tick, also while its own modes are active.

## 8. Using it

**Bitwig:** add both controllers, the Electra One and the Maschine JAM, as two DrivenByMoss
(Rust) extensions in one Bitwig. Both must run the same library (see §4); with
`DBM_DEV_LIBRARY`, restart both after a rebuild. Select + CONTROL on the JAM.

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
- `tests/surface_link.rs` (generic): the mode switch (DrivenByMoss preset ↔ another, the
  capability present/absent, without the DrivenByMoss preset), the JAM's input as the exact bytes
  (strips, touches, selection, handles, reset, lists, pads, Custom inputs, covered cells, other
  control sets, pages), `7E 63`, `7E 62` with the request handling, page switches;
- `page_view.rs` unit tests: the `02 52` → model mapping (a single control, an envelope and its
  handle, a list, a Custom control, hidden controls, other control sets, hidden pages);
- `surface_ext.rs` unit tests: the bytes.

**End to end with the generic provider:** load other presets into further slots of the emulator
(`--load "…/Acid Machine v0.9.3.1 mk2.epr" --slot 2`, electra-one's
`tests/fixtures/surface/surface-view.epr` for an envelope), switch to one on the device; the JAM
shows its page.

## 9. Open questions

- Transport outside one process (the emulator as a provider, a JAM on another host): OSC or UDP
  with the same messages. `Version` is there for it.
- Acceleration: the provider passes the deltas to the Electra's knob path. A surface with
  detented encoders may want acceleration; then the provider (or `LinkProvider`) should add it,
  not the surface.
- The 12-encoder layout (two rows per control set) for surfaces with 12+ encoders, and the Mini
  (2x8) as a provider.
- The generic provider: the scaling of a link step (half a panel step) makes lists slow on a
  detented encoder (16 detents per item). A surface-independent answer would be an acceleration
  in the provider (see above) or `14 55 step` for lists on `SelectedTurn`.
- The generic view draws covered cells dark; a surface could draw them dim in the control's
  color (the slot carries it).
- A provider in the host app instead of a controller: zynthian's Electra-like control grid
  (zynthian-ui branch `electra-grid`) could provide the page model directly, so the JAM shows it
  without the Electra. That is the next step, planned separately (STATUS.md, "Where to pick up";
  electra-one `docs/zynthian-electra-ui-brainstorm.md`).
