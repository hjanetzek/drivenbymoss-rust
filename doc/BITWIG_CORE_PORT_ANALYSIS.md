# Bitwig core port: analysis (2026-10-30)

The method-level comparisons behind `BITWIG_CORE_PORT_PLAN.md`: the Rust core (ported from the
Java version for Reaper) against the Java version for Bitwig, and what the Bitwig extension
already reports and executes. Read-only analysis; "patched" means covered by the changes of
2026-10-30 listed in the plan.

Paths:
- Rust: `rust/src/daw/`
- RP (Java for Reaper): `src/main/java/de/mossgrabers/reaper/framework/daw/`
- BW (Java for Bitwig): `~/bastel/github/DrivenByMoss/src/main/java/de/mossgrabers/bitwig/framework/daw/`
- Extension: `rust/backends/bitwig/src/main/java/de/mossgrabers/dbm/bitwig/`; BM = `BitwigModel`,
  CH = `CommandHandler`

---

# Part 1: Transport, Application, Arranger, Mixer, Project, Groove, Marker, Browser, Host, Model

## 1. Transport
- Rust: `transport.rs` (`Transport`, `TempoParameter`, `MetronomeVolumeParameter`); parser
  `parse_transport`, `parse_click`, `parse_time`, `parse_beats`
- RP `TransportImpl.java`; BW `TransportImpl.java`, `data/MetronomeVolumeParameterImpl.java`

| Method | Rust (Reaper) | Bitwig Java | Extension needs | Status |
|---|---|---|---|---|
| `play` / `stop` | If playing and recording, first sends `record` (record off), then `play` (CH `togglePlay`) or `stop`. | `transport.play ()` / `transport.stop ()`; record is not touched. | Skip the extra `record` on Bitwig. | open, small |
| `restart` | `stop` if playing, then `play`. | `transport.restart ()` | `transport restart` → `transport.restart ()`. | open, small |
| `stop_and_rewind` | `stop`, then at once `time 0` → CH `setPosition` (`playStartPosition.set (0)` + `jumpToPlayStartPosition` while the cached isPlaying is still true). | `stop ()`, then 100 ms later `transport.setPosition (0)` (the playhead). | `transport stopAndRewind`: stop + `scheduleTask (setPosition (0), 100)`. | open, small |
| `is/toggle/set_metronome_ticks` | Always false, no-op (used by `framework/commands/transport.rs`). | `isMetronomeTickPlaybackEnabled ().get/toggle/set` | Report `/click/ticks`, command `transport metronomeTicks`. | open, small |
| Metronome volume `change/set/inc/reset`, text | Sends `metro_vol ±` / `metro_vol <0..127>` (unhandled). Reset 0.592449. Text `displayedValue ()`. | `metronomeVolume ().inc (calcKnobChange, upperBound)` / `.set (value, upperBound)`; reset `setNormalizedValue (0.8)`; text `displayedValue ().getLimited (6) + " dB"`. | CH `metro_vol`; BM the limited text + " dB"; reset 0.8. | open, small |
| `preroll_measures` / setter | Sends `inifile reaper/prerollmeas` (unhandled); parser reads `/click/prerollMeasures`, never reported. | `preRoll ()` none/one_bar/two_bars/four_bars ↔ 0/1/2/4. | Report and set. | open, small |
| `set_punch_in/out`, `is_punch_*` | One local `punch_mode`: in and out exclusive, both getters `punch_mode != OFF`; never read from the host. | Independent `isPunchInEnabled ()` / `isPunchOutEnabled ()`. | Report `/punchIn`, `/punchOut` (only marked interested today); two flags in the core. | open, small |
| `change_loop_start/length` | Fixed step (4 quarters, 1 slow), not rounded. | Step from the zoom (`calcScrollFraction`), rounded: `Math.round (arrangerLoopStart.get () / fraction) * fraction`. | Core only (`/time/hzoom` is reported). | open, small |
| `select_loop_start/end` | `playStartPosition.set` + jump if playing. | `transport.setPosition (beats)` if `beats >= 0`. | Command for the playhead. | open, small |
| `return_to_arrangement` | No-op. | `transport.returnToArrangement ()` | Command. | open, small |
| `set_tempo_indication` | No-op. | `tempo ().setIndication (isTouched)` | `tempo indicate`. | open, small |
| `minimum_tempo` / `maximum_tempo` | 2 / 960 | 20 / 666 | Core. | open, trivial |
| `set_time_signature` | `time signature` n + d·100, unhandled. | `timeSignature ().numerator ().set (n)`, `.denominator ().set (d)` | CH `handleTime` `signature`. | open, small |
| Clip launcher post recording action / time offset | Off / 0, no-op. | `clipLauncherPostRecordingAction ()`, `getClipLauncherPostRecordingTimeOffset ()` | Report and set. | open, small |
| Default launch quantization | ResNone, no-op. | `defaultLaunchQuantization ()` | Report and set. | open, small |
| `is/set/toggle_fill_mode_active` | False, no-op (Maschine `core/command/note_repeat.rs`). | `isFillModeActive ()` | Report `/fill`, command `transport fill`. | open, small |
| Tempo steps, automation, overdub | | | | patched |
| Position, loop in beats, texts, zoom | Same as Bitwig. | | | same |

