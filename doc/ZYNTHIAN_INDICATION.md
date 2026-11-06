# Showing a DrivenByMoss controller's window in zynthian's UI

Status: analysis, 2026-11-07. Nothing implemented. Zynthian code read in the worktree
`~/bastel/github/zynthian-dbm` (branch `dbm`, from `nura`); paths below are relative to it
(`zyngine/…`, `zyngui/…`, `zynqt/…`) or to this repository (`src/…`, `backends/…`, `bindings/…`,
`doc/…`).

Tags: **[code]** read in the code, **[unverified]** plausible, not checked, **[guess]** a design
idea or an assumption.

## 0. Summary

- zynthian has **no way to show where a controller is**. The only channel is the other way round:
  the GUI tells the drivers where *it* is (`SS_GUI_VIEW_POS`, the active chain, the selected
  phrase), and the stock drivers follow the GUI through their scroll modes. Nothing is drawn for a
  controller's window, in Tk or in Qt. [code]
- The adapter already **receives** the track and scene bank offsets (`bank track|scene|fxtrack`)
  and keeps them in `banks`; the page sizes come from the model setup. The selection (chain,
  phrase) already reaches zynthian's UI, which highlights it and scrolls to it. [code]
- It **does not receive** the indication flags of volume, pan, sends and the clip launcher: it
  does not report the host feature `indication`, so the core never sends them. Device parameter
  `indicate` arrives but is stored on a throwaway object. [code]
- For the pattern editor the adapter knows the **step window** of each step grid (`clip
  grid/<id>/scroll`, `stepSize`), but not **which grid is on the pads** nor the **key rows** the
  pads show: these stay inside the core. [code]
- Smallest useful step: a pull-style "controller window" API in zynthian (one base-class method,
  one GUI signal without payload), a **shared, toolkit-independent module**
  (`zyngui/zynthian_ctrldev_window_data.py`, like the other `*_data.py` modules shared by the Tk
  and the Qt UI) which turns the drivers' windows into ranges per view, the frames in the **Qt UI
  first** (`zynqt/`), and the DBM driver publishing `track_offset/num_tracks` and
  `scene_offset/num_scenes`. The Tk UI can use the same module later. The UI should **mark**, not
  follow; following already happens through the shared selection (§6).

## 1. What zynthian has today

### 1.1 Scroll modes and the GUI → driver hooks (`zyngine/ctrldev/zynthian_ctrldev_base.py`)

- Five scroll modes: `SCROLL_MODE_DISABLED|FIXED|GUI_SEL|GUI_VIEW|CTRLDEV`
  (`zynthian_ctrldev_base.py:45-49`). Each driver has `cols`, `rows`, `scroll_h` (first chain),
  `scroll_v` (first phrase), `scroll_mode` (`:96-105`); the default is DISABLED (`:105`). [code]
- `init` registers (queued) `S_CHAIN_MAN/SS_SET_ACTIVE_CHAIN → on_active_chain` and
  `S_GUI/SS_GUI_VIEW_POS → on_gui_view_pos` (`:145-147`); the zynpad base adds
  `S_STEPSEQ/SS_SEQ_SELECT_PHRASE → on_active_phrase` (`:399`). [code]
- `on_active_chain` (`:348-360`): in GUI_SEL, scrolls `scroll_h` so the active chain is inside
  the driver's `cols`, then `refresh()`. `on_active_phrase` (`:497-507`): same for `scroll_v` and
  `rows`. `on_gui_view_pos(left_chain, top_phrase)` (`:362-375`): in GUI_VIEW, copies the GUI's
  view position. [code]
- All three are **GUI → driver**: the driver reacts to where the GUI is. There is no driver →
  GUI counterpart for positions. [code]
- `SS_GUI_VIEW_POS` is sent only by the Tk mixer, at the end of a scroll
  (`zyngui/zynthian_gui_mixer.py:1709`, in `scroll_canvas`, `:1669-1733`); `left_chain` is the
  index among the mixer's scrollable strips (all chains in display order, mixbuses included),
  `top_phrase` the first visible launcher row. The Qt UI never sends it (no hit in `zynqt/`).
  [code]
