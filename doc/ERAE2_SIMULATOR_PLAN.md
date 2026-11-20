# Embodme Erae 2: research and a simulator plan

Status: research and planning only. There is no simulator code yet. Researched 2026-11-20.

The Erae 2 is a 42 × 24 RGB LED touch surface with XYZ multitouch. Its developer API is
**documented and public**: SysEx to draw on the LEDs and a "finger stream" of raw touches. A
host can draw on rectangular "API zones" that the user places in a layout, or, on current
firmware, on the whole surface through "plugin surface" sessions, with no layout needed. So a
DrivenByMoss port is feasible without reverse engineering. The simulator can follow the spec
byte for byte, and it is a good template for the planned tablet controller.

Conventions: **[guess]** marks assumptions nobody has checked. "Manual (de), p. N" is the
printed page number of the German user manual `~/Downloads/Erae-User-Manual-de.pdf` (PDF dated
2026-11-19; the printed page is the PDF page minus 11). The English web manual has the same
content.

## Sources

| Key | Source |
| --- | --- |
| [M2] | Erae 2 user manual (web, marked "beta"): https://www.embodme.com/manual/erae-2, esp. Appendix D "Developer API" (`#appendix-d-developer-api`) and Appendix B "MIDI Implementation Chart" |
| [M2de] | Erae user manual (de), local PDF, same content as [M2] |
| [API] | "ERAE API V2", Rev 004, 2025-04-24: https://gitlab.com/embodme/erae_api_sysex/-/blob/main/Erae_API_V2.pdf |
| [PY] | Embodme's Python example (python-rtmidi, OpenCV): https://gitlab.com/embodme/erae_api_sysex (`erae/erae_api_sysex.py`, `erae/utils.py`, `run_erae_api.py`) |
| [LAB] | Erae Lab manual: https://www.embodme.com/manual/erae-lab |
| [REL] | Release repos with version lists: https://gitlab.com/embodme/erae_lab_release, https://gitlab.com/embodme/erae_touch_release |
| [BC] | Bitwig extension for the Erae Touch (MPE note input only): https://github.com/BeneathClouds/erae-touch |
| [KVR] | "flush spam" thread, an Erae 2 clip launcher script for Bitwig: https://www.kvraudio.com/forum/viewtopic.php?p=9089558 |
| [SA] | Synth Anatomy on the Erae II launch: https://synthanatomy.com/2024/02/embodme-erae-ii-redesigned-mpe-touch-controller-for-the-modular-world.html |
| [FD] | Embodme forum threads on the API (login wall, seen only as search snippets): https://embodme.freshdesk.com/support/discussions/topics/80000664838 |

## 1. The device and its protocol

### 1.1 Hardware

- **Surface:** a grid of 42 columns × 24 rows with 1,008 RGB LEDs. It sits on an FSR matrix with
  16 sensing sites per LED cell, over 16,000 in total, scanned 1,600 times per second. Touch
  positions are finer than one cell. [M2] ch. 3; manual (de), p. 11-12. The physical size and cell
  pitch aren't in the manual. **[guess]** about 1 cm per cell.
- **Touch:** up to 10 fingers. Each finger has X, Y and Z (pressure, normalized 0..1) and a motion
  speed of 0..100 cm/s. Two fingers closer than about 1.5 cells count as one touch. Short pressure
  dips don't release a finger. Manual (de), p. 12-13.
- **LEDs:** full RGB with colour correction and a global brightness setting. In Low Power mode
  (a source below 15 W, e.g. a computer port) the brightness is reduced. [M2] ch. 13 and ch. 2;
  manual (de), p. 98 and p. 4.
- **Other controls (Erae 2):** 18 panel switches with RGB LEDs (N1-N8 select a layout; Play/Rec,
  Stop, Plus, Minus, Scale, Sequencer, Mapping, Arp, Settings, Return), a rotary encoder with a
  push switch, and a 280 × 240 colour LCD. [M2] ch. 11-12; manual (de), p. 94. **These are not
  reachable from the host.** The API only covers the surface. The switches and the LCD belong to
  the firmware.
- **Connectivity:** three USB-C ports (Power 65 W; Computer = USB device, class compliant; Synth =
  USB host). MIDI In and MIDI Out A and B on 3.5 mm TRS jacks. 24 CV outputs on 12 jacks, two pedal
  inputs. An SD card holds the projects. No BLE is mentioned. [M2] ch. 2; [SA].
- **Firmware:** firmware 2.1.0 was the first public release (shipped December 2024) [SA]. The web
  manual describes a newer **beta** ("What's New in This Beta"). Erae Lab ships the firmware and
  updates it, also on Linux. [REL] lists the Erae Lab versions 1.3.0, 1.4.0 and 1.4.1, and the Erae
  Touch firmware 1.3.0 to 1.4.1. A MIDI 2.0 USB mode exists (a reboot toggles it) but is off by
  default.
- **Erae Touch (first generation):** the same 42 × 24 surface and MIDI engine. Its sensor reads Z
  only (X/Y come from the pressure spread). It has 5 buttons (Home, Alt, Scale, Plus, Minus), no
  LCD, no encoder, no CV and no TRS MIDI input, and is powered by a barrel jack. [M2] ch. 17; manual
  (de), p. 125-130. Same API, with family member code `00 01` instead of `00 02`.