## 2. Application
- Rust `application.rs` `Application`; RP/BW `ApplicationImpl.java`

| Method | Rust | Bitwig | Extension needs | Status |
|---|---|---|---|---|
| `is_engine_active`, `set/toggle_engine_active` | `project engine <bool>` unhandled; `/project/engine` not reported (stays true). Launchpad project view. | `hasActiveEngine ().get ()`, `activateEngine ()` / `deactivateEngine ()` | Report and handle. | open, small |
| `panel_layout`, `is_*_layout`, `previous/next_panel_layout` | Local index, `application layout <i>` handled; never read; `previous` does not wrap. Maschine JAM, `misc.rs`. | `panelLayout ().get ()`, `previousPanelLayout ()` / `nextPanelLayout ()` | Report `/layout`. | open, small |
| `add_audio/effect/instrument_track`, `add_channel` | `track addTrack` with string args; `DbmExtension.processStringArgs` only logs it: nothing happens. Launchpad Shift view, JAM, `misc.rs`. | `application.createAudioTrack (-1)` etc.; `TrackBankImpl.addChannel` also sets name, color, devices. | Handle the string args command. | open, medium |
| `invoke_action (id)` | `action "<id>"` unhandled. | `application.getAction (id).invoke ()` | CH `action` text. | open, small |
| `show_help` | No-op. | invokes `show_online_learning` | CH. | open, trivial |
| Record quantization | Per track (Reaper `inQuantLengthEnabled` / `inQuantResolution`). | Global: `recordQuantizeNoteLength ()`, `recordQuantizationGrid ()` | Report globally / map onto the track values. | open, small–medium |

## 3. Arranger (`application.rs` `Arranger`; BW `ArrangerImpl.java`)
`are_cue_markers_visible`, `has_double_row_track_height`, `is_clip_launcher_visible`,
`is_timeline_visible`, `is_io_section_visible`, `are_effect_tracks_visible`: constant false in
Rust; Bitwig `arranger.areCueMarkersVisible ().get ()` etc. Report `/arranger/<name>` (only
`/followPlayback` today). Small.

## 4. Mixer (`application.rs` `Mixer`; BW `MixerImpl.java`)
The six section visibility getters are constant false; Bitwig
`mixer.isClipLauncherSectionVisible ().get ()` etc. Report `/mixer/<name>`. Small.

## 5. Project (`project.rs`; BW `ProjectImpl.java`)

| Method | Rust | Bitwig | Extension needs |
|---|---|---|---|
| `name` | `/project/name` never reported. | `application.projectName ().get ()` | Report. |
| `is_dirty` | `/project/isDirty` never reported. | `project.isModified ().get ()` | Report. |
| Cue volume / cue mix (value, text, change, set, reset, touch) | No-ops. Used by Maschine `core/encoder_mode_manager.rs`. | `ParameterImpl` on `project.cueVolume ()` / `cueMix ()` | Report value and texts, handle commands. Medium. |
| `parameter_bank` (project remote controls) | Bank on `Processor::Master` (Reaper master FX), `master param/…` unhandled, nothing reported. Launchpad user view. | `rootTrackGroup.createCursorRemoteControlsPage (numParams)` | Page cursors on the root track group, `/project/param/…`. Medium. |
| `has_solo/mute`, `clear_solo/mute` | Over the bank window, per track. | `project.hasSoloedTracks ()`, `hasMutedTracks ()`, `unsoloAll ()`, `unmuteAll ()` | Report and handle. Small. |

