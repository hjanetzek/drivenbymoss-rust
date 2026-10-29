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
| Track bank | Bank of `numTracks` (ModelSetup), flat or hierarchical, follows the cursor track, scrolled by Bitwig | Flat bank of 64 tracks (`BitwigModel.NUM_TRACKS`) that never scrolls; the core pages over it. Tracks beyond 64 are not reachable. Hierarchical track lists (`hasFlatTrackList` false) are not supported |
| Track depth | From the hierarchy | Not reported (0) |
| Effect tracks, scene banks, slot banks, drum pad banks, device layers, browser, note clip grids | Created from the ModelSetup | Not created yet (protocol v2 items 3–8) |
| Device siblings | `createSiblingsDeviceBank (numDevicesInBank)` | Always 8 siblings (the Reaper protocol's fixed window) |
| Device parameters | Remote-control page bank: named pages, page selection by Bitwig | `numParamPages` page cursors, each fixed to one page, reported as one flat list of parameters; the core names the pages "Page n". Page names are not shown (protocol v2 item 4) |
| Parameter extras | `modulatedValue`, `setIndication`, `touch` for all parameters | `touch` only for track and master volume and pan |

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

## 4. Note repeat

| Topic | Java | Rust |
| --- | --- | --- |
| Modes | All arpeggiator modes of Bitwig | The core's four modes (down, up, down-up, up-down) |
| Octaves, shuffle, free running, note latch | Supported | Not exposed |

## 5. Commands

| Command | Java | Rust |
| --- | --- | --- |
| Arrow keys | `arrowKey*` | Same |
| Previous / next track (Reaper actions in the core) | - | `cursorTrack.selectPrevious/selectNext` |
| Track volume/pan "touch" | `touch` | Same |
| Device parameter commands of the first instrument and drum devices | The specific device | Not yet: the extension has only the cursor device |

## 6. Open: not handled yet

Logged as `Not handled yet: …` in the controller's script console and Bitwig's log.

- `metro_vol`, `automation`, `inifile` (pre-roll measures), `quantize`, `groove`, `clip`, `scene`,
  `eq`, `browser`, `cursor`, user-configured Reaper actions (`action` with a string).