- Small inconsistency: drivers use `scroll_h` as an index into `chain_ids_filtered` (their chain
  type filter, `:279-285`), the mixer's `left_chain` indexes all chains. Equal only without a
  filter. [code]
- The scroll mode is chosen in MIDI config → controller options
  (`zyngui/zynthian_gui_midi_config.py:329-336`), but `controller_options_cb` calls
  `set_scroll_mode(cal)` instead of `val` (`:346`): it always fails. Known as Z3 in
  `doc/ZYNTHIAN_INTEGRATION.md:494`. The menu also only appears if the mode is already > 0
  (`:330`). [code]

### 1.2 The stock drivers

- **Launchpads** (e.g. `zynthian_ctrldev_launchpad_x.py:130-160`): pads map to `scroll_h + col`,
  `scroll_v + row`; the arrows send the CUIAs `ARROW_UP|DOWN|LEFT|RIGHT` (`:143-149`), i.e. they
  move the **GUI's** selection; the pads then follow the GUI only if the scroll mode says so.
  [code]
- **APC40 mk2**: sets `SCROLL_MODE_GUI_SEL` itself (`zynthian_ctrldev_akai_apc_40_mk2.py:200`);
  its pads select chain and phrase in zynthian (`:575-579`). [code]
- **Mackie Control**: pages its own `scroll_h` (bank and channel buttons,
  `zynthian_ctrldev_mackiecontrol.py:295-316`) and tells the GUI nothing. [code]
- **Launchkeys, MPK, SMK25**: react to `SS_GUI_SHOW_SCREEN` (which screen is shown) only. [code]
- None of them marks its window in the GUI; the stock model is "the controller follows the GUI".
  [code]

### 1.3 Signals (`zyngine/zynthian_signal_manager.py`)

- `S_GUI` sub-signals 0-7 are used (`SS_GUI_SHOW_SCREEN` … `SS_GUI_VIEW_POS`, `:76-83`);
  `last_subsignal = 10` (`:127`) sizes the register, so 8, 9, 10 are free without touching the
  table. A sub-signal above 10 is silently dropped by `register`/`process_signal` (`:162`,
  `:197`). [code]
- `send` calls synchronously in the sender's thread; `send_queued` / `register_queued` go through
  one queue thread (`:196-245`). The Tk mixer registers its callbacks queued in `build_view` and
  **unregisters them in `hide`** (`zynthian_gui_mixer.py:1338-1384`): a hidden mixer misses
  signals, so a state pushed only by signals would be lost while another screen is shown. [code]

### 1.4 The Tk mixer and launcher (`zyngui/zynthian_gui_mixer.py`)

The launcher is the mixer in launcher mode (`set_launcher_mode`, `:1811-1849`; the screen
"launcher" in the Qt UI likewise).

- **Selection**: `highlight_chain` (`:1601-1640`) colours the active chain's legend and fader
  overlay and calls `scroll_to_chain_index` (`:1642-1667`), which scrolls only if the strip is out
  of view. `highlight_launcher` (`:1541-1567`) clears all pad outlines, outlines the selected pad
  yellow (`zynthian_gui_launcher_pad.highlight`, `:221-224`) and scrolls the launcher rows so it
  is visible. Both run on `SS_SET_ACTIVE_CHAIN` / `SS_SEQ_SELECT_PHRASE` (`:1346`, `:1351`), so
  **a selection made by any controller already moves and marks the GUI**. [code]
- **Geometry**: `visible_chains` strips (`:1120-1140`), `strip_width`, `launcher_y`,
  `launcher_height`, `_scroll_y` (`:1144-1201`). Chains in display order; the last
  `get_pinned_count()` chains (the main chain at least) sit in `right_canvas`
  (`build_mixer`, `:1223-1262`; `zyngine/zynthian_chain_manager.py:799-805`). Horizontal scrolling
  is `xview` of `left_canvas`; vertical launcher scrolling **moves the items tagged "launcher"**
  (`scroll_canvas`, `:1713-1717`). [code]
- `build_mixer` deletes everything (`:1232-1233`), `build_launchers` deletes the "launcher" items
  (`:1266-1267`), `refresh_launchers` and `set_launcher_mode` lower them (`:1289`, `:1825-1826`):
  an overlay must be redrawn after these and raised after the lowering. [code]