## 6. Groove (`groove.rs`, `parameter.rs` `ParameterKind::Groove`; BW `GrooveImpl.java`)
Used by Launchpad tempo view, Maschine JAM swing, `number_display_view.rs`.
- Rust has only Enabled and ShuffleAmount; ShuffleRate, AccentAmount, AccentRate, AccentPhase are
  empty. Bitwig: 6 `ParameterImpl`s (`groove.getEnabled ()`, `getShuffleAmount ()`,
  `getShuffleRate ()`, `getAccentAmount ()`, `getAccentRate ()`, `getAccentPhase ()`).
- Rust maps Reaper's −1..1 to (v+1)/2 and sends `groove amount <v·2−1>`, text computed as
  `(v·2−1)·100 %`; Bitwig uses the normalized value and `displayedValue ()`.
- `groove active|amount` unhandled, nothing reported: the groove does not work on Bitwig.
- `set_indication` no-op; Bitwig on all 6.

## 7. Marker / MarkerBank (`marker.rs`; BW `data/MarkerImpl.java`, `data/bank/MarkerBankImpl.java`)
No ported controller uses it. BM reports no markers (Bitwig: `arranger.createCueMarkerBank`).
`launch`, `remove_marker`, `select`, `add_marker` are unhandled (Bitwig: `marker.launch
(quantized)`, `deleteObject ()`, `transport.setPosition (marker.position ().get ())`,
`transport.addCueMarkerAtPlaybackPosition ()`); `previous/next` work. Names: Rust falls back to
"Marker N", Bitwig `name ().getLimited`.

## 8. Browser (`browser.rs`; BW `BrowserImpl.java`; extension `BitwigBrowser.java`)
Essentially the same. `insertClapDevice` is missing (unused); slots outside the window cannot be
browsed (documented).

## 9. Host capabilities (`host.rs`, `constants.rs` `Capability` unused; Reaper constants in
`config/configuration.rs` `NoteRepeatCapabilities::REAPER`, `launchpad/view/device_view.rs`,
`maschine/jam/view/note_repeat_view.rs`)
Bitwig `HostImpl.CAPABILITIES`: all NOTE_REPEAT_* (octaves, free running, latch, swing),
QUANTIZE_INPUT_NOTE_LENGTH, QUANTIZE_AMOUNT, CUE_VOLUME, HAS_SLOT_CHAINS, HAS_DRUM_DEVICE,
HAS_CROSSFADER, HAS_DEVICE_LAYERS, HAS_PINNING, HAS_PARAMETER_PAGE_SECTION, HAS_EFFECT_BANK,
HAS_BROWSER_PREVIEW, MPE_PITCH_RANGE; `supports (NoteAttribute)` true for all. The core needs a host
capability set (from the extension) instead of compile-time Reaper constants; `getName`,
`getVersion`, `restart` are missing.

## 10. Model and ModelSetup (`model.rs`, `model_setup.rs`; BW `ModelImpl.java`; BM constructor)

