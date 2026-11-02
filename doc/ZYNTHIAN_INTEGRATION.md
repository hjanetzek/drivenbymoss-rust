# DrivenByMoss Rust core as a Zynthian ctrldev driver: integration points

Status: 2026-11-02, investigation (read only); nothing implemented. The Python binding and the
reference host it builds on: [PYTHON_HOST_PLAN.md](PYTHON_HOST_PLAN.md), `bindings/python/`.

- zynthian-ui: `~/bastel/github/zynthian-ui` (HEAD `d6ed3592`). Paths below are relative to it unless prefixed `dbm:`.
- drivenbymoss-rust: `~/bastel/github/drivenbymoss-rust`, prefixed `dbm:`.
- `zyncoder` (lib_zyncore, the C MIDI router) is a dangling symlink in zynthian-ui (`zyncoder -> ../zyncoder`).
  The only local copy, `/home/jeff/Code/Audio/zynthian/zyncoder`, is from 2020 and much older than this UI.
  **All statements about lib_zyncore's C-side behaviour are therefore unverified.** Only how the Python code calls it was checked.

Legend: **[code]** = read in the code; **[unverified]** = the API exists but I did not see what it does; **[guess]** = an inference.

---

## 1. Driver lifecycle

**Discovery** [code]
- `zynthian_ctrldev_manager` scans two folders under `$ZYNTHIAN_UI_DIR/zyngine/`: `ctrldev_user` first, then `ctrldev` (`zyngine/zynthian_ctrldev_manager.py:50-51`).
- It scans them once in `__init__`, and `__init__` runs from `state_manager.start()` (`zyngine/zynthian_state_manager.py:222`). `DRIVER_DEVELOPMENT` re-scans on every load, but it is hard-coded off (`zynthian_ctrldev_manager.py:40`).
- Every `*.py` is imported as `zyngine.<dirname>.<module>` (`:76-88`). Modules whose names start with `__` or `zynthian_ctrldev_base` are skipped (`:78`).
- **The class must have exactly the module's name**: `getattr(module, module_name)` (`:90`). So one module provides one driver class.
- The manager then builds `available_drivers[dev_id] -> [classes]` from each class's `dev_ids` list (`:96-102`). `"*"` means "generic".

**Matching** [code]
- `dev_id = zynautoconnect.get_midi_in_devid(izmip)`: the part of the port uid after the first `/` (`zynautoconnect/zynthian_autoconnect.py:281-292`).
- `build_midi_port_name` builds the uid for USB devices (`:1405-1490`):
  - `USB:<bus>.<ports>/<card name> IN <n>`.
  - `<card name>` is the first line of `/proc/asound/cardN/midi0`.
  - `IN <n>` is the 1-based index of the device's capture port.
  - So the dev_ids look like `"Launchpad X IN 1"` or `"APC40 mkII IN 1"`.
- **Matching is an exact dict lookup.** There are no patterns or regexes (`zynthian_ctrldev_manager.py:124-129`).
- **Autoload** happens when a new input port appears (`zynthian_autoconnect.py:1010-1023`), unless a ZS3 has a `midi_capture` entry for that uid, in which case the stored driver is loaded.
  - Autoload takes the first class in `available_drivers[dev_id]` whose `get_autoload_flag()` is true (`zynthian_ctrldev_manager.py:126-129`).
  - `"*"` drivers are never autoloaded. They are only offered in the menu (`zyngui/zynthian_gui_midi_config.py:503-505`).
  - The list order follows the scan order, so **a `ctrldev_user` driver wins autoload over a stock driver with the same dev_id** [code, from the insertion order of the dict].
- **Loading** (`zynthian_ctrldev_manager.py:104-176`), in this order:
  1. `izmop = dev_in_2_dev_out(izmip)`.
  2. `driver_class(state_manager, izmip, izmop)`.
  3. Unroute the port from the chains (`zmip_set_ui_midi_chans`).
  4. Register the driver in `drivers[izmip]`, so it already gets sysex during `init()`.
  5. `driver.init()`.

  Exceptions are logged and the driver is dropped.
- **Unloading** (`:178-202`) restores the routing, then calls `driver.end()`. It runs when:
  - the user unloads the driver from the MIDI config menu (`zynthian_gui_midi_config.py:563-570`, with `disable=True`, which is stored in the ZS3 as `disable_ctrldev`);
  - the device disappears (`zynthian_autoconnect.py:742-751`);
  - the state manager stops (`zynthian_state_manager.py:265`).