- `refresh_status` runs every 0.2 s on the "Status" thread while the mixer is shown
  (`zyngui/zynthian_gui.py:3006-3030`, `zynthian_gui_mixer.py:1404-1437`). [code]

### 1.5 The Tk pattern editor (`zyngui/zynthian_gui_pated_base.py`, `zynthian_gui_pated_notes.py`)

- Horizontal window: `step_offset` (first column, `pated_base.py:112`), `view_steps`; scrolled by
  `set_step_offset` (`:1100-1116`) through `xview_moveto`. Vertical: `keymap_offset` (bottom
  row, `pated_notes.py:95`), `view_rows`; `set_keymap_offset` (`:599-612`) also stores it in
  zynseq (`setRefNote`). The grid canvas has the whole pattern (`total_width × total_height`),
  so a rectangle in canvas coordinates scrolls with it. [code]
- `refresh_status` (`pated_base.py:1564-1577`) polls zynseq every 0.2 s (`isPatternModified`,
  playhead): notes the core writes appear without a signal. [code]
- No signal about its view; the stock drivers read it directly (`selected_cell`, `keymap` in
  `zynthian_ctrldev_base_ui.py:71-73`). [code]

### 1.6 The Qt UI (`zynqt/`, plan in `docs/qt-port-plan.md`)

(`nura/pyside6` only holds the PySide6 APKBUILD; the UI is `zynthian_qt_main.py` and `zynqt/`.)

- `zynqt/bridge.py`: `SignalBridge` re-emits zynsigman signals as Qt signals (queued into the
  GUI thread): chains, active chain, mixer values, sequencer play state/state/phrase (`:16-57`).
  Nothing for `S_GUI`. [code]
- `zynqt/qml/MixerScreen.qml`: strips in a horizontal `ListView` (`:42-61`); the active strip
  gets a 2 px border (`:137-139`); launcher pads in each strip, `selected` = active strip and
  selected phrase, yellow border (`:253`, `:375-376`); `showPhrase` keeps the selected phrase in
  view (`:17-29`). The strips do **not** scroll to the active chain (no `positionViewAtIndex`).
  [code]
- `zynqt/models/launcher.py`: `Launcher` QObject (`phrase`, `phrases`, `padInfo`); no view
  position. [code]