| Item | Rust (Reaper) | Bitwig | Status |
|---|---|---|---|
| `create_note_clip` | `createClip/<slot>`, select, if overdub `start_recording ()` (arranger). No launch. | `track.createClip`; `slot.select ()`; `slot.launch (true, false)`; if overdub `setLauncherOverdub (true)`. | open, small |
| `record_note_clip` | `track <n>/recordClip`, unhandled. | `if (!slot.isRecording ()) slot.startRecording (); slot.launch (true, false);` | open, small |
| `can_convert_clip` | Always true. | Cursor track exists, `canHoldAudioData`, selected slot has content. | open, trivial |
| `is_cursor_device_on_master_track` | The master is selected. | `cursorDevice.channel ().createEqualsValue (master)` | open, trivial |
| Crossfader | User parameter 0 of the master (Reaper JS), unhandled. | `transport.crossfade ()` | open, small |
| Cursor track pinning | Local, `track <n>/pin`, unhandled. | `cursorTrack.isPinned ()` | open, small |
| `cursor_track_swap_with_previous/next` | `movePrev/moveNext`, unhandled. | `CursorTrackImpl` via track insertion points | open, small |
| Effect track bank | None. | `createEffectTrackBank` | documented |
| Focused parameter, clip launcher navigator | Not implemented. | `createLastClickedParameter`, `ClipLauncherNavigatorImpl` | open, unused |
| Cursor layer, NI Komplete | Komplete treated as first instrument; BM creates the first instrument only for `"FIRST_INSTRUMENT"`. | cursor layer; `KompleteDevice` matchers | open |
| ModelSetup | BM ignores numMarkers, numFxTracks, numDevicesInBank (8 fixed), numDeviceLayers/numDrumPadLayers (fixed), wantsCursorLayer, wantsFocusedParameter, wantsClipLauncherNavigator, numParams (project bank), additionalDrumDevices. | | mostly documented |

---

# Part 2: Banks and channels

Most important:
1. **Slot commands go to the wrong track once the track bank is scrolled.** `Slot::send_track_clip`
   (and `set_color`, `paste`, `insert_file`, the source of `paste`) sends
   `"{track_index}/clip/{pos}/…"` with the index in the page (`Track::set_index` →
   `slot_bank.set_track_info (channel.index)`); `handleTrack` reads it as the absolute position
   (`getWindowTrack (position)`). Offset 8: "outside of the window", nothing happens; offset 1: the
   clip of the track one position to the left. Fix: on host paging use `channel.position`.
2. **Regression of the `select_previous_page` patch.** `PagedBank::select_previous_item` and
   `TrackBank::select_previous_item` call `select_previous_page`, which now selects item 0. Bitwig
   `AbstractItemBankImpl.selectPreviousItem`: `this.host.scheduleTask (() -> this.getItem
   (this.getPageSize () - 1).select (), 75);`. Scenes too.
3. **`TrackBank::scroll_to` selects a track on Bitwig.** It sends `<pos>/scrollto` (page aligned if
   `adjust_page`); CH treats `"select", "scrollto"` alike (`selectChannel`). Bitwig `scrollTo`:
   `bank.scrollPosition ().set (position)`, ignoring `adjustPage`, selects nothing. Affects
   session_view, track mode, misc.rs, `select_item_at_position`.
4. **Selecting right after a page change fails for scenes.** `PagedBank::select_next_page`,
   `select_previous_page`, `select_item_at_position` select at once; the `bank scene <offset>` and
   `<pos>/select` arrive in the same tick and `getScene` still sees the old scroll position. Java
   waits 75 ms; `TrackBank` already does.

## PagedBank (`bank.rs`; RP `AbstractPagedBankImpl`, `AbstractBankImpl`; BW `AbstractItemBankImpl`, framework `AbstractItemBank`)
- `select_previous_item` at index −1: see 2. Let it scroll and select `page_size - 1` itself.
- `select_next_page`, `select_previous_page`, `select_item_at_position`: see 4; Bitwig's
  `selectItemAtPosition` also calls `firePageObserver ()`.
- `scroll_to (pos, adjust_page = true)`: Rust aligns to the page, Bitwig sets the exact position
  (layer banks: `parameter_mode.rs` `layer_bank_mut ().scroll_to (position, true)`).
- `set_bank_offset` posts the page event on every call; Bitwig fires the page observer only when
  `position / pageSize` changes (`handlePageSelection`). Low impact.

## TrackBank (`track_bank.rs`; RP `AbstractTrackBankImpl`, `TrackBankImpl`; BW `AbstractTrackBankImpl`, `TrackBankImpl`, `EffectTrackBankImpl`, `ModelImpl`)
- `select_previous_item`: regression 2.
- `scroll_to`: see 3; with host paging call `set_bank_offset (position)` (no alignment).
- `handle_bank_track_selection`: when the selected track leaves the window, Rust scrolls to its
  page; Java uses `tb.followCursorTrack (cursorTrack)` and lets Bitwig scroll (the extension does
  not call it). The visible window may differ. Medium.