- **The output port.** `dev_in_2_dev_out` takes the input uid, does `name.replace("IN", "OUT")` and looks it up among the outputs (`zynthian_autoconnect.py:208-223`).
  - [guess, likely bug] `str.replace` replaces *every* "IN". A card named "APC MINI" (DBM's APCmini Mk1 discovery name) becomes "APC MOUTI OUT 1", so the driver would get no output port. Verify on a device.

**Can one generic "dbm" driver cover many devices?** Yes. There are two workable shapes:
- **(a) One class `zynthian_ctrldev_dbm`** whose `dev_ids` lists the Zynthian dev_ids of every supported controller.
  - `init()` reads `zynautoconnect.get_midi_in_devid(self.idev)` and picks the DBM UUID from a table.
  - Weaknesses:
    - Every device shows the same name and description in the menu, because `driver_name` and `driver_description` are class attributes (`zyngine/ctrldev/zynthian_ctrldev_base.py:68-69, 108-121`).
    - One autoload flag covers all devices.
- **(b) One thin module per controller**, generated from `dbm_list_controllers()`.
  - For example `ctrldev_user/zynthian_ctrldev_dbm_launchcontrol_xl.py` containing `class zynthian_ctrldev_dbm_launchcontrol_xl(DbmDriverBase): dev_ids=[...]; dbm_uuid="..."; driver_description="DrivenByMoss: Novation Launch Control XL"`.
  - The shared base must live in a module named `zynthian_ctrldev_base_dbm.py`, so the manager skips it (`zynthian_ctrldev_manager.py:78`).
  - This gives per-device names, descriptions and autoload flags. **Recommended.**
- **Port names do not come from the catalog for free.**
  - The `discovery.linux` names in `dbm_list_controllers()` are ALSA-seq or JACK port names, such as `"Launchpad X MIDI 2"`, `"Launchpad X LPX MIDI Out"` and `"Launch Control XL"`.
  - Zynthian's dev_id is `<card name> IN <port index>`.
  - The generator needs a mapping (`<card> MIDI <n>` becomes `<card> IN <n>`; a single-port device becomes `<name> IN 1`) plus hand fixes. [guess for each device: confirm on hardware with `/proc/asound/cardN/midi0`]
  - Examples:
    - DBM's Launchpad X uses the 2nd port ("MIDI 2", dbm:`src/controller/launchpad/definition.rs:376-380`), which is `"Launchpad X IN 2"`. Zynthian's own driver uses `IN 1` (`zyngine/ctrldev/zynthian_ctrldev_launchpad_x.py:44`).
    - The Launchpad Mini MK3 is the same case. Zynthian's chorder example already uses `"Launchpad Mini MK3 IN 2"` (`zyngine/ctrldev/examples/zynthian_ctrldev_launchpad_mini_mk3_chorder.py:44`).
    - The APC40 mkII is `"APC40 mkII IN 1"`, the same dev_id as the stock driver (`zyngine/ctrldev/zynthian_ctrldev_akai_apc_40_mk2.py:153`).
- **Multi-port controllers do not fit.** These are MCU/HUI with extenders, and the "MPC Live/Force" (2 inputs).
  - The ctrldev framework binds one driver to one zmip and one zmop.
  - The core numbers a multi-port controller's ports `0..count-1` in single mode (dbm:`src/app.rs:259-262`; the header only mentions device 0).
  - A driver would have to locate and claim the other zmips itself, with nothing in the manager to support that. Leave them out at first.

## 2. MIDI path

**Input** [code]
- The C router puts events for the UI into a ring buffer.
- The state manager's *fast thread* drains it every ~10 ms in `zynmidi_read` (`zynthian_state_manager.py:672-681, 709-745`).
- Events are 32-bit words: byte 0 is `izmip`, then up to 3 MIDI bytes.
- Sysex is put back together from consecutive words until `F7` (`:722-740`). An unterminated sysex at the end of a batch is dropped and logged (`:734-736`).
- `ctrldev_manager.midi_event(izmip, ev)` gets `ev` as `bytes`: a 1–3 byte short message (status first) or a complete sysex (`:745`, `zynthian_ctrldev_manager.py:266-277`).
- If the driver returns `True`, the state manager stops processing that event (no CUIA, MIDI learn or snapshot program change).
- This matches `dbm_on_midi_event` closely. Pad 2-byte messages to 3 bytes as HOST_BRIDGE_GUIDE §5 requires.

**Output** [code for the calls; C side unverified]
- `lib_zyncore.dev_send_midi_event(self.idev_out, bytes, len)` sends raw bytes, sysex included (`zynthian_ctrldev_base.py:127-129`; `zynthian_ctrldev_launchpad_x.py:47-51`).
- There are helpers `dev_send_note_on` / `dev_send_ccontrol_change` / `dev_send_pitchbend_change`.
- `send_midi_data(0, data)` maps to `dev_send_midi_event(self.idev_out, data, len(data))`.
- The Launchpad X driver sleeps 50 ms after every sysex (`zynthian_ctrldev_launchpad_x.py:51`). That suggests there are throughput or buffer limits on the output path. [unverified: size of the zmop ring buffer] The core's `flush` can send many LED messages per tick, and a Fire OLED update is a large sysex. Measure this early.

**Fully owning the device's MIDI** [code]
- `unroute_from_chains = True` makes the manager call `zmip_set_ui_midi_chans(izmip, 0xFFFF)` (`zynthian_ctrldev_manager.py:153-160`). An int is a per-channel mask.
- Every stock DAW-style driver uses this (`launchpad_x`, `apc_40_mk2`, `mackiecontrol.py:47`, ...).
- [unverified C-side] What it does is "only to the UI, not to the chains". The names and comments say so (`zynthian_ctrldev_base.py:62-65`), and the MIDI config GUI treats the port as "not available to chains" (`zynthian_ctrldev_manager.py:217-222`).
- So yes: a dbm driver can keep the device away from the chains and see everything itself.

**Note input: the core's filters, key and velocity tables, `process_midi_arg`.** There is no Bitwig-style "note input" object. Options:
1. **Python inject** (simplest, recommended for v1).
   - Keep `unroute_from_chains = True`.
   - In the driver thread, for every incoming message that matches a `set_note_input_filters` prefix, apply the key and velocity tables (−1 = drop).
   - Then send it to the active chain, e.g. `lib_zyncore.ui_send_note_on(chain.midi_chan, note, vel)` with `chain = chain_manager.get_active_chain()`, as `zyngine/zynthian_zcmidi.py:43-52` does for the UI keyboard.
   - `process_midi_arg` (chord notes) goes the same way.
   - [unverified] Whether `ui_send_*` goes to all chains on that channel. The launchkey mini mk4 driver instead uses `lib_zyncore.write_zynmidi(ev)` "to route to chains" (`zyngine/ctrldev/zynthian_ctrldev_launchkey_mini_mk4_37.py:243`). In the 2020 zyncoder, `write_zynmidi` wrote to the *UI* ring buffer, so check the current zyncoder.
   - Cost: latency and jitter up to the 10 ms fast-thread period plus the driver's processing.
2. **A realtime midiproc** (`midiproc_task`).
   - The base class forks a JACK client process.
   - zynautoconnect inserts it between the device and the zmip (`zynthian_ctrldev_base.py:167-212`; `zynthian_autoconnect.py:723-739`).
   - `zynthian_ctrldev_base_moder` shows a key remap done in it, with the mode index shared through `multiprocessing.Value` (`zyngine/ctrldev/zynthian_ctrldev_base_moder.py:75-174`).
   - The key and velocity tables could be shared as an `mp.Array('i', 128)`.
   - Problem: the driver then sees the midiproc's *output* (on the zmip), but the core needs the raw messages too (Bitwig: `setShouldConsumeEvents(false)`).
   - The midiproc would have to forward the raw stream and put translated notes onto a separate, chain-routed path. zynautoconnect only wires `out_1` to `ZynMidiRouter:dev<n>_in` (`zynthian_autoconnect.py:733-735`), so this needs either a second, self-connected port (and does autoconnect leave it alone? [unverified]) or a patch to zynautoconnect.
   - Worth it later for low-latency playing on Launchpad, Maschine or Fire.
3. A channel mask in `unroute_from_chains` is not enough. DBM's filters cover notes on all channels, and the translation tables cannot be expressed as a mask.

## 3. Timing and threading

- **Threads, not asyncio** [code]. I found no asyncio. The threads are:
  - the state manager's *slow* thread (0.2 s loop) and *fast* thread (10 ms, MIDI in) (`zynthian_state_manager.py:230-238, 547-681`);
  - the signal manager's queue thread "SIGNAL_QUEUE" (`zyngine/zynthian_signal_manager.py:224-240`), which runs all `register_queued` callbacks;
  - the Tk GUI thread;
  - zynautoconnect's threads.
- **`midi_event` runs on the fast thread** (`zynthian_state_manager.py:745`).
- Signal callbacks run on the SIGNAL_QUEUE thread (queued) or on the sender's thread (`zynsigman.send` without queueing, e.g. `zynlibs/zynmixer/zynmixer.py:252`).
- **There is no periodic hook suitable for 30 ms.**
  - `add_slow_update_callback` checks once per second (`zynthian_state_manager.py:683-692, 656-664`).
  - Its `remove_slow_update_callback` is broken: the loop variable shadows the argument (`:694-703`).
- Drivers already start their own threads:
  - `RunTimer`/`IntervalTimer`/`ButtonTimer` (`zyngine/ctrldev/zynthian_ctrldev_base_extended.py:52-232`);
  - an LED worker thread in `zynthian_ctrldev_sinco_smk25.py:197-218`.
- **Recommendation:** the dbm driver owns one `threading.Thread`, which makes **all** `dbm_*` calls. Each loop it waits on an `Event` with a 30 ms timeout, then:
  1. drains a `SimpleQueue` of MIDI that `midi_event` filled (then `midi_event` returns `True` at once);
  2. calls `dbm_tick`;
  3. collects the changes and calls `dbm_update_model`;
  4. calls `take_notifications` and `take_setting_changes`.

  This is the loop of `dbm:bindings/python/dbm_host/bridge.py:110-120` (`step`), with the real clock instead of `dbm_set_time`.
- **GIL** [code for the binding, behaviour by ctypes semantics]:
  - The binding loads the library with `ctypes.CDLL` (dbm:`bindings/python/drivenbymoss/_ffi.py:59`). ctypes releases the GIL during the foreign call, and the callbacks (CFUNCTYPE trampolines) take it back.
  - The core's mutex therefore cannot deadlock against the GIL.
  - It can deadlock against a *Python* lock: a callback that waits for a lock held by a thread that is blocked in a `dbm_*` call. With a single dbm thread, this cannot happen.
  - The binding raises `RuntimeError` on re-entry from a callback (dbm:`bindings/python/README.md`, "Threading").
  - Commands such as `zctrl.set_value` run synchronously in the callback on the dbm thread. That can fire non-queued signal handlers, e.g. the mixer GUI's `update_control` (`zyngui/zynthian_gui_mixer.py:1539`), on the dbm thread. Stock drivers already do the same from the fast thread, so this is accepted practice there.
  - [guess] At 33 Hz, Python-side diff reporting of a few hundred addresses should be cheap on a Pi 4/5, but it competes with the Tk GUI for the GIL. Not measured.
- `zynthian_ctrldev_base.py:34` calls `mp.set_start_method('fork')`. With `DBM_FLAG_HOST_TICK` the core has no thread of its own, so forking a midiproc child after the library is loaded is safe as long as the child never touches it [guess].

## 4. The DAW model mapping

Zynthian's main objects are:
- `state_manager.chain_manager` (chains = tracks);
- `state_manager.zynmixer_chan` and `state_manager.zynmixer_bus` (`zynthian_state_manager.py:143-144`);
- `state_manager.zynseq` (`:148`);
- `state_manager.audio_recorder` (`:152`).

Most values are `zynthian_controller` objects ("zctrls"). A zctrl has `.value`, `get_value()`, `set_value(v)`, `nudge(d)`, `toggle()`, `get_value2label()` (`zyngine/zynthian_controller.py:400-579`).
- **There is no general change signal on a zctrl.** The single `send_value_cb` slot (`:381-385, 521-525`) has one owner, so don't take it.
- Signals exist for the mixer (`SS_ZYNMIXER_SET_VALUE`), zynseq play states, tempo, time signature and beat, chain add/remove/move/rename/active, ZS3 and snapshot loading, and the audio recorder (`zynthian_signal_manager.py:41-124`).
- **Recommended observation strategy: poll and diff every 30 ms**, as the DBM reference `Reporter` does (dbm:`bindings/python/dbm_host/reporter.py`). Use signals only to request a full dump: `SS_LOAD_SNAPSHOT`, `SS_LOAD_ZS3` and the chain add/remove signals send everything again.

| Core area | Zynthian equivalent, read | Commands map to | Gaps |
| --- | --- | --- | --- |
| **Tracks** (`/track/<p>/…`) | Chains in display order: `chain_manager.get_chain_ids_filtered(filter)` / `chains[id]` (`zyngine/zynthian_chain_manager.py:641-659`). The base class keeps `chain_ids_filtered` (`zynthian_ctrldev_base.py:279-285`). Name: `chain.get_title()`/`get_name()` (`zyngine/zynthian_chain.py:227, 298`). Type: `is_synth/is_audio/is_midi/is_mixbus` (`:499-532`), mapped to `INSTRUMENT/AUDIO/…`. Select: `chain_manager.get_active_chain()` (`:537`), signal `SS_SET_ACTIVE_CHAIN` (`:977`). Color: none per chain; `zynthian_gui_config.LAUNCHER_COLOUR[midi_chan]` (`zyngui/zynthian_gui_config.py:662`) can serve | `track p/select` → `chain_manager.set_active_chain_by_id` (`:951`). `track p/name` → `set_chain_title` (`:469`). Move → `move_chain` (`:494`). `remove` → `remove_chain` (`:365`) | No group tracks or depth. Track creation (`addTrack`) needs engine selection (`add_chain` + `add_processor`, `:220, 1054`), so leave it out at first |
| **Volume, pan, mute, solo, arm, VU** | `chain.zynmixer_proc.controllers_dict[...]`: `level` (0..1, default 0.8), `balance` (−1..1), `mute`, `solo`, `record` (arm for the audio recorder), plus `pfl`, `mono`, `phase` (`zyngine/zynthian_engine_audio_mixer.py:60-160`). Direct calls: `zynmixer.get_level/get_balance/get_mute/get_solo` (`zynlibs/zynmixer/zynmixer.py:276-450`). Signal `S_MIXER/SS_ZYNMIXER_SET_VALUE(chan, symbol, value, mixbus)` (`zynmixer.py:252` etc.). VU: `ZynMixer.dpm` structs (`a`, `b`, `a_hold`, `b_hold`) after `update_dpm_states()` (`:39-46, 857-882`) | `zctrl.set_value` / `nudge` / `toggle`. The helpers `set_mixer_param` / `nudge_mixer_param` / `toggle_mixer_param` (`zynthian_ctrldev_base.py:547-642`) work by filtered index, with −1 for the main chain. Pan: `balance = 2*v-1`. `valueInc` maps to `nudge`, `valueReset` to `zctrl.reset_value()` (`zynthian_controller.py:608`) | Display texts: build dB from `zynmixer.norm_to_db` (`zynmixer.py:138`). [unverified] the DPM scale against the core's 0..1 VU |
| **Master** | Chain 0, "Main", whose `zynmixer_proc` is the main mixbus `MR` (`zynthian_state_manager.py:226`; `zynthian_chain.py:92-94`). The base class uses `chains[0]` (`zynthian_ctrldev_base.py:555-556`) | As tracks, on chain 0 | – |
| **Sends / FX tracks** | Mixbus chains (`is_mixbus`). Per input strip, the zctrls `send_<mixbus_chain_id>_level` / `_mode` are built by `chain_manager.refresh_mixbus_sends` (`zynthian_chain_manager.py:1266-1340`). Direct: `zynmixer.get_send_level(chan, send)` (`zynmixer.py:801`). Mixbus chains map well to the core's `/fxtrack/…` | `set_value` on `send_<id>_level` | The send index is `mixer_chan - 2` of the return (`:1309`), so map the order explicitly. `send/<s>/active` (a toggle) has no direct equivalent; `send_<id>_mode` is pre/post |
| **Scenes / clip slots** | zynseq: rows are *phrases* (DBM scenes), columns are MIDI channels (one sequence per chain's `midi_chan`). Clippy uses channels 16–31, and the phrase launcher is `PHRASE_CHANNEL = 32` (`zynlibs/zynseq/zynseq.py:105-108`). State: `zynseq.state["scenes"][zynseq.scene]["phrases"][phrase]["sequences"][chan]` with `state`, `group`, `repeat`, and patterns for "empty" (see `zynthian_ctrldev_base.py:411-464` for the exact recipe). Play states `SEQ_STOPPED/PLAYING/STARTING/STOPPING…` (`zynseq.py:92-99`) map to `playing`/`playQueued`/`stopQueued`. Signals `S_STEPSEQ/SS_SEQ_PLAY_STATE(phrase, chan)`, `SS_SEQ_STATE`, `SS_SEQ_SELECT_PHRASE` (`zynseq.py:674-710`) | Slot launch → `zynseq.libseq.togglePlayState(zynseq.scene, phrase, midi_chan)` (as `zynthian_ctrldev_launchpad_x.py:133`). Scene launch → `togglePlayState(scene, phrase, PHRASE_CHANNEL)` (`:155`). Select → `zynseq.select_phrase` + `libseq.selectSequence` (`zynthian_ctrldev_base_ui.py:47-51`). Insert/duplicate/remove a scene → `insert_phrase/duplicate_phrase/remove_phrase` (`zynseq.py:373-409`). Stop → `setPlayState(..., SEQ_STOPPED)` (`zynlibs/zynseq/zynseq.h:964`) | Launch is a *toggle*; "launch" on a playing slot stops it [guess at what the core expects; check against Bitwig semantics]. Audio-only chains have no slots unless clippy. No `recording` / `recQueued` / clip record. Slot names: `get_sequence_name` (`zynseq.py:363`) |
| **Devices / parameter pages** | Processors of a chain: `chain.get_processors()` (`zynthian_chain.py:592`), cursor = `chain.current_processor` (`:77, 848`). Pages: `processor.get_ctrl_screens()`, a dict of name → list of up to 4 zctrls (`zyngine/zynthian_processor.py:615-694`). The core's 8-parameter pages are 2 Zynthian screens, or one screen of 4 with 4 empty. Bypass: `is_bypassed()` / `set_bypass` / `toggle_bypass` (`:752-778`). Signal `SS_PROCESSOR_CTRL_SCREENS` when the screens change (`:633`) | `device param/i/value` → `zctrl.set_value(min + v*(max-min))` or `nudge`. `device +/-` and `selected` → `chain.set_current_processor`. `parameterPage` → `processor.set_current_screen_index` (`:671`). Bypass → `toggle_bypass` | No drum pads or layers (Fabla keymaps exist, `:798`). `device window/expand` has no meaning. Value normalization differs per zctrl (`value_min/max`, logarithmic, `ticks`); use `get_value2label` for `/str` |
| **Transport / tempo** | Tempo: `zynseq.get_tempo()` / `zctrl_tempo` (`zynseq.py:622-631`), signal `SS_SEQ_TEMPO` (`:642`). Time signature: `zynseq.bpb` (beats per bar; the denominator is effectively 4) with `SS_SEQ_TIMESIG`. Beat: `libseq.getBeat()` with `SS_SEQ_BEAT`. Record: `audio_recorder` with `SS_AUDIO_RECORDER_STATE` (`zyngine/zynthian_audio_recorder.py:65-129`). **No Python getter for "transport rolling"** was found. Options: zynautoconnect's JACK client `transport_state` [unverified], or "any sequence playing" (`libseq.getPlayingSequences`) | `tempo` → `zynseq.set_tempo`, `tempo +/-` → `nudge_tempo`, `tapTempo` → `tap_tempo` (`:622-631`). `play` → `zynseq.transport_toggle("dbm")` (`:618`) or the audio player (`cuia_toggle_audio_play`, `zyngui/zynthian_gui.py:1524`). `record` → `audio_recorder.toggle_recording()`. Metronome → `libseq.setMetronomeMode/Volume` (`zynseq.h:1378-1393`) | What "play" means is a design decision: Zynthian's play/stop CUIAs are context-dependent (`zynthian_gui.py:1587-1624`). No arranger: no position, loop/`repeat`, punch or automation. Undo/redo only exists for patterns (`zynseq.h:145-154`) |
| **Note clip / step grid** | The selected sequence's pattern: `libseq.getPattern(scene, phrase, chan, 0, 0)` (`zynseq.py:807`), `selectPattern`, `getSteps`, `getStepsPerBeat`, `getNoteVelocity`/`getNoteDuration`/`getNotePlayChance`/stutter, `getPatternPlayhead` (`zynseq.h:270-652`) | `…/step/<ch>/<x>/<y>/toggle` → `addNote(step, note, vel, dur)` / `removeNote` (`zynseq.h:295-305`). Attributes → `setNoteVelocity`, `setNotePlayChance`, `setNoteStutter*`. `clearAll` → `clearPattern` | Report the host step grid (24 values): velocity, duration and chance map; the rest are defaults. One pattern per sequence track. The pattern editor GUI does not follow pattern changes (`zynthian_ctrldev_base_ui.py:53-64` refreshes it by hand) |
| **Browser** | Processor bank/preset lists: `get_bank_list`, `load_preset_list`, `set_bank`, `set_preset` (`zynthian_processor.py:198-470`) | Could map to columns = banks and results = presets | Optional. The core's browser model is Bitwig-shaped (content types, 7 filter columns), so it needs a custom mapping. Skip in v1 |
| **Markers** | – | – | None in Zynthian. Leave `numMarkers` unreported |

Reusing the DBM reference host:
- `Reporter` and `CommandHandler` (dbm:`bindings/python/dbm_host/`) are written against `SimDaw`'s object graph (`Channel`, `Track`, `Slot`, `Scene`, `Parameter`; dbm:`model.py:97-700`).
- A `ZynthianDaw` adapter exposing the same attributes as live properties over chains, zynmixer and zynseq could reuse both and their host-feature logic (re-report after value commands, `/host/features`).
- [guess] This is less work than a new reporter, but `dbm_host` is a reference/test host, not a stable API.

**Host features for v1.**
- Report `/track/offset`, `/scene/offset` and `/host/features valueInc valueReset soloToggle` (keep the list small).
- Report all chains flat with absolute positions, as the Python reference host does (dbm:`doc/HOST_BRIDGE_GUIDE.md` §6 rule 4).

## 5. State and settings

- **Per-ZS3 driver state** [code].
  - `ctrldev_manager.get_state_drivers()` collects `driver.get_state()` per uid (`zynthian_ctrldev_manager.py:224-234`).
  - It ends up in `zs3[...]["midi_capture"][uid]["ctrldev_state"]` (`zynthian_state_manager.py:2058-2088`), which is saved with each ZS3 and so with the snapshot.
  - On ZS3 load, `set_midi_capture_state` reloads the stored driver class. That is a no-op if it is already loaded (`zynthian_ctrldev_manager.py:140-143`). Then it calls `set_state` (`zynthian_state_manager.py:1530-1532, 2147-2191`).
  - **Map:** `get_state()` returns `{"dbm_document": app.get_formatted_document_settings(), ...base state}`, and `set_state()` calls `app.set_formatted_document_settings(...)`.
  - The "document" therefore follows the ZS3, which changes more often than a DAW project [guess at whether that is wanted]. The base state holds `scroll_mode` (`zynthian_ctrldev_base.py:317-330`). Keep it or drop it.
- **Global settings.**
  - Use the core's own `<model>.config` in `config_path`. Point it to `$ZYNTHIAN_CONFIG_DIR/ctrldev/dbm/`; the Mackie driver keeps its YAML in `$ZYNTHIAN_CONFIG_DIR/ctrldev` (`zyngine/ctrldev/zynthian_ctrldev_mackiecontrol.py:49-50, 129-137`).
  - Alternatively, store `get_settings()` values with global scope in a JSON file there and push them with `set_setting` at start (HOST_BRIDGE_GUIDE §1, step 5).
- **A settings UI.**
  - There is no generic per-driver options UI. "Controller Options" only offers the scroll modes and is hard-coded (`zyngui/zynthian_gui_midi_config.py:606-635`). It also contains a bug: `set_scroll_mode(cal)` instead of `val` (`:633`).
  - A counterpart of `dbm_get_settings` / `dbm_set_setting` needs one of:
    - a small zynthian-ui patch: e.g. `show_controller_options` asks the driver for `get_options()` and builds the option list from it, using the `option` screen with enums, booleans and integers;
    - a webconf page (zynthian-webconf, not inspected);
    - menus on the controller itself.

## 6. UI feedback

- **Notifications** [code].
  - `zynsigman.send_queued(zynsigman.S_GUI, zynsigman.SS_GUI_SHOW_MESSAGE, message=...)` shows the text in the current screen's title bar (`zyngui/zynthian_gui.py:2732, 2801-2805`). Drivers already use it (`zyngine/ctrldev/zynthian_ctrldev_worlde_mini_moder.py:90`).
  - This is a good fit for `dbm_take_notifications()`.
  - `zyngui.show_info(text, tms)` (`zynthian_gui.py:847-854`) takes over the whole screen, so it is too intrusive.
- **Controller displays.**
  - Text and graphic displays that the core drives over MIDI (MCU LCD sysex, Fire OLED sysex) pass through `send_midi_data`, so through `dev_send_midi_event`. Throughput is the risk (§2).
  - Maschine Mk2 and JAM are HID devices on Linux. They need `dbm-nihost` (ALSA ports, which Zynthian sees as ordinary MIDI devices) or the planned in-process route (dbm:`doc/NI_HOST_LINUX_PLAN.md` §4.1-4.2, step 9 open).
- Showing the controller's display contents on Zynthian's screen would need GUI work. None exists.

## 7. Packaging

- **Native library.**
  - Cross-build `cargo build --release --target aarch64-unknown-linux-gnu`, as BINDINGS_PLAN.md intends (dbm:`doc/BINDINGS_PLAN.md:93-94`).
  - [guess] Zynthian OS is Raspberry Pi OS / Debian bookworm, arm64.
  - Install `libdrivenbymoss.so` e.g. under `/zynthian/zynthian-sw/drivenbymoss/` or `/usr/local/lib`.
- **The Python binding.**
  - It is pure ctypes and needs Python ≥ 3.11 (dbm:`bindings/python/README.md`). [guess] Bookworm ships 3.11.
  - It finds the library through `DBM_LIBRARY`, then a repository's `target/{debug,release}` (dbm:`bindings/python/drivenbymoss/_ffi.py:39-63`). The driver should `os.environ.setdefault("DBM_LIBRARY", ...)` before the import, or the binding could take an explicit path.
  - Deploy the `drivenbymoss/` package (and `dbm_host/` if it is reused) to site-packages, or vendor it next to the driver.
- **Where the drivers live.**
  - **User drivers can be added without patching zynthian-ui.** Drop the modules into `$ZYNTHIAN_UI_DIR/zyngine/ctrldev_user/` (`zynthian_ctrldev_manager.py:50-51`).
  - It is imported as the namespace package `zyngine.ctrldev_user` (no `__init__.py` needed, Python ≥ 3.3) [code for the import; guess for the namespace behaviour on the target].
  - The folder is inside the zynthian-ui git checkout but not tracked. `.gitignore` does not list it [guess: a `git pull` update leaves it alone].
  - Drivers are scanned only at UI start (§1), so a restart is needed after install.
- **Distribution** [guess]. A Debian package or an install script that:
  - places the `.so` and the Python packages;
  - generates the per-controller driver modules from `dbm_list_controllers()`;
  - creates `$ZYNTHIAN_CONFIG_DIR/ctrldev/dbm/`.

## 8. Recommendation

**Smallest first integration: Novation Launch Control XL** (dbm UUID from `dbm_list_controllers()`, "Novation | Launch Control XL").
- No stock Zynthian driver exists for it, so there are no conflicts.
- It is single-port and has no note input that matters.
- It is already a conformance target of the core.
- It exercises zynmixer through the faders, knobs (sends/pan/device) and buttons (mute/solo/arm/select), plus track paging.

**Second step:** a Launchpad (X or Mini MK3, port `IN 2`) on zynseq. That brings in scenes/slots, then note input (§2, option 1) and the step grid.

**Sketch of `zynthian_ctrldev_base_dbm.DbmDriverBase(zynthian_ctrldev_zynmixer, zynthian_ctrldev_zynpad)`** (prose and pseudocode):
```
class DbmDriverBase(zynthian_ctrldev_zynpad, zynthian_ctrldev_zynmixer):
    dev_ids = []                 # set by each generated subclass
    dbm_uuid = None              # set by each generated subclass
    unroute_from_chains = True   # the core owns the device's MIDI
    autoload_flag = False        # opt-in at first: don't hijack devices that have stock drivers

    __init__(state_manager, idev_in, idev_out):
        super().__init__(...)
        self.inq = SimpleQueue(); self.wake = Event(); self.stop = False
        self.note_inputs = {}        # index -> filters, keys[128], velocities[128]
        self.daw = ZynthianDaw(state_manager, self)   # adapter: chains/zynmixer/zynseq as the core model
        self.reporter, self.commands = Reporter(self.daw, ...), CommandHandler(self.daw, ...)

    init():                       # called by the manager; do NOT call super().init()'s refresh-LED logic
        register queued signals (SS_LOAD_SNAPSHOT, SS_LOAD_ZS3, chain add/remove/move) -> self.reporter.request_dump()
        self.thread = Thread(target=self.run, daemon=True, name="dbm"); start it

    run():                        # the only thread that calls dbm_*
        app = drivenbymoss.App.create_controller(self, self.dbm_uuid, CONFIG_DIR, "Zynthian <version>")
        app.start_infrastructure(); self.banks.configure(app.get_model_setup())
        push stored global settings (set_setting), discard take_setting_changes once
        loop until self.stop:
            self.wake.wait(0.03); self.wake.clear()
            while inq not empty: app.on_midi_event(0, pad3(ev))
            app.tick()
            lines = self.reporter.collect(); if lines: app.update_model(lines)
            for n in app.take_notifications(): zynsigman.send_queued(S_GUI, SS_GUI_SHOW_MESSAGE, message=n)
            store app.take_setting_changes()
        app.close()               # dbm_destroy: writes final MIDI and the .config

    midi_event(ev):               # fast thread
        self.inq.put(bytes(ev)); self.wake.set()
        (v1 note input) if it matches a note-input filter: translate with keys/velocities and
            lib_zyncore.ui_send_note_on(active_chain.midi_chan, ...)  [routing unverified]
        return True

    end(): self.stop = True; self.wake.set(); join the thread; unregister signals; super().end()

    # drivenbymoss.Backend callbacks (run on the dbm thread, inside dbm_* calls; never call app.* here)
    open_midi_input/output(id): return id == 0
    send_midi_data(id, data):   lib_zyncore.dev_send_midi_event(self.idev_out, data, len(data))
    process(processor, command, value): self.commands.handle(...)   # zynmixer / zynseq / chain_manager calls
    process_midi_arg(id, s, d1, d2):    inject like a played note
    set_note_input_filters / key_translation_table / velocity_translation_table: store in self.note_inputs
    log(msg): logging.debug

    get_state():  {"dbm_document": <cached formatted document settings>}   # cached by the dbm thread
    set_state(s): queue set_formatted_document_settings for the dbm thread
    sleep_on/off: stop the tick and blank the LEDs / resume with a refresh (no counterpart in the C ABI)
    light_off():  no-op (the core owns the LEDs)
```
Notes on the sketch:
- `get_state` and `set_state` are called from other threads (state manager, GUI). They must not call `dbm_*` directly; they cache or queue for the dbm thread.
- Do not use the zynpad and zynmixer base classes' LED logic (`update_pad`, `update_mixer_strip`, `refresh` lighting). Inherit them only for `dev_zynpad`/`dev_zynmixer` flags and helpers, or inherit `zynthian_ctrldev_base` only.
- The generated subclass per device: `class zynthian_ctrldev_dbm_launchcontrol_xl(DbmDriverBase): dev_ids=["Launch Control XL IN 1"]; dbm_uuid="…"; driver_description="DrivenByMoss: Launch Control XL"`. The dev_id is [guess]: confirm the card name on hardware.

**Main risks**
1. **The MIDI output rate and sysex sizes** through `dev_send_midi_event`. Stock drivers sleep after sysex, and the C ring-buffer size is unknown (zyncoder not available). LED storms or OLED frames could be dropped or delayed.
2. **Note-input latency and routing.** The Python inject path adds 0–10 ms of jitter plus processing, and which lib_zyncore call reaches the right chain is unverified. The realtime route needs a midiproc and probably a zynautoconnect change.
3. **Model mismatch.**
   - Zynthian has no arranger, loop, markers, groups or per-chain colors.
   - Launch semantics are a toggle.
   - "Play" is ambiguous (zynseq transport vs the audio player).
   - The 4-per-screen parameter pages differ from the core's 8-per-page.
   - Controllers degrade where the host lacks a feature. The core tolerates missing reports and ignored commands (dbm:`doc/BACKEND_PROTOCOL.md` §5), but some views will look empty.
4. **Python cost at 33 Hz on the Pi**, competing with the Tk GUI for the GIL. Not measured.
5. **Device naming.** The dev_id ↔ DBM port mapping must be built and checked per device, and `dev_in_2_dev_out`'s `replace("IN","OUT")` breaks names that contain "IN" (§1).
6. **Multi-port controllers** (MCU/HUI with extenders, MPC) do not fit one-zmip drivers.
7. **Documents follow ZS3s**, which changes more often than a DAW project.
8. [guess] Thread safety of calling chain_manager, zynmixer and zynseq from the driver thread. The stock drivers do the same from the fast thread, but there is no locking discipline in the core Zynthian code.

**Conflicts with or replacements of existing Zynthian drivers** (stock drivers are in `zyngine/ctrldev/`)
- **Same dev_id, so both appear in the menu and only one can be loaded per port:**
  - `zynthian_ctrldev_akai_apc_40_mk2` (`"APC40 mkII IN 1"`).
  - With `autoload_flag=True`, a `ctrldev_user` driver would win the autoload (§1). Keep `autoload_flag=False` or let the user choose.
- **Same physical device, different port, so both can be loaded and both send mode sysex to one device:**
  - `zynthian_ctrldev_launchpad_x` (IN 1, DAW session mode) vs DBM on IN 2;
  - `zynthian_ctrldev_launchpad_mini_mk3` (IN 1) vs DBM on IN 2;
  - `launchpad_pro_mk3` (`"Launchpad Pro MK3 IN 3"`), `launchpad_pro_mk2` (`"Launchpad Pro IN 1"`) and `launchpad_mini` (`"Launchpad Mini IN 1"`), against DBM's Launchpad Pro / Pro Mk3 / MkII. Check each port.

  The user must disable the stock driver on the other port (unload with "disable", which is persisted as `disable_ctrldev`), or the dbm driver claims both zmips.
- **Generic `"*"` drivers:**
  - `zynthian_ctrldev_mackiecontrol` (`dev_ids=["*"]`, YAML config) would be replaced by the DBM MCU/HUI drivers for single-unit setups.
  - It is never autoloaded, so it does not conflict, but it is offered on every port.
- **No conflict** (DBM has no driver for them): the Launch Control XL and Fire (no stock driver), and Launchkey MK3 88 / Launchkey Mini (no DBM implementation, so the stock drivers stay).

## 9. Zynthian bugs found

Found while reading zynthian-ui (HEAD `d6ed3592`); checked in the code, not reported upstream yet.

| # | Where | Bug | Effect |
| --- | --- | --- | --- |
| Z1 | `zynautoconnect/zynthian_autoconnect.py:220` (`dev_in_2_dev_out`) | `name.replace("IN", "OUT")` replaces every "IN" in the port name, not only the port suffix | The output of a device whose name contains "IN" is not found, e.g. "APC MINI IN 1" becomes "APC MOUTI OUT 1"; its driver gets no output |
| Z2 | `zyngine/zynthian_state_manager.py:693-703` (`remove_slow_update_callback`) | The loop variable `cb` shadows the argument, and `cb.remove(cb)` is called on the entry instead of the list | No callback is ever removed; `add_slow_update_callback` (which calls it first) can add a callback twice |
| Z3 | `zyngui/zynthian_gui_midi_config.py:633` | `set_scroll_mode(cal)` instead of `val` | Setting the scroll mode of a driver always fails (NameError, caught and logged as a warning) |