- `zynqt/qml/PatternEditorScreen.qml`: a `Flickable` grid (`:124-138`), `showCursor` keeps the
  cursor in view (`:20-27`); `zynqt/screens/pattern_editor.py` has no step/key offset of its
  own (the Flickable's `contentX/Y` is the window). [code]

## 2. What the DrivenByMoss core tells the host

| Information | Command / source | Condition | In the core |
| --- | --- | --- | --- |
| Track window offset | `bank track <offset>` | host paging (`/track/offset` reported) | `src/daw/track_bank.rs:376-393` [code] |
| Scene window offset | `bank scene <offset>` | `/scene/offset` reported | `src/daw/bank.rs:136-153` [code] |
| Effect track window | `bank fxtrack <offset>` | `/fxtrack/offset` reported | as above [code] |
| Page sizes | the model setup: `numTracks`, `numScenes`, `numSends`, … | always | `bindings/python/dbm_host/banks.py:46-50` [code] |
| Clip launcher frame | `track indicate <0\|1>` (`fxtrack indicate`) | feature `indication` | `src/daw/track_bank.rs:602-607` [code] |
| Volume, pan, sends frame | `track <p>/volume\|pan/indicate`, `track <p>/send/<s>/volume/indicate`, `master volume\|pan/indicate` | feature `indication` | `src/daw/parameter.rs:356-372` [code] |
| Device parameters | `device\|primary param/<i>/indicate` | always | `src/daw/parameter.rs:357-362` [code] |
| Drum pads | `drum\|primary indicate` | always | `src/daw/device.rs:469-470` [code] |
| Parameter page | `device\|primary parameterPage <p>` | named pages reported | HOST_BRIDGE_GUIDE §4.6 [code] |
| Selection | `track <p>/select`, `scene <s>/select`, `track <p>/clip/<s>/select` | always | HOST_BRIDGE_GUIDE §4.4, §4.5 [code] |
| Step window | `clip grid/<id>/scroll <step>`, `page/+\|-`, `stepSize <beats>` | host step grid (`/clip/loopStart…` reported) | `src/daw/clip/cursor_clip.rs:616-623`, `:1380-1417` [code] |
| Key rows of the step grid | – | never | `NoteGrid.key_offset`: "the core does not move it" (`bindings/python/dbm_host/clip.py:268-269`); the grids are 128 rows (`noteClips [[8,128]]`, `src/parser.rs:2496`); the views keep their key mapping [code] |
| Which view/mode is active | – (indirectly through the indications) | – | e.g. Launchpad: `track indicate` only in session, volume/pan/send/param indication per mode (`src/controller/launchpad/setup.rs:393-406`) [code] |

`HostFeature::Indication` is used only by these two senders (`track_bank.rs:604`,
`parameter.rs:366`); turning it on adds commands and changes nothing else in the core. [code]

The indications are how Bitwig draws its frames: the clip launcher frame from
`setShouldShowClipLauncherFeedback` (track bank × scene bank), the frames of mixer controls and
remote controls from `setIndication` per parameter (HOST_BRIDGE_GUIDE §4.4, §4.6). [code for the
mapping, unverified how Bitwig draws them]

## 3. What the Zynthian adapter does with it today

Files in `backends/zynthian/ctrldev/dbm_zynthian/`.

- **Bank offsets: kept.** `bank` → the reference `CommandHandler._bank` → `Banks.scroll`, which
  stores `track_offset`, `scene_offset`, `fx_track_offset` (`commands.py:83`,
  `bindings/python/dbm_host/commands.py:258-259`, `banks.py:60-71`). The reporter reports every
  track and scene with its absolute position and the offsets back (`reporter.py:120-123`,
  `:203`, `:217`). Nothing in zynthian's UI sees them. [code]
- **Track list ≠ mixer strips.** The core's tracks are the chains without the main chain and the
  mixbuses (`model.py:398-405`); with `hasFullFlatTrackList` (LCXL) the mixbuses and the master
  follow (`:410-419`). The mixer shows all chains in display order with the pinned ones on the
  right. A window must therefore be passed as **chain ids**, not as a strip index. Whether
  mixbuses can sit between normal chains in display order: [unverified]. [code for the rest]
- **Scenes** are the phrases of zynseq's current scene (`reporter.py:198-213`); scene `s` =
  launcher row `s`. [code]
- **Host features**: `valueInc valueReset immediately soloToggle transportCommands
  deviceCommands` (`commands.py:68-70`); **no `indication`** (also listed in
  `doc/ZYNTHIAN_PLAN.md:169`). So `…/volume|pan/indicate`, `send/…/indicate` and `track
  indicate` are never sent. [code]
- `track indicate` and `fxtrack indicate` would be accepted and dropped (`commands.py:116-119`,
  `:129-131`; the reference stores `banks.show_track_clip_launcher_feedback`,
  `bindings/python/dbm_host/commands.py:852-854`). `drum indicate` is accepted and dropped
  (`commands.py:242-245`). [code]
- Volume/pan `…/indicate` would land in `ZctrlParameter.set_indication`, which stores a flag
  (`model.py:171-172`); the volume/pan parameters live as long as their `ChainTrack`
  (`model.py:225-226`), but **sends are rebuilt on every access** (`model.py:311-314`) and so are
  **device parameters** (`device.py:174-175`): their flag is lost at once. Device parameter
  indication therefore arrives today and has no effect. [code]
- **Parameter page**: `parameterPage p` selects controller screen `2p` of the processor
  (`device.py:157-161`; a DbM page of 8 = two zynthian screens of 4). `set_current_screen_index`
  sends no signal (`zyngine/zynthian_processor.py:675-681`); whether the Tk control screen
  follows while shown: [unverified]; `doc/ZYNTHIAN_PLAN.md:704` lists "zynthian's control screen
  follows" as a device check. [code / unverified]
- **Selection**: `track <p>/select` → `set_active_chain_by_id` (`model.py:329-330`), slot/scene
  select → `zynseq.select_phrase` + `selectSequence` (`launcher.py:181-188`): both emit the
  signals the GUI highlights and scrolls on (§1.4). [code]
- **Step grids**: one `NoteGrid` per size of the model setup's `noteClips` (`commands.py:102`);
  `clip grid/<id>/scroll|page|stepSize` set `grid.scroll` and `grid.step_size`
  (`bindings/python/dbm_host/commands.py:481-491`, `clip.py:312-317`). So the adapter knows, per
  grid, the first step and the step length; the window in zynseq steps is
  `[scroll·stepSize·spb, (scroll+num_steps)·stepSize·spb)` with `spb` the pattern's steps per
  beat (`clip.py` module docstring for the conversion). It does **not** know which grid the
  active view uses, whether a sequencer view is on the pads at all, nor the key rows. [code]
- Threads: all core calls and commands run on the driver's tick thread (30 ms,
  `zynthian_ctrldev_base_dbm.py:268-359`); notifications go to the GUI with
  `send_queued(S_GUI, SS_GUI_SHOW_MESSAGE)` (`:357-358`). The DBM driver keeps scroll mode
  DISABLED and overrides `get_state` without the base's `scroll_mode` (`:244-249`), so
  `on_active_chain` / `on_gui_view_pos` do nothing for it. [code]

