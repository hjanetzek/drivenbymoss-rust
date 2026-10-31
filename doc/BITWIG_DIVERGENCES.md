# Bitwig backend: differences from the Java DrivenByMoss

The Rust core with the Bitwig extension (`backends/bitwig`) is meant to behave like the Java
DrivenByMoss for Bitwig (`de.mossgrabers.bitwig.framework`, written by Jürgen Moßgraber). The
Bitwig side follows the Java implementation where it can. This document lists every place where
it does not, for the maintainer of the Java version. It is updated with each change of the
backend.

As of 2026-10-31 (core port of the Bitwig classes, P1 and P2: see BITWIG_CORE_PORT_PLAN.md). "Java" means the Java DrivenByMoss for Bitwig, "Rust" the core with the Bitwig
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
| Markers | `MarkerBankImpl`: a cue marker bank of `numMarkers`, scrolled by Bitwig, plus a bank of 100 for previous/next | One cue marker bank of 100 (if `numMarkers` > 0), reported completely (`/marker/<i>/…`), the core pages over it |
| Track remote controls | `CursorTrackImpl.getParameterBank`: a remote-control page cursor on the cursor track | As the project remote controls: `numParamPages` page cursors on the cursor track, each fixed to its page, reported as `/track/fx/param/…` (the Reaper address of the selected track's FX); the page names and Bitwig's selected page from one more cursor that follows Bitwig (`/track/fx/page/…`), `track parameterPage` shows the page in Bitwig |
| Bank paging | `canScrollPageBackwards` of the banks is Bitwig's `canScrollBackwards`: true for any position > 0 | Same with host paging (the Java version for Reaper requires a full page); found 2026-10-30 (Maschine JAM session, Shift + arrow scrolls by 1 track) |
| Selection after a page change | `AbstractItemBankImpl`: the previous item at the first item of a page scrolls back and selects the *last* item, the previous page selects the *first* item, the next page the first item, `selectItemAtPosition` scrolls and selects; all selections 75 ms after the page change | Same for tracks and scenes (core port P1 2 and 4, 2026-10-30; before, the previous item selected the first item of the previous page, and the scene selections were sent at once, while the extension still had the old window, so they were lost). Banks without a place in the model (slot banks of the tracks) still select at once |
| Track bank `scrollTo` | `scrollPosition ().set (position)`, exact (`adjustPage` ignored), selects nothing | Same with host paging (`bank track <position>`; core port P1 3, 2026-10-30; before, `<pos>/scrollto` selected the track). Reaper keeps `<pos>/scrollto` (the Reaper backend selects and scrolls to the track) |
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
| Automation write mode | `isArrangerAutomationWriteEnabled` / `isClipLauncherAutomationWriteEnabled`, `automationWriteMode` | Same switches and modes (see "Automation" in §6) |
| Pre-roll | `preRoll` (none, 1, 2, 4 bars) and the metronome during pre-roll | Same; the length is reported as `/click/prerollMeasures` and set with Reaper's command `inifile reaper/prerollmeas <bars>` (other lengths are logged, Java logs an error) |

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

**Relative value changes (core port P1 5, 2026-10-30).** As `RangedValueImpl.inc` /
`changeValue`: knobs change the parameters, the volume and panorama of tracks, the master, drum
pads and layers, the sends and the EQ parameters relatively. The core sends
`<value command>/inc <delta>` (e.g. `track 3/volume/inc`, `device param/9/value/inc`,
`track 2/send/1/volume/inc`, `drum pad/36/pan/inc`) with the delta normalized to the upper bound
of the value changer (`increment / upperBound`, as `inc (increment, upperBound)`), and the
extension calls `inc (delta)`; the core does not change its value, Bitwig reports it. Before, the
core sent its value plus the increment with `set`, which the takeover mode (catch) could swallow,
and fractional increments were cut. The clip's play start and end, loop start and length change
with `clip start|end|loopStart|loopLength/inc <beats>` (`getPlayStart ().inc (frac)` etc.; before:
absolute, the loop length could become ≤ 0). Not yet for the groove. Only with the host feature
`valueInc` (see "Host features" below); Reaper gets the absolute values.

**Reset (core port P1 6, 2026-10-30).** As `ParameterImpl.resetValue` (`parameter.reset ()`):
resetting a device parameter, the volume or panorama of a track, the master, a drum pad or layer,
a send or an EQ parameter sends `<value command>/reset` without a value (e.g. `track 3/volume/reset`,
`eq param/4/value/reset`, `master pan/reset`), and Bitwig sets its own default. Before, the core
set Reaper's defaults (0 for device parameters and sends, 0.716 = Reaper's 0 dB for the volume).
Only with the host feature `valueReset`; Reaper keeps the core's defaults.