### 1.2 Software model: projects, layouts, elements

- A **project** has 8 layouts (Erae Touch: 4 projects of 8). N1-N8 switch between them. [M2] ch. 4.
- A **layout** is a set of rectangular **elements** on the 42 × 24 grid: Keygrid (isomorphic,
  scales, MPE), Keyboard (piano style, MPE), Drumpad (chromatic grid), Key (a single expressive
  pad), Button (Note/CC/PC/CV/Tap Tempo, momentary or latched), Fader 1D, Fader 2D (XY), Spring 1D
  and 2D (return to centre), LivePad (Ableton session control), API Zone, and Pedal (no area).
  [M2] ch. 5; manual (de), p. 31 for the API zone.
- Each element has its own channel, its outputs (USB device or host, MIDI A or B), its expression
  mappings (pressure as channel or poly AT, X as pitch bend, Y and relative Y as CCs, CC74 slide,
  motion speed, 14-bit CC, RPN or NRPN) and its LED style and animations. The firmware draws the
  element's LEDs from touch, not from incoming MIDI. "Press feedback comes from touch detection,
  not from MIDI output" ([M2] ch. 13). As far as the manual shows, **native elements take no LED
  feedback from the host**. The exception is the LivePad, which the Ableton Remote Script drives
  with private messages ("Erae Lab and the Ableton Live Remote Script use their own messages",
  Appendix B; manual (de), p. 151).
- **Layouts are made in Erae Lab** (macOS, Windows, Linux; Ubuntu 22.04+ recommended) and synced
  to the device over a private USB connection. That connection no longer shows up as a MIDI port:
  older firmware had a third MIDI port for Lab and the API. [M2] ch. 6 and ch. 14; [LAB]. A
  project is exported as a folder. The legacy formats are `.erproj`, `.eraeproject` and `.emk`
  (Erae Touch layouts). On Linux the projects live in `~/.local/share/Embodme/EraeLab/projects`
  [LAB]. **The file format isn't documented.** **[guess]** It may be JSON, written with
  Embodme's open "enki" serializer (https://gitlab.com/embodme/enki, which has binary and JSON
  writers).
- **USB ports** (MIDI 1.0 mode), [M2] ch. 2 and ch. 6; manual (de), p. 6-7:

  | Port | macOS / Linux | Windows | Carries |
  | --- | --- | --- | --- |
  | Main | `Erae 2 MIDI` | `Erae 2` | all non-MPE elements; the only port the device receives on (clock, LED feedback, **API**) |
  | MPE | `Erae 2 MIDI (MPE)` | `MIDIIN2 (Erae 2)` | MPE keyboards only, output only |

- **MPE:** lower zone (master channel 1, members upward from 2) or upper zone (master 16), with
  dynamic member allocation. Pitch bend range 48 by default with MPE (12 without). CC74 slide,
  channel pressure. **No MPE configuration message (MCM) is sent** and no pitch bend range RPN.
  Manual (de), p. 149-150.
- **Identity:** the device answers the universal identity request on the port it came in on, with
  `F0 7E 01 06 02 00 21 50 00 01 00 02 v1 v2 v3 v4 F7`, where the last four bytes are the firmware
  version. Manual (de), p. 150.

### 1.3 The developer API (SysEx)

The fullest description is [M2] Appendix D (manual (de), p. 174-189). [API] is the older PDF of the
same protocol. It has copy-paste errors: its "Draw pixel" line shows `0x22` with WIDTH/HEIGHT, and
several end bytes are printed as `0xF0`. Where the two differ, Appendix D is authoritative.

**Host → device header:**

```
F0 00 21 50 00 01 00 02 01 01 04 <sub> <payload> F7
   │manuf.  │family│model │dev │svc│API
```

`00 01 00 02` is the Erae 2 (`00 01 00 01` the Erae Touch). The device ID is always `01`, and the
broadcast `7F` is ignored.

| Sub | Command | Payload |
| --- | --- | --- |
| `01` | StartFingerDataStreaming ("API mode enable" in [API]) | prefix, 0..16 bytes ([API]: 1..16). Sending it again replaces the prefix |
| `02` | EndFingerDataStreaming | none |
| `10` | ZoneBoundaryRequest | zone |
| `20` | Clear zone | zone |
| `21` | SetPixel | zone x y r g b (7-bit, the device shifts each left by 1) |
| `22` | DrawRectangle | zone x y w h r g b (clipped to the zone) |
| `23` | DrawImage | zone x y w h, the bitized 8-bit RGB data, then an XOR checksum |
| `7F` | VersionRequest | prefix (answered even without streaming) |
| `30` | Plugin surface: Open | len (1..24), name (ASCII), prefix (0..16) |
| `31` / `32` / `33` | Show / Heartbeat (s, prefix) / Close | session id |
| `40`..`43` | Session Clear / SetPixel / DrawRectangle / DrawImage | as `20`..`23`, with the session id in place of the zone |

**Device → host:** `F0 <prefix> <body> F7`. There is no Embodme header, and the prefix is copied
verbatim. Embodme suggests starting it with `7D` (non-commercial) followed by a tag. That keeps the
replies valid SysEx and tells devices apart.