## 4. Per view

### 4.1 Mixer

1. **Today**: the active chain is highlighted and scrolled into view (§1.4); no controller window.
   The GUI → driver signal `SS_GUI_VIEW_POS` exists, nothing the other way. [code]
2. **DbM information**: `track_offset` + `num_tracks` → the chain ids of the window (and
   `fx_track_offset` + `num_fx_tracks` for the mixbuses); which of them the controls address
   comes from `volume|pan|send/indicate` (needs `indication`): e.g. the Launchpad indicates
   volume only in its volume mode (`src/controller/launchpad/setup.rs:396-400`), the MCU/HUI the
   tracks of its strips. Received today: the offsets and sizes. Missing: the indication flags.
   [code]
3. **Options**:
   - *Reuse*: nothing to reuse for drawing. Selection already marks one chain. A text on page
     change through `SS_GUI_SHOW_MESSAGE` (the driver already forwards core notifications) costs
     no zynthian change, but is not a frame. Sending `SS_GUI_VIEW_POS` from the driver is wrong:
     only drivers listen to it (stock drivers in GUI_VIEW would follow the DbM controller, the
     mixer would not move). [code / guess]
   - *Small zynthian addition* (§5): a frame (outline rectangle, a controller colour) around the
     window's strips in `left_canvas` (and `right_canvas` if it reaches the pinned chains), one
     rectangle per contiguous run of strips, full strip height in mixer mode. With indication
     flags, a thinner frame or a coloured bar over the fader/balance of the indicated strips
     only. `zynthian_gui_mixer.py`: a new `draw_ctrldev_windows()` (~60-80 lines), called from
     the new signal's callback, at the end of `build_mixer`/`build_view` and after
     `set_launcher_mode`; register/unregister with the others (`:1338-1384`, 2 lines). [guess]
   - *Qt*: `SignalBridge` gets `ctrldevWindowsChanged` (~6 lines); a small model
     (`zynqt/models/ctrldev_windows.py`, ~40 lines) exposes the windows as a list of
     `{chainIds, firstPhrase, numPhrases, color, …}`; in `MixerScreen.qml` the `Strip` delegate
     gets `readonly property var frame` and draws top/bottom edges plus the left/right edge on the
     first/last strip of a run (~25 lines). A single Rectangle over the `ListView` (x from the
     first index × (stripWidth + spacing) − `contentX`) is the alternative. [guess]
4. **Follow or mark**: mark. The view already follows the selection. [guess, §6]

### 4.2 Launcher (the mixer in launcher mode)

1. **Today**: the selected pad has a yellow outline and is scrolled into view; the active chain's
   strip is highlighted (§1.4; Qt: `MixerScreen.qml:253`, `:375-376`). [code]