**Device commands (core port P1 7, 2026-10-30).** As `SpecificDeviceImpl` and `DeviceImpl`: the
cursor device, the first instrument, the drum device and the EQ act on themselves with
`<device|primary|drum|eq> window/toggle|expand/toggle|bypass|remove|duplicate|select` (no value;
`isWindowOpen ().toggle ()`, `isExpanded ().toggle ()`, `isEnabled ().toggle ()`,
`deleteObject ()`, `afterDeviceInsertionPoint ().copyDevices (device)`, `selectInEditor ()`), the
siblings with `device sibling/<index + 1>/bypass|remove|duplicate|select` by their index in the
sibling window. Before, window and expand of all kinds went to the cursor device, and the bypass,
remove and duplicate of the specific devices were sent as `<index+1>/…` with index −1 or the
window index, which the extension matched against the position in the chain (wrong from position
8 on). Only with the host feature `deviceCommands`; Reaper keeps its commands.

**Host features (2026-10-31).** The Java version has one implementation per DAW; the core has
one for both and switched the Bitwig command forms with the host paging (`/track/offset`). Now the
extension reports the command forms it handles with `/host/features valueInc valueReset
immediately indication soloToggle transportCommands deviceCommands layoutStep` (first report and
every dump), and the core checks the feature per command (`DawContext::host_supports`);
`is_host_paging` only rules the paging itself. A paging host without `/host/features` gets all
features (compatibility with the recordings). Some switches now come from their own reports: the
pin commands with `/track/pinned`, `track selectPrevious|selectNext` with `/track/hasPrevious|hasNext`,
`project unsoloAll|unmuteAll` with `/project/hasSolo|hasMute`, (de)activating the master with
`/master/active`, and the VU clip state from the VU (Bitwig) unless the host reports the VU hold
values (Reaper). See HOST_BRIDGE_GUIDE.md §2.

**Step attributes (core port P1 8, 2026-10-30).** As `CursorClipImpl.updateStep*`: outside of
an edit, changing one attribute of a step writes only that attribute,
`clip grid/<id>/step/<channel>/<x>/<y>/<attribute> <value>` with the attributes `muted`,
`duration`, `velocity`, `velocitySpread`, `releaseVelocity`, `pressure`, `timbre`, `pan`,
`transpose`, `gain`, `chanceEnabled`, `chance`, `occurrenceEnabled`, `occurrence` (index),
`recurrenceEnabled`, `recurrenceLength`, `recurrenceMask` (the other recurrence value is Bitwig's,
as `setRecurrence (length, noteStep.recurrenceMask ())`), `repeatEnabled`, `repeatCount`,
`repeatCurve`, `repeatVelocityCurve`, `repeatVelocityEnd`; flags as 0/1. In an edit
(`startEdit` / `stopEdit`, every 100 ms) all attributes are written with `…/update` as before
(`sendClipData`). Before, every change wrote all 21 attributes, including values not received yet.
The gain is still reported as `gain () / 2` and written with `setGain (value)` as in the Java
version (`StepInfoImpl.updateData`, `sendClipData`); whether both use the same range is open.

**Step attribute knobs (core port P1 9, 2026-10-30).** The knob changes of the step attributes
keep their fraction (`toNormalizedValue (calcKnobChange (control))` of `CursorClipImpl` for Bitwig);
the core cut them to integers as the Java version for Reaper does (`(int) calcKnobChange`), so at
the default sensitivity (1.09 per step) the changes were smaller. Also on Reaper.