| Body | Meaning |
| --- | --- |
| `7F 01 zone w h` | zone boundary reply (`7F 7F` = no such zone) |
| `7F 02 ver` | version reply, `02` today. The Erae Touch and early Erae 2 firmware don't answer |
| `7F 03 s w h max lease` | session opened (example `2A 18 04 14`: 42 × 24, 4 sessions, a 2 s lease) |
| `7F 04 reason` | open refused (01 full, 02 bad name, 03 busy) |
| `7F 05 s on reason` | focus on or off (01 show, 02 user, 03 superseded, 04 busy, 05 preview) |
| `7F 06 s reason` | closed (01 closed, 02 expired, 03 unknown) |
| `act zone fid[10] xyz[14] chk` | **finger stream**: act 0 click, 1 slide, 2 release. fid is a uint64 finger ID, bitized. xyz are 3 little-endian float32 values, bitized: zone-local x and y in cells (bottom origin), z pressure ≈ 0..1. chk is the XOR of the 14 xyz bytes |

- **Bitize-7:** each group of 7 source bytes becomes 8 bytes. The first byte holds the MSBs (source
  byte 0 in bit 6), the next 7 the low 7 bits. The checksum is the XOR of all *encoded* bytes, MSB
  bytes included. Reference code in Python and C++ is in [API] and [PY].
- **Coordinates:** zone-local, origin bottom left, Y up, the same for touches and drawing. Most
  UIs (and QML) put the origin at the top left, so the simulator flips Y in one place.
- **DrawImage is silently ignored** if the size isn't `w*h*3`, if it exceeds 1008 pixels, or if
  the checksum is wrong. Out-of-range SetPixels are dropped. Incoming SysEx may be up to 8 KB, and
  device → host messages are at most 128 bytes. [API] recommends images of at most 32 pixels per
  message. Appendix D says a full surface is about 3.5 KB and should be paced.
- **Plugin surface (D.9)** lets the host take the whole surface without an API zone in the
  layout. Up to 4 sessions can be open, and each has a 42 × 24 buffer. One source is visible at a
  time. Only the visible session gets touches, and it doesn't need StartFingerDataStreaming. The
  2 s lease is renewed by any message; send a heartbeat every 500 ms. Show waits until 1 s after
  the last finger lifted. A layout switch (N1-N8 or Return) takes the screen back with Focus off,
  and the firmware releases every finger of the session first. To detect support, send Open and
  treat no answer within 500 ms as "not supported". **This is documented in the beta manual.**
  Its reference client (`erae_plugin_surface`, built on mido, with the demo game "Erae Rally") is
  not public as far as I could find. Which firmware has it is an open question.
- **Linux caveat (D.1):** in MIDI 2.0 mode, Linux can corrupt SysEx longer than 6 data bytes on
  the MIDI 1.0 ports, and the device stays silent. Keep MIDI 2.0 off.
- **Python example caveat ([PY]):** after a zone boundary reply, `receive_midi_message` checks
  `API_VERSION_REPLY` against the wrong offset. Don't copy it blindly.

### 1.4 Existing integrations

- **DrivenByMoss:** no Erae support. I checked `git-moss/DrivenByMoss` `origin/master` (824f8740,
  2026-11-16), the user's fork, and `git-moss/DrivenByMoss4Reaper` `origin/HEAD` (sources only:
  no `erae` or `embodme` in any `.java`, `.cpp`, `.h` or `.md`). The Rust port has none either.
- **Ableton Live:** Embodme's "Erae2" MIDI Remote Script drives the LivePad element (session,
  mixer and split views; clip colours follow Live). Its messages are private. [M2] Appendix C.
- **Bitwig:** the manual recommends the ROLI Seaboard Rise script or Generic Flexi, i.e. no
  session control. [BC] is a minimal Java extension for the Erae Touch: two note inputs (main and
  MPE with expressive MIDI, PB range 48) and the MPE zone RPN. No API use. [KVR] (June 2025) is
  someone writing an Erae 2 clip launcher in Bitwig JS. It stalled on the finger stream: "hundreds
  of flush calls" because every incoming SysEx triggers a flush. There were no replies.
- **Others:** Max patches and TouchDesigner are the intended API users. Forum posts mention a
  companion app that shows the live finger stream, and the "SP-Tools" Max package [FD]. None of
  them is a DAW control surface.

## 2. Simulator design on our stack

### 2.1 Where it goes and what is reused

A new package `support/controller_simulator/erae_simulator/`, next to `maschine_simulator` and
`midi_simulator`, plus a `--model erae2|eraetouch` case in `support/controller_simulator/run.sh`.
`midi_simulator`'s `GridDevice` assumes one MIDI message per control. The Erae has a pixel
surface and a SysEx stream, so it needs its own model, but it reuses everything around it:

| Reused | For |
| --- | --- |
| `sim_common.alsa_seq` | client `Erae 2`, the ports, `connect_both`, the reader thread. `BUFFER_SIZE` is 64 KB, enough for the 8 KB SysEx limit |
| `sim_common.app` | `--connect`, `--monitor`, `--no-alsa`, `--quit-after`, `--screenshot`, `--size`, `open_ports`, `run` |
| `sim_common.qt_base.SimulatorBase` | revision tick, status bar, log, MIDI monitor. Subclassed as `EraeSimulator` with its own signals (see the PySide6 notify pitfall in CONTROLLER_SIMULATORS.md §5) |
| `sim_common.midi` | `MidiParser`, `hexstr`, `describe`, extended by an Erae SysEx decoder for the monitor |
| `SimCommon` QML | `SimWindow` (status bar, F2 monitor), `LedButton` for the 18 panel switches, `Encoder` for the Erae 2 encoder |
| `push_simulator` patterns | the image provider (`display.py`, `QQuickImageProvider`) for the LED framebuffer; `device.py`'s MPE channel rotation; the DBMV socket for later (§4) |

### 2.2 Module structure

```
erae_simulator/
  __init__.py
  __main__.py / main.py   options: --model erae2|eraetouch, --layout FILE|NAME, --suffix,
                          --port-name, --no-plugin-surface, --api-version none|2, --stream-rate HZ
  protocol.py             no Qt: headers, bitize7/unbitize7, checksum, float32 packing,
                          builders and parsers for every message in §1.3
  layout.py               no Qt: the element model (dataclasses) and the built-in layouts, as data
  elements.py             no Qt: touch → MIDI for each element type (notes/MPE, faders, buttons)
  device.py               no Qt: EraeDevice = framebuffer + layout + API/session state +
                          finger tracking; receive(data, port), touch_down/move/up(id, x, y, z)
  qt_bridge.py            EraeSimulator(SimulatorBase): properties, slots, image provider
  qml/Main.qml            SimWindow + TouchSurface + panel switches
  qml/TouchSurface.qml    the 42 × 24 surface (multitouch, pressure)
tests/test_erae.py, test_erae_alsa.py, test_erae_smoke.py
tools/erae_feed.py        a feeder playing the host: version, streaming, zone query, drawings
```

### 2.3 The protocol model (no Qt)

**State:**
- `layout`: a list of elements with `kind`, `x, y, w, h` in cells (bottom origin, as on the
  device) and their parameters (channel, note/base note, CC, MPE on/off, zone index, colour).
- `pixels`: the visible 42 × 24 RGB framebuffer, composed from the native element styles (solid
  colour per element, a touch highlight, the fader fill), the API zone buffers (one per zone, in
  zone coordinates) and the visible session buffer.
- `streaming` and `prefix` (global); `sessions` (id → name, prefix, buffer, lease deadline,
  shown); `visible_session`; `fingers` (simulator finger id → uint64 fid, element, last x/y/z).

**`receive(data, port)` (main port only):**
- identity request → identity reply with a configurable firmware version;
- the API header with the member code for `--model` (the other code is logged as "wrong model":
  the real device is silent, and the monitor should say why);
- `01`/`02`/`10`/`20`/`21`/`22`/`23`/`7F` exactly as in Appendix D, including the silent drops
  (wrong checksum, wrong size, prefix > 16, unknown zone, duplicate zone index: the first wins).
  Each silent drop is logged in the monitor, because that is the main debugging value over
  hardware;
- `30`-`33`/`40`-`43`: the sessions with their lease (a 2 s timer per session, renewed by every
  message), Show with its 1 s "no finger" rule, superseding, and Focus replies. The Erae Touch
  model and `--no-plugin-surface` don't answer `30`, so the host's 500 ms timeout path can be
  tested;
- anything else is recorded in `unknown`, as in the other models (the in-process core test asserts
  it stays empty).

**Touches** (`touch_down/move/up(sim_id, x, y, z)`, x/y in surface cells as floats, z 0..1):
- If a session is visible, every touch goes to it as a finger stream with the session id in the
  zone byte and surface coordinates.
- Otherwise the element under the first contact owns the finger until release, like the
  firmware: elements don't steal moving fingers. **[guess]** the device does the same.
  - **API zone:** if streaming is on, a finger stream message (`act`, zone, fid, zone-local x/y,
    z). `slide` messages are rate-limited with `--stream-rate` (the real rate is unknown; see the
    open questions).
  - **Keygrid / Keyboard / Drumpad / Key:** note on with a velocity from z (a simple curve, or a
    "velocity from the first N ms" option), then pressure (channel or poly AT). With MPE, one
    member channel per finger (rotation as in `push_simulator/device.py`), pitch bend from the x
    offset (range 48), CC74 from y, all on the **MPE port**. Without MPE, everything goes on the
    main port.
  - **Fader 1D / Spring 1D:** CC from y (0..127), optional pressure CC. Spring returns to centre
    on release.
  - **Fader 2D / Spring 2D:** two CCs.
  - **Button:** note or CC, momentary or latched.
  - **LivePad:** not emulated (the messages are private); drawn as a grey area with a label.
- **Finger merge:** two touches closer than 1.5 cells become one finger. Optional, off by default
  in the simulator, because tablets have fat fingers too and the merge would only confuse
  tests.

**Output and listeners:** `send(data, port)` with `port in {"main", "mpe"}`, `monitor(direction,
port, data, note)` and `listeners("pixels", "switches", "status")`, as in the other models.

### 2.4 The QML surface