- `set_indication`: no-op; Bitwig `trackBank.setShouldShowClipLauncherFeedback (enable)` (called
  by the Launchpad, Launch Control, JAM and Mk2 setups).
- `is_clip_recording`: always false; Bitwig true if a slot of the page `isRecording ()`
  (reported as `slot/…/recording`). Unused.
- `can_edit_send`: Rust any track of the page; Bitwig only item 0:
  `this.getItem (0).getSendBank ().getItem (sendIndex).doesExist ()`.
- `edit_send_name`: Rust `"Send N"`; Bitwig the send's name of item 0 (reported). Launchpad sends
  view.
- `toggle_rec_arm` (bank): Rust all created tracks from absolute 0; Bitwig the page's items.
- `has_solo/mute`, `clear_solo/mute`: see Project.
- `is_muted_by_solo`: derived from the other tracks; Bitwig `deviceChain.isMutedBySolo ().get ()`.
- `has_parent`, `select_parent`: false / no-op with a flat list; Bitwig `this.getItem (0).hasParent
  ()`, `cursorTrack.selectParent ()`. Unused.
- `enter` (group): Rust selects and `enter_current_folder` (no-op flat); Bitwig
  `cursorTrack.selectFirstChild ()` (after 100 ms when it selects first).
- `add_channel`: see Application.

## Cursor track and pinned track (`model.rs`, `Track::is_selected`; BW `CursorTrackImpl`)
- Pinned track: the core keeps the pin (`Track::is_selected` returns the pinned track), sends
  `track <sel>/pin <pos>` (unhandled). Bitwig `cursorTrack.isPinned ()`, so Bitwig's cursor device
  and remote controls stay on the pinned track; with Rust they follow the selection while the core
  shows the pinned track. Maschine Mk2 Pin, Launchpad device view. Medium.
- `cursor_track_select_previous/next`, `can_select_*`: Rust via the bank; Bitwig
  `cursorTrack.selectPrevious/Next ()`, `hasPrevious/Next ()`.
- `cursor_track_swap_with_*`: unhandled; Bitwig `track.afterTrackInsertionPoint ().moveTracks
  (prevTrack); prevTrack.selectInEditor ();` (large bank).

## Channel (`channel.rs`; RP/BW `ChannelImpl`)
- `is_activated`: not reported for tracks and master (only pads and layers); Rust stays true and
  `toggle_is_activated` toggles locally; breaks `set_skip_disabled_items`. Bitwig
  `deviceChain.isActivated ().get ()` / `.toggle ()`.
- `set_is_activated` on the master: no-op in Rust.
- `reset_volume`: 0.716 (Reaper 0 dB); Bitwig `parameter.reset ()`.
- `set_volume_indication`, `set_pan_indication`: no-ops; Bitwig `setIndication` (Launchpad setup).
- `change_volume`, `change_pan`: absolute from the local value; Bitwig `rangedValue.inc`.
- `modulated_volume/pan`: plain value; Bitwig `getModulatedValue` (documented).
- `toggle_solo`: `solo !local`; Bitwig `solo ().toggleUsingPreferences (false)` (exclusive solo
  preference).
- `toggle_mute`: equivalent.
- `vu_*_clip_state`: `vuhold*` never reported; Bitwig `this.vuLeft >= CLIP_BORDER` (16241/16384).
- Selecting the master: CH only `selectInEditor ()`; Bitwig also `selectInMixer`,
  `makeVisibleInArranger`, `makeVisibleInMixer`.
- `add_device`, `add_equalizer_device`: `device add/0 <creation name>` unhandled, `eq add` on the
  cursor track's chain; Bitwig `deviceChain.endOfDeviceChainInsertionPoint ()` of this channel.
- `name_limited` (also scenes, slots): `optimize_name`; Bitwig `name ().getLimited (limit)`.