2. **DbM information**: the track window (as 4.1) × `scene_offset` + `num_scenes`; whether the
   pads currently show the clip launcher: `track indicate` (needs `indication`; the Launchpad
   sends 1 only in session mode, `launchpad/setup.rs:393`; most other controllers set it once to
   true at start, e.g. `src/controller/mackie/mcu/setup.rs:160`, `maschine/mk/setup.rs:144`).
   Received: offsets and sizes. Missing: `track indicate`. [code]
3. **Options**:
   - *Reuse*: as 4.1; the selected phrase is already outlined. [code]
   - *Small addition*: one outline rectangle per contiguous run of strips, from
     `launcher_y − _scroll_y + scene_offset·launcher_height` over `num_scenes` rows, tagged
     `"launcher"` so `scroll_canvas` moves it with the pads (`:1713-1717`) and `tag_raise`d after
     `refresh_launchers`/`set_launcher_mode` lower the pads (`:1289`, `:1825-1826`). Redraw after
     `build_launchers` (`:1264-1280`). Same function as 4.1, ~20 extra lines. Not 64 pad
     outlines: `highlight_launcher` resets *all* pad outlines (`:1546-1547`), per-pad frames
     would fight with it. [code for the constraints, guess for the design]
   - *Qt*: the `Pad` delegate gets `inWindow` (chain in the window's chain ids and phrase in the
     range) and draws an inner border in the controller colour under the yellow selection
     border; or one Rectangle per run inside the pads' `Column` (it scrolls with
     `launcherScroll`). ~15 lines. [guess]
4. **Follow or mark**: mark; the launcher rows already scroll to the selected phrase. [guess]

### 4.3 Pattern editor

1. **Today**: nothing about controllers; the editor polls zynseq and shows the core's note
   changes within 0.2 s (§1.5). The stock drivers read the editor's cursor directly. [code]
2. **DbM information**: per step grid `scroll` and `step_size` (adapter has them); the pattern
   (cursor clip = selected phrase × active chain, `model.py:421-450`), which is also what the
   editor edits. Missing: which grid is on the pads, whether a sequencer view is active, and the
   **key rows** (the note sequencer's scale/octave window, the drum view's 16 pads): the core
   keeps them internally and reports nothing; the grids are 128 rows. [code]
3. **Options**:
   - *Reuse*: none.
   - *Small addition, steps only*: a vertical band (outline or a translucent stipple) over the
     window's steps on `grid_canvas`, full height, at `x = start·step_width`; drawn in
     `redraw_grid_pending` when the grid lines are rebuilt (`pated_base.py:1244-1275`) and on the
     signal. ~30 lines in `zynthian_gui_pated_notes.py`/`pated_base.py`. Which grid: the one
     that received the last grid command, or all grids whose step size matches the pattern's
     (heuristic, [guess]); without `track indicate` the adapter can't tell a session view from a
     sequencer view, so the band shows even in session mode unless it is shown only while the
     cursor clip's grids get commands. ~20 lines in the driver.
   - *Larger, core change*: report the active view's note window to the host, e.g. a new command
     `clip grid/<id>/view <firstKey> <numKeys>|-1` (or a set of keys for scales) sent by the
     sequencer views when they activate, deactivate or move (`abstract_note_sequencer_view.rs`,
     `abstract_drum_view.rs`, `drum64_view.rs`). Rust side ~40-80 lines plus a host feature so
     Bitwig and Reaper are not affected; then the editor can draw a rectangle (steps × keys).
     [guess]
   - *Qt*: a Rectangle in the grid's content item (`PatternEditorScreen.qml:124-…`), x/width
     from the step range, full height (or rows once keys are known); ~15 lines plus the model
     property. [guess]
4. **Follow or mark**: mark. Following (moving `step_offset` / `keymap_offset`) would move the
   user's view while they edit on the screen; the stock drivers never move it either. [guess]

### 4.4 Device parameters (for completeness)

`param/<i>/indicate` arrives already; it maps to `zynthian_gui_controller.set_hl`
(`zyngui/zynthian_gui_controller.py:389-396`) of the controllers on the control screen, but one
DbM page is two zynthian screens of four, only one of which is visible. Needs persistent flags in
the adapter (`device.py:174-175`). Lower priority than the three views. [code / guess]

## 5. A design for the small zynthian addition