- **Rendering:** the framebuffer is published as a 42 × 24 `QImage` through an image provider
  (the Push display pattern), with a `revision` in the URL. QML scales it with `smooth: false` and
  draws a thin cell grid on top (one `Canvas`, or a `ShaderEffect` later), plus optional element
  outlines and labels from `sim.elements` (a "show layout" toggle; the hardware has no labels).
  That costs one texture upload per change instead of 1,008 bound `Rectangle`s. **[guess]** both
  are fast enough, but the image also gives the screenshot tests one pixel per cell to check.
- **Input:** one handler over the whole surface. It turns item coordinates into cells and flips
  Y: `cx = x / cellW`, `cy = 24 - y / cellH`.
  - Qt 6 `MultiPointTouchArea` gives `touchPoints` with `pointId`, `x`, `y`, `pressure`,
    `area` and `ellipseDiameters`, and `mouseEnabled` turns the mouse into one touch point. The
    alternative is several `PointHandler`s (one point each). Recommended: **one
    `MultiPointTouchArea`** (`mouseEnabled: false`) for the touchscreen, plus a separate
    `MouseArea`/`WheelHandler` path for mouse emulation. That makes it explicit which pressure
    source is active and allows the right-button and wheel gestures of `Pad.qml`.
  - A tablet stylus arrives through the pointer handlers with a real `pressure` (QTabletEvent
    becomes a QPointerEvent in Qt 6). A `HoverHandler`/`PointHandler` with
    `acceptedDevices: PointerDevice.Stylus` reads `point.pressure`.
- **Pressure emulation**, from the best source available:
  1. a stylus or a touchscreen that reports `pressure` (where the platform delivers it; many
     capacitive panels report a constant 1.0);
  2. contact size (`ellipseDiameters`/`area`) mapped to z: **[guess]** usable on Android and some
     Linux panels;
  3. the mouse, as in `SimCommon/Pad.qml`: the left button presses with z from a default, the
     wheel while held changes z, the right button maps the vertical drag to z (x/y then stay
     where the press started), Shift means z = 1;
  4. a "pressure" slider in the status bar for the default z of plain touches.
  `sim.pressureSource` shows which one is active.
- **Panel:** the Erae 2 has the 18 switches and the encoder around a placeholder LCD box (the LCD
  content is the firmware's and isn't emulated; the box shows the active layout and the session
  state). The Erae Touch has its 5 buttons. N1-N8 switch the simulator's built-in layouts and,
  like the device, take the screen back from a session (Focus off, user, after releasing all
  fingers).

### 2.5 Ports

- Client `Erae 2` (`Erae Touch` with `--model eraetouch`) with two ports: `Erae 2 MIDI` (duplex,
  receives the host's API and clock, sends the non-MPE MIDI and the API replies) and
  `Erae 2 MIDI (MPE)` (output only). The names are as [M2] gives them for Linux. **[guess]** On
  ALSA the client may be named after the USB product (`Erae 2`), and with older kernels the ports
  may be numbered (`Erae 2 MIDI 1`/`2`): offer `--port-style numbered` as in the Push simulator.
  Someone with a device should check with `aconnect -l`.
- `--suffix " (sim)"` if the name is taken, and tests use their own suffix
  (CONTROLLER_SIMULATORS.md §3).
- Bitwig opens raw MIDI only: `--connect "VirMIDI 5-3"` works as for the others. The MPE port then
  needs a second virmidi device (`--connect-mpe`).

### 2.6 Layouts to emulate first

The simulator's layouts are its own data (Python or JSON in `layout.py`). Erae Lab's project
format is unknown, so importing it waits until someone sees one (§5).

1. **`api-full`:** one API zone 0 of 42 × 24. It is the one setup a DAW integration would ask the
   user to make, and the same as the plugin surface minus the session handling.
2. **Plugin surface** (no layout): enabled by default for `erae2`, and in parallel with any
   layout.
3. **`api-split`:** API zone 0 on the left 26 columns (e.g. an 8 × 8 clip grid at 3 × 3 cells,
   the scenes and a control strip), and an MPE Keygrid on the right 16 columns, the
   "DrivenByMoss plus native MPE notes" setup of §3.2.
4. **`native-demo`:** a Drumpad of 4 × 4 pads (2 × 2 cells each), a bank of 8 Fader 1Ds, a Fader
   2D and a few CC buttons, for MIDI-learn tests and the Generic Flexi.
5. **`keyboard`:** a full-width MPE Chroma Keyboard, for Bitwig's MPE input and the Push 3 MPE
   comparison.

### 2.7 Tests (the usual set, CONTROLLER_SIMULATORS.md §6 and §11)

- Unit tests: bitize7 against Appendix D's worked examples (`D.6.4`: `44 7F 00 00 00 7F 00`,
  checksum `44`) and [API]'s 2 × 2 example (checksum `3C`). Exact bytes for every command and
  reply. Silent drops. Session lease and focus timing with an injected clock. The finger stream
  encode and decode round trip (float32). Y flipping.
- An ALSA loopback test (`--suffix " (test)"`).
- An offscreen smoke test with `erae_feed.py` drawing known colours; check the cells in the PNG.
- Later, the in-process core test with the Rust port (§3.4).

