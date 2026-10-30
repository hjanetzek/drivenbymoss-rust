# Bitwig backend: differences from the Java DrivenByMoss

The Rust core with the Bitwig extension (`backends/bitwig`) is meant to behave like the Java
DrivenByMoss for Bitwig (`de.mossgrabers.bitwig.framework`, written by Jürgen Moßgraber). The
Bitwig side follows the Java implementation where it can. This document lists every place where
it does not, for the maintainer of the Java version. It is updated with each change of the
backend.

As of 2026-10-29. "Java" means the Java DrivenByMoss for Bitwig, "Rust" the core with the Bitwig
extension. The core is a port of the Java version for Reaper, so where Java for Bitwig and Java
for Reaper differ, Rust usually behaves like Java for Reaper.

## 1. Structure

These do not change what a controller does, but explain the other differences.

| Topic | Java | Rust | Why |
| --- | --- | --- | --- |
| DAW model | `ModelImpl` with observers on the Bitwig objects | The extension reads all values once per tick (30 ms) and sends the changes as text lines to the core (`ModelReporter`, BACKEND_PROTOCOL.md §3) | The core is shared with Reaper and speaks the backend protocol |
| Timer | `flushWorkaround`: `scheduleTask` every 100 ms plus `requestFlush` | `scheduleTask` every 30 ms ticks the core | As the Reaper version |
| Settings | `SettingsUIImpl` on `getPreferences ()` / `getDocumentState ()` | Same settings and places (`HostSettings`, from the core's list); Bitwig keeps the values, the core's `.config` file in `~/.config/DrivenByMoss-Bitwig` is only a fallback. Not supported: enabling/disabling a setting (Java: only the MPE pitch bend range, which no ported controller activates); signal, color, number and action settings (no ported controller uses them) | The core owns the settings |
| Notifications | `showPopupNotification` | Same, queued by the core and shown after each tick (up to 30 ms later) | The core runs in the tick |
| Extension IDs | One definition per controller with the controller's UUID | Generated definitions with own UUIDs and "(Rust)" names | Both can be installed side by side |
| Hardware surface | `HwSurfaceFactoryImpl` builds a `HardwareSurface` (simulator, Bitwig mappings) | Raw MIDI only, the core does all mapping | Not planned yet |

## 2. Banks and the model

| Topic | Java | Rust |
| --- | --- | --- |
| Track bank | Bank of `numTracks` (ModelSetup), flat or hierarchical, follows the cursor track, scrolled by Bitwig | Bank of `numTracks`, flat, scrolled by Bitwig to the core's bank offset (host paging); a bank of 400 tracks without sends and slots for commands to tracks outside of the window, as `CursorTrackImpl.largeTrackBank`. Sends and slots of tracks outside of the window cannot be addressed. Hierarchical track lists (`hasFlatTrackList` false) are not supported |
| Track depth | From the hierarchy | Not reported (0) |
| Track selection | Selection observers of the bank items | The cursor track's position, so that a track outside of the window is deselected as well |
| Scenes and slots | Scene bank of `numScenes`, scrolled by Bitwig; slot banks per track | Scene bank of `numScenes` (the track bank's), scrolled by Bitwig to the core's scene bank offset, the slots of the window tracks follow it. Extra scene and slot banks of other sizes (`getSceneBank`, `getSlotBank`) are not created, the core's extra scene banks only see the window |
| Drum machine | Main drum device of `numDrumPadLayers` pads plus additional drum devices of other page sizes (e.g. 64) on the same drum machine, scrolled by Bitwig | One pad bank of all 128 pads of the cursor track's drum machine (same matcher), reported completely; the core's drum devices page over it. Pad sends and the devices of a pad are not reported |
| First instrument | `SpecificDeviceImpl` on the instrument matcher: one remote-control page cursor, `numDeviceLayers` layers and `numDrumPadLayers` pads, scrolled by Bitwig; a cursor layer if `wantsCursorLayer` | Same matcher; the parameters as for the cursor device (`numParamPages` fixed page cursors, page names, the page selected in Bitwig in both directions), all 128 pads and 32 layers reported completely, the core pages over them with the pad page size. Layer sends, the devices of a layer and the cursor layer are not reported |
| Browser | `PopupBrowser` with 7 filter columns (no smart collections) of `numFilterColumnEntries` items, `numResults` results; observers only while used | Same columns and pages, reported only while the browser is open. Slots outside of the track and scene window cannot be browsed |
| Note clips | `getNoteClip (cols, rows)` creates a launcher cursor clip per size when a view asks for it | Same, but the sizes must be known before the extension creates its objects: the core collects the sizes of the sequencer views created during the setup of the controller (plus 8x128 for the Drum 64 views) and lists them in the model setup. A size first requested later has no grid in Bitwig. The clip properties come from the first grid; `pinned` is set on all grids |
| Effect tracks | Created from the ModelSetup | Not created yet |
| Bank paging | `canScrollPageBackwards` of the banks is Bitwig's `canScrollBackwards`: true for any position > 0 | Same with host paging (the Java version for Reaper requires a full page); found 2026-10-30 (Maschine JAM session, Shift + arrow scrolls by 1 track) |
| Device siblings | `createSiblingsDeviceBank (numDevicesInBank)` | Always 8 siblings (the Reaper protocol's fixed window) |
| Device parameters | One remote-control page cursor; its selected page is the page shown on the controller and in Bitwig, page names from `pageNames` | Same page names and the same page selection in both directions (an extra page cursor follows Bitwig's selection). The parameter values come from `numParamPages` page cursors, each fixed to one page, as one flat list; pages beyond `numParamPages` are not reachable. When pages are switched quickly, the controller can briefly show the previous page while Bitwig catches up |
| Parameter extras | `modulatedValue`, `setIndication`, `touch` for all parameters | `modulatedValue`, `setIndication` and `touch` for the cursor device's parameters, `touch` for track and master volume and pan; not for sends, the track and project remote controls |

## 3. Transport and time

The play position and the loop are reported and set in beats as in Java (`TransportImpl`):
the position is the play start position (`playStartPosition`, setting it jumps there while
playing), the loop is `arrangerLoopStart` / `arrangerLoopDuration`, the texts are formatted with
the same functions (`TimeFormat`, a copy of `StringUtils.formatTimeLong` and
`formatMeasuresLong`), the zoom resolution comes from `getContentPerPixel` with the same table.

| Topic | Java | Rust |
| --- | --- | --- |
| Changing the loop start and length | Step from the zoom resolution, rounded to it (`calcScrollFraction`) | Fixed step of 4 quarters, 1 quarter with Shift/slow, not rounded (as Java for Reaper, `INC_FRACTION_TIME`) |
| Play position in seconds | Not used | Derived at the current tempo, only used by the core for Reaper's slot states |
| Metronome volume | `metronomeVolume` | Not handled yet (`metro_vol` commands are ignored) |
| Automation write mode | `isArrangerAutomationWriteEnabled` / `isClipLauncherAutomationWriteEnabled`, `automationWriteMode` | Not handled yet; the core sends Reaper's override numbering |
| Pre-roll | `preRoll` (none, 1, 2, 4 bars) and the metronome during pre-roll | Only the metronome during pre-roll; the pre-roll length is not handled yet |
| Punch in / out | `isPunchInEnabled` / `isPunchOutEnabled` | Same, but the core keeps one punch state (Reaper's record modes): punch in and punch out exclude each other |

## 4. Clip launcher

Slots and scenes follow `SlotImpl`, `SceneImpl` and `SceneBankImpl`: the slot states, launch on
press and release (normal and alternative), record, select, remove, duplicate, insert file,
paste (`copySlotsOrScenes`), slot color (group tracks show the track color), track stop,
alternative stop and return to arrangement, new clips in a slot (`createNewLauncherClip` with the
absolute slot), scene launch, select, remove, duplicate, name and color, stop all clips. Scenes
are created with `createScene` and `createSceneFromPlayingLauncherClips`.

| Topic | Java | Rust |
| --- | --- | --- |
| Clip muted | Not available in the API, always false | Same |

**Slot commands** (core port, P1 1): the slot commands (`track <p>/clip/<s>/…`, also the color,
the paste source and insert file) address the track by its position, as `SlotImpl` acts on the
slot of its own track. They used the index in the page, so with a scrolled track bank the clip of
another track (or nothing) was launched. Also on Reaper (its track processor reads the same
position for all track commands).

## 5. Note repeat

| Topic | Java | Rust |
| --- | --- | --- |
| Modes | All arpeggiator modes of Bitwig | The core's four modes (down, up, down-up, up-down) |
| Octaves, shuffle, free running, note latch | Supported | Not exposed |

## 6. Commands

| Command | Java | Rust |
| --- | --- | --- |
| Arrow keys | `arrowKey*` | Same |
| Previous / next track (Reaper actions in the core) | - | `cursorTrack.selectPrevious/selectNext` |
| Track volume/pan "touch" | `touch` | Same |
| Device parameter commands of the first instrument | The specific device | Same (`primary param/…`) |
| Drum device parameters | The drum device's parameter bank | Not reported, the drum device has no parameters |
| Equalizer (`DeviceID.EQ`) | `EqualizerDeviceImpl`: the first EQ+ of the cursor track; type, frequency, gain, Q of 8 bands | Same device (`EqualizerDevice`), reported in the layout of the Reaper EQ (`/eq/param/<3 * band + 0, 1, 2>` frequency, gain, Q; `/eq/band/<band>/value` the type as Reaper index). A type knob selects one of the 7 types (as `EqBandTypeParameterImpl` of the Java version for Reaper), Java's sets the EQ+ type parameter (16 values) directly; with the takeover mode catch, Java's is ignored when it jumps |

**Automation.** As `TransportImpl` of the Java version: Bitwig has two write switches (arranger
and clip launcher) and the modes Read, Latch, Touch and Write; the mode counts as Read while no
switch is on. The extension reports `/automode/arranger`, `/automode/launcher` and `/automode`
(the index of the core's mode); the core then toggles the switches with the commands
`automation toggleArranger|toggleLauncher|resetOverrides`, and setting a mode sets both switches
(Read: off). Without the switches (Reaper) the core keeps Reaper's modes. Found by the conformance
runner 2026-10-30 (Maschine JAM AUTO, Shift + AUTO).

**Played notes.** The core sends notes "as if played on the device" (`process_midi_arg`, e.g. the
further notes of a chord in the chord views) to the default note input, as
`MidiInputImpl.sendRawMidiEvent`; the extension did not handle them before (only the root note of
a chord played). The extension reports the notes which play on the selected track
(`cursorTrack.playingNotes`) as `/track/<position>/playingnotes`, like Reaper, so that the views
light the pads of the played notes; the Java version observes the notes of all tracks of the bank
(`TrackImpl.addNoteObserver`), the views use those of the selected track. Found by the conformance
runner 2026-10-30 (Maschine JAM Chords mode).

**Overdub.** As the automation: Bitwig's arranger and clip launcher overdub switches
(`/overdub/arranger`, `/overdub/launcher`, the commands `transport overdub/arranger|launcher`),
as `TransportImpl.toggleOverdub` / `toggleLauncherOverdub`; Reaper has the overdub record mode of
the selected track for both. Found 2026-10-30 (Maschine Mk2 LOCK).

**Device pinned.** The cursor device reports `/device/pinned` and takes `device pinned`
(`isPinned` of the cursor device); in Reaper it counts as always pinned. Found 2026-10-30
(Maschine Mk2 Pin button).

**Shortened texts.** The Java version shows the texts which Bitwig shortens itself
(`getDisplayedValue (limit)`, `getVolumeStr (limit)`: `displayedValue ().getLimited`, e.g. without
the unit). The extension reports them for 6 and 8 characters (`<…>/str/6`, `<…>/str/8`) for the
volume and panorama of the tracks and the master, the sends and the device parameters (not for
layers and drum pads); the core uses them for these lengths and cuts the text for other lengths
(Reaper). Found 2026-10-30 (Maschine Mk2 display).

**Maschine Mk2 button labels (not in the Java version).** The core sends the labels of the 8
buttons above the display (`F0 7D 44 42 4D 10 00 <56 characters> F7`, 8 cells of 7 characters as
the MCU display, the active mode marked with `>`, Pin and Active as `PinDev`/`PinTrk`,
`DevOn`/`TrkOn`), only when they change. dbm-nihost draws them above the MCU lines on the two
displays; NI's driver ignores the message. Also on Reaper (the same core).

**Repeated values.** The extension reports the values of a track again after a command to it
(see the takeover mode); the core handles a track selection only when it changes, otherwise a
repeated `select 1` recalled the preferred view of the track (Maschine Mk2: the Shift view
ended while Shift was held). Found 2026-10-30.

**Tempo steps.** The slow tempo change (`tempo +`/`-`) is 0.01 BPM as `TransportImpl.changeTempo`
(Reaper: 0.1), the normal one 1 BPM.

**Values Bitwig does not take (takeover mode).** Bitwig applies its global takeover mode
(Settings > Controllers: immediate, catch, relative scaling) to `SettableRangedValue.set`; with
catch, a value is ignored until the control reaches Bitwig's current value. It is set to catch here, which is why
track volume and sends did not change in the fixture project, even from OSC. The Java version
uses `set` for knobs and faders (so does the Rust core, as on Reaper), and `setImmediately`
(`setValueImmediatly`) where the control has no position, e.g. the pads of the Launchpad Mix,
Device, User and Note Edit views. The Rust core sends these with the suffix `/immediately`
(`Parameter::set_value_immediatly`, only with host paging, i.e. Bitwig), and the extension calls
`setImmediately`. The core sets a value it sends locally right away, as for Reaper, and the
extension reports only changes, so a value which Bitwig ignored stayed on the controller; the
Java version always shows Bitwig's value. The extension now forgets the reported values of the
addressed channel or parameter 300 ms after a value command (`CommandHandler.reportAgain`,
`ModelReporter.forgetLater`), so the next collect reports Bitwig's values again and the LEDs fall
back to them. Found by the conformance runner 2026-10-30 (Launch Control XL knob LEDs).

## 7. Open: not handled yet

Logged as `Not handled yet: …` in the controller's script console and Bitwig's log.

- `metro_vol`, `inifile` (pre-roll measures), `groove`, `cursor`,
  user-configured Reaper actions (`action` with a string).

## 8. Known issues

- **Maschine Mk2, Shift view plays notes** (fixed and confirmed 2026-10-29): Shift (STEP
  LEFT on the Mk2) shows the Shift view with the undo/redo pads; the pads also played their MIDI
  notes. Not specific to Bitwig: the port's views kept the key translation table of the previous
  view, since the default `update_note_mapping` did nothing and was not called on activation.
  Now, as `AbstractView` of the Java version, activating a view calls `update_note_mapping`,
  whose default blocks the pads.
- **Launch Control XL, Layer Mix (3rd factory template):** in a first test only the knobs
  changed a panning, of an unknown target; the layers were not followed. To be investigated with
  the OSC monitor (which commands arrive, `/primary/hasDrumPads` and `hasLayers` of the tested
  instrument). Background: the device level differs between the Java versions. For Bitwig,
  `SpecificDeviceImpl` reports the real `hasDrumPads` / `hasLayers`, and the mode switches between
  the pad bank and the layer bank of the first instrument. For Reaper, `SpecificDeviceImpl` always
  reports pads (`hasDrumPads` true, "to support drum sequencers") and no layers (an empty
  `LayerBankImpl`); its pads are virtual and their channel commands went to `track <pad>/…`
  (BACKEND_PROTOCOL.md mismatch f). So with the Java version for Reaper the Layer Mix pan knobs
  change the pan of the *tracks* at the pad positions. In Rust the commands now go to
  `primary pad/<p>/…`, which Reaper ignores. The core assumes pads until the host reports
  otherwise, so a host without `/primary/hasDrumPads` stays on the (virtual) pads. Found
  2026-10-29. Later test on Bitwig: the Track Focus buttons select the pads of the drum machine,
  so the pads of the first instrument arrive.
- **Launch Control XL, Note Sequencer, Device + Track Select (also in the Java version):** with
  the Device button toggled on (pressed and released without use: the 3rd knob row controls
  device parameters), Track Select pages the clip, so the step LEDs change; only with Device
  held it selects the previous/next device (`XLBaseNoteEditMode.selectNext/PreviousItemPage`).
  The Java version behaves the same, so Rust keeps it; a candidate for the maintainer. Tested in
  `note_sequencer_track_select_with_device`. Found 2026-10-30.
- **Note attributes on Bitwig (fixed and confirmed for the Launch Control XL 2026-10-30):**
  the note editors were ported with the attributes which Reaper supports (former
  `IHost.supports (NoteAttribute)` of the Java version for Reaper: pitch, duration, mute,
  velocity). On Bitwig all attributes are supported. The Launch Control XL Note and Drum
  Sequencer now edit chance, repeat (ratchet), velocity spread and panning when the clip uses
  the step grid of the host. Still limited to the Reaper attributes: the Maschine edit note mode
  (`mk/mode/edit_note.rs`: velocity spread, release velocity, chance, occurrence, gain, pan,
  transpose, timbre, pressure) and the Launchpad note edit view (`launchpad/view/note_edit_view.rs`).
- **Launch Control XL, Note Sequencer Track Control row after a template change (Java bug):**
  the Java version registers the same note edit mode object (`noteEditMode`, also
  `drumPadEditMode`) in the main and in the fader mode manager. On a template change it is
  deactivated twice; the second `XLAbstractMainMode.onDeactivate` stores the track button mode
  which the new main mode has just activated (track mute of the Mixer), and the next activation
  restores it. After going to another template and back, the Track Control row of the Note
  Sequencer shows and toggles the track mutes instead of the mute state of the steps (manual:
  "Mute/Solo/Rec Arm buttons are off: Press to toggle the mute state of the note of the step").
  The Rust port registers separate fader modes and follows the manual. Found by the conformance
  runner 2026-10-30 (`tests/conformance/launchcontrol_xl/note_sequencer.toml`).
- **Master track twice (fixed 2026-10-30):** the extension always created Bitwig's full track
  bank (with effect tracks and the master) and the core added its own master for a full flat
  track list, so the master appeared twice (Maschine Mk2/JAM, the 8th track of the Launch Control
  XL Mixer). Now the extension creates the main track bank unless `hasFullFlatTrackList`, as
  `ModelImpl` of the Java version, and the core adds the master only if the host does not list
  one (Reaper).