Pull, not push, so a screen shown later draws the current state and several controllers combine:

1. `zyngine/ctrldev/zynthian_ctrldev_base.py`: `get_gui_window()` returning `None` by default,
   else a small immutable dict, e.g.
   `{"chains": (chain_id, …), "phrases": (first, count) | None, "steps": (pattern, first, count)
   | None, "mixer": bool, "launcher": bool, "color": "#RRGGBB", "name": str}`, and a helper
   `notify_gui_window()` that sends `S_GUI/SS_GUI_CTRLDEV_WINDOW` queued, without payload.
   ~20 lines. The stock zynpad/zynmixer drivers could return
   `chain_ids_filtered[scroll_h:scroll_h+cols]`, `(scroll_v, rows)` in ~10 lines each: the frames
   would then work for the stock Launchpads and the APC too (upstream value). [guess]
2. `zyngine/zynthian_signal_manager.py`: `SS_GUI_CTRLDEV_WINDOW = 8` (1 line; fits
   `last_subsignal = 10`). [code for the limit]
3. Shared module `zyngui/zynthian_ctrldev_window_data.py` (no toolkit imports, like
   `zynthian_pated_data.py`, `zynthian_midi_learn_data.py`): iterates
   `state_manager.ctrldev_manager.drivers.values()`, collects the non-None windows and gives each
   view what it draws: per mixer strip the controllers covering its chain (by chain id, so the
   main chain, mixbuses and pinned chains need no special case), per launcher cell the
   controllers covering chain × phrase, the step band of the pattern editor's pattern; each with
   the controller's colour and inset. Pure functions plus one "changed" check, so it can get unit
   tests without a UI. ~50 lines. [guess]
3a. Qt UI (first target): `SignalBridge` passes `SS_GUI_CTRLDEV_WINDOW` on (it passes no `S_GUI`
   signals today, §1.6); a small `QObject` model exposes the module's ranges as a list property.
   **The `border` of the strips and pads is already taken by the selection**
   (`zynqt/qml/MixerScreen.qml`): the active chain's strip has a 2 px border in the text colour,
   red while the chain is moved (`:137-139`), and the selected launcher pad a 2 px yellow border
   (`:375-376`); the Tk mixer outlines the selected pad in yellow the same way
   (`zyngui/zynthian_gui_mixer.py:221-224`). [code] So the controller window must not use the
   delegates' `border` (it would hide the selection or be hidden by it). Instead, one overlay
   `Rectangle` per controller **around the whole window** (the block of strips, or of strips ×
   phrases in launcher mode: one frame, not one per cell), in the controller's colour, drawn
   outside the cells' borders (or a coloured bar above the covered strips); the selection frames
   stay as they are and can lie inside the window. ~60-80 lines. [guess]
3b. Tk UI (later, through the same module): `zynthian_gui_mixer.py` and the pattern editor call
   it on the signal and in `build_view` and only move a few canvas items. [guess]
4. DBM driver (`backends/zynthian/ctrldev/zynthian_ctrldev_base_dbm.py`): after `bridge.tick` in
   `step` (`:345-358`), compute the window from `bridge.banks` and `self.model.track_list(...)`
   (chain ids of `[track_offset, track_offset+num_tracks)`), store it if it changed and call
   `notify_gui_window()`. ~30 lines. Later: `indication` in `TRACK_FEATURES`, persistent
   indication flags (sends, device parameters), `track indicate` stored, ~30 lines in
   `commands.py`/`model.py`/`device.py`.

Lines, roughly: zynthian base + signal 20-25, shared module 50, Qt 60-80; later Tk
mixer/launcher 50-70 and Tk pattern editor 20 on top of the shared module; DBM side 30 (offsets)
+ 30 (indication) + 20 (step band). [guess]

### Risks

- **Several controllers**: the pull design lists all drivers' windows; each needs its own colour
  (a per-driver palette entry by `idev`, [guess]); overlapping frames need an inset per
  controller (2 px each) or they hide each other. Two DbM controllers each have their own core
  and their own offsets (one `ZynthianModel` per driver, `zynthian_ctrldev_base_dbm.py:85-86`)
  but share zynthian's single selection. [code / guess]
