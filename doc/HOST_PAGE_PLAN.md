# Host-provided parameter pages on the surface link: plan

Status: revised after the owner's review (2026-11-24); P1-P5 built (§13), P6 handed over (§12).
Goal (from the owner): a mode for the Maschine JAM which
shows an Electra-like parameter page **provided by the host app** (first host: zynthian, its
control grid on the zynthian-ui branch `electra-grid`) instead of the Electra controller over the
surface link. Not JAM-only (any controller with a grid and encoders) and not zynthian-only (any
host that can provide pages; Bitwig later).

Background: SURFACE_LINK.md, TYPED_PROTOCOL_PLAN.md, ZYNTHIAN_PLAN.md, ZYNTHIAN_CPU.md,
HOST_BRIDGE_GUIDE.md; electra-one `docs/zynthian-electra-ui-brainstorm.md` (paths A/B/C),
`docs/surface-link.md` §6b/§6c, `docs/lv2-to-electra.md`; zynthian-ui `electra-grid`
`docs/qt-control-grid.md`, `zyngui/zynthian_control_grid_data.py`.

**Tags:** **[V path:line]** = verified by reading the code at that place; **[A]** = an
assumption, not checked in code; **[P]** = proposed here.


## 0. Summary (revised 2026-11-24 after the owner's review)

- **Option (a), but without a second link.** The host's pages reach the controller in the **same
  core instance**, so the surface link's registry, queues and attach states add nothing for them.
  The page model and the generic surface view/mode move out of `surface_link` into a neutral
  module, `framework::page_surface`. A small trait, **`PageSource`**, gives a surface its model
  changes and takes its input. Two sources:
  - **the link source** (`surface_link::LinkSource`): the surface side of the surface link, i.e.
    the Electra controller providing in another instance — unchanged behaviour;
  - **the host source** (`page_surface::host::HostSource`): fed directly from the new `Msg`
    records (block 0x0C) which the parser applies to `daw::host_page::HostPage`, and sending the
    surface's input straight as `Cmd`s (block 0x8C).