**New note clips (core port P1 10, 2026-10-30).** As `ModelImpl.createNoteClip`: the clip is
created (`track <p>/createClip/<slot> <length>`), the slot selected and launched, overdub switches
the launcher overdub on (`transport overdub/launcher 1`); before, arranger recording started and
the clip was not launched. `recordNoteClip` records the slot unless it records already
(`track <p>/clip/<s>/record`) and launches it; before, `track <p>/recordClip` was sent, which the
extension did not handle. Also on Reaper.

**Transport (core port P2, 2026-10-30).** As `TransportImpl` of the Java version for Bitwig:
`transport play|stop|restart` (`transport.play ()`, `stop ()`, `restart ()`, record is not touched;
before, play and stop switched the recording off first and restart was stop + play),
`transport stopAndRewind` (`stop ()`, `setPosition (0)` 100 ms later; before, the play start
position was set at once), the metronome ticks (`/click/ticks`, `transport metronomeTicks <0|1>`,
`transport toggleMetronomeTicks`; before: always off), the fill mode (`/fill`, `transport fill <0|1>`,
`transport toggleFill`; before: always off), independent punch in and out (`/punchIn`, `/punchOut`,
`transport togglePunchIn|togglePunchOut`; before, one punch state in which in and out excluded each
other), the metronome volume (`transport metronomeVolume <value>` and `metronomeVolume/inc <delta>`
as `set`/`inc (…, upperBound)`, reset to 0.8, the text `displayedValue ().getLimited (6) + " dB"`;
before, `metro_vol` was not handled). Only with the host feature `transportCommands`; Reaper keeps its commands. Note: the
Pause behaviour of the play button calls `play`, which in Bitwig starts but does not pause (as in
the Java version).

**Application and project (core port P2, 2026-10-30).** As `ApplicationImpl`, `ProjectImpl`,
`ArrangerImpl` and `MixerImpl`: the extension reports the audio engine (`/project/engine`,
`project engine <0|1>` → `activateEngine ()` / `deactivateEngine ()`), the panel layout
(`/layout <ARRANGE|MIX|EDIT|PLAY>`; previous and next with `application layout/-|+` →
`previousPanelLayout ()` / `nextPanelLayout ()`), whether any track of the project is soloed or
muted (`/project/hasSolo`, `/project/hasMute`; clearing with `project unsoloAll|unmuteAll`, before:
a solo/mute command per known track), the cue volume and cue mix (`/project/cueVolume`,
`/project/cueMix` with `…/str`, `…/str/6|8`; `project cueVolume|cueMix` with `/inc`, `/reset`, `/touch`;
before: no-ops) and the visibility flags `/arranger/cueMarkers|doubleRowTrackHeight|clipLauncher|timeline|ioSection|effectTracks`
and `/mixer/clipLauncherSection|crossFadeSection|deviceSection|ioSection|meterSection|sendSection`
(before: always false).

**Tracks and channels (core port P2, 2026-10-30).** As `ChannelImpl`, `TrackImpl`, `SendImpl` and
`AbstractTrackBankImpl`: the extension reports `active` for tracks and the master
(`isActivated ()`; before only for pads and layers, the core kept true and toggled locally, which
also broke `set_skip_disabled_items`; the master can be (de)activated), `/track/<p>/monitor` is the
effective monitoring `isMonitoring ()` (also AUTO while armed; before: monitor mode ON),
`/track/<p>/canHoldNotes|canHoldAudio` (`canHoldNoteData ()`, `canHoldAudioData ()`; before: true for
every track), the send colors `/track/<p>/send/<s>/color` (`sendChannelColor ()`; before: black).
The solo button toggles with `<channel> solo/toggle` (`solo ().toggleUsingPreferences (false)`, so
the exclusive solo preference of Bitwig applies; before: `solo <!local>`). Indications:
`<channel> volume/indicate|pan/indicate <0|1>`, `track <p>/send/<s>/volume/indicate <0|1>`
(`setIndication`) and `track indicate <0|1>` (`trackBank.setShouldShowClipLauncherFeedback`);
before: no-ops. The VU clip state is `vuLeft >= 16241/16384` (`CLIP_BORDER`; before: Reaper's hold
values, never reported). The Java bug of `TrackImpl.setMonitor (false)` (sets ON) is not copied.