## Track (`track.rs`; BW `TrackImpl`, `MasterTrackImpl`, `CrossfadeParameter`)
- `is_monitor`: CH reports `monitorMode == "ON"`; Bitwig `track.isMonitoring ().get ()` (also AUTO
  while armed); `toggle_monitor` decides on it.
- `set_monitor (false)`: Rust OFF; Bitwig always ON (`monitorMode ().set (MONITOR_MODE_ON)`), a
  Java bug: document only.
- `set_rec_arm`, `set_monitor` on the master: no-ops in Rust.
- `can_hold_notes`, `can_hold_audio_data`: true for every track; Bitwig `canHoldNoteData ()`,
  `canHoldAudioData ()` (Mk2 play view, `framework/ctx.rs`).
- `has_drum_device`: false; Bitwig a drum matcher device bank. Unused.
- `crossfade_parameter`: None; Bitwig `CrossfadeParameter` (`crossFadeMode ()`). Unused.
- Master `crossfader_parameter`: `master user/param/0/value`, unhandled.
- Record quantization: per track, unhandled, note length toggle does nothing; Bitwig global
  (`framework/commands/clip.rs`).
- `toggle_multi_select`: unhandled command; Bitwig does nothing.

## SceneBank and Scene (`scene.rs`; BW `SceneBankImpl`, `SceneImpl`)
- Paging selects and `select_previous_item`: findings 2 and 4.
- Every Bitwig `SceneBankImpl.select*` / `scroll*` also moves the cursor track's slot bank
  (`this.slotBank.selectNextItem (); super.selectNextItem ();`).
- `can_scroll_backwards/forwards`: Rust offset based; Bitwig `selIndex > 0 ||
  canScrollPageBackwards ()`, `selIndex < ps-1 && next exists || canScrollPageForwards ()`.
- `name`: Rust `"Scene N"` for an empty name; Bitwig unchanged.

## SlotBank and Slot (`slot.rs`; BW `SlotBankImpl`, `SlotImpl`)
- Track index: finding 1.
- `empty_slot (start_from)`: Rust always the slot at `start_from`; Bitwig `getEmptySlot` searches
  the page (wrapping) for `!item.hasContent ()`, `Optional.empty ()` if none. Unused.
- Slot paging goes through the scene bank: equivalent.

## SendBank and Send (`channel.rs` `SendBank`, `parameter.rs` `Send`; BW `SendBankImpl`, `SendImpl`)
- `color`: not sent, black; Bitwig `send.sendChannelColor ()`.
- `set_indication`: no-op; Bitwig `setIndication` (Launchpad setup).
- Paging beyond `numSends`: the extension reports only 0..numSends; Bitwig scrolls each track's
  send bank. Medium.
- `exists`: from a non-empty name; Bitwig `send.exists ()`. Low.

## LayerBank, DrumPadBank, DrumPad (`device.rs`; BW `LayerBankImpl`, `DrumPadBankImpl`, `LayerImpl`, `DrumPadImpl`)
- `LayerBank::can_scroll_page_*`: page based on the reported count; Bitwig
  `cursorDeviceLayer.hasPrevious/Next ()`.
- Layer `scroll_to (…, true)`: page aligned; Bitwig exact.
- Drum pad select: CH `selectChannel` (all four); Bitwig `DrumPadImpl.select` only
  `deviceChain.selectInEditor ()`. `DrumPad`'s `BankItem::select` is a no-op, so the generic
  `select_next_item` etc. do nothing on pads.
- `can_edit_send`, `edit_send_name`, `has_devices`, `enter`: false / ""; Bitwig item 0's sends,
  `firstItem.exists ()`, `selectInEditor ()` (documented).

---

# Part 3: Devices, parameters, clips

Most important:
1. Relative knob changes are sent as absolute `set`; Java uses `inc`.
2. Bypass, remove, duplicate of devices fail for chain positions 8 and up (window index vs.
   absolute position).
3. Window, expand, bypass, remove, duplicate of the first instrument, drum device and EQ go to
   `device`.