## 3. How it relates to DrivenByMoss

### 3.1 Does a port make sense?

Yes, with a twist: the Erae is not a fixed grid with fixed MIDI numbers like every other DBM
controller. Its "controls" are whatever the host draws, so the port needs its own **zone layer**
between the views and the SysEx. The useful parts, roughly in order of value:

1. **Session/clip grid** drawn through the API: 8 × 8 pads at 4 × 3 cells (32 × 24), the scene
   column (4 × 3, columns 32-35) and a 6-column control strip (mode, shift, transport, track
   navigation). Colours are RGB, so track and clip colours can be shown exactly (better than a
   128-colour palette).
2. **Mixer and device views** with API-drawn faders: 8 faders of 4 × ~20 cells that follow the
   DAW value, the exact feedback native Erae faders can't give. Plus mute, solo and arm rows. The
   touch y gives absolute values, and a "pickup/relative" mode avoids jumps.
3. **Drum and sequencer views:** 4 × 4 drum pads (velocity from z) and a 16 or 32-step row. The
   42-column width suits a 32-step sequencer with a 4-row lane per note (the Erae Touch firmware
   does the same).
4. **Note input:** either native MPE keyboard elements (the device does the expression, the notes
   go straight to the DAW's MPE note input, and DBM only needs a second input port), or DBM's play
   view drawing the scale onto the surface and turning the finger stream into notes with
   `send_raw_midi_event` (per-finger channels, pitch bend from x, CC74 from y, pressure from z).
   The first is cheaper and plays better. The second gives DBM's scale and layout handling on the
   whole surface.

**Constraints that shape the port:**
- **No text** on the surface (24 rows, one colour per cell) and no access to the LCD. Mode
  feedback has to be colour and position. **[guess]** A 3 × 5 pixel font can fit short
  numbers/names on a strip, but they are hard to read.
- **No hardware buttons** reach the host. Shift, mode and transport have to be drawn zones.
- **User priority:** a layout switch on the device takes the plugin surface away. DBM gets
  Focus off, and the firmware releases the fingers. The port needs a way back. Options: a small
  API zone in every user layout ("tap to return to DBM": touching it makes DBM send Show), or a
  DAW-side action.
- **Velocity:** the finger stream has no velocity. It has to come from z at click time, or from
  the maximum z in the first few ms (delay the note-on slightly). This needs tuning on hardware.
- **Flood:** every finger movement is a SysEx of about 30 bytes, at an unknown but probably high
  rate (1.6 kHz scan, 10 fingers). [KVR] shows that this hurts Bitwig scripts. In our stack, the
  ALSA or Bitwig path into the core and the flush-per-message behaviour of the hosts must cope.
  Mitigation: drop slide messages for zones that only need press and release (buttons, pads),
  coalesce slides per finger per tick, flush at most once per tick.

### 3.2 What it would look like in `src/controller/erae/`

| File | Content |
| --- | --- |
| `definition.rs` | Erae 2 and Erae Touch (model codes `00 02`/`00 01`). Discovery pairs: `Erae 2 MIDI` in/out plus `Erae 2 MIDI (MPE)` in, and the Windows names. `num_midi_in_ports: 2` (as `launchkey/definition.rs`) |
| `protocol.rs` | header, bitize7/unbitize7, checksum, the command builders, the finger stream and reply parsers. Pure functions with unit tests against Appendix D's examples |
| `canvas.rs` | the 42 × 24 RGB framebuffer (size from the Opened or boundary reply, **not hard-coded**: see §4), with dirty-rectangle tracking. The flush emits `DrawRectangle` for uniform rects and `DrawImage` chunks for the rest, paced per tick |
| `zones.rs` | the zone map: named rectangles → control ids (button, pad of a grid, fader, strip). Hit-testing a finger to a zone, finger ownership until release, press, release and continuous value |
| `surface.rs` | the pad grid as a zone block. Its `light(note, colour)` paints the block's cells. Blinking is emulated with the scheduler (`BlinkMode::Emulated`-like, but painting cells instead of sending notes) |
| `setup.rs` | startup: identity request, VersionRequest, plugin surface Open (fallback after 500 ms: StartFingerDataStreaming and ZoneBoundaryRequest for zone 0). A heartbeat every 500 ms via `ctx.schedule`. The Focus handling. The second input (the MPE port) as a note input with expressive MIDI (filters like the Push 3's `midi_filters_mpe`). Exit: Close, EndFingerDataStreaming |
| `colors.rs` | colour id → RGB (7-bit for rects, 8-bit for images), the DAW colours passed through as RGB |
| `view/` | thin adapters that give the framework views (session, mixer via `VirtualFader`, drum, play) Erae geometry |

**Framework parts that already exist** (from FRAMEWORK_MAP and the source): `PadGrid` with
translation and blink modes (`src/framework/grid.rs`), the session, play, drum64 and sequencer
views (`src/framework/view/`), the Launchpad's fader, mix and drum views and `VirtualFader`
(`src/framework/virtual_fader.rs`), scales (`src/scale/`), `MpeStatus` (`src/scale/mpe_status.rs`),
the Push 3 MPE note input filters and pitch bend range handling (`src/controller/push/setup.rs`),
`send_raw_midi_event` for injected notes (`src/midi.rs`, used by `play_view.rs`), `send_sysex` and
`ctx.sysex_handler` (Launchpad), `ctx.schedule` for the heartbeat and blinking, the 2-input setup
(Launchkey), the raster and bitmap graphics (`src/framework/graphics/`, if the canvas wants to
reuse a drawing API), and multi-surface (`doc/MULTI_SURFACE_PLAN.md`) for an Erae next to a Push.

**Missing:**
- The canvas and zone layer (new, Erae-specific, but the tablet would reuse it).
- Continuous touch values as control input. The framework binds CCs and notes; a fader zone has
  to feed `VirtualFader`/the parameter directly or synthesize "CC" events for a bound continuous
  control. The second is less code but loses resolution: y is a float, and 14 bits would be
  natural.
- Finger stream → notes with expression (a small "MPE generator": channel rotation, bend range,
  CC74, pressure) for the play view on the API surface. `MpeStatus` only tracks incoming MPE
  today.
- Lifecycle state (session id, focus, lease) and a "come back" path.
- The incoming SysEx rate: a fast path that parses finger streams without allocations and keeps
  them out of the per-message flush (a host or bridge check, see the risks).

**Rough effort** **[guess]**: protocol and canvas with tests, small (about a day). Zones, the grid
surface and the session view: medium (a few days). Mixer, device and drum views with good touch
behaviour: medium. MPE generation from the finger stream: medium. Tuning velocity and flood
handling on hardware: open-ended. With the simulator in place, everything except the last item
can be developed and tested without the device.

### 3.3 Developing it in parallel with the simulator

The protocol is specified, so the two sides can be built against the spec independently and then
checked against each other:

1. The simulator's `protocol.py` and the port's `protocol.rs` both test against Appendix D's byte
   examples, which ties them to the same ground truth without hardware.
2. **The in-process core test** (`tests/test_midi_simulators.py`, `CoreTest` pattern): a
   `dbm_host` `Bridge` with the simulated DAW, the core's MIDI fed into `EraeDevice.receive`, the
   device output fed back. Assertions:
   - `device.unknown` is empty and no silent drop happened (bad checksum, out of range,
     oversized);
   - the core opened a session or started streaming, and sent heartbeats within the lease;
   - after the DAW state is set (clips with colours), the expected cells have the expected RGB;
   - a scripted touch on a clip cell launches the clip in the simulated DAW;
   - a slide on a fader zone changes the volume;
   - Focus off is followed by no further draws until Show;
   - the core logs no error.
3. The interactive loop: `run.sh --model erae2 --monitor` plus `dbm_host_alsa.py --model erae2`,
   and then Bitwig through virmidi (two devices: main and MPE).
4. Hardware, once available: run the same scripted sequences, compare the monitor logs, and
   measure the stream rate, the velocity behaviour and the timing.

## 4. Towards the tablet controller

The Erae's model is close to what a tablet controller needs, and closer than the fixed-layout
simulators:

- **Zones over a pixel surface:** the host owns the layout (rectangles of cells), draws the
  feedback, and receives raw touches with id, x, y and z. Nothing in DBM has to know the form
  factor beyond "a W × H canvas". The Opened reply already carries the width and height, so a
  tablet can report e.g. 84 × 48 or 64 × 40 cells, and a port that reads the size works
  unchanged. The protocol allows up to 127 × 127 per zone or session (7-bit coordinates).
- **The same wire protocol on another transport:** the tablet app speaks the Erae API (the
  plugin surface subset) over a network socket instead of USB MIDI. The DBM side gets a transport
  abstraction (MIDI port or TCP/WebSocket; PySide6 includes QtWebSockets and Rust has
  `std::net`), and one controller implementation serves the Erae, the simulator and the tablet.
  The 7-bit framing and the 8 KB limit don't matter on a socket, but keeping them makes the
  simulator and the tablet the same code.
- **What the Erae lacks and the tablet should add,** as extensions in a private sub-service
  range (or as new DBMV kinds, HOST_BRIDGE_GUIDE §5a, which already has a 7-bit-safe framing):
  - labels and text per zone (track names, parameter names and values), which the DBMV value
    channel already carries for the Push encoders;
  - zone declarations (kind, value range, "send presses only" vs "continuous"), so the client can
    do hit-testing, fader behaviour and slide filtering locally. That cuts the stream traffic and
    makes touch latency independent of the network;
  - finer rendering (rounded pads, value arcs) driven by the declared zone state instead of
    pixels.
- **The simulator becomes the tablet's first client:** the QML `TouchSurface` with its pressure
  handling and the no-Qt `EraeDevice` model are exactly the parts a tablet build needs. Keep the
  model free of Qt and of ALSA (CONTROLLER_SIMULATORS.md §1), and put the transport behind a
  `send/receive` pair, so the same model runs on ALSA (simulator), on a socket (tablet) or
  in-process (tests).
- **Pressure on tablets** is the weak point: few capacitive screens report force, while a stylus
  does. The Erae's own 0..1 z plus our emulation ladder (§2.4) gives one place to plug in
  whatever a device offers.

## 5. Phases

Each phase ends with a verifiable result.

1. **Protocol core, no UI:** `erae_simulator/protocol.py` (bitize7, the checksum, all builders
   and parsers) with tests against Appendix D's examples. Done when the D.6 and D.9.6 examples
   round-trip byte for byte.
2. **Device model:** `device.py` with API zones, streaming, version and boundary replies, the
   draw commands with the silent drops, sessions with lease and focus (injected clock), and
   touch → finger stream. The `api-full` layout. Unit tests. Done when a scripted host session
   (open, show, draw, touch, focus off) gives the expected bytes and framebuffer.
3. **Native elements:** Keygrid, Keyboard, Drumpad, Fader 1D/2D, Button, with MPE on the MPE
   port. Layouts 3-5. Done when `aseqdump` shows correct MPE (channel rotation, bend, CC74, AT)
   and the CCs.
4. **The Qt app:** the ports, the image provider, `TouchSurface` with mouse emulation, the panel,
   `run.sh --model erae2`, the ALSA loopback and smoke tests with `erae_feed.py`, and the README
   protocol table. Done when the screenshot shows the fed pixels and a mouse drag produces a
   finger stream in the monitor.
5. **Multitouch and pressure:** `MultiPointTouchArea`, stylus pressure, the contact-size
   fallback. Tested on a touchscreen or tablet (the user's hardware). Done when 3+ simultaneous
   fingers stream with independent ids and z.
6. **DBM port, step 1:** `src/controller/erae/` with the definition, protocol, canvas and
   lifecycle, plus a session view on an 8 × 8 grid. The in-process core test (§3.3). Done when the
   core test passes and the clip grid shows the simulated DAW's clip colours.
7. **DBM port, step 2:** mixer and device faders, drum and sequencer, the play view (native MPE
   input first, then the finger-stream MPE generator). Each with core-test assertions.
8. **Hardware validation** (needs a device): port names on ALSA, identity bytes, stream rate,
   velocity tuning, the timing of large images, and plugin surface availability on the release
   firmware.
9. **Tablet transport:** the socket transport, a size other than 42 × 24, and the label/zone
   extensions, in its own plan.

### Open questions for the user

1. **Hardware:** do you have an Erae 2 (or an Erae Touch) to validate with? Without one, phases
   1-7 are spec-driven and phase 8 waits. Which firmware does it run (shown on the LCD at
   startup)?
2. **Plugin surface vs API zone:** the plugin surface (D.9) is the natural fit (no user layout,
   full screen), but it is documented in the beta manual. Should the port require it, or also
   support the "full-screen API zone 0" fallback (works with 2.x release firmware and the Erae
   Touch)? Recommendation: support both. It is one byte of difference in the drawing commands.
3. **Note input:** native MPE elements (the device plays, DBM only listens on the MPE port), or
   DBM drawing the play view and generating MPE from the finger stream, or both?
4. **Scope of the first port:** session grid first (the [KVR] use case), or mixer/faders, or note
   input?
5. **Erae Touch:** in scope for the simulator (cheap: other model code, 5 buttons, no plugin
   surface) and for the port?
6. **Erae Lab files:** if you install Erae Lab on Linux, could you export a project with an API
   zone, so we can see the format (in `~/.local/share/Embodme/EraeLab/projects`)? Then the
   simulator could load real layouts, and the port could ship a ready-made project for the "API
   zone" mode.
7. **Tablet:** which tablet or OS (Android or iPad via a browser, Linux tablet with Qt)? That
   decides whether the QML surface runs on the tablet itself or the tablet is a web client, and
   which pressure sources exist.

### Risks

- **The plugin surface is beta:** its commands or timing may change before or after release.
  Mitigation: the API zone fallback, the 500 ms detection, and tolerant parsing ("ignore codes
  you don't know and bytes after the fields you expect", D.9.5).
- **Firmware differences:** VersionRequest isn't answered by the Erae Touch or early Erae 2
  firmware. The API port changed (a third MIDI port on old firmware, the main port now). The
  member code differs between the models ([FD], [M2] D). The port must not depend on the version
  reply.
- **Undocumented parts:** the stream rate, the velocity at click, the LivePad messages, the Lab
  file format, and how the device behaves with incoming MIDI LED feedback on native elements
  (the manual says the main port receives "LED feedback" but describes none for elements). The
  simulator encodes our best reading and marks these places, so hardware results can correct them
  in one spot.
- **Spec inconsistencies:** [API] has copy-paste errors and says the prefix must be ≥ 1 byte,
  where D says 0..16. Use a non-empty prefix (`7D` plus a tag) to be safe with both.
- **SysEx throughput:** USB is fast, but host stacks (ALSA, virmidi, Bitwig) may split or delay
  big SysEx, and the device's buffer handling is unknown. Pace the redraws (dirty rects,
  ≤ 32-pixel images per [API]) and measure.
- **Finger-stream flood into the host:** [KVR]. Keep slide messages for press-only zones out of
  the DAW path; coalesce in the core.
- **Linux MIDI 2.0 mode** corrupts the API SysEx (D.1). Document "keep MIDI 2.0 off" in the
  port's README.
- **Low Power mode** (USB bus power) dims the LEDs. That matters for testing on hardware, not for
  the protocol.