**Pinned cursor track, previous/next track (core port P2, 2026-10-30).** As `CursorTrackImpl`: the
pin is Bitwig's `cursorTrack.isPinned ()` (`track pinned <0|1>`, `track pinned/toggle`); the extension
reports `/track/pinned <position>` (−1 if not pinned) and the core shows the pinned track as the
cursor track; the selection is reported from a second cursor track which is never pinned, so that
Bitwig's cursor device and remote controls stay on the pinned track as in the Java version (before,
the core kept the pin itself and sent `track <p>/pin`, which the extension ignored, so the device
followed the selection). Previous and next track move the cursor track (`track selectPrevious|selectNext`,
`cursorTrack.selectPrevious ()` etc.) and their availability is `/track/hasPrevious|hasNext`
(before: via the track bank). The playing notes are those of the cursor track.

**Groove (core port P2, 2026-10-30).** As `GrooveImpl`: six `ParameterImpl`s on `host.createGroove ()`
(`enabled`, `shuffleAmount`, `shuffleRate`, `accentAmount`, `accentRate`, `accentPhase`), reported as
`/groove/<name>`, `/groove/<name>/name`, `/groove/<name>/str` (+ `/6`, `/8`), changed with `groove <name>`
(`/inc`, `/reset`, `/touch`, `/indicate`, `setIndication` on all six). Before, only the enabled state and
the shuffle amount existed with Reaper's range −1..1 and texts computed by the core, and the extension
handled none of them (the groove did not work on Bitwig). Reaper keeps `groove active|amount`.

**Send names, accent reset (core port P2, 2026-10-30).** `can_edit_send` / `edit_send_name` use the send
of the first track of the page and its name, as `AbstractTrackBankImpl.canEditSend` /
`getEditSendName` (before: any track of the page, the name "Send N"); also on Reaper. Resetting the
accent of the clip sends `clip accent/immediately 0.5` (`getAccent ().setImmediately (0.5)`; before:
`set`, which the takeover mode could swallow).

**Scenes, parameter pages, touch, pads, layers (core port P2, 2026-10-30).** With host paging:
the scene bank's `can_scroll_backwards|forwards` are those of `AbstractItemBank` (a selected scene
after the first one / an existing next scene on the page, or a previous / next page; before: the
offset as the Java version for Reaper); after a device change the parameter page is the one
Bitwig reported last (`remoteControls.selectedPageIndex ()`, Bitwig remembers the page per device;
before: the page the core remembered for the device name); `ParameterPageBankImpl.canScrollPageForwards`
asks for a next page *of pages* (before: a next parameter page); touching a parameter only sends
`touch` (`ParameterImpl.touchValue`), the host's values are shown while touched (host feature
`valueInc`; before: held back until released, as Reaper); selecting a drum pad selects it only in the editor
(`DrumPadImpl.select`: `selectInEditor ()`, host feature `deviceCommands`; before: also in the
mixer and made visible) and the
generic bank selections (`select_next_item` etc.) work on pads (before: no-ops); the layer bank
scrolls exactly to a position (`scrollTo`, `adjustPage` ignored; before: page aligned). Selecting
the master selects it in the editor and the mixer and makes it visible (`ChannelImpl.select`).

**Device siblings (core port P2, 2026-10-30).** As `CursorDeviceImpl.selectPrevious/Next`: at the
first (last) device of the sibling page the page moves as well (`device page/-|+`), and as
`AbstractItemBankImpl.selectPreviousPage/NextPage` the first device of the new page is selected
75 ms later (`device sibling/1/select`); without the guard on the core's position and count. Before,
the sibling window did not follow the cursor device and a page change did not select. Note: so
`selectPrevious` at the first device of a page ends on the first device of the previous page (as in
the Java version).

**Record quantization (core port P2, 2026-10-30).** Global as in `ApplicationImpl` (`TrackImpl` delegates):
the extension reports `/recordQuantization <OFF|1/32|1/16|1/8|1/4>` (`recordQuantizationGrid ()`) and
`/recordQuantization/noteLength` (`recordQuantizeNoteLength ()`); the tracks show and change these
(`application recordQuantization <grid>`, `application recordQuantization/noteLength/toggle`). Before,
per track as Reaper (`inQuantResolution`), unhandled, and the note length toggle did nothing.