- **The host owns the selection** (the owner's decision, §D). The page, the row, the control set
  and the selected cell are what the host's screen shows; the core displays only what the host
  reports and never applies a selection of its own. JAM input that selects goes to the host as a
  command; the host applies it to its screen and reports the result. Changes on the host's screen
  are reported the same way. There is one source of truth, so there is no echo and no ping-pong.
- **The host does the value math** (zynthian: `zctrl.nudge`, list steps, toggles), as the Electra
  firmware does for `14 53`. The core sends deltas with a unit (a strip's travel or an encoder's
  detent) and a resolution (normal or fine).
- **The page model 1.2** (part of the page model, carried by the link too): sizes up to 8×8, explicit
  control sets (a zynthian page = the 4 cells its encoders turn), sections, `Shown` (the
  subscription: the host reports only while a surface shows its pages), the turn unit.
- **The JAM** (one layout for both sources): row 1 the page list with its last two pads previous /
  next page; rows 2-7 the cells, left-aligned, up to 8 columns (Electra pages are 6 wide, columns 7-8
  stay empty); row 8 the sections (zynthian) or nothing; no row-selector column (a fader or list cell
  press selects it and its row, pads only switch). Strips 1-8 the active row; with 6-wide pages
  strips 7-8 keep the selected slot / the extra. The main encoder the selected cell. UP/DOWN the row;
  LEFT/RIGHT the handle of the Electra's generic provider, or the previous / next control set of a
  model with control sets (zynthian). **Select + SONG cycles** through the available sources
  (off → the next available → … → off).
- **Phases:** P1 page_surface + `PageSource` + the new JAM layout for the Electra + model 1.2;
  P2 the protocol entries; P3 the host source; P4 the JAM with both sources and 8 columns; P5 the
  simulated host pages in the Python reference host (`dbm_host_alsa.py --host-pages`); P6 zynthian
  (the zynthian-ui session; hand-over in §12).

## D. Decisions (the owner, 2026-11-24)

1. No second link for the host: one instance, a `PageSource` trait with the link source and the host
   source; no `HostPageProvider`, no `"host"` link.
2. The JAM layout of §5 for both sources; the Electra moves to it in P1.
3. Model 1.2 as part of the page model, not only a link version.
4. **The selection is shared with zynthian and owned by the host.** zynthian's screen is the display
   for the names (the JAM grid alone is not useful without it). Page, control set / row and selected
   cell are host state in both directions: the JAM's selecting input becomes a command, the host
   applies it to its screen and reports back; changes on the host's screen (touch, its own encoders)
   are reported and the JAM follows. The core never changes the selection locally.
5. The zynthian side (P6) is done by the zynthian-ui session; the owner coordinates. zynthian-ui is
   not modified from here.

## 1. What exists

### 1.1 The surface link (DrivenByMoss)

- **Model:** fixed 6×6: `GRID_SIZE = 6`, `NUM_SLOTS = 36` [V src/framework/surface_link/model.rs:9-12].
  `Slot { kind, name, color, value 0..1, value_text, steps, on, enabled }` [V model.rs:103-117];
  kinds `Fader, List, Pad, Relative, Empty` [V model.rs:80-92]; `PageModel { page, pages,
  slots, rows_per_set, active_row, selected_slot, extras }` [V model.rs:143-157]. `diff` makes a
  snapshot when the page, page list, slot/extra count or row grouping change, else per-slot
  `Slot`/`Value` changes [V model.rs:213-245].
- **Input:** `SlotTurn{id,delta,fine}`, `SlotPress/Release/Touch`, `RowSelect`, `PageSelect/Next/
  Prev`, `SelectedTurn`, `SelectedReset`, `ValueNext/Prev`, `ExtraTurn` [V model.rs:264-283].
  "Deltas are steps of the provider: it applies its ranges and its acceleration."
- **Version** 1.1, same major = compatible [V model.rs:17-36].
- **Registry:** a process-wide static keyed by link name; one provider per link
  (`ProvideError::AlreadyProvided`), any number of surfaces; queues per direction, no calls into
  the other instance [V src/framework/surface_link/mod.rs:1-24, 125-138]. The crate's
  controllers use `DEFAULT_LINK = "pages"` [V mod.rs:40].
- **Provider helper:** `LinkProvider::new(link, name)`, `model_mut()`, `poll()`, `publish()`
  (snapshot to new surfaces, diffs otherwise) [V src/framework/surface_link/provider.rs:23-89].
- **Surface:** `SurfaceMapping` (pads, encoders, kind Relative/Absolute, main encoder, colors)
  [V src/framework/surface_link/client.rs:77-106]. The 6×6 is hard-wired in several places:
  `slots_at` loops over `GRID_SIZE` [V client.rs:108-116], the active row's slot id is
  `active_row * GRID_SIZE + column` [V client.rs:313, 342, 375], the "no provider" cross uses
  `id / 6`, `id % 6` [V src/framework/surface_link/view.rs:109]. `Capabilities.grid` is the whole
  pad grid (the JAM: 8×8) [V client.rs:179-187].
- **The JAM surface:** the mapping (6×6 at column 2, row selectors in column 1, page list in
  column 8, strips 1-6 the row, 7 the selected slot fine, 8 the extra, the main encoder
  two's complement) [V src/controller/maschine/jam/surface_link.rs:46-73]; it attaches to
  `DEFAULT_LINK` at start [V surface_link.rs:77]; enter/leave keep the previous view and mode
  [V surface_link.rs:124-152]; Select + SONG toggles it [V src/controller/maschine/jam/command/mod.rs:383-387];
  PAGE LEFT/RIGHT, the arrows and the encoder push go through `SurfaceOr`
  [V src/controller/maschine/jam/setup.rs:391-392, 490-493, 573-584].
- **The group buttons A-H** select tracks in every view (no surface check)
  [V src/controller/maschine/core/command/group_button.rs:73-125, jam/setup.rs:441-449]. In
  zynthian a track is a chain, so chain switching on the JAM works in the host-page mode unchanged.
- **The tick:** `App::tick` runs per instance: tasks, `handle_changes`, `flush`
  [V src/app.rs:664-684].

### 1.2 The typed protocol and the features

- Messages are a Rust table (`messages!` in `src/protocol/msg.rs`, `commands!` in
  `src/protocol/cmd.rs`); the generator writes `dbm_protocol.h`, `dbm_protocol.hpp`,
  `protocol.py`, `Protocol.java`, and a staleness test compares them
  [V doc/TYPED_PROTOCOL_PLAN.md §2.1]. IDs are never renumbered; one block of 0x100 per area.
  **Free blocks:** host → core 0x0A and 0x0C [V src/protocol/msg.rs:16-27]; core → host 0x8C
  [V src/protocol/cmd.rs:8-22].
- Every `Msg` has a text template (for logs, `.rec` recordings and the text hosts); every `Cmd`
  has a legacy `(processor, text)` form, so a new command needs a `Processor`
  [V cmd.rs:33-54, src/backend.rs:14-55].
- Record values: none, bool, i32, f64, text, color, struct, struct array, text list, feature set
  [V src/protocol/bin.rs:37-40]; up to four u16 indices per record [V TYPED_PROTOCOL_PLAN §2.2].
- **Command features:** `HostFeature` [V src/daw/context.rs:79-105]; a binary host has a command
  feature if its handled-commands bitset contains at least one of the feature's commands
  [V context.rs:148-164]; `host_supports` uses that bitset for binary hosts and `/host/features`
  for text hosts [V context.rs:210-220].
- Coverage checks: the Python reference host's `UNHANDLED_COMMANDS` set and `handled_commands()`
  [V bindings/python/dbm_host/commands.py:302, 400]; the Bitwig extension's exhaustive switch over
  `Protocol.Cmd` [V TYPED_PROTOCOL_PLAN §2.5, step 3].
- Subscriptions (core says what it needs, `SUBSCRIBE`) are planned but not built
  [V TYPED_PROTOCOL_PLAN §6.7; no `Subscribe` in cmd.rs]. `HOST_PAGE_SHOW` below is a small,
  specific instance of that idea.

### 1.3 The zynthian backend (DrivenByMoss side)

- One core instance per controller driver; all core calls on the driver's tick thread, 30 ms
  [V backends/zynthian/ctrldev/zynthian_ctrldev_base_dbm.py:28-33, 65].
- `ZynReporter`: change-only reports, two rates (fast pass every tick, full pass on signals, on
  commands and every 250 ms), subtrees `host, tracks, mixer, slots, transport, device, clip,
  browser` [V backends/zynthian/ctrldev/dbm_zynthian/reporter.py:25-50, 68, 75].
- `bridge.change_signals()`: chain manager, processor (bypass, `SS_PROCESSOR_CTRL_SCREENS`), mixer,
  sequencer, ZS3 signals; parameter values have no signal (the 250 ms pass) [V bridge.py:40-80].
- The cursor device today: processors as devices, a parameter page of 8 = two controller screens
  of 4, the selected page = `current_screen_index // 2` [V dbm_zynthian/device.py:26-41, 213-221].
  Stepped zctrls accumulate increments to whole steps (`remainders`, `MAX_STEP_DISTANCE`)
  [V device.py:54-71]. This is the value math the host page needs for strip input too.

### 1.4 zynthian-ui `electra-grid`: the control grid model

- `zyngui/zynthian_control_grid_data.py`, UI independent (no Qt, no Tk) [V :1-34]:
  - `GridPage(key, title, section, section_index, zctrls ≤ 4, empty)` [V :141-155]: a zynthian
    controller page = **4 cells in one row**;
  - `LAYOUTS`: `grid4x6` (4 columns, 6 rows, 1 page a row), `grid8x6` (8 columns, 6 rows, 2 pages a
    row) [V :41-46];
  - `ControlGrid`: `pages`, `layout`, window `first_row` (scrolls by rows to keep the selection in
    view), `selected` (a page), `cells` (row-major, id = row · columns + column) [V :202-213,
    290-349];
  - cell kinds `empty, float, integer, list, toggle, trigger, path` [V :49-71]; name, value text
    (`get_value2label`), fraction 0..1 (logarithmic as their MIDI mapping), list items, enabled
    (not read-only) [V :74-199];
  - events `structure`, `state`, `values` [V :217-230]; `ValueWatcher` compares values per
    consumer (never `zctrl.is_dirty`) [V :451-475];
  - input: `select_page/row/cell` (through `on_select`), `turn(cell, delta, fine)` =
    `zctrl.nudge`, `press` (toggle, trigger, next list item, file selector), `release` (nothing),
    `touch` (nothing), `set_default` [V :373-438];
  - **not thread safe**: "a provider on another thread needs a lock or the GUI thread"
    [V :32-33].
- **The pages are built in the Qt screen**, not in the shared module: `ChainControl._build`
  (chain zctrls by 4, then each processor's `get_ctrl_screens()`, an empty processor as an empty
  page) [V zynqt/screens/control.py:526-575] and `ControlGridModel.set_pages` (sections, section
  index) [V zynqt/screens/control_grid.py:166-190]. Section colours are QML theme tokens
  (`controlGridColors`) [V docs/qt-control-grid.md, "Looks per theme"]; the Python model has
  only the `section_index`.
- `zctrl.nudge(val, fine)`: ticks one index per step; floats by `nudge_factor` (range ≤ 2: 0.01,
  ≤ 20: 0.1, ≤ 250: 1, …), logarithmic in the log domain [V zyngine/zynthian_controller.py:316-345,
  400-433]. So one nudge is about 1/100-1/250 of a continuous range and **one whole item** of a list.
- `processor.set_current_screen_index` only stores the index, **no signal** [V
  zyngine/zynthian_processor.py:674-680]; the Qt `ChainControl` does not learn about a page
  change made elsewhere [A: no other path found in control.py].
- **Branch:** `electra-grid` is 6 commits on `main`; `dbm` (the DrivenByMoss drivers' branch) is
  not an ancestor of it [V `git merge-base --is-ancestor dbm electra-grid` → false].

### 1.5 Where zynthian's grid and the surface link differ

| | Surface link 1.1 | zynthian's grid |
| --- | --- | --- |
| size | 6×6 fixed | 4×6 or 8×6 (more rows scroll) |
| what the encoders turn | the active **row** (6) | the selected **page** (4 cells; in 8×6 half a row) |
| selection | one slot (the main encoder fine-tunes it) | one page; no selected cell |
| "page" | the whole 6×6 | 4 cells; the grid's window is 6 rows of pages |
| sections | none | chain / processor per page, coloured by section |
| value steps | the provider's, `fine` | `nudge`: one list item per step, floats ~1/100 |

These drive the 1.2 changes in §3.2.

## 2. Architecture options

| | (a) host provider in the core (recommended) | (b) dedicated JAM mode + own protocol | (c) reuse the parameter/device messages + layout hint |
| --- | --- | --- | --- |
| What the host reports | a page model: cells, sets, sections, pages, selection (new `Msg` 0x0C) | the same data, protocol shaped for the JAM | the cursor device's parameters (`ParamName/Value/ValueStr/Steps`, `PageName`) with a larger bank (48) and a "grid" hint |
| Surfaces | every surface-link surface (JAM now; Launchpad/Push/APC later; the Electra as a surface, P7) | the JAM only | every surface, if the core builds a provider from the device model |
| Hosts | any host which handles the 0x8C commands (zynthian, the reference host, Bitwig) | as (a) | every host incl. Reaper, no host change for 1 page |
| zynthian fit | full: whole chain (chain controllers + all processors), kinds, sections, its own layout and selection | full | one processor at a time; kinds only as `steps`; no sections, no per-cell colour, no control sets; zynthian pages ≠ 8-param bank pages in 4×6 |
| Value math | host (`nudge`, list steps), as the Electra's `14 53` | host | host (`/inc`), already done in device.py |
| Latency | ≤ 1-2 ticks per round trip (§3.6) | same | same |
| New code (rough) | core: surface link 1.2 ~350 lines, provider + model ~450, protocol table ~70, JAM ~150; Python reference host ~300; zynthian adapter ~400 + zynthian-ui ~150 | the host side of (a) plus a new JAM mode (view, mode, input, ~800 lines) duplicating the surface view | core ~300 (a device-model provider), protocol: bank size, a hint; nothing in the hosts for one page |
| Multiple providers | a second link (`"host"`), the surface picks | n/a | a third provider kind |

**Recommendation: (a).** It reuses the whole surface-link machinery (registry, queues, states,
snapshot/diff, the generic view and mode, the JAM's mapping and input wiring), fits zynthian's
model with small extensions, and keeps every surface and every host independent. (b) duplicates
the generic view for one controller. (c) is cheap but covers one device at a time and loses what
makes the grid useful (kinds, sections, the chain); it stays a possible later **"DAW device
provider"** in the core for hosts without host pages (Bitwig without P8, Reaper), built from the
existing model and not part of this plan.


The review (§D) kept (a) but replaced "a provider on a second link" by "a source in the same
instance": the multiple-providers row of the table becomes "the surface has several sources and
cycles through them".

## 3. The design

### 3.1 Overview

```
 host (zynthian, reference host)           core instance (e.g. the JAM driver's)
 ───────────────────────────────           ─────────────────────────────────────────────────
 reports /hostpage/... (change only) ──►   Parser::apply(Msg 0x0C) → model.host_page (HostPage)
                                             HostSource (PageSource) ── PageModel ──┐
 handles Cmd 0x8C                    ◄──     input → Cmd 0x8C (with the epoch)       │
   (value math, selection on its screen)                                             ▼
                                           SurfaceClient (the JAM's) ◄── LinkSource ◄── surface link
                                             view (grid), mode (strips)               (Electra in
                                                                                       another instance)
```

### 3.2 `framework::page_surface`

- `model.rs`: the page model (moved from `surface_link/model.rs`): `Rgb`, `SlotKind`, `Slot`,
  `Extra`, `ControlSet`, `Section`, `PageModel`, `ModelChange`, `Input`, `TurnUnit`, `ColorDepth`,
  `Capabilities`. `surface_link` re-exports them, so the Electra's code keeps its paths.
- `source.rs`: `LinkState` (Missing, Attached, Detached, Incompatible; moved), `SourceEvent`
  (`State`, `Model`) and the trait:

  ```rust
  pub trait PageSource {
      fn state(&self) -> LinkState;
      fn name(&self) -> &str;                       // for the notifications
      fn poll(&mut self, ctx: &Ctx) -> Vec<SourceEvent>; // at the controller's tick
      fn send(&mut self, input: Input);            // the surface's input
  }
  ```
- `client.rs`: `SurfaceClient` (was `SurfaceLinkClient`): the mapping, **several sources** (each
  with its own mirror of the model), the current one, the gestures. `select_source`,
  `next_available(after)`.
- `view.rs`: `SurfaceView`, `SurfaceMode` (were `SurfaceLinkView`/`Mode`), `palette_index`,
  `state_text`.
- `host.rs`: `HostSource`.
- `surface_link` keeps the registry, the provider helper and `LinkSource` (the `SurfaceHandle`
  as a `PageSource`).

### 3.3 The page model 1.2

- **Size:** `columns`, `rows` (default 6×6, at most 8×8). Slot id = row · columns + column.
- **Control sets:** `Slot.set: Option<usize>`, `PageModel.sets: Vec<ControlSet { name, section,
  color }>`, `active_set`. Without explicit sets (the Electra) nothing changes. The view draws the
  cells outside the active set one level lower (NI's roles: `Inactive`).
- **Sections:** `sections: Vec<Section { name, color }>`, `active_section`.
- **Changes:** a snapshot when the size, the sets or the sections change; new changes
  `ActiveSet`, `ActiveSection`.
- **Input:** `SlotTurn` and `SelectedTurn` get `unit: TurnUnit` (`Travel`: 1 = 1/127 of a strip's
  or fader's travel; `Detent`: one click of a relative encoder), set by the client from the encoder
  kind. New: `Shown { on }`, `SetSelect { set }`, `SetStep { delta }`, `SectionSelect { section }`.
  The Electra providers ignore the new input.
- **Capabilities:** `slot_area` (the cells a surface shows: the JAM 8×6) and `row_encoders`.
- The link's version goes to 1.2 (the model and the input changed).

### 3.4 The host source

- **The model** `src/daw/host_page.rs`: `HostPage { exists, title, epoch, columns, rows, pages,
  page, cells, sets, sections, row, set, section, selected_cell }`, set by `Parser::apply` from the
  0x0C messages (an exhaustive match, src/parser.rs:211). A change counter tells the source to
  rebuild.
- **Available** when the host supports `HostFeature::HostPages` (binary hosts: they handle the 0x8C
  commands; text hosts never). Availability must not depend on reports, since the host reports
  only while shown.
- **Poll:** when the host page changed, build a `PageModel` and diff it with the mirror (snapshot
  or changes); send the queued input as `Cmd`s.
- **Input → Cmd** (the selection is never applied locally, §D.4):

  | Input | Cmd |
  | --- | --- |
  | `Shown { on }` | `HostPageShow { columns, rows, encoders, rows_per_set } on` (the surface's shape) |
  | `SlotTurn` | `HostPageCellTurn { cell, epoch, unit, resolution } delta` |
  | `SelectedTurn` | the same for the selected cell |
  | `SlotPress` / `SlotRelease` | `HostPageCellPress { cell, epoch } true / false` |
  | `SlotTouch` | `HostPageCellTouch { cell, epoch } on` |
  | `SelectedReset` | `HostPageCellReset` of the selected cell |
  | `RowSelect` | `HostPageRow row` |
  | `PageSelect`, `PageNext`, `PagePrev` | `HostPagePage {end: first} page` (± 1 of the reported page, clamped) |
  | `PagePrevLast` (`<` from a window's first set) | `HostPagePage {end: last} page`: the previous window on its last page |
  | `SetSelect`, `SetStep` | `HostPageSet set` (± 1 of the reported set, clamped) |
  | `SectionSelect` | `HostPageSection section` |
  | `ValueNext/Prev`, `ExtraTurn` | not sent |

- **The epoch** (from the host, u16): the host increments it on every change of the cells'
  assignment (chain, pages, window, layout); cell commands carry it, the host drops those of
  another epoch (a strip turned just before a chain switch must not change the new chain's cell).

### 3.5 The protocol

**Host → core (`Msg`, block 0x0C)**, reported change-only, everything again (a dump) after
`HostPageShow` on and after `refresh`:

| ID | Name | Address | Value |
| --- | --- | --- | --- |
| 0x0C01 | `HostPageExists` | `/hostpage/exists` | bool (false: nothing to show) |
| 0x0C02 | `HostPageTitle` | `/hostpage/title` | text (the source's name: "zynthian · Synth") |
| 0x0C03 | `HostPageEpoch` | `/hostpage/epoch` | int |
| 0x0C04 | `HostPageColumns` | `/hostpage/columns` | int 1..8 |
| 0x0C05 | `HostPageRows` | `/hostpage/rows` | int 1..8 |
| 0x0C08 | `HostPageCount` | `/hostpage/page/count` | int |
| 0x0C09 | `HostPageSelected` | `/hostpage/page/selected` | int (the shown page / window) |
| 0x0C0A | `HostPageName` | `/hostpage/page/{page}/name` | text |
| 0x0C10 | `HostCellKind` | `/hostpage/cell/{cell}/kind` | int: 0 empty, 1 fader, 2 list, 3 toggle, 4 trigger, 5 relative, 6 action |
| 0x0C11 | `HostCellName` | `/hostpage/cell/{cell}/name` | text |
| 0x0C12 | `HostCellColor` | `/hostpage/cell/{cell}/color` | color |
| 0x0C13 | `HostCellValue` | `/hostpage/cell/{cell}/value` | f64 0..1 |
| 0x0C14 | `HostCellText` | `/hostpage/cell/{cell}/text` | text |
| 0x0C15 | `HostCellSteps` | `/hostpage/cell/{cell}/steps` | int (list items) |
| 0x0C16 | `HostCellOn` | `/hostpage/cell/{cell}/on` | bool |
| 0x0C17 | `HostCellEnabled` | `/hostpage/cell/{cell}/enabled` | bool |
| 0x0C18 | `HostCellSet` | `/hostpage/cell/{cell}/set` | int (-1 none) |
| 0x0C20 | `HostSetCount` | `/hostpage/set/count` | int |
| 0x0C21 | `HostSetName` | `/hostpage/set/{set}/name` | text |
| 0x0C22 | `HostSetSection` | `/hostpage/set/{set}/section` | int |
| 0x0C23 | `HostSetColor` | `/hostpage/set/{set}/color` | color |
| 0x0C24 | `HostSetSelected` | `/hostpage/set/selected` | int (-1 none) |
| 0x0C30 | `HostSectionCount` | `/hostpage/section/count` | int |
| 0x0C31 | `HostSectionName` | `/hostpage/section/{section}/name` | text |
| 0x0C32 | `HostSectionColor` | `/hostpage/section/{section}/color` | color |
| 0x0C33 | `HostSectionSelected` | `/hostpage/section/selected` | int |
| 0x0C40 | `HostPageRow` | `/hostpage/row` | int (the active row) |
| 0x0C41 | `HostCellSelected` | `/hostpage/cell/selected` | int (-1 none) |

Cell, set and section indices are positions in the shown page (cell = row · columns + column).

**Core → host (`Cmd`, block 0x8C)**, processor `hostpage`, feature `hostPages`:

| ID | Name | Text form | Value |
| --- | --- | --- | --- |
| 0x8C01 | `HostPageShow` | `hostpage show/{columns}/{rows}/{encoders}/{rows_per_set}` | bool |
| 0x8C02 | `HostPageCellTurn` | `hostpage cell/{cell}/turn/{unit}/{resolution}/{epoch}` | int delta |
| 0x8C03 | `HostPageCellPress` | `hostpage cell/{cell}/press/{epoch}` | bool |
| 0x8C04 | `HostPageCellTouch` | `hostpage cell/{cell}/touch/{epoch}` | bool |
| 0x8C05 | `HostPageCellReset` | `hostpage cell/{cell}/reset/{epoch}` | – |
| 0x8C06 | `HostPageRow` | `hostpage row` | int |
| 0x8C07 | `HostPagePage` | `hostpage page` | int |
| 0x8C08 | `HostPageSet` | `hostpage set` | int |
| 0x8C09 | `HostPageSection` | `hostpage section` | int |

`unit`: `travel` | `detent`; `resolution`: `normal` | `fine`.

**Value math in the host:** `detent` → one encoder step (zynthian: `zctrl.nudge(delta, fine)`, a
list one item per click); `travel` → the normalized value moves by delta / 127 (fine: / 10), so a
swipe across a strip is the whole range; stepped values accumulate and step when they cross
1 / steps. No acceleration in the core.

**Features and versions:** `HostFeature::HostPages` (`hostPages`) is a command feature: a binary
host has it when its handled-commands bitset contains the 0x8C commands. A text host (Reaper) does
not list it in `/host/features`, so the core never creates a host source for it and never sends the
commands; it never reports `/hostpage/...`. The protocol table's hash changes; the generated
bindings are regenerated (the staleness test). Bitwig and the zynthian adapter list the commands as
unhandled until they implement them.

### 3.6 Latency

Input during a MIDI event → queued in the source → sent at the controller's tick (the same tick) →
the host executes it in the command callback → its next report (zynthian: the fast pass) → the
parser → the source's next poll. About 1-2 ticks (30-60 ms), as the Electra path.

### 3.7 Layout

The host lays out (zynthian has the layouts and knows its pages); `HostPageShow` carries the
surface's shape (the JAM: 8 × 6, 8 encoders a row, 1 row per set). zynthian answers with
`grid8x6`; a 6-column surface would get `grid4x6`, an Electra-shaped one (12 knobs in 2 rows) a
later 2×2-block 6×6 (`docs/lv2-to-electra.md:101-108`).

## 4. The zynthian side

Done by the zynthian-ui session (§D.5); the exact contract is the hand-over in §12. In short:

- **zynthian-ui** needs a shared page builder (today in the Qt screen: zynqt/screens/control.py:526-575,
  control_grid.py:166-190), section colours in Python, and the **shared selection**: a way for a
  driver to set the screen's page, control set (zynthian page) and selected cell, and a change
  signal for each (`set_current_screen_index` sends none, zynthian_processor.py:674-680). A branch
  with `electra-grid` and `dbm` together.
- **The adapter** (`dbm_zynthian/host_page.py`): a `ControlGrid` of its own on the driver's tick
  thread (the grid is not thread safe) whose selection follows the screen's (by the signals) and
  which sets the screen's (by the setter, on the GUI thread); a reporter subtree `page` (structure in
  the full pass, values of the shown cells in the fast pass, only while shown); typed handlers for
  0x8C.
- **CPU:** nothing while not shown; while shown ≤ 48 value compares per fast pass (the Qt grid
  measured 5-7 µs per poll) and a rebuild on structural signals.

## 5. The JAM

**One layout for both sources** (the Electra's pages are 6 wide):

| JAM | Function |
| --- | --- |
| grid row 1, pads 1-6 | the page list (a window of 6 pages around the current one), the current page orange |
| grid row 1, pads 7-8 | the previous / next page |
| grid rows 2-7 | the cells, left-aligned: 8 columns (zynthian `grid8x6`) or 6 (the Electra; columns 7-8 empty). A fader or list cell press selects it and its row (the source decides), a pad cell switches |
| grid row 8 | the sections (zynthian: Chain, the processors) in their colours, the active one full; a press selects the section. Nothing for the Electra |
| cell colours | the slot's colour; the selected cell pale and blinking; with control sets the cells outside the active set one level lower |
| strips 1-8 | the active row's cells; with a 6-wide page strip 7 the selected slot (fine) and strip 8 the extra, as before |
| main encoder / push | the selected cell (Shift fine) / its reset |
| UP / DOWN | the previous / next row |
| LEFT / RIGHT | the previous / next control set when the model has sets (zynthian); else the previous / next value of the selected slot (the Electra generic provider's envelope handle). No conflict: a model has one or the other |
| PAGE LEFT / RIGHT | the previous / next page |
| CONTROL | with host pages: device parameters → the host page strips (§5.1) → device parameters |
| group buttons A-H | the tracks (zynthian: the chains), unchanged |
| Select + SONG | off → the next available source → … → off (sources in the order: Electra pages, host pages; missing ones are skipped; with one source the toggle of before) |

There is no row-selector column any more; rows are selected by a cell press or UP/DOWN. Entering a
source sends `Shown { on: true }` to it, leaving or switching away `Shown { on: false }`.

### 5.1 The host page strips (CONTROL)

The host's active row on the **8 touch strips only**, the grid keeps its view (step sequencer,
pads, drums, session, …) and works as before (`Modes::HostStrips`, a `PageSurfaceMode` on the
JAM's client, in `src/controller/maschine/jam/surface_link.rs`).

| JAM | Function |
| --- | --- |
| CONTROL | with the host's pages (`hostPages`): device parameters → host page strips → device parameters (the strips take the user mode's slot; zynthian has no user parameters). Without them (Bitwig, a stock zynthian) device parameters ↔ user mode, as in Java. From another mode the last of the two (Java's `ModeMultiSelectCommand`) |
| Select + CONTROL | with the host's pages: as Select + SONG (the full surface; may get its own function later); without them the user mode's project / track toggle, as before |
| Shift + CONTROL | with the host's pages: the host page strips on the first remote control page (`HostPageSection 0`, zynthian's "Chain Controllers"; notification "Remote Controls"); without them the device window, as before |
| strips 1-8 | the 8 cells of the host's active row (zynthian grid8x6: two zynthian pages of 4), values and colours on the LEDs; a swipe is `HostPageCellTurn` by the travel, the touch `HostPageCellTouch`. Turning never selects: the selection stays the host's |
| Select + strip n | `HostPageCellAssign {cell, slot: n}` (n = 0..7; the host maps 4-7 onto the next remote page) |
| PAGE LEFT / RIGHT (`<` `>`) | the previous / next control set (zynthian's page, `HostPageSet`); at the first / last set of the page the previous / next page (`HostPagePage`), so `<` `>` walk through all of a chain's pages. zynthian's selection indicator follows the host's report |
| Select + PAGE LEFT / RIGHT | the previous / next row (`HostPageRow`) |
| Shift + PAGE LEFT / RIGHT | metronome / loop, as before |
| arrows, main encoder, grid, scene buttons | the grid's view and the encoder modes, unchanged |
| Select + SONG | the full surface, with the host source first, then the next sources (the Electra), then off; off returns to the strips. CONTROL on the full surface leaves it for the strips too |

**Why not the arrows:** every grid view of the JAM uses them (play, chords, drum and sequencer
views: the octave; session: scrolling; Shift view: bank pages), so on the strips they stay with
the view. PAGE LEFT / RIGHT belong to the mode (the device parameter pages in the device mode),
which the strips replace. In the device parameter mode the page buttons send
`DeviceParameterPage {dev: Device, page}` (0x851D, absolute), as before.

**Shown:** one `HostPageShow` on while the full surface shows the host source or the strips are
active, one off when neither does (`surface_link::sync`, at every transition and tick). Switching
between the strips and the full surface on the host source sends nothing; the full surface on the
Electra sends the host's off. A track change keeps the strips (the mode is not touched; the grid's
view follows the track as before). Without the host's pages the strips fall back to the device
parameters.

## 6. Relation to the Electra path (brainstorm A/B/C)

- This plan is the DrivenByMoss half of **path B** (and of C's UI part): zynthian owns the model
  and the UI; controllers attach through DrivenByMoss. It answers the brainstorm's questions for
  DrivenByMoss (§4.3): the transport is **the existing host bridge** of the zynthian driver (no
  OSC/socket, no in-process registry access from Python); the home is the **DBM zynthian adapter**
  (`dbm_zynthian`), fed by the shared grid model; **the host owns** acceleration and fine steps.
- **Coexistence:** the Electra controller provides on the surface link (its DrivenByMoss pages, or
  the generic provider for another preset) and reaches the JAM through the link source; the host's
  pages reach it through the host source of its own instance. The JAM cycles with Select + SONG (§5).
- **The Electra as a surface of the host's pages (P7, optional):** an Electra mode in DrivenByMoss
  which uses a host source (in its own instance) as a surface (capabilities 6×6, 6 encoders a row, rows per set 2) and
  renders the cells into the DrivenByMoss preset's 36 controls with what the Electra provider
  already sends (names `14 07`, values as CC feedback, texts `14 0E`, rows/selection §6b), its
  knobs' relative CCs → `SlotTurn` (`Detent`). Works on a real Electra and on the emulator, without
  firmware work (no Z1). The preset's fixed control kinds (mostly faders) limit lists and toggles to
  the value text and a press. One Electra instance can be a provider on the link and show host pages; only one is shown on
  the device at a time.
- **Path C** (the emulator as a headless source runtime) is independent: its sources' pages could
  become a section of zynthian's grid (zynthian forwards to the emulator over §6c), invisible to
  this protocol.

## 7. Bitwig (P8, optional)

The extension creates per-page remote-control objects already: project pages, track pages and
device pages (`createCursorRemoteControlsPage("DBM page i", 8, "")`)
[V backends/bitwig/src/main/java/de/mossgrabers/dbm/bitwig/BitwigModel.java:209-215, 226-230,
259-261]. A Bitwig host page: 8 columns, rows = remote-control pages (8 parameters each, one set a
row), sections "Project", "Track", the cursor device; values from those objects, `CellTurn` →
`inc` with Bitwig's own resolution. Bitwig's observers must exist from init, so the number of
pages comes from the model setup (as now) [A: enough pages are created for 6 rows when a host-page
surface exists; to check].


## 8. Testing without hardware

**Isolation:** the owner runs their own dbm_host, simulators, emulator and zynthian. Every run uses
a unique name suffix (`" (hp-<pid>)"`) and selects ALSA clients by PID, never by name alone
(`find_port` matches substrings, CONTROLLER_SIMULATORS.md §3).

1. **Rust:** the page_surface unit tests (model 1.2, client and view with 6- and 8-wide models,
   sources, cycling), `tests/surface_link.rs` (the Electra with the JAM: the exact bytes; only the
   JAM's pad positions follow the new layout), a new `tests/host_page.rs` (the JAM with host pages
   fed as reports, the input as typed `Cmd`s, the selection reported back, epochs).
2. **The Python reference host:** `dbm_host.host_page` (`SimHostPages`: chains with processors and
   grid8x6 pages, lists, toggles, value texts, section colours, and a **simulated screen selection**:
   the JAM's input moves it, `select_on_screen` moves it as zynthian's own encoders would);
   `dbm_host_alsa.py --host-pages` with the JAM simulator, and with `--model electra --model jam`
   and the emulator (both sources).
3. **zynthian:** the zynthian-ui session's tests, the DBM driver tests (`backends/zynthian/tests`).

## 9. Phases

| Phase | Content | Acceptance |
| --- | --- | --- |
| **P1** | `framework::page_surface` (model, `PageSource`, client, view), `LinkSource`; model 1.2 (sizes, sets, sections, units, `Shown`); the JAM's new layout for the Electra | all tests green; the Electra's bytes in `tests/surface_link.rs` unchanged |
| **P2** | `Msg` 0x0C, `Cmd` 0x8C, `TurnUnit`/`Resolution` domains, `Processor::HostPage`, `HostFeature::HostPages`; the generated files; unhandled in Bitwig and the zynthian adapter | `cargo test` (round trip, staleness); Maven builds; conformance unchanged |
| **P3** | `daw::host_page`, `Parser::apply`, `HostSource` | unit tests: reports → model, input → `Cmd`s, epochs, no local selection |
| **P4** | The JAM with both sources (Select + SONG cycling) and 8 columns | `tests/host_page.rs` |
| **P5** | The simulated host pages in the reference host, `dbm_host_alsa.py --host-pages` | Python tests; end-to-end runs with the JAM simulator (screenshots), with the emulator for the Electra |
| P6 | zynthian (the zynthian-ui session; §12) | its tests; a Qt desktop run |
| P7, P8 (opt.) | the Electra as a surface of host pages; Bitwig host pages | |

## 10. Open questions

1. zynthian: whether the JAM always asks for `grid8x6` or follows the screen's layout; zynthian
   answers `HostPageShow` with the layout it shows (§12).
2. When several surfaces show host pages with different shapes: the last shown wins (rare).
3. The Electra as a surface of host pages (P7) and Bitwig (P8): wanted, and when?

## 11. Risks

- **Threads in zynthian:** the grid model is GUI-thread only; the adapter's own instance on the
  tick thread avoids sharing it, but both write the same zctrls (as the drivers do today). The shared
  selection crosses threads: use queued signals, never call the Qt model from the tick thread.
- **Selection ping-pong:** avoided by design: the host owns the selection (§D.4), the core only shows
  what the host reports.
- **Layering:** `zyngine/ctrldev` importing `zyngui.zynthian_control_grid_data`; the adapter must
  work without it (no feature declared).
- **Branch drift:** `electra-grid` (on `main`) and `dbm` diverge until combined.
- **Surface link regressions:** the size generalization touches the Electra path; the existing
  `tests/surface_link.rs` (exact bytes) guard it.
- **Epoch races:** without the epoch a strip turn could hit another chain's cell after a switch.
- **Two zynthian drivers** each have their own host source; each shows the pages to its own
  controller (the screen's selection is shared by both).
- **The JAM palette:** neighbouring sections' hues may map to the same NI hue; pick the section
  palette from distinct NI hues (SURFACE_LINK §5, colour roles).
- **CPU** if the page values ran in the slow pass only (laggy) or the structure in the fast pass
  (costly): keep the split of §4.2 and measure.

## 12. Hand-over for the zynthian-ui session (P6)

What the zynthian DrivenByMoss adapter (`backends/zynthian/ctrldev/dbm_zynthian/`, in the
drivenbymoss-rust repository) must report and handle so that a controller (the JAM) shows
zynthian's control grid. The reference is the simulated host in
`bindings/python/dbm_host/host_page.py` (P5): the same reports and the same command handling, with
a simulated screen in place of zynthian's.

### 12.1 What zynthian-ui must provide

1. **Pages without Qt:** the page list of a chain (chain controllers by 4, then each processor's
   controller screens, an empty processor as an empty page; titles, sections) as a function of the
   shared `zynthian_control_grid_data`, used by the Qt screen and the adapter.
2. **Section colours** as RGB in Python (per section index, as the theme's `controlGridColors`).
3. **The shared selection** (§D.4), callable from a driver's thread (queued to the GUI thread):
   - set the screen's **page / window** (the 6 rows shown), the **control set** (zynthian's selected
     page) and the **selected cell** (a controller of that page);
   - a **change signal for each** (`zynsigman`), sent for every change, whatever its origin (touch,
     zynthian's encoders, the page list, a driver), with the new value. `set_current_screen_index`
     sends none today (zyngine/zynthian_processor.py:674-680).
   - The screen needs a notion of the **selected cell** (today it has none: the four encoders turn
     the selected page). Suggestion: the last touched or turned controller of the selected page,
     else its first.
4. **`turn_travel(cell, delta, fine)`** next to `turn` (or in the adapter): the normalized value
   moves by delta / 127 (fine / 10); lists, integers, toggles accumulate and step at 1 / steps.
5. A branch with `electra-grid` and `dbm` together.

### 12.2 What the adapter reports (host → core, change-only)

Only after `HostPageShow` on (and until off); everything again after `HostPageShow` on and after
`refresh`. Reports of one change go in one report (one buffer), the epoch first.

| Address | Value | zynthian |
| --- | --- | --- |
| `/hostpage/exists` | bool | a chain is shown |
| `/hostpage/title` | text | "zynthian · <chain name>" |
| `/hostpage/epoch` | int | +1 on every change of which controller a cell holds (chain, processors, screens, window, layout) |
| `/hostpage/columns`, `/hostpage/rows` | int | the layout chosen for the shape of `HostPageShow` (`grid8x6`: 8, 6) |
| `/hostpage/page/count`, `/hostpage/page/{p}/name`, `/hostpage/page/selected` | int, text, int | the windows of 6 grid rows; name: the first page's section and title |
| `/hostpage/cell/{c}/kind` | int | `empty` 0, `float`/`integer` 1, `list` 2, `toggle` 3, `trigger` 4, `path` 6 |
| `/hostpage/cell/{c}/name`, `/text`, `/value` | text, text, f64 | `cell_name`, `cell_value_text`, `cell_fraction` |
| `/hostpage/cell/{c}/color` | color | the section colour |
| `/hostpage/cell/{c}/steps` | int | the number of values of a stepped cell: list items, 2 for a toggle, max − min + 1 for an integer (0 otherwise), as the reference host |
| `/hostpage/cell/{c}/on` | bool | a toggle: value ≠ min |
| `/hostpage/cell/{c}/enabled` | bool | `GridCell.enabled` |
| `/hostpage/cell/{c}/set` | int | the index of the cell's zynthian page among the shown ones (-1 none) |
| `/hostpage/set/count`, `/hostpage/set/{s}/name`, `/section`, `/color` | int, text, int, color | the shown zynthian pages: title, section index, section colour |
| `/hostpage/set/selected` | int | the screen's selected page (among the shown; -1 if not shown) |
| `/hostpage/section/count`, `/hostpage/section/{s}/name`, `/color`, `/hostpage/section/selected` | | "Chain", the processors; the selected page's section |
| `/hostpage/row` | int | the row of the selected page |
| `/hostpage/cell/selected` | int | the screen's selected cell (-1 none) |

Values and texts of the shown cells belong in the **fast** pass (a turn shows within a tick); the
structure in the full pass and on the structural signals.

### 12.3 What the adapter handles (core → host)

All with typed handlers; a cell command whose epoch is not the current one is dropped. Every
selecting command changes the **screen's** selection (12.1 item 3), and the adapter reports the
resulting state — the core never shows a selection which the host did not report.

| Command | Action |
| --- | --- |
| `HostPageShow {columns, rows, encoders, rows_per_set} on/off` | on: pick the layout, start the `page` subtree, dump; off: stop it |
| `HostPageCellTurn {cell, epoch, unit, resolution} delta` | `detent`: `grid.turn(cell, delta, fine)`; `travel`: `turn_travel` |
| `HostPageCellPress {cell, epoch} true` | `grid.press`: a toggle switches, a trigger fires, a list steps, a path opens the file selector; a continuous cell is selected (its page and the cell, on the screen) |
| `HostPageCellPress … false` | `grid.release` |
| `HostPageCellTouch {cell, epoch} on` | nothing (later: the page's widget, `gridWidgetPoke`) |
| `HostPageCellReset {cell, epoch}` | `grid.set_default` |
| `HostPageRow row` | select the page in that row (8×6: the same half as before) |
| `HostPagePage {end} page` | show the window `page`: select its first page (`end` = `first`), or its last one (`last`: `<` from a window's first set pages back through the sets). Text form `hostpage page/first <page>`, `hostpage page/last <page>` |
| `HostPageSet set` | select that page |
| `HostPageSection section` | select the section's first page |
| `HostPageCellAssign {cell, epoch} slot` | (0x8C0A, zynthian-ui remote control pages) put the controller of `cell` into slot `slot` (0..7) of the chain's current remote page; an empty cell clears the slot; `slot` -1 removes the controller from every remote page. The JAM sends it for Select + a strip touch: the active row's cell n → slot n |

Declare the 0x8C commands handled (the handled-commands bitset): that is the `hostPages` feature;
without the grid module (a stock zynthian) leave them undeclared, and the JAM has no host source.


### 12.4 Chain controller pages (remote-control pages; 2026-11-25, protocol agreed-pending with the core)

zynthian-ui `electra-grid` has named pages of chain controllers per chain (docs/remote-control-pages.md
there; upstream's "Chain Controllers" with positions and names, since 2026-11-25 **pages of 4
slots**, the size of a zynthian page), each slot any controller of the chain. They are the first
section of `chain_pages` ("Chain", one zynthian page per chain controller page: grid8x6 two a
row), so the adapter reports them as any other pages, no new reports. Their section has a fixed
colour of its own (zynthian-ui's `page_color`; the processors keep their colours), sent through
the existing colour fields (section, set, cell): nothing new on the wire. Proposed command (the
core's and the JAM's side: the dbm-surface-link session):

| ID | Name | Text form | Value |
| --- | --- | --- | --- |
| 0x8C0A | `HostPageCellAssign` | `hostpage cell/{cell}/assign/{epoch}` | int slot |

slot 0-7 (the JAM's strips 1-8): the cell's controller to slot `n % 4` of chain controller page
`current + n // 4` (current: the page last selected on the screen or by a controller, default the
first): strips 1-4 the current page, 5-8 the next one; the pages up to it are created (named
"Controllers n"). An empty cell clears the slot (and creates nothing); a controller already on
that page moves (swaps). slot -1 removes the controller from all pages (an emptied default-named
last page goes); another epoch: dropped. The JAM: hold a pad of a cell row and touch strip n =
that row's n-th cell to slot n. The adapter handles it (`ZynHostPages.assign`, through
`shared_selection.request_remote_assign` in zynthian's GUI thread, which maps the slot;
`SS_GUI_CONTROL_GRID_REMOTE` brings the new pages) once `Cmd.HOST_PAGE_CELL_ASSIGN` exists
(`commands.HOST_PAGE_CELL_ASSIGN`, guarded).

### 12.5 zynthian's screen follows the surface (2026-11-25)

No protocol change. `HostPageShow` on (the JAM's CONTROL strips, Shift + CONTROL, Select + SONG /
CONTROL) opens chain control of the active chain on zynthian's screen if it isn't shown
(`shared_selection.request_show`, applied in the GUI thread; not over a menu, a dialog, the
keyboard, the file selector, a move or the busy screen), on the page the surface shows (its
selection) when it wasn't shown. The navigation commands (`HostPageSection` / `Set` / `Page` /
`Row`) and a cell press bring it to the front again if the user went elsewhere on the screen;
turns and `HostPageShow` off don't change the screen. Device pages (`DeviceParameterPage`, the
JAM's `<` `>` in CONTROL) move chain control's selection while it is shown.

## 13. Implementation status (2026-11-24)

| Phase | Done | Where |
| --- | --- | --- |
| P1 | `framework::page_surface` (model 1.2, `PageSource`, `SurfaceClient` with several sources, `PageSurfaceView`/`Mode`), `surface_link::LinkSource`, the JAM's new layout for the Electra | `src/framework/page_surface/`, `src/framework/surface_link/source.rs`, `src/controller/maschine/jam/surface_link.rs`; `tests/surface_link.rs` (the Electra's bytes unchanged, the JAM's pad positions moved) |
| P2 | `Msg` 0x0C (29 reports), `Cmd` 0x8C (9 commands), the domains `TurnUnit`, `Resolution`, `Processor::HostPage`, `HostFeature::HostPages`; `daw::host_page::HostPage` and `Parser::apply`; the generated bindings; unhandled in Bitwig, the reference host (without host pages) and the zynthian adapter | `src/protocol/msg.rs`, `cmd.rs`, `types.rs`, `src/daw/host_page.rs`, `src/parser.rs`; HOST_BRIDGE_GUIDE.md §3.10, §4.12 |
| P3 | `HostSource`: available with `hostPages`, the host page → page model, input → commands (epochs, steps from the reported selection, no local selection) | `src/framework/page_surface/host.rs` |
| P4 | The JAM's sources: the surface link, then the host's pages; Select + SONG cycles | `src/controller/maschine/jam/surface_link.rs`, `tests/host_page.rs` |
| P5 | The simulated host pages (chains per track, sections, grid8x6 / grid4x6, a screen), `dbm_host_alsa.py --host-pages` (SIGUSR1 / SIGUSR2 move the screen's selection) | `bindings/python/dbm_host/host_page.py`, `bindings/python/tests/test_host_page.py`, `support/push_simulator/tools/dbm_host_alsa.py` |
| JAM strips | The host page strips (§5.1): CONTROL's second slot with host pages, the host's active row on the strips with the grid in its view; `<` `>` sets / pages, Select + `<` `>` rows, Select + strip assign; one `HostPageShow` across the strips and the full surface | `src/controller/maschine/jam/surface_link.rs`, `command/mod.rs`, `Modes::HostStrips`, `SurfaceClient::step_set_or_page`; `tests/host_page.rs`, `tests/surface_link.rs` (`host_page_strips_with_the_electra`) |
| P6 | zynthian: the shared page list, section colours, the shared selection (page, cell, window, zynsigman signals), `turn_travel` in zynthian-ui `electra-grid` (`zyngui/zynthian_control_grid_data.py`, `zynqt/screens/control.py`, docs/qt-control-grid.md); the adapter's `ZynHostPages` (its own paged `ControlGrid` on the tick thread, reports full / fast, typed 0x8C handlers with the epoch, declared only with the grid module) and `ZYNTHIAN_DBM_DEVICE_<MODEL>` | `backends/zynthian/ctrldev/dbm_zynthian/host_page.py`, `backends/zynthian/tests/test_host_page.py` |

**Chain types (2026-11-25, zynthian in the QEMU VM, the adapter's reports and the JAM in the
core in-process):** every chain type has its pages, the same as the screen's: instrument, main
(the `MR` mixer strip: level, toggles, signal, global, aux, recorder), mixbus, Pure Data and the
audio player (an empty page for the processor without patch / file, then the mixer strip with its
sends), audio input, clippy; a MIDI-only or an empty chain has none (`exists` with no pages, the
screen has none either). Selection, presses and travel work on all of them. Fixed then: the audio
player without a file couldn't be selected (an exception in zynthian-ui's waveform view); a press
on an empty page is the screen's tap on its header (`request_open`: the selected one opens its
banks / presets); an exception reading a chain's pages reports `exists` false (logged once), a
failing host page command is logged. **Open (the core / the JAM):** every change of the selected
track (`TrackSelectionChanged`, also from zynthian's screen) makes the JAM leave the host pages:
`jam/setup.rs` `handle_track_change` → `ctx.recall_preferred_view()` activates the track's
preferred view (Play), and `surface_link::poll` then leaves the surface for it. A switch to a
mixbus (not in the track bank) keeps it.

**Try it** (isolated: own suffix, clients by PID):

```sh
cargo build
support/controller_simulator/run.sh --model jam --suffix " (hp)" &
support/controller_simulator/tools/dbm_host_alsa.py --model jam --host-pages --suffix " (hp)" \
    --library target/debug/libdrivenbymoss.so
# Select + SONG on the simulator; kill -USR1 <the runner's pid> moves the host's screen
```

With the Electra emulator as well (`--model electra --model jam --electra-client <n>`), Select +
SONG goes Electra pages → host pages → off.
