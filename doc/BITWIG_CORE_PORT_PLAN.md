# Porting the Bitwig framework classes into the core: plan for an implementation agent

Status: 2026-10-30, analysis done, implementation not started.

## Goal

The core (`rust/src/daw/`) was ported from the Java DrivenByMoss **for Reaper**
(`src/main/java/de/mossgrabers/reaper/framework/daw/`). On Bitwig it runs through the extension
(`rust/backends/bitwig/src/main/java/de/mossgrabers/dbm/bitwig/`: `BitwigModel` reports the state,
`CommandHandler` executes the core's commands). The conformance runs against the Java version for
Bitwig keep finding places where the core does what Reaper's classes do and Bitwig's classes do
something else (see `BITWIG_DIVERGENCES.md`, the per-controller `tests/conformance/*/CONFORMANCE.md`).

Port the classes of the Java version **for Bitwig**
(`~/bastel/github/DrivenByMoss/src/main/java/de/mossgrabers/bitwig/framework/daw/`, `data/`,
`data/bank/`, and the shared base classes in `framework/daw/`) into the core **as they are**,
replacing the current Reaper-derived implementations, and extend the extension so that it
reports every Bitwig API value and executes every Bitwig API call these classes use.

Work on a **git worktree** (branch e.g. `bitwig-core`, from `rust-port`), so that the result can be
compared with the current implementation (see Verification).

## Rules

- The Java Bitwig class is the specification. Translate its logic method by method; where it
  calls the Bitwig API, the core sends a command which the extension executes with exactly that
  call (`inc`, `reset`, `toggle`, `setIndication`, `setImmediately`, …), and where it reads the
  API, the extension reports the value and the core keeps it. Do not compute in the core what
  Bitwig provides (e.g. `hasPrevious`, `isMonitoring`, `canHoldNoteData`, `getLimited`).
- Keep the structure and names of the Rust code (one Rust type per Java class as now); comments
  name the Java class and method (`// TransportImpl.stopAndRewind`).
- **Reaper:** the core also runs on Reaper (`backends/reaper`). Replacing the implementations
  breaks the Reaper behaviour where the Reaper backend lacks the new commands and values. Decide
  per class and note it in the class's section of `BITWIG_DIVERGENCES.md`:
  - default: replace, and list what the Reaper backend would need in a section "Reaper backend
    follow-ups" at the end of this file;
  - only where a Reaper test of `cargo test` encodes Reaper semantics that the Reaper backend
    really needs, keep the Reaper path behind the host check (`DawContext::is_host_paging()`, which
    is true on Bitwig; better: add a `DawContext::host()` kind set from the model setup).
- Every protocol addition goes into `BITWIG_DIVERGENCES.md` §6 and the parser
  (`rust/src/parser.rs`) gets a test for it.
- LF line endings only in `rust/`. Commit only when the user asks; never push.
- `cargo test` must pass at the end; replays which change because of a corrected behaviour are
  recorded again (see Verification), not adjusted by hand.

## Already done (2026-10-30, keep them)

Host paging of the banks (`DawContext::is_host_paging`), `select_previous_page` selects the first
item, `can_scroll_page_backwards` from offset > 0, automation write switches, overdub switches,
tempo steps 0.01/1, cursor device pinned, shortened texts (`getLimited`) for volume, pan, sends
and parameters, playing notes of the selected track, raw MIDI to the note input
(`sendRawMidiEvent`), track selection change detection, re-reporting values 300 ms after a value
command (takeover mode), `setImmediately`, EQ+ via the Reaper EQ layout, delayed notification end.

## Order of work

Start with **P1** (bugs which ported controllers hit today), then P2 (functions which ported
controllers use but which do nothing or the wrong thing on Bitwig), then P3 (completeness).

### P1: bugs

1. **Slot commands address the wrong track when the track bank is scrolled.**
   `Slot::send_track_clip` (and `set_color`, `paste` incl. its source, `insert_file`) sends
   `"{track_index}/clip/{pos}/…"` with the page index (`Track::set_index` →
   `slot_bank.set_track_info(channel.index)`); `CommandHandler.handleTrack` reads it as the
   absolute position. With offset 8 nothing happens, with offset 1 the clip of the track left of
   it is launched. Use the track position (`SlotImpl` acts on its own `ClipLauncherSlot`).
2. **`select_previous_item` at the first item selects item 0 of the previous page** (regression of
   today's patch): Bitwig `AbstractItemBankImpl.selectPreviousItem` selects
   `getPageSize () - 1` after 75 ms; only `selectPreviousPage` selects item 0. `PagedBank`,
   `TrackBank`, `SceneBank`.
3. **`TrackBank::scroll_to` selects a track:** it sends `<pos>/scrollto`, which the extension
   handles like `select`. Bitwig `scrollTo` only sets `scrollPosition` (exact, `adjustPage`
   ignored). With host paging send `bank track <position>`.
4. **Selections right after a page change are dropped for scenes:** `PagedBank::select_next_page`,
   `select_previous_page`, `select_item_at_position` select at once; the extension still has the
   old window. Bitwig schedules the select after 75 ms (as `TrackBank` already does).
5. **Relative value changes are sent as absolute values:** `Parameter::inc` / `change_value`
   (all kinds: device parameters, volume, pan, sends, EQ, groove) compute `local + increment` and
   send `…/value`; the extension calls `set`, which the takeover mode (catch) can swallow, and
   fractional increments are cut (`as i32`). Bitwig: `rangedValue.inc (increment, upperBound)`.
   Add `…/value/inc <delta>` → `value ().inc (delta, upperBound)`. Same for the clip's
   `changePlayStart/End`, `changeLoopStart/Length` (`getPlayStart ().inc (…)`).
6. **Reset sets Reaper's defaults:** `Parameter::reset_value` sets 0 (device parameters), 0.716
   (volume), 0 (sends). Bitwig: `parameter.reset ()`. Add `…/reset` for parameters, volume, pan,
   sends, EQ.
7. **Window, expand, bypass, remove, duplicate of the specific devices and of siblings:**
   `SpecificDevice` sends `window|expand` to `Processor::Device` for all kinds (opens the cursor
   device's window for the first instrument) and `<index+1>/bypass|remove|duplicate` with the
   window index, which `handleDeviceIndexed` matches against the absolute position (wrong from
   position 8 on, unhandled for `primary|drum|eq`). Bitwig acts on the device itself
   (`isWindowOpen ().toggle ()`, `isEnabled ().toggle ()`, `deleteObject ()`,
   `afterDeviceInsertionPoint ().copyDevices (device)`, `selectInEditor ()`).
8. **Step attribute edits write all attributes and halve the gain:** `update_step` /
   `send_clip_data` send the whole `to_wire`, `BitwigNoteClip.update` sets all 21 attributes, and
   `gain` is reported as `gain () / 2.0` but written with `setGain (value)`. Bitwig
   (`CursorClipImpl`) writes only the changed attribute outside of an edit session
   (`getNoteStep (pos).setVelocity (v)`), all of them in `startEdit/stopEdit`. Check the gain range
   against the API; send per-attribute commands.
9. **Step attribute knob changes lose fine steps:** `change_step_*` cast
   `calc_knob_change (control) as i32`; Bitwig keeps the double.
10. **`create_note_clip` / `record_note_clip`:** Rust starts arranger recording and does not launch;
    Bitwig `track.createClip; slot.select (); slot.launch (true, false)`, overdub via
    `setLauncherOverdub (true)`. `record_note_clip` sends `recordClip`, which the extension does
    not handle (Bitwig: `if (!slot.isRecording ()) slot.startRecording (); slot.launch (true,
    false)`).

### P2: functions ported controllers use

| Area | Rust now | Bitwig (port this) | Extension |
| --- | --- | --- | --- |
| Pinned track | core keeps the pin, sends `track <n>/pin` (unhandled); Bitwig's cursor track keeps following | `CursorTrackImpl`: `cursorTrack.isPinned ()` | report `/track/pinned`, command `track pinned` |
| Add track | `track addTrack <type> <name> <color> <devices…>` as string args, dropped by `DbmExtension.processStringArgs` | `TrackBankImpl.addChannel`: `application.createInstrument/Audio/EffectTrack (-1)`, then name, color, devices after 300 ms | handle the command; device names are Reaper creation names: map to Bitwig devices or drop |
| Groove | only Enabled and ShuffleAmount, Reaper range −1..1, commands unhandled | `GrooveImpl`: 6 `ParameterImpl`s on `host.createGroove ()` | create the groove, report all 6 (value, text), handle value/inc/touch/indicate/reset |
| Engine | `/project/engine` not reported, `project engine` unhandled | `ApplicationImpl`: `hasActiveEngine ()`, `activateEngine ()` | report and handle |
| Project remote controls | `ParameterBank` on the master processor (Reaper master FX), unhandled | `ProjectImpl`: `rootTrackGroup.createCursorRemoteControlsPage` | page cursors as for the cursor device, own prefix (e.g. `project param/…`) |
| Cue volume, cue mix | no-ops (Maschine encoder modes) | `ProjectImpl`: `ParameterImpl` on `project.cueVolume ()`, `cueMix ()` | report value and texts, handle value/inc/touch/reset |
| Solo/mute state of all tracks | only the window's tracks; `clear_solo/mute` per track | `ProjectImpl`: `hasSoloedTracks ()`, `hasMutedTracks ()`, `unsoloAll ()`, `unmuteAll ()` | report and handle |
| Panel layout | kept in the core, never read | `ApplicationImpl`: `panelLayout ()`, `previous/nextPanelLayout ()` | report `/layout` |
| Fill mode, metronome ticks, punch in/out | false / no-op; punch in and out exclusive, never read | `TransportImpl`: `isFillModeActive ()`, `isMetronomeTickPlaybackEnabled ()`, independent `isPunchIn/OutEnabled ()` | report and handle |
| Metronome volume | `metro_vol` unhandled, reset 0.592 | `MetronomeVolumeParameterImpl`: `inc`/`set`, reset 0.8, text `getLimited (6) + " dB"` | handle, report the limited text |
| Play / stop / restart / stop and rewind | play/stop also switch record off; restart = stop + play; rewind via play start | `transport.play/stop/restart ()`, stopAndRewind = stop + 100 ms `setPosition (0)` | commands |
| Track activation | not reported, toggles the local value (also breaks `set_skip_disabled_items`) | `ChannelImpl`: `isActivated ().get/toggle ()` | report `active` for tracks and master |
| Monitor | `monitorMode == "ON"` | `TrackImpl`: `isMonitoring ()` (effective, also AUTO) | report `isMonitoring` |
| Solo toggle | `solo !local` | `solo ().toggleUsingPreferences (false)` | command `solo/toggle` |
| Indications | no-ops for volume, pan, sends, track bank | `setIndication`, `trackBank.setShouldShowClipLauncherFeedback` | commands |
| VU clip state | `vuhold*`, never reported | `vuLeft >= CLIP_BORDER` (16241 of 16384) | derive in the core |
| `can_hold_notes`, `can_hold_audio_data` | true for every track | `canHoldNoteData ()`, `canHoldAudioData ()` | report |
| Record quantization | per track (Reaper), unhandled | global: `application.recordQuantizationGrid ()`, `recordQuantizeNoteLength ()` | report and handle |
| Previous/next track | via the bank | `CursorTrackImpl`: `selectPrevious/Next ()`, `hasPrevious/Next ()` | handle, report |
| Send names, colors | `"Send N"`, black | the send's name (item 0), `sendChannelColor ()` | report the color |
| Device siblings | the window does not follow the cursor device; page change does not select | `CursorDeviceImpl.selectPrevious/Next` moves the bank at the edge; `selectPreviousPage` selects item 0 after 75 ms | scroll the sibling bank, select after a page change |
| Parameter page memory | the core remembers a page per device name and jumps there | Bitwig's `remoteControls.selectedPageIndex ()` only | re-apply the reported page on a device change |
| Parameter page bank paging | "next parameter page" | `ParameterPageBankImpl`: next *page of pages* | core only |
| Touch | values from the host are held back while touched | `parameter.touch (b)` only | core only |
| Arranger and mixer visibility | always false | `ArrangerImpl`, `MixerImpl` getters | report the booleans |
| Clip accent reset | `accent 0.5` with `set` | `getAccent ().setImmediately (0.5)` | `/immediately` |
| Scene navigation | moves only the scene bank | `SceneBankImpl.select*` also moves the cursor track's slot bank | select the cursor track's slot as well |
| Scene `can_scroll_*` | offset based | `AbstractItemBank`: selected item and full pages | core only |
| Layer bank `scroll_to` | page aligned; paging from the reported count | exact position; `cursorDeviceLayer.hasPrevious/Next ()` | report the cursor layer |
| Drum pad select | all four selects, `BankItem::select` of pads is a no-op | `DrumPadImpl.select`: `selectInEditor ()` only | adjust |

### P3: completeness (no ported controller depends on it yet)

Transport: `return_to_arrangement`, tempo indication, tempo limits 20..666, time signature,
clip launcher post recording action and offset, default launch quantization, pre-roll measures.
Application: `show_help`, `invoke_action` with a string. Markers: the cue marker bank (exists,
name, color, position, launch, remove, select, add). Devices: `select_parent`, `is_nested`,
`select_channel`, slots (`slotNames`, `selectFirstInSlot`), `is_plugin`, `preset_name`,
remote controls section, device swap (`movePrev/moveNext` with a large device bank), names via
`getLimited`, `has_devices`/`enter` of pads and layers, drum device parameters. Tracks:
`enter` (`cursorTrack.selectFirstChild ()`), swap with previous/next, `is_muted_by_solo`,
`has_parent`/`select_parent`, crossfade mode, master crossfader (`transport.crossfade ()`),
`has_drum_device`, `empty_slot` searching for a slot without content, send paging beyond
`numSends`. Host: a capability set from the extension (Bitwig `HostImpl.CAPABILITIES`) instead of
the Reaper constants (note repeat options, MPE, layers, slot chains, …). Model: focused parameter
(`createLastClickedParameter`), clip launcher navigator, cursor layer, NI Komplete device,
`can_convert_clip`, `is_cursor_device_on_master_track`, the effect track bank,
`toggle_current_track_bank`.

## Protocol additions (summary)

Report (`BitwigModel`): `/punchIn`, `/punchOut`, `/fill`, `/click/ticks`, `/click/prerollMeasures`,
`/click/volumeStr` (limited + " dB"), `/project/engine`, `/project/name`, `/project/isDirty`,
`/project/hasSolo`, `/project/hasMute`, `/project/cueVolume…`, `/project/cueMix…`,
`/project/param/…`, `/layout`, `/arranger/<flag>`, `/mixer/<flag>`, `/groove/<param>/…`,
`/track/pinned`, `/track/<p>/active`, `/track/<p>/monitoring`, `/track/<p>/canHoldNotes|Audio`,
`/track/<p>/send/<s>/color`, `/master/active`, the global record quantization, `/marker/…`,
`/device/nested|plugin|preset|slots|onMaster`, names `…/name/6|8`, `/crossfade`.

Execute (`CommandHandler`): `…/value/inc`, `…/reset`, `…/indicate` for all parameters, volume, pan
and sends; `solo/toggle`; `transport restart|stopAndRewind|returnToArrangement|fill|metronomeTicks|preRoll`;
`time signature`; `metro_vol`; `groove …`; `project engine|unsoloAll|unmuteAll|param/…`;
`application layout/+|-`; `track addTrack` (string args); `track pinned`, `track <p>/enter`,
`movePrev|moveNext`; `track indicate`; `primary|drum|eq bypass|remove|duplicate` without index;
`device movePrev|moveNext`; `step/…/<attribute>` per attribute; clip `start|end|loopStart|loopLength/inc`;
`marker …`.

Each new address: parser test in `rust/src/parser.rs`; each new command: a unit test of the core
type which sends it (as `parser::tests::host_automation_switches`).

## Verification

1. `cargo test` (the replay test compares the core's output with the recordings; recordings
   which change because a behaviour was corrected are recorded again in step 3).
2. Build the worktree's core and extension and let Bitwig load them:
   - the core: the Rust extension in dev mode loads `rust/target/debug/libdrivenbymoss.so` of
     the main checkout (see `DbmExtension`, `NativeLibraryLoader`); point it at the worktree's
     library for the comparison, or copy the library there; note the way you choose in this file;
   - the extension: `cd rust/backends/bitwig && mvn -o -q install` (installs
     `DrivenByMoss-Rust.bwextension`).
3. Run the conformance pairs of all controllers with scenario files, on the worktree's build and
   on the current one, and compare both against Java:
   `cd rust/backends/bitwig/tools && PYTHONUNBUFFERED=1 uv run --quiet conformance_runner.py pair
   ../../../tests/conformance/<controller>/*.toml --java-port "VirMIDI 5-2" --rust-port "VirMIDI
   5-1" --fresh-project 8 --restart 15` (setup: `doc/CONFORMANCE_TESTING_PLAN.md`; the user sets
   the controllers on the ports; close used projects in Bitwig now and then). The worktree must
   not have more differences than the current build; record the replays from the worktree run.
4. Update `BITWIG_DIVERGENCES.md` (remove what now behaves as Bitwig, keep the real divergences)
   and the per-controller `CONFORMANCE.md` files.

## Sources of this analysis

Three comparisons of the Rust core with the Java classes for Reaper and for Bitwig (2026-10-30):
transport/application/arranger/mixer/project/groove/marker/browser/host/model; banks, tracks,
channels, scenes, slots, sends, layers and drum pads; devices, parameters, parameter banks,
clips and steps. The full reports with the method-level details and the quoted Java lines are
in `BITWIG_CORE_PORT_ANALYSIS.md`; the Java classes themselves are the specification.

## Reaper backend follow-ups

(To be filled during the implementation: commands and values the Reaper backend needs after a
class was replaced.)
