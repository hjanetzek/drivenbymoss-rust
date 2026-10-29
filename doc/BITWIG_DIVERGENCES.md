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
| Settings | `SettingsUIImpl` on `getPreferences ()` / `getDocumentState ()` | The core's `.config` files in `~/.config/DrivenByMoss-Bitwig` | Not yet ported (plan step 5) |
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
| First instrument | `SpecificDeviceImpl` on the instrument matcher: one remote-control page cursor, `numDeviceLayers` layers and `numDrumPadLayers` pads, scrolled by Bitwig; a cursor layer if `wantsCursorLayer` | Same matcher; the parameters as for the cursor device (`numParamPages` fixed page cursors, page names, no page selection), all 128 pads and 32 layers reported completely, the core pages over them with the pad page size. Layer sends, the devices of a layer and the cursor layer are not reported |
| Effect tracks, browser, note clip grids | Created from the ModelSetup | Not created yet (protocol v2 items 7–8) |
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

## 7. Open: not handled yet

Logged as `Not handled yet: …` in the controller's script console and Bitwig's log.

- `metro_vol`, `automation`, `inifile` (pre-roll measures), `quantize`, `groove`, `clip` (the
  cursor clip), `eq`, `browser`, `cursor`, user-configured Reaper actions (`action` with a
  string).

## 8. Known issues

- **Maschine Mk2, Shift view plays notes:** Shift (STEP LEFT on the Mk2) shows the Shift view
  with the undo/redo pads. Undo and redo are executed, but the pads also play their MIDI notes
  into Bitwig. Probably the note input still passes the pads through while the Shift view is
  active (the key translation table should block them, as the Java version does); to be
  investigated. Found 2026-10-29.