- **Stock drivers' syncing**: a DbM controller and a stock driver in GUI_SEL both react to
  the same selection; nothing new. A UI that *follows* a controller would send
  `SS_GUI_VIEW_POS` after its scroll and move stock drivers in GUI_VIEW along with the DbM
  controller, and the user's own touch scrolling would be undone at the next page change: one
  more reason to only mark. The DbM core ignores the host's view anyway (`/track/offset`'s value
  is ignored, HOST_BRIDGE_GUIDE §2), so there is no loop as long as the DBM driver never maps
  `on_gui_view_pos` to bank commands. [code / guess]
- **Redraws**: the Tk mixer's full rebuild is expensive (~400 canvas items, ~80 ms on ARM, the
  comment at `zynthian_gui_mixer.py:1318-1319`); the frames must only `coords`/`itemconfig` a
  handful of items, never call `build_mixer`. Changes are rare (page changes), the driver
  publishes only on change, and the queued signal coalesces nothing by itself: the GUI callback
  should compare with what it drew. Qt: bindings on a small list property, read once per update
  (the lesson in `docs/qt-port-plan.md`, M0 results). [code / guess]
- **Threads**: the DBM tick thread only stores a value and sends a queued signal; Tk draws from
  the signal queue thread as all mixer callbacks do; Qt goes through `SignalBridge`. [code]
- **Conformance/regression tests**: turning on `indication` changes the command stream the core
  sends to zynthian; zynthian's tests (`backends/zynthian/tests/`, being edited by another agent
  now) may compare command logs. [guess]
- **Mapping**: index-based offsets would break with mixbuses and pinned chains; pass chain ids.
  [code]

## 6. Follow, mark, or both

- Stock drivers: the controller follows the GUI (GUI_SEL/GUI_VIEW) or pages itself (Mackie,
  Launchpad pads by CUIA arrows move the GUI selection); the GUI never shows a controller's
  window. [code]
- Bitwig with DrivenByMoss: frames mark the windows; selecting a track also makes it visible
  (`makeVisibleInArranger/Mixer` on `track <p>/select`, HOST_BRIDGE_GUIDE §4.4); paging the bank
  alone does not scroll Bitwig's views. [code for the select command, unverified for Bitwig's
  view behaviour on paging]
- zynthian with the DbM adapter already scrolls to the selected chain and phrase (§1.4), so the
  Bitwig behaviour is there except the frame.
- Recommendation: **mark only**; following stays tied to the selection. A "follow controller"
  option per driver could come later, but it conflicts with touch scrolling, with stock drivers
  in GUI_VIEW and with several controllers (whose window wins). [guess]

## 7. Recommendation: the first, smallest step

The first target is the **Qt UI** (`zynqt/`); the logic goes into the **common UI code**, the
toolkit-independent `zyngui/*_data.py` modules which the Qt port moved out of the Tk screens, so
that the Tk UI (and upstream) can use it later without a second implementation.

1. In zynthian (one patch, upstream-able): `SS_GUI_CTRLDEV_WINDOW = 8`,
   `zynthian_ctrldev_base.get_gui_window()` (None) and `notify_gui_window()`. ~25 lines.
2. The shared module `zyngui/zynthian_ctrldev_window_data.py`: the drivers' windows → ranges per
   view (mixer strips by chain id, launcher cells chain × phrase), with unit tests. ~50 lines.
3. Qt: `SignalBridge` passes the new signal on, a small model, and one overlay frame per
   controller around its block of strips (mixer) or strips × phrases (launcher). Not the
   delegates' `border`: that already marks the active chain (strip) and the selected pad
   (yellow), §5 3a. ~60-80 lines.
4. In the DBM driver: publish `chain ids of [track_offset, +num_tracks)` and `(scene_offset,
   num_scenes)` after each tick when they change. ~30 lines. No core change, no new host
   feature, so the command streams and tests stay as they are.

Then, in this order: the pattern editor's step band (shared module + Qt view); `indication` in
the adapter so the frame narrows to what the controls address and the launcher frame shows only
in session mode; the Tk mixer, launcher and pattern editor on the same shared module; last, a
core command for the sequencer views' key window.