**Add track (core port P2, 2026-10-30).** `track addTrack <type> <name> <color> <devices…>` (string
arguments) is handled now, as `AbstractTrackBankImpl.addChannel`: `createInstrumentTrack (-1)`,
`createEffectTrack (-1)` or `createAudioTrack (-1)`, then 300 ms later the name and color of the new
(cursor) track. Differences: the devices are Reaper creation names which Bitwig does not know, they
are dropped (logged as not handled); the track bank is not scrolled to the new track
(`scrollIntoView`), the core pages the bank itself. Before, `DbmExtension.processStringArgs` only logged
the command.

**Project remote controls (core port P2, 2026-10-30).** As `ProjectImpl`: the remote controls of the
root track group (`rootTrackGroup.createCursorRemoteControlsPage`), with fixed page cursors as for
the devices (`numParamPages`), reported as `/project/param/<i>/…`, `/project/param/count`,
`/project/page/<p>/name`, `/project/page/count`, `/project/page/selected`; commands
`project param/<i>/value|touch|indicate` (with `/inc`, `/reset`) and `project parameterPage <page>`. Before,
the core used a bank on the master FX (Reaper), whose `master param/…` commands the extension did not
handle. The extension creates them if the model setup has `numParams` > 0.

**Commands the extension did not handle (2026-10-31).** These were sent by the core but logged as
not handled:
- `clip clear` (the cursor clip without a host step grid): `clearSteps ()` as `CursorClipImpl.clearAll`.
- `time signature <numerator + denominator * 100>`: `timeSignature ().numerator ().set` and
  `denominator ().set` as `TransportImpl.setTimeSignature`.
- `time loop/start|length <seconds>` (Reaper form): converted to beats at the tempo,
  `arrangerLoopStart|arrangerLoopDuration ().set` (the core sends `beats loop/…` itself).
- `track <p>/movePrev|moveNext`: as `CursorTrackImpl.swapWithPrevious|swapWithNext` (the neighbour
  from the 400-track bank is moved with `afterTrackInsertionPoint|beforeTrackInsertionPoint ().moveTracks`
  and selected in the editor), but on the track of the command instead of Bitwig's cursor track
  (the core sends it for its cursor track or a track of the bank).
- `track <p>/toggleMultiSelect`: ignored, as `ChannelImpl.toggleMultiSelect` (no multi-selection in
  the API).
- `device <i>/movePrev|moveNext`: as `CursorDeviceImpl.swapWithPrevious|swapWithNext` on the cursor
  device (a bank of 100 devices of the cursor device's chain, `moveDevices`, the neighbour is
  selected); the index of the core is not needed.
