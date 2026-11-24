# Host-provided parameter pages on the surface link: plan

Status: plan (2026-11-24), nothing built. Goal (from the owner): a mode for the Maschine JAM which
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

## 0. Summary

- **Recommendation: option (a), a host provider in the core.** The host reports a page model with
  new typed `Msg`s (block 0x0C); a `HostPageProvider` in the core instance turns it into the
  surface link's `PageModel` and registers it on a second link, `"host"`. The generic
  `SurfaceLinkView`/`SurfaceLinkMode` and the JAM's surface stay the same code; the surface's
  input becomes typed `Cmd`s (block 0x8C) to the host. **The host does all the value math**
  (zynthian: `zctrl.nudge`, list steps, toggles), as the Electra firmware does for `14 53`.
- **The surface link grows to 1.2** to fit zynthian's grid: a model size up to 8×8 instead of
  fixed 6×6, explicit **control sets** (a zynthian page = the 4 cells its encoders turn), an
  optional section list, a `Shown` input (the surface displays the provider: the host reports only
  then), and a turn unit (a strip's travel vs an encoder's detent). The Electra providers keep
  working unchanged (their sets derive from `rows_per_set`).
- **The host owns the layout.** When a surface shows the host's pages, the core sends
  `HOST_PAGE_SHOW` with the surface's shape (columns, rows, encoders per row, rows per set); the
  host picks its layout (zynthian: `grid8x6` for the JAM, `grid4x6` or a 2×2-block 6×6 for a
  6-column surface) and reports it.
- **The JAM: the same surface mode, another provider.** Select + SONG cycles through the attached
  providers (off → Electra pages → host pages → off, skipping missing ones; with one provider it
  behaves as today). For an 8-column model the JAM uses all 8 columns: rows 2-7 the cells, row 1
  the page list (windows), row 8 the sections (processors), strips 1-8 the active row, LEFT/RIGHT
  the previous/next control set; the group buttons A-H keep selecting tracks (= chains).
- **zynthian:** a `host_page.py` collector/handler in `dbm_zynthian` with its own `ControlGrid`
  instance (the grid model is UI independent but not thread safe), plus three things zynthian-ui
  lacks: a shared page builder (today in the Qt screen), section colours in Python, and a signal
  for the selected page. The `electra-grid` branch is based on `main`, not on `dbm`, so the
  DrivenByMoss drivers cannot run with it yet.
- **Cost on zynthian:** zero while no surface shows the pages (subscription); while shown about 48
  value compares per fast pass (the Qt grid measured 5-7 µs per poll) and a page rebuild on
  structural signals.
- **Phases:** P1 surface link 1.2 → P2 protocol entries → P3 the core provider → P4 the JAM →
  P5 the Python reference host's simulated pages (testable end to end with the JAM simulator) →
  P6 zynthian → optional P7 the Electra as a surface of the host's pages, P8 Bitwig.

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

## 3. The design

### 3.1 Overview

```
 zynthian (host)                         core instance (e.g. the JAM driver's)            surface
 ─────────────────                       ──────────────────────────────────────           ───────
 dbm_zynthian/host_page.py               Parser::apply(Msg 0x0C) → model.host_page
   own ControlGrid + ValueWatcher   ──►  HostPageProvider (App::tick)
   reports /hostpage/... (change only)     HostPage → PageModel, LinkProvider("host")  ──► SurfaceLinkClient
                                           publish() snapshot / diffs                       (JAM: Select+SONG)
   ZynCommandHandler 0x8C           ◄──    poll() inputs → Cmd 0x8C (with the epoch)   ◄── Input (1.2)
     grid.turn / press / select ...        Shown → HOST_PAGE_SHOW {shape} on/off
```

The provider and the surface may be in different core instances of one process (the registry
connects them, SURFACE_LINK §4): e.g. the Mk2 driver's instance provides, the JAM's shows.

### 3.2 Surface link 1.2 [P]

Additions only; the Electra providers and their tests stay as they are.

- **Size:** `PageModel.columns`, `rows` (default 6, 6; at most 8, 8). Slot id = row · columns +
  column. `diff` makes a snapshot when they change. Every `GRID_SIZE` use in client.rs and view.rs
  becomes the model's size (the places in §1.1).