4. Reset of a device parameter sets 0, not Bitwig's default.
5. Step attribute edits halve the note gain on every knob step (check against the API).
6. Step attribute knob changes are cut to whole numbers before normalizing.
7. After a device change the page the core remembers can differ from Bitwig's.
8. Device swap (`movePrev/moveNext`) is not handled by the extension.

## Parameter and Send (`parameter.rs`; RP `data/parameter/ParameterImpl` and subclasses; BW `data/ParameterImpl`, `data/RangedValueImpl`, `data/SendImpl`)
1. `inc` / `change_value`: Rust `local value + increment`, clamped, sent absolute
   (`param/N/value` → `set`); fractional increments truncated (`as i32`). Bitwig
   `this.rangedValue.inc (Double.valueOf (increment), Integer.valueOf
   (this.valueChanger.getUpperBound ()))`; `changeValue` → `this.inc (valueChanger.calcKnobChange
   (value))`. Encoders go through the takeover mode. Needs `…/value/inc <delta>`. All kinds.
2. `resetValue`: Rust `set_value (default_value)` (devices 0, volume 0.716, sends 0); Bitwig
   `this.parameter.reset ()`. Needs `…/reset` in `handleParameter`, `handleChannel`, `handleSend`,
   EQ.
3. `touchValue`: Rust holds back host values while touched (`last_received_value`); Bitwig only
   `parameter.touch (b)`. With host paging skip the deferral.
4. `touch` / `setIndication` for sends, pad/layer channels, crossfader: no-ops in Rust
   (documented).
5. `getName (limit)`: Rust `util::optimize_name`; Bitwig `parameter.name ().getLimited (limit)`.
   Needs `name/6|8`.
6. `getNumberOfSteps`: the extension reports `steps -1`; Bitwig `discreteValueCount ().get ()`.
   No core user yet.
7. Send `set_value`: `inc` does not clamp sends; values outside 0..1 can reach
   `SettableRangedValue.set (double)`. Clamp.
8. Existence from a non-empty name; Bitwig `parameter.exists ()`. Minor.

## SpecificDevice and Device (`device.rs`; RP/BW `SpecificDeviceImpl`, `CursorDeviceImpl`, `DeviceImpl`, `DrumDeviceImpl`, `EqualizerDeviceImpl`)
1. `toggleWindowOpen` / `toggleExpanded` of first instrument, drum device, EQ: Rust
   `ctx.send_bool (Processor::Device, "window"|"expand", …)` for all kinds (copied from Reaper's
   hard-coded `Processor.DEVICE`); Bitwig `this.device.isWindowOpen ().toggle ()`. CH already
   handles `primary|drum|eq window|expand`; use the device's processor.
2. `toggleEnabledState` / `remove` / `duplicate` / `select`: Rust `<index+1>/bypass|remove|duplicate`
   and `selected <index+1>` with index = position % page size; for the specific devices `index ()`
   is −1 → `device 0/bypass`. CH `handleDeviceIndexed` uses `findSibling (index-1)` (absolute
   position): unhandled for specific devices, wrong from position 8 or a scrolled window;
   `selected` uses `siblings.getItemAt (index)` (window index): inconsistent. Bitwig
   `this.device.isEnabled ().toggle ()`, `deleteObject ()`, `afterDeviceInsertionPoint ().copyDevices
   (this.device)`, `selectInEditor ()`.
3. `selectPrevious` / `selectNext`: Rust guards with its position and count, sends `-`/`+`; Bitwig
   also `if (moveBank) this.deviceBank.selectPreviousPage ();` at the edge of the page. On Bitwig the
   8-device window does not follow; `DeviceBank::selected_item` becomes None.
4. `DeviceBank::select_previous/next_page`: Rust `page/-|+` → `siblings.scrollPage…` only; Bitwig
   `scrollPageBackwards ()` + `scheduleTask (() -> this.getItem (0).select (), 75)`.
5. `swapWithPrevious/Next`: `device <i>/movePrev|moveNext` unhandled; Bitwig `largeDeviceBank`
   (100), `this.device.afterDeviceInsertionPoint ().moveDevices (prevDevice); prevDevice.selectInEditor ();`.