- `device preset <index>` (Reaper's own preset browser, only without a host browser) and
  `device add/0 <creation name>` (Reaper's device creation names; `ChannelImpl.addDevice` uses
  Bitwig's device metadata, which the core does not have): ignored.
- Markers (`MarkerBankImpl`, `MarkerImpl`): `marker add` → `addCueMarkerAtPlaybackPosition ()`,
  `marker <i>/launch` → `launch (true)` (all controllers of the Java version launch quantized),
  `marker <i>/select` → the play start position at the marker (`TransportImpl.setPosition`),
  `marker <i>/remove` → `deleteObject ()`. Previous/next marker: as Java's `findAndSelect`, the play
  start position goes to the closest marker before/after the play position, searched in the
  marker bank of 100. Without a marker bank (the controller uses no markers, Java then does
  nothing) Bitwig's `jumpToPrevious|NextCueMarker ()`.
- `inifile reaper/prerollmeas <bars>`: the pre-roll length (see §3); other keys are logged.
- `action <id>`: `application.getAction (id).invoke ()` as `ApplicationImpl.invokeAction`; IDs which
  Bitwig does not know (Reaper's numbers) are ignored silently, as in Java.
- The crossfader (`TransportImpl.getCrossfadeParameter`, a `ParameterImpl` on `transport.crossfade ()`):
  the extension reports `/crossfade` (+ `/str`, `/str/6|8`, `/name`); the core then sends
  `transport crossfade` with the value suffixes, `crossfade/touch` and `crossfade/indicate` instead of
  Reaper's master FX parameter `master user/param/0/value`.
- The track parameters (`track <p>/param/<i>/value|touch|indicate`, e.g. the Launchpad User view):
  the remote controls of the cursor track (see §2), reported as `/track/fx/param/…`; with Bitwig's
  pinning the core puts them on its cursor (pinned) track instead of the selected track.
- `device indicate` (the pad bank of the cursor device) is not sent by the core: the drum views
  indicate the pads of the drum devices (`drum indicate`).

**Tempo steps.** The slow tempo change (`tempo +`/`-`) is 0.01 BPM as `TransportImpl.changeTempo`
(Reaper: 0.1), the normal one 1 BPM.

**Values Bitwig does not take (takeover mode).** Bitwig applies its global takeover mode
(Settings > Controllers: immediate, catch, relative scaling) to `SettableRangedValue.set`; with
catch, a value is ignored until the control reaches Bitwig's current value. It is set to catch here, which is why
track volume and sends did not change in the fixture project, even from OSC. The Java version
uses `set` for knobs and faders (so does the Rust core, as on Reaper), and `setImmediately`
(`setValueImmediatly`) where the control has no position, e.g. the pads of the Launchpad Mix,
Device, User and Note Edit views. The Rust core sends these with the suffix `/immediately`
(`Parameter::set_value_immediatly`, only with the host feature `immediately`, i.e. Bitwig), and the extension calls
`setImmediately`. The core sets a value it sends locally right away, as for Reaper, and the
extension reports only changes, so a value which Bitwig ignored stayed on the controller; the
Java version always shows Bitwig's value. The extension now forgets the reported values of the
addressed channel or parameter 300 ms after a value command (`CommandHandler.reportAgain`,
`ModelReporter.forgetLater`), so the next collect reports Bitwig's values again and the LEDs fall
back to them. Found by the conformance runner 2026-10-30 (Launch Control XL knob LEDs).

## 7. Open: not handled yet

Logged as `Not handled yet: …` in the controller's script console and Bitwig's log.

- `cursor`, `inifile` keys other than the pre-roll length.

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
  the step grid of the host. Fixed 2026-10-31 (not yet confirmed on Bitwig): the Maschine edit
  note mode (`mk/mode/edit_note.rs`) has the 'Note', 'Expressions' and 'Repeat' pages of the Java
  version, and the Launchpad note edit view (`launchpad/view/note_edit_view.rs`) edits all 8
  attributes, when the clip of the edited notes uses the step grid of the host
  (`NoteEditor::is_host_grid`).
- **Maschine Mk2, page selection with STEP LEFT (also in the Java version):** the Mk2 emulates
  the encoder up/down with STEP LEFT (Shift) + the arrows below the encoder
  (`MaschineCursorCommand`). The command does not consume STEP LEFT (no `setStopConsumed`), so
  its release still runs the stop function. Rust keeps it; a candidate for the maintainer.
  Tested in `edit_note_mode_has_3_pages_with_the_host_grid`. Found 2026-10-31.
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
- **Chords which share notes (Java bug):** the chord views (`AbstractChordView`) and the chord
  mode of the Maschine play view send a note off for every note of a released chord, also for
  notes which another held chord still plays. The Rust port keeps the chords of the held pads
  (`HeldChords`, `framework/view/play_view.rs`): an added note is only sent off when no held
  chord plays it. The root comes from the note input of the pad, which always ends it. If
  another held chord contains it, it is sent on again. Found by the user on the JAM and Mk2, in
  both versions (2026-10-31). Upstream candidate K (UPSTREAM_CANDIDATES.md).
- **Master track twice (fixed 2026-10-30):** the extension always created Bitwig's full track
  bank (with effect tracks and the master) and the core added its own master for a full flat
  track list, so the master appeared twice (Maschine Mk2/JAM, the 8th track of the Launch Control
  XL Mixer). Now the extension creates the main track bank unless `hasFullFlatTrackList`, as
  `ModelImpl` of the Java version, and the core adds the master only if the host does not list
  one (Reaper).