- **Control sets** (what the encoders of a surface turn together; Electra: a control set of 2 rows;
  zynthian: a page of 4 cells):
  - `Slot.set: Option<u16>` and `PageModel.sets: Vec<ControlSet { name, section: Option<u16>,
    color }>`, `active_set: Option<usize>`;
  - without explicit sets (the Electra providers) the sets derive from `rows_per_set` as today, and
    nothing changes for them;
  - the view draws the cells of the active set at full level and the other cells one level lower
    on palettes with levels (NI's roles) — only for models with explicit sets.
- **Sections** (optional): `PageModel.sections: Vec<Section { name, color }>`, `active_section`
  (zynthian: "Chain", each processor).
- **Input:**
  - `Shown { on }`: the surface displays this provider (the JAM's enter/leave). The provider asks
    its host for the pages only while one surface shows them.
  - `SlotTurn { id, delta, fine, unit }`, `SelectedTurn { delta, fine, unit }` with
    `TurnUnit::{Travel, Detent}`: `Travel` = an absolute control (strip, fader: 1 = 1/127 of its
    travel), `Detent` = one click of a relative encoder (the JAM's main encoder). The client sets it
    from the encoder's `EncoderKind` (client.rs:46). The Electra providers ignore it (or use it for
    their open list problem, SURFACE_LINK §9).
  - `SetSelect { set }`, `SetStep { delta }`, `SectionSelect { section }`.
- **Capabilities:** add `slot_area: (columns, rows)` per supported layout (the JAM: 6×6 and 8×6),
  `row_encoders` (JAM 6 or 8 strips), `rows_per_set` preference (JAM 1, an Electra 2).
- **Version 1.2.** In one process all sides are one build, so this is about a later transport: a
  provider sends a model larger than 6×6 only to surfaces of 1.2 or newer (it knows each surface's
  version, mod.rs `Conn.version`), a 1.1 surface gets it clipped to 6×6.

### 3.3 The host page provider in the core [P]

- **The host model** `src/daw/host_page.rs`: `HostPage { exists, title, epoch, columns, rows,
  pages, page, cells: Vec<HostCell>, sets, sections, active_row, active_set, selected_cell }`,
  filled by `Parser::apply` (exhaustive match, src/parser.rs:211) from the 0x0C messages.
- **The provider** `src/framework/surface_link/host.rs`: `HostPageProvider` (per instance, owned
  by the running instance in `app.rs`). In `App::tick`, before `handle_changes`:
  1. `poll()` the surfaces' input and translate it to `Cmd`s (§3.4) with the model's epoch;
  2. on a change of the shown surfaces: `HOST_PAGE_SHOW` with the shape of the most recently
     shown surface (on), or off when none shows;
  3. convert `HostPage` → `PageModel` and `publish()` (after the host's whole report is applied:
     one buffer is one report, TYPED_PROTOCOL_PLAN §2.2, so a page switch is one snapshot).
  Publishing before `handle_changes` lets a surface in the same instance draw it in the same tick.
- **Registration:** only if `host_supports(HostFeature::HostPages)` (§3.5). Link `HOST_LINK =
  "host"`, name from the host (`/hostpage/title`, e.g. "zynthian"). If the link is provided already
  (a second zynthian driver), the provider is inactive and tries again every second, so it takes
  over when the first driver stops.
- **The epoch** (u16, from the host): incremented by the host on every structural change (chain,
  pages, layout, window). The core puts the current epoch into every cell command; the host drops
  a cell command of another epoch (a strip turned on the previous chain's cell 5 must not turn the
  new chain's cell 5).
- **Translations in the core** (keeps the host simple):

  | Input | Cmd |
  | --- | --- |
  | `SlotTurn` | `HOST_PAGE_CELL_TURN` |
  | `SelectedTurn` | `HOST_PAGE_CELL_TURN` of `selected_cell` |
  | `SlotPress` / `SlotRelease` | `HOST_PAGE_CELL_PRESS` 1 / 0 |
  | `SlotTouch` | `HOST_PAGE_CELL_TOUCH` |
  | `SelectedReset` | `HOST_PAGE_CELL_RESET` of `selected_cell` |
  | `RowSelect` | `HOST_PAGE_ROW` |
  | `PageSelect`, `PageNext`, `PagePrev` | `HOST_PAGE_PAGE` (page ± 1, clamped) |
  | `SetSelect`, `SetStep` | `HOST_PAGE_SET` (absolute; the core computes the step) |
  | `SectionSelect` | `HOST_PAGE_SECTION` |
  | `ValueNext/Prev`, `ExtraTurn` | not sent (the host model has no handles or extras in v1) |

### 3.4 The protocol [P]

**Host → core (`Msg`, block 0x0C)**, reported change-only like every other value (the host's
`Reporter` keys by id and indices, the dump after `refresh` and after `HOST_PAGE_SHOW` on):

| ID | Name | Text template | Value |
| --- | --- | --- | --- |
| 0x0C01 | `HostPageExists` | `/hostpage/exists` | bool (false: nothing to show, e.g. no chain) |
| 0x0C02 | `HostPageTitle` | `/hostpage/title` | text (the provider name the surface notifies: "zynthian · Synth 1") |
| 0x0C03 | `HostPageEpoch` | `/hostpage/epoch` | i32, first record of a structural change |
| 0x0C04 | `HostPageColumns` / 0x0C05 `HostPageRows` | `/hostpage/columns`, `/hostpage/rows` | i32 (≤ 8) |
| 0x0C08 | `HostPageCount` | `/hostpage/page/count` | i32 (the windows) |
| 0x0C09 | `HostPageSelected` | `/hostpage/page/selected` | i32 |
| 0x0C0A | `HostPageName` | `/hostpage/page/{page}/name` | text |
| 0x0C10 | `HostCellKind` | `/hostpage/cell/{cell}/kind` | i32, domain `CellKind`: empty 0, fader 1, list 2, toggle 3, trigger 4, relative 5, action 6 |
| 0x0C11 | `HostCellName` | `/hostpage/cell/{cell}/name` | text |
| 0x0C12 | `HostCellColor` | `/hostpage/cell/{cell}/color` | color |
| 0x0C13 | `HostCellValue` | `/hostpage/cell/{cell}/value` | f64, 0..1 |
| 0x0C14 | `HostCellText` | `/hostpage/cell/{cell}/text` | text (the host's value label) |
| 0x0C15 | `HostCellSteps` | `/hostpage/cell/{cell}/steps` | i32 (list items; 0 continuous) |
| 0x0C16 | `HostCellOn` | `/hostpage/cell/{cell}/on` | bool (toggle state) |
| 0x0C17 | `HostCellEnabled` | `/hostpage/cell/{cell}/enabled` | bool (false: read-only, gap) |
| 0x0C18 | `HostCellSet` | `/hostpage/cell/{cell}/set` | i32 (-1: none) |
| 0x0C20 | `HostSetCount` | `/hostpage/set/count` | i32 |
| 0x0C21 | `HostSetName` | `/hostpage/set/{set}/name` | text (zynthian's page title) |
| 0x0C22 | `HostSetSection` | `/hostpage/set/{set}/section` | i32 |
| 0x0C23 | `HostSetColor` | `/hostpage/set/{set}/color` | color |
| 0x0C24 | `HostSetSelected` | `/hostpage/set/selected` | i32 |
| 0x0C30 | `HostSectionCount` / 0x0C31 `HostSectionName` / 0x0C32 `HostSectionColor` / 0x0C33 `HostSectionSelected` | `/hostpage/section/...` | i32 / text / color / i32 |
| 0x0C40 | `HostPageRow` | `/hostpage/row` | i32 (the active row) |
| 0x0C41 | `HostCellSelected` | `/hostpage/cell/selected` | i32 (-1: none) |

- **Snapshot vs changes:** the host does not distinguish; it reports changed fields. The core's
  `PageModel::diff` decides (a new size, page or page list → snapshot). A whole 8×6 dump is about
  48 × 9 + 30 ≈ 460 records (~11 KB), a value change two records (value, text).
- One record per field (not a struct per cell): texts cannot be inside the fixed structs
  (bin.rs struct arrays are numeric, e.g. `Note`), and per-field records give the change-only
  reporting for free.

**Core → host (`Cmd`, block 0x8C)**, processor `Processor::HostPage` (text `hostpage`), all with
`feature = HostPages`:

| ID | Name | Indices | Value | Host action (zynthian) |
| --- | --- | --- | --- | --- |
| 0x8C01 | `HostPageShow` | columns, rows, row_encoders, rows_per_set | bool | on: choose the layout, dump; off: stop collecting |
| 0x8C02 | `HostPageCellTurn` | cell, epoch, unit (`TurnUnit`), fine (0/1) | i32 delta | §3.5 |
| 0x8C03 | `HostPageCellPress` | cell, epoch | bool (press/release) | `grid.press` / `release`: toggle, trigger, next list item, file selector; a continuous cell selects its page and becomes the selected cell |
| 0x8C04 | `HostPageCellTouch` | cell, epoch | bool | nothing yet (later: show the page's widget on zynthian's screen, `gridWidgetPoke`) |
| 0x8C05 | `HostPageCellReset` | cell, epoch | – | `grid.set_default` |
| 0x8C06 | `HostPageRow` | row | – | select the page in that row (8×6: the same half as before) |
| 0x8C07 | `HostPagePage` | page | – | the window `page` (rows 6·page …) |
| 0x8C08 | `HostPageSet` | set | – | `grid.select_page(set)` |
| 0x8C09 | `HostPageSection` | section | – | select the section's first page |

### 3.5 Value math, features, versions [P]

- **Value math in the host** (as the Electra's `14 53`, SURFACE_LINK §6.2):
  - `Detent` (main encoder): `grid.turn(cell, delta, fine)` = `zctrl.nudge` — the same as
    zynthian's own encoders: one list item per click, floats by `nudge_factor`.
  - `Travel` (strips): normalized: Δfraction = delta / 127 (fine: / 10), so a swipe across the
    strip is the whole range (as on the Electra's pages); continuous zctrls move in their fraction
    space (logarithmic as `nudge` does), stepped ones (lists, integers, toggles) accumulate the
    fraction per zctrl and step when it crosses 1 / steps (as `DeviceParameter.remainders` in
    device.py). A helper `turn_travel(cell, delta, fine)` in the shared grid model, next to `turn`.
  - **Acceleration:** none in the core. A host may add it for `Detent`.
- **Feature negotiation:** a new command feature `HostFeature::HostPages` (added to
  `is_command_feature`, context.rs:148). A binary host has it when its handled-commands bitset
  contains the 0x8C commands (a host which handles one should handle all; the reference host's
  coverage test checks it). Only then the core creates the provider.
- **Text hosts (Reaper):** `/host/features` never lists `hostPages`, so the provider is never
  created and no 0x8C command is sent; Reaper never reports `/hostpage/...`. The text templates
  exist only for logs and recordings. Should a 0x8C command ever reach a text host, its legacy
  form (`hostpage cell/3/turn …`) ends as "not handled" in its log. [A: the Reaper C++ does not
  switch exhaustively over the generated `DbmCmdId`; it is on the text path.]
- **Versioning:** the protocol's minor version (IDs added), the table hash changes, the generated
  files are regenerated (the staleness test). The Bitwig extension adds the new commands to its
  "unhandled" arm until P8; the reference host adds them to `UNHANDLED_COMMANDS` until P5. The
  surface link goes to 1.2 (§3.2).

### 3.6 Latency

Host report → `dbm_update_model_bin` at the host's tick → `publish` at `App::tick` → the JAM's
poll in the same tick → LEDs at `flush`. Input: strip → the up queue → the provider's next
`poll` → `Cmd` → the host executes it in the command callback → its next fast pass reports the new
value. About 1-2 ticks (30-60 ms) for a round trip, the same order as the Electra path (SysEx to
the device and back). The page values must be in zynthian's **fast** pass (§4.2), otherwise a turn
shows only after the 250 ms full pass.

### 3.7 Layout: who maps a host page onto a surface

The host lays out, the core does not re-arrange cells:
- zynthian knows its pages and has the layout code (`LAYOUTS`, `_fill`; its doc names "add a
  `GridLayout`" for other sizes).
- The right arrangement depends on the surface: one-row encoders (the JAM's strips) want a page in
  one row (4×6, 8×6); 12 knobs in two rows (an Electra) want the 2×2 blocks of lv2-to-electra
  (`docs/lv2-to-electra.md:101-108`: "a zynthian page is a 2 × 2 block", three per control set).

So `HOST_PAGE_SHOW` carries the shape, and the host answers with `/hostpage/columns|rows`:

| Surface shape (columns × rows, row encoders, rows per set) | zynthian layout |
| --- | --- |
| JAM: 8 × 6, 8, 1 | `grid8x6` (2 pages a row; strips 1-8 = the row) |
| a 6-column grid with 6 encoders (Launchpad + LCXL, the JAM's 6-column mapping) | `grid4x6` (4 of 6 columns) |
| an Electra: 6 × 6, 6 a row, 2 | a new `grid6x6` of 2×2 blocks (zynthian side, later) |

Several surfaces showing at once with different shapes: the most recently shown one wins; the
others clip or leave empty columns (rare: one person, one surface at a time) [P].

## 4. The zynthian side

### 4.1 What zynthian-ui needs (the zynthian agents' part)

1. **A shared page builder:** move the page list of `ChainControl._build` (control.py:526-575) and
   the section logic of `ControlGridModel.set_pages` (control_grid.py:166-190) into
   `zynthian_control_grid_data` (e.g. `chain_pages(chain, processors, current) -> (pages,
   selected)`), used by the Qt screen and the DBM adapter, so both show the same pages.
2. **Section colours in Python:** an RGB per section index (or per processor/chain), so a
   controller shows the same hues as the screen's theme (`controlGridColors` lives in QML).
3. **Selection in both directions** (if the owner wants shared selection, §9 Q3): a signal when
   the control screen's page changes (UI → adapter), and a way to select the screen's page from a
   driver (adapter → UI, on the GUI thread, e.g. a CUIA or a `zynsigman` signal which
   `ChainControl` handles). Today `set_current_screen_index` stores the index without a signal
   (zynthian_processor.py:674-680).
4. **`turn_travel`** next to `turn` (§3.5), or the adapter does it.
5. **Later:** `grid6x6` (2×2 blocks) for an Electra-shaped surface.
6. **A branch with both:** `electra-grid` on top of `dbm` (or merged), so the DBM drivers run with
   the grid model; and the DBM test stand-ins (`backends/zynthian/tests/zynstubs.py`) need zctrls
   with `nudge`, `ticks`, `labels`, `is_toggle`, `is_trigger`, `get_value2label`.

Not needed: a per-zctrl change signal (the brainstorm's Q6): `ValueWatcher` compares 48 values,
which also sees engines that write `zctrl.value` directly.

### 4.2 The adapter `backends/zynthian/ctrldev/dbm_zynthian/host_page.py` [P]

- **`HostPageSource`:** its own `ControlGrid` (layout from `HOST_PAGE_SHOW`) and `ValueWatcher`,
  used only from the driver's tick thread (the grid is not thread safe; the Qt screen's instance is
  on the GUI thread). Writing zctrls from the tick thread is what the drivers do already
  (device.py `set`/`inc`). Without the grid module (a stock zynthian) the adapter does not declare
  the 0x8C commands, so the core has no host provider.
- **Mapping:**

  | Host page | zynthian |
  | --- | --- |
  | cells | `ControlGrid.cells` (row-major), kind: `float`/`integer` → fader (integer with ≤ 16 values: list with steps), `list` → list (steps = items), `toggle` → toggle (`on` = value ≠ min), `trigger` → trigger, `path` → action (opens the file selector on zynthian's screen), `empty` → empty |
  | name, text, value | `cell_name`, `cell_value_text`, `cell_fraction` |
  | enabled | `GridCell.enabled` |
  | sets | the grid's pages in the window (title, section, colour) |
  | sections | "Chain", then the processors (`section_index`) |
  | pages | windows of 6 rows (page k = rows 6k…6k+5; the adapter scrolls by whole windows, not by rows as the Qt grid does) |
  | selection | `ControlGrid.selected` (set), its row; the selected cell: the adapter's (the last pressed or turned cell of the selected set, else its first) |
  | title | "zynthian · <chain name>" |
  | epoch | +1 on every `structure` event |

- **Rebuild** (structure, epoch + 1) on `SS_SET_ACTIVE_CHAIN`, `SS_ADD/REMOVE_PROCESSOR`,
  `SS_PROCESSOR_CTRL_SCREENS`, `SS_MOVE_CHAIN`, `SS_LOAD_ZS3` (already in `change_signals`,
  bridge.py) and on `HOST_PAGE_SHOW`. [A: a preset change which replaces a processor's zctrls
  sends `SS_PROCESSOR_CTRL_SCREENS`; to check on the zynthian side.]
- **Reporter:** a subtree `"page"` (only while shown): the structure in the full pass, the values
  of the shown cells (`ValueWatcher` → value and text of the changed cells) in the **fast** pass.
- **Commands:** typed handlers in `ZynCommandHandler` for 0x8C (`register_typed`), each checks the
  epoch, then calls the grid's input methods; a command requests the next full pass (as now).
- **CPU** (ZYNTHIAN_CPU.md): nothing while no surface shows the pages. While shown: the fast pass
  compares ≤ 48 values (the Qt grid: 5-7 µs per poll in the QEMU VM, docs/qt-control-grid.md
  "Performance"), formats the text only of changed cells; a rebuild builds the pages and dumps
  ~460 records, only on structural changes. Measure with `backends/zynthian/tests/test_cpu.py`
  (the JAM, shown, idle and while a value changes every tick).

## 5. The JAM side [P]

**One surface mode, two providers.** The JAM keeps one `SurfaceLinkClient` per link (`"pages"`,
`"host"`), both attached at start (as now). Select + SONG cycles: off → the next attached provider
→ … → off, skipping missing ones, starting with the last one used. With only one provider it is
today's toggle. The notification names the provider ("Surface: zynthian · Synth 1"). Leaving sends
`Shown { on: false }`, entering `Shown { on: true }`. A detach leaves the surface (as now).

**Two mappings, chosen by the model's width** (the 6-column one is today's):

| JAM | 6-column model (Electra, zynthian 4×6) | 8-column model (zynthian 8×6) |
| --- | --- | --- |
| grid rows 2-7 | columns 2-7: the cells; column 1: row selectors; column 8: page list | columns 1-8: the cells |
| grid row 1 | (column 1 top: previous page) | the page list (windows), the current one orange |
| grid row 8 | (column 1 bottom: next page) | the sections in their colours, the active one full, the others dim; press → `SectionSelect` |
| cell colours | the slot colour (zynthian: the section's hue) | same; the active set full, other cells one level lower, the selected cell blinks |
| strips 1-8 | 1-6 the active row, 7 the selected cell (fine), 8 the extra | 1-8 the active row (two zynthian pages) |
| main encoder / push | selected cell (`Detent`), Shift fine / reset | same |
| UP / DOWN | previous / next row | same |
| LEFT / RIGHT | previous / next value of the selected slot (envelope handle) | previous / next control set (`SetStep`: the other half, then the next row) |
| PAGE LEFT / RIGHT | previous / next page | previous / next page (window) |
| group buttons A-H | the tracks (zynthian: chains), unchanged | same |

- In 8×6, pressing any cell selects its page (zynthian's tap) and the row follows; turning a strip
  in the other half does not move the selection (zynthian's drag on a cell does not either).
- The JAM has no display: names and value texts reach the user through the notifications
  (`state_text`, a touched strip's cell name and value) [A: where `ctx.notify` shows on zynthian:
  to check].
- `fader_configs` and `encoder_value` follow the model width (today they index `active_row *
  GRID_SIZE`).

## 6. Relation to the Electra path (brainstorm A/B/C)

- This plan is the DrivenByMoss half of **path B** (and of C's UI part): zynthian owns the model
  and the UI; controllers attach through DrivenByMoss. It answers the brainstorm's questions for
  DrivenByMoss (§4.3): the transport is **the existing host bridge** of the zynthian driver (no
  OSC/socket, no in-process registry access from Python); the home is the **DBM zynthian adapter**
  (`dbm_zynthian`), fed by the shared grid model; **the host owns** acceleration and fine steps.
- **Coexistence:** the Electra controller provides on `"pages"` (its DrivenByMoss pages, or the
  generic provider for another preset), zynthian on `"host"`. Both can be attached; the JAM picks
  with Select + SONG (§5).
- **The Electra as a surface of the host's pages (P7, optional):** an Electra mode in DrivenByMoss
  which attaches to `"host"` as a surface (capabilities 6×6, 6 encoders a row, rows per set 2) and
  renders the cells into the DrivenByMoss preset's 36 controls with what the Electra provider
  already sends (names `14 07`, values as CC feedback, texts `14 0E`, rows/selection §6b), its
  knobs' relative CCs → `SlotTurn` (`Detent`). Works on a real Electra and on the emulator, without
  firmware work (no Z1). The preset's fixed control kinds (mostly faders) limit lists and toggles to
  the value text and a press. One Electra instance can be a provider on `"pages"` and a surface on
  `"host"` at the same time (two links); only one is shown on the device.
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

**Isolation:** the owner runs their own dbm_host, simulators, emulator and zynthian. Every test
run uses a unique name suffix (`" (hp-$$)"`) and selects ALSA clients by PID (`aconnect -l`,
`/proc/<pid>`), never by name alone (`find_port` matches substrings, CONTROLLER_SIMULATORS.md
§3). Test instances may be stopped; say so.

1. **Rust:** `cargo test --lib surface_link` (1.2 model, client and view with an 8×6 model on a
   JAM-like mapping, `Shown`, units), `cargo test --lib protocol` (round trip of the new entries,
   the staleness test), a new `tests/surface_link_host.rs`: an `App` with the JAM, host pages fed as
   text (`dbm_update_model` lines `/hostpage/cell/3/name Cutoff`) and binary, asserting the JAM's
   pads and strips and the typed `Cmd`s with epochs; the existing `tests/surface_link.rs` unchanged.
2. **The Python reference host:** a simulated provider in `dbm_host` (`SimHostPages`: a chain
   "Synth" with sections Chain/Helm/Reverb, 10 pages, every kind, zynthian's nudge rules and the
   travel accumulator), the reporter collector, the 0x8C handlers; tests in `bindings/python/tests`.
   `dbm_host_alsa.py --model jam --host-pages --suffix " (hp-$$)"` with
   `support/controller_simulator/run.sh --model jam --suffix " (hp-$$)"`: Select + SONG, swipe,
   press, page, sections. Also `--model electra --model jam --host-pages` (two providers, cycling).
3. **zynthian in-process tests:** `backends/zynthian/tests` with `zynstubs` and the shared grid
   module (needs the combined branch, §4.1 item 6): the adapter's mapping, epochs, signals → rebuild,
   commands → zctrls, CPU (`test_cpu.py`).
4. **zynthian in Qt mode** (`ZYNTHIAN_UI=qt ZYNTHIAN_CTRLDEV_PATH=…/backends/zynthian/ctrldev
   …/desktop/zynthian_desktop.sh`) with the JAM simulator: the zynthian JAM driver expects the
   device ID `Maschine Jam - 1 IN 1` (zynthian_ctrldev_dbm_maschine_jam.py), so a suffixed
   simulator is not found, and the plain name collides with a running dbm-nihost or the owner's
   simulator. Run it only when the owner agrees (or add an override of `dev_ids` for tests, §9 Q8).
   Check: the screen and the JAM show the same pages, a turn on either shows on both, a chain
   switch with the group buttons, a preset load.

## 9. Phases

Each phase leaves everything green; existing tests and the Bitwig conformance unchanged.

| Phase | Content | Acceptance |
| --- | --- | --- |
| **P0** | The owner's answers to §10; on the zynthian side a branch with `electra-grid` and `dbm` | decisions written into this plan |
| **P1** | Surface link 1.2 (§3.2): size, sets, sections, `Shown`, `TurnUnit`, capabilities; the JAM's 8-column mapping in the generic client/view | `cargo test` green; `tests/surface_link.rs` unchanged; new unit tests: an 8×6 model on a JAM-like mapping (pad colours, strips, set dimming) |
| **P2** | Protocol: `Msg` 0x0C, `Cmd` 0x8C, `CellKind`/`TurnUnit` domains, `Processor::HostPage`, `HostFeature::HostPages`; generated files; reference host `UNHANDLED_COMMANDS`, Bitwig "unhandled" arm | `cargo test` incl. staleness and round trip; Python and Java build; Bitwig conformance runs (allowed for agents) unchanged |
| **P3** | `HostPage` model and `apply`; `HostPageProvider` on `"host"` in `App::tick`; `HOST_PAGE_SHOW`; translations, epoch | `tests/surface_link_host.rs`: snapshot and changes, every input as a `Cmd`, stale epochs, provider takeover after the first instance stops |
| **P4** | The JAM: two clients, Select + SONG cycling, `Shown`, LEFT/RIGHT `SetStep` in 8 columns, notifications | tests of the cycling (none / one / two providers), the 8-column layout end to end |
| **P5** | The Python reference host's simulated pages; `dbm_host_alsa.py --host-pages` | Python tests; a manual run with the JAM simulator (checklist in §8.2) |
| **P6** | zynthian: the zynthian-ui items of §4.1 (zynthian agents), `host_page.py`, reporter subtree, handlers, docs (ZYNTHIAN_PLAN, HOST_BRIDGE_GUIDE §3/§4) | zynthian tests green; `test_cpu.py` numbers; Qt desktop checklist (§8.4) |
| P7 (opt.) | The Electra as a surface of `"host"` | emulator run: the Electra shows zynthian's pages; real Electra if available |
| P8 (opt.) | Bitwig host pages from remote-control pages | Bitwig run with the JAM; conformance unchanged |

## 10. Open questions

For the owner:
1. **Entry gesture:** Select + SONG cycling through the providers (recommended), or a separate
   gesture for the host pages (which button)?
2. **JAM layout for 8×6:** rows 1 and 8 as page list and sections (recommended), or 8×8 cells with
   navigation only on buttons? Strips 1-8 on the whole row (recommended) or only the selected
   page's 4?
3. **Selection:** shared with zynthian's screen (the JAM selects the page the screen and zynthian's
   four encoders show; needs §4.1 item 3), or independent (the JAM has its own selection;
   simpler)?
4. **Layout:** the JAM always `grid8x6`, or follow the screen's choice (`ZYNTHIAN_QT_CONTROL_LAYOUT`)?
5. **Colours:** cells in their section's hue (recommended) or the chain's colour?
6. Electra as a surface (P7) and Bitwig (P8): wanted, and in which order?

For the zynthian side:
7. Where the shared page builder and the section colours live; whether a driver may select the
   screen's page (a CUIA or a signal), and the signal for page changes.
8. A way to run the JAM driver against a suffixed simulator in tests (e.g. `dev_ids` from an
   environment variable).
9. Whether a preset change that replaces zctrls always sends `SS_PROCESSOR_CTRL_SCREENS`.
10. Whether a `grid6x6` of 2×2 blocks (lv2-to-electra) should exist in the shared model.

## 11. Risks

- **Threads in zynthian:** the grid model is GUI-thread only; the adapter's own instance on the
  tick thread avoids sharing it, but both write the same zctrls (as the drivers do today). A shared
  selection (Q3) crosses threads: use queued signals, never call the Qt model from the tick thread.
- **Selection ping-pong** between the screen and the adapter: tag the origin, ignore the echo.
- **Layering:** `zyngine/ctrldev` importing `zyngui.zynthian_control_grid_data`; the adapter must
  work without it (no feature declared).
- **Branch drift:** `electra-grid` (on `main`) and `dbm` diverge until combined.
- **Surface link regressions:** the size generalization touches the Electra path; the existing
  `tests/surface_link.rs` (exact bytes) guard it.
- **Epoch races:** without the epoch a strip turn could hit another chain's cell after a switch.
- **Two providers in one process** (two zynthian drivers): first wins, the other retries.
- **The JAM palette:** neighbouring sections' hues may map to the same NI hue; pick the section
  palette from distinct NI hues (SURFACE_LINK §5, colour roles).
- **CPU** if the page values ran in the slow pass only (laggy) or the structure in the fast pass
  (costly): keep the split of §4.2 and measure.