6. Not supported in Rust: `select_parent` / `is_nested` (`selectParent ()`, `isNested ()`),
   `select_channel` (`channel ().selectInEditor ()`), slots (`hasSlots ()`, `slotNames ()`,
   `selectFirstInSlot (name)`), `is_plugin` (`isPlugin ()`), `preset_name` (`presetName ()`),
   remote controls section (`isRemoteControlsSectionVisible ()`).
7. `getName (limit)`, `set_name`: Rust strips "type: " and a trailing "(…)"
   (`remove_type_and_manufacturer`); Bitwig `name ().getLimited (limit)` of the raw name.
8. `getID`: Rust the first parameter's name (Reaper); Bitwig `""`.
9. `canSelectPrevious/Next`: derived; Bitwig `hasPrevious/Next ()`. Equivalent.
10. Drum pads and layers: `has_devices` false, no `enter`; Bitwig `LayerImpl`: `firstItem.exists ()`,
    `firstItem.selectInEditor ()`.
11. Drum device parameters: `Processor::Device` (would address the cursor device), count 0
    (documented).

## ParameterBank / ParameterPageBank (`parameter_bank.rs`; RP/BW `ParameterBankImpl`, `ParameterPageBankImpl`)
1. Page memory per device (`device_name_changed`): Rust jumps to the page remembered for the
   name; Bitwig only `remoteControls.selectedPageIndex ()` (Bitwig remembers per device). After
   switching between two devices both on page 2 in Bitwig, Rust shows another page until the next
   change. With host paging re-apply the last reported page.
2. Page names: Rust "Page N" fallback; Bitwig `pageNames` as is. Minor.
3. `ParameterPageBank::can_scroll_page_forwards`: Rust a next parameter page; Bitwig `final int end
   = sel / ps * ps + ps; return end < this.getItemCount ();` (a next page of pages).
4. Parameter bank `canScrollPage…`: equivalent within `numParamPages`.
5. Item count limited to `numParamPages` (documented).

## CursorClip / NoteClip (`clip/cursor_clip.rs` host grid; BW `CursorClipImpl`; extension `BitwigNoteClip`, `CommandHandler.handleClip`)
Most of the clip follows Bitwig already.
1. Single step attribute updates send all attributes: `update_step` and `send_clip_data` send
   `step/…/update` with the whole `to_wire`; `BitwigNoteClip.update` sets all 21. Bitwig writes only
   the changed attribute outside an edit (`this.getNoteStep (notePosition).setVelocity (v);`), all of
   them in `startEdit`/`stopEdit`/`delayedUpdate`. Gain: `StepInfoImpl.updateData` reports `gain () /
   2.0`, `update` writes `setGain (values[11])`: every knob step on any attribute writes back half
   the gain (check whether `gain ()` and `setGain` use the same range). Recurrence rewritten with
   `max (1, length)`; values not yet received are overwritten.
2. `change_step_*`: `value_changer.calc_knob_change (control) as i32 as f64`; Bitwig no cast
   (`info.getVelocity () + this.valueChanger.toNormalizedValue (this.valueChanger.calcKnobChange
   (control))`).
3. `changePlayStart/End`, `changeLoopStart/Length`: local ± frac, absolute; play start/end clamped,
   loop not (length can go ≤ 0). Bitwig `this.getClip ().getPlayStart ().inc (increase ? frac :
   -frac);` etc.
4. `resetAccent`: `accent 0.5` with `set`; Bitwig `getAccent ().setImmediately (0.5);`.
5. Velocity spread: the extension reports the real value; Java's `StepInfoImpl.updateData` never
   reads it. Rust is more correct; document.
6. `getNextNote`/`getPreviousNote`: Rust the channel where the note was found; Bitwig the channel
   of the start position (a Java bug with `ignoreChannel`). Note only.
7. `num_pages`: 1 with a host grid; no callers.

## StepInfo / Note / NoteOccurrence (`clip/step_info.rs`, `clip/note.rs`, `clip/note_occurrence.rs`)
Defaults as `DefaultStepInfo`; the wire format matches `BitwigNoteClip.format/update`; only the
gain asymmetry and velocity spread differ. `Note` is Reaper-only (not used with a host grid).
