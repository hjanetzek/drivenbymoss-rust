# Electra One simulator: research and plan

Status: 2026-11-20, second version. Research and planning only, no simulator code yet. This
document covers the device, what DrivenByMoss uses of it, four ways to simulate it, a
recommendation, and phases. The Electra One is a touchscreen controller driven by presets and
Lua, which makes it the closest relative of the planned tablet controller
(doc/CONTROLLER_SIMULATORS.md). Section 6 looks at what the tablet should take from it.

What changed since the first version:

- **The license is no factor.** The project is GPL-3.0, so firmware code may be used and
  copied; only the notices of copied code are recorded (§3 (b)).
- **A host shim exists:** the MicroDexed-touch port's Linux build, which runs a Teensy sketch on
  the PC.

With both, the recommendation moves from the native re-implementation (a) to the real firmware
on the host, (b) with our QML front end (d), and (a) later for the tablet (§3).

Sources:

- **The firmware**: `~/bastel/github/firmware-mk1`, i.e. github.com/electraone/firmware-mk1,
  GPL-3.0, at commit 93ad374 (2026-10-30). Martin Pavlas wrote 741 of its 757 commits. The firmware
  was revived in 2026 to match the mk2 (it reports v5.0.6, `FIRMWARE_SEQ 500000600`). Its
  documents are `ARCHITECTURE.md`, `SYSEX-API.md`, `LUA-API.md`, `COMMANDS.md` and `POTS.md`;
  `tools/compat/` has a harness and sample `.epr` presets.
- **The vendor documentation**: <https://docs.electra.one/developers/presetformat.html> (preset
  format), <https://docs.electra.one/developers/midiimplementation.html> (SysEx),
  <https://docs.electra.one/developers/luaext.html> (Lua),
  <https://docs.electra.one/userguide-mk2/overview.html> and
  <https://docs.electra.one/userguide-mini/overview.html> (hardware),
  <https://electra.one/products/electra-one-mini/>.
- **Other repositories**: GitHub lists only two public ones under electraone, `firmware-mk1` and
  `electraone-cli` (a C++ API library and command line client, GPL-3.0). The firmware README
  points at `electraone/demos` for custom apps, but that repository is no longer public. The
  mk2 firmware ("firmware-mk2", named in `apps/link/README.md`) is not public either.
- **DrivenByMoss Java**: `src/main/java/de/mossgrabers/controller/electra/one/` (5,441 lines:
  `base/`, `normal/` for the mk1/mk2, `mini/`) and `resources/Electra.One/DrivenByMoss.eproj`
  and `DrivenByMoss Mini.eproj`.
- **The MicroDexed-touch port's Linux build**: `~/bastel/cynthian/port` (codeberg
  hjanetzek/cynthian, at 11dc641). Read: `README.md` ("Linux desktop build"),
  `LINUX_BUILD_PLAN.md`, `host/` (`CMakeLists.txt`, `shim/`, `core/`, `linux/`),
  `addon/arduino` (the Arduino/Teensy API and the Teensy 4 core subset), `app/mdt/ili9341emu.*`
  and `nativefont.*`, `hal/zynthian_v4`, `tools/mdtctl.py`, `tools/uiwalk.py`, `tests/host`.

## 1. The device

### 1.1 Hardware

| | mk1 | mk2 | Mini |
| --- | --- | --- | --- |
| MCU | NXP MK66FX1M0 (Teensy 3.6 class, Cortex-M4F at 180 MHz, 256 KB SRAM, 1 MB flash) | not public (newer, faster MCU; the firmware is not open) | not public |
| Display | 1024×600, RA8876 controller over SPI, with 16 MB SDRAM that also holds fonts, strings and SysEx buffers | 1024×600 | 800×480, **no touchscreen** |
| Screen touch | capacitive, FT5x16 over I²C, polled every 40 ms | yes | none |
| Knobs | 12 endless "pots" (dual-gang potentiometers read as an absolute angle, `POTS.md`), each touch-sensitive (BU21072) | 12, touch-sensitive | 8, touch-sensitive and **pushable** |
| Buttons | 6 (left/right × top/middle/bottom) | 6 (same idiom) | 4 pads/buttons (pots 9–12 in Lua), 6 hardware buttons on the product page |
| USB device | 3 MIDI cables: **Port 1, Port 2, CTRL** | the same | the same |
| Other MIDI | USB host (2 devices), 2 DIN in/out pairs | the same | (not checked) |
| Storage | SD card (`ctrlv2/pNNN.epr`, Lua scripts, snapshots, config) | internal | internal |

The ALSA sequencer names are `Electra Controller Electra Port 1`, `… Electra Port 2` and
`… Electra CTRL` (`tools/compat/electra_midi.py`). The vendor's USB IDs are 0x1FC9:0x82CF
(`bsp/usb/usb_desc.h`).

### 1.2 What the screen shows

A preset has up to 12 pages; the Mini allows 16. A page is normally a 6×6 grid of 36 controls in
three **control sets** (sections) of 12, one set for each two rows. Only the active set is bound
to the 12 knobs, and the three left buttons select it. Groups are labelled frames above the
controls and never send MIDI. There is a status bar at the top and a bottom bar with the preset
name, page name and a 20–40 character info text (`14 77`, `info.setText`). Popup windows sit on
top of the page: the detail view (one control large, with a knob per value), preset and page
selection, snapshots and the USB host list. The standard slot geometry is 146×56 controls with
167 px column pitch and 90 px row pitch, starting at x = 20, y = 28. Group labels sit 22 px above
the controls (`src/Utils/helpers.cpp:controlSlotToBounds`, `helpers.slotToBounds` in Lua).

### 1.3 The preset model (`.epr`, JSON, version 2 or 3)

From the vendor's preset format page and `apps/controller/src/Model/`:

- **Top level:** `version`, `name` (up to 20 characters), `projectId`, `pages`, `devices`,
  `overlays`, `groups` and `controls`. Groups and controls share one id space.
- **pages:** `id` (1–12), `name`, `defaultControlSetId`, `hidden`.
- **devices** (up to 32): `id`, `name`, `port` (1–2), `channel` (1–16), `interfaces`, `rate`,
  `patch` (request/response rules), `messages` (SysEx templates).
- **overlays:** `id`, `items` of `{value, label, bitmap?}`. These are the text and icon lists of
  list controls; bitmaps are 48×18 XBM.
- **groups:** `id`, `pageId`, `name`, `bounds`, `color`, `variant`
  (`default`/`highlighted`/`buttonlike`), `visible`.
- **controls:** `id`, `type`, `name`, `color`, `bounds [x,y,w,h]`, `pageId`, `controlSetId`
  (1–3), `visible`, `variant`, `mode`, `inputs [{potId 1–12, valueId}]`, `values [...]`, and
  `events` (switch/touch → message, Lua or command).
  - Types: `fader`, `vfader`, `knob`, `list`, `pad`, `adsr`, `ahdsr`, `adssr`, `adr`, `ar`,
    `dx7envelope`, `xypad`, `relative`, `textBox`, `custom`, `macro`.
  - Value ids: `value`; `attack`/`decay`/`sustain`/`release`/`hold`/`break`/`slope` for the
    envelopes; `f1`–`f4` (vfader); `x`, `y` (xypad); `l1`…`r4` (dx7).
- **value:** `id`, `min`, `max` (the display range), `defaultValue` (`"on"`/`"off"` for pads),
  `overlayId`, `formatter` and `function` (Lua function names), `sensitivity`, `message`.
- **message:** `deviceId`, `type` (`cc7`, `cc14`, `nrpn`, `rpn`, `sysex`, `note`, `program`,
  `start`, `stop`, `tune`, `atpoly`, `atchannel`, `pitchbend`, `spp`, `relcc`, `virtual`,
  `none`), `parameterNumber`, `min`, `max` (the MIDI range), `onValue`/`offValue`, `lsbFirst`,
  `signMode`, `bitWidth`, `relative` + `relativeMode` (`signBit`, `signBit2`, `binOffset`,
  `twosComplement`), `accelerated`, `data` (SysEx template), `direction`.

**Value binding.** The hub is the **parameter map**: `(device, message type, parameter number)`
maps to one stored MIDI value plus the chain of control values that show it
(`ARCHITECTURE.md` §4.1). A knob turn or a touch changes the value. The map stores it, the
outgoing message is built from the device's port and channel and the message template, the
Lua `parameterMap.onChange` runs, and the entry is marked dirty. Incoming MIDI looks up the
device by port and channel, sets the entry and marks it dirty. A 25 ms task repaints dirty
entries, calling the value's Lua formatter if it has one. The display value is the MIDI value
translated between the message's `min..max` and the value's `min..max`, with the sign mode
applied.

**The editor's project format is different.** The Electra web editor (app.electra.one) stores a
project (`.eproj`: `schemaVersion` 2 or 3, `tiles`, `layouts`, `lua`) and sends the device a
`.epr` it generates from that. The firmware reads only `.epr`. DrivenByMoss ships only
`.eproj` files, so a simulator gets the DBM preset either from the editor's upload or from a
converter (§2.3, §3 (b) Fidelity).

### 1.4 Lua

There is one Lua 5.4 state per loaded preset, on the main loop (`ARCHITECTURE.md` §7,
`LUA-API.md`, the vendor's luaext page). The modules:

- `midi` (send functions and `on*` callbacks), `transport`, `timer`, `controller`, `logger`,
  `bytes`, `window`, and `graphics` (custom control paint).
- `parameterMap` (get/set/updateValue/map/onChange), `preset` (onLoad/onReady/onEnter/onLeave/
  onExit, userFunctions), `patch`, `pages`, `controls`, `groups`, `devices`, `overlays`,
  `events`, `commands`, `info`, `schedule` and `helpers`.
- The objects are Control, ControlValue, Message, Page, Group, Device, Overlay and SysexBlock.

A value's formatter, function and SysEx template byte functions are named global functions.
The mk1 firmware now matches the mk2 Lua API by name. Only data pipes and captures are missing.

### 1.5 The SysEx API (CTRL cable)

Every message is `F0 00 21 45 <command> <object> … F7`. An optional transaction id
`00 lsb msb` may follow the manufacturer id and is echoed in the ACK or NACK. Every request is
answered: data queries with data, the rest with `7E 01` ACK or `7E 00` NACK, and an unknown one
with NACK. Replies go out on the cable the request came in on. The full table is in
`SYSEX-API.md`. The groups:

- **Information:** `02 7F` → reply `01 7F {versionText, versionSeq, serial, hwRevision[, model, modelNum]}`.
  The mk1 omits `model`; the mk2 says `"mk2"` and the Mini `"mini"`.
- **Presets:** `01 01` upload, `02 01` fetch, `02 04` → `01 04 {version, current, presets:[{bankNumber, slot, name, projectId, hasLua, …}]}`,
  `09 08 bank slot` switch, `14 08` select, `08 08` reload.
- **Lua:** `01 0C` upload a script, `08 0D` run a line of Lua.
- **Live control:** `14 07 lsb msb {json}` (name, color, visible, value), `14 0E lsb msb valueIdx text`
  (value text override), `09 0A page` switch page, `09 0B set` switch control set,
  `14 77` bottom bar text, `08 13` run a command (`COMMANDS.md`: 35 named commands).
- **Events:** `14 79 flags` subscribes (01 pages, 02 control sets, 04 USB host, 08 pots; 10, 20
  and 40 are reserved). `14 7B port` sets the port events go to.
- **System:** `7F 7A 0|1` stops/resumes repaints, `7F 7D on level` sets the logger, `14 7D port`
  its port, `7F 76` takes a screenshot, `7F 78` reboots.
- **Configuration, snapshots, files, staged upload:** `01 02`/`02 02`, `02 05`/`01 03`/…,
  `02 34`/`02 35`/…, `01 2D`/`01 2E`/`01 2F`/`04 2D`.
- **What the controller sends by itself:** `7E 02 bank slot` preset switched, `7E 06 page`
  page switched, `7E 07 set` control set switched, `7E 0A potId ctlLsb ctlMsb touched` pot
  touch, `7E 05` preset list changed, `7F 00 text` a log line (with the `lua: ` prefix for
  `print`).

## 2. What DrivenByMoss uses

### 2.1 Ports and discovery

There are two port pairs (`ElectraOneControllerDefinition`, `…ControllerSetup.createSurface`):

- **Port 1** (MIDI index 0): the controls. All of them are CCs on **channel 16**. The input also
  creates note inputs for channels 1–15, so a preset can play notes into Bitwig.
- **CTRL** (MIDI index 1): SysEx only.

The Linux discovery names are `Electra Controller Electra Port` (a substring of `… Port 1`) and
`Electra Controller Electra CTRL`. Port 2 is unused.

### 2.2 The conversation

1. `startup` → `02 7F` (device info).
2. The info reply `01 7F`: the mk1/mk2 controller requires `versionSeq ≥ 300400300` and
   `model` absent, `mk1` or `mk2`; the Mini requires `≥ 500000400` and `mini`. Then
   `14 79 39` (subscribe pages, pots, and the reserved touch/button flags) and `02 04`.
3. The preset list `01 04`: DBM finds the preset named **`DrivenByMoss`** (or
   `DrivenByMoss Mini`) and remembers every other preset by name, for "a preset named after
   the cursor device". If the preset is missing, it throws. Then `09 08 bank slot`.
4. `7E 02 bank slot` → online if it is the DBM slot (mode Volume, `09 0A page`), otherwise
   offline (mode Dummy; button LEDs suppressed).
5. While online:
   - **Values:** a CC on channel 16 to Port 1, CC `10 + 10·row + col` (rows 0–5, columns 0–5),
     the value 0–127 (`updateValue`).
   - **Control name, color, visibility:** `14 07 id {"name":…,"color":"RRGGBB","visible":…}`.
     The control id is `page·36 + 6·row + col + 1` (`PageCache.getControlID`).
   - **Value text:** `14 0E id 00 text`.
   - **Group labels:** Lua `08 0D sgt(501,"1: Track")`. `sgt` is defined in the preset's own Lua
     (`groups.get(id):setLabel(t)`, and `setVisible(#t > 0)` when that changes). The group ids
     are 501.. per page.
   - Every flush is wrapped in `7F 7A 00` … `7F 7A 01`.
   - The logger: `7F 7D on 00`; DBM prints `7F 00` lines.
6. **From the device:**
   - Knobs: relative CC, two's complement, channel 16, CC `10 + 10·row + col`
     (`TwosComplementValueChanger(128, 1)`, `addRelativeKnob`).
   - Pads: CC `70 + 10·row + col` with 127/0, momentary (`ButtonRowModeCommand`).
   - `7E 06 page` → mode by page index: Volume, Send, Device, EQ, Transport, Session, Project.
   - `7E 0A pot ctl touched` → knob touch for the control and the **touch-combination
     commands**: 38 patterns of three touched knobs map to configurable actions, Shift among
     them.

DBM sends no `.epr`, no Lua upload, no preset upload, no snapshots or files. The user installs
the preset with the editor.

### 2.3 The preset DBM expects

`DrivenByMoss.eproj` (schemaVersion 2) has 7 pages (Mixer, Sends, Devices, Equalizer,
Transport, Session, Project/Track) and 307 tiles: 92 faders, 145 pads, 55 labels (groups),
9 relative controls and 6 lists (the EQ types, with overlay bitmaps). Two devices: id 1
(port 1, ch 1) and id 16 (port 1, ch 16). Its `lua` is only `sgt`. Notes for the conversion:

- A tile's `reference` is the control id (1–252) or group id (501…).
- Its `slotId` encodes page and position: 72 slots a page (12 half rows × 6). Even half rows
  hold group labels, odd ones controls.
- Many tiles carry no `bounds` and no `pageId`; the editor derives them from the slot.
- `type:"relative"` messages with `relativeMode:"twosComplement"` and `accelerated:true` become
  `relative:true` messages in the `.epr`.
- Values have display ranges (e.g. 0–100 over MIDI 0–127).

`DrivenByMoss Mini.eproj` is schemaVersion 3, with `targetDevice:"mini"` and `layouts`
(`mini-4x2`). The Mini controller uses CC 10–17 for the knobs, 20–27 for the knob pushes, 30–33
for the buttons and 40 for knob touch, and has 3 modes (Volume, Device, Transport).

### 2.4 The minimum a simulator must implement for DBM

| Area | Minimum |
| --- | --- |
| Ports | ALSA client `Electra Controller` with `Electra Port 1`, `Electra Port 2` and `Electra CTRL`, duplex |
| Info | `02 7F` → `01 7F` JSON, selectable model (`mk1`: no `model` key; `mk2`; `mini`) and version |
| Preset list | `02 04` → `01 04` JSON with the loaded slots (at least the DBM preset) |
| Switching | `09 08` → ACK + `7E 02 bank slot`; `09 0A` → ACK + `7E 06 page`, and the page is shown |
| Subscriptions | `14 79` stored, events only for the subscribed classes |
| Live updates | `14 07` (name, color, visible; `value` optional), `14 0E` (value text), `7F 7A` (repaint batching), `7F 7D` (logger on/off) |
| Lua | `08 0D` with at least `sgt(id,"text")` executed against the preset's `sgt` (the real Lua in (b), a shim in (a)) |
| Parameter map | an incoming CC on (port 1, ch 16) sets every value bound to it (cc7 and relative messages); pad on/off |
| Controls | fader (incl. relative message), relative, pad (momentary/toggle, color from `14 07`), list with overlay, group label (variant highlighted, visibility) |
| Input | 12 knobs bound by the active control set, sending relative two's-complement CCs (accelerated); knob touch → `7E 0A` (0-based pot id, control id); screen touch on a pad → `onValue`/`offValue`; 6 buttons (control set 1–3, page selection) |
| Preset loading | `.epr` files in slots; the shipped DBM `.eproj` installed by the editor's upload, or by a converter |
| Robustness | ACK/NACK with the echoed transaction id; NACK for everything unknown, logged as "unhandled" in the monitor; long SysEx reassembled from ALSA fragments |

A Rust port of the controller needs exactly the same subset. In (b) all of it comes with the
firmware. Only the ports, the input and the card are host work.

## 3. Simulator approaches

Marks used in this section: **verified** = read in the code or measured here; **assumed** = an
estimate or a plausible guess still to be checked.

### (a) A native re-implementation in our PySide6/QML stack

A pure Python model (`electra_simulator/device.py`, no Qt) with:

- a preset parser (`.epr`, plus an `.eproj` converter or the editor's own upload);
- the parameter map with the MIDI ↔ display translation;
- the SysEx API subset of §2.4 (later: preset/Lua upload, `08 13` commands, `14 77`, `09 0B`,
  events for control sets);
- the slot store (a directory of files, 6 banks × 12).

QML renders the controls natively: a `PageView` of `Repeater`s over the page's groups and
controls, with one component each for fader, pad, list, relative and group, the bars, 12
`Encoder`s with touch and 6 `LedButton`s from `SimCommon`.

- **Effort (assumed):** the DBM subset takes roughly 1,500–2,500 lines of Python + QML and fits
  the `midi_simulator` pattern (listeners, monitor, layout test, smoke screenshot). Envelopes,
  XY pad, vfader, knob, detail view and snapshots come later, as needed.
- **Fidelity:** protocol behaviour as specified, with pixels "in the style of" the device, not
  identical. Lua (below) is the largest gap. Since GPL-3.0 code may be used, (a) can translate
  firmware logic and tables instead of inferring them: the slot geometry, `ControlMetrics.h`,
  the sign and relative modes, the preset parser's defaults. That narrows the gap, but each
  of them is then a copy that has to follow upstream by hand.
- **Lua, three options:**
  1. **None, plus a shim (stdlib only).** `08 0D` text is matched against `name(arg, …)` calls
     with literal arguments. A function the simulator knows (DBM's `sgt`: set the group label,
     visibility from the text length) runs natively; anything else is logged as unhandled Lua.
     That covers DBM completely, and 50 lines do it.
  2. **`lupa`** (Python binding to Lua 5.4/LuaJIT, MIT). This gives real Lua with the API
     modules in Python, but it is a new dependency, against the project's no-deps rule, and needs
     the owner's decision.
  3. **Lua through our Rust library.** `mlua` with Lua 5.4 is already the one approved
     dependency (doc/LUA_PLAN.md). A small C ABI ("a sandboxed Lua state with host callbacks")
     in `libdrivenbymoss` would give the simulator, and later the tablet controller, real Lua
     with no new dependency.
  - In every option the ~25 Lua modules of §1.4 have to be written again; only the language
    comes for free.

### (b) Running the real firmware on the host

Compile `src/`, `apps/controller` and the portable libraries (`lib/lua`, `ArduinoJson`,
`base64`, `CircularBuffer`, `TaskScheduler`, `arduino_midi_library`) natively, against a host
BSP that replaces `bsp/` (the firmware's copy of the Teensy 3 core with Electra's changes) and
the hardware libraries (`RA8876`, `FT5x16`, `BU21072`, `ADC`, `SdFat`, `USBHost_t36`, `i2c_t3`).

#### What the firmware needs from a host (verified)

- **A narrow boundary.** Only 14 files in `src/` and `apps/controller` include hardware
  headers. Only 11 lines touch registers directly: `__disable_irq`/`__enable_irq` in
  `TaskReadMidi.cpp`, `TaskProcessSysex.cpp` and `SysexHandlerCtrl.h`, and the DWT cycle counter
  in `Profile.h`. There are 76k lines of application code and 68k lines of libraries.
- **A clean entry point.** `bsp/main.cpp` calls `initialise()` once and then `processEvents()`
  forever (`src/main.cpp`). There is no RTOS. The only timers are two `IntervalTimer`s:
  `readMidi` every 500 µs, switched to `readCtrlMidi` every 6 µs during a CTRL upload, and the
  boot spinner (`SystemTasks.cpp`).
- **USB MIDI device:** `bsp/usb/usb_midi.c` with 3 cables and Electra's partial-SysEx
  callback with a cable argument. The firmware uses `usbMIDI.*`, `usb_midi_write_packed`,
  `usb_midi_read_sysex` and the `usb_midi_handle*` hooks.
- **Storage:** SdFat v2 (`SdFs`, `FsFile`, `openNext`, `SdioConfig`), behind
  `PersistentStorage`, `LocalFile` and `FileIoStream`.
- **The display:**
  - `Graphics`/`FrameBuffer` derive from the `RA8876` class. That class has about 125
    methods, each writing the chip's registers over SPI.
  - The firmware calls about 55 of them: mostly `fillRect` (71 call sites), `fillAll`,
    `drawLine`, round rectangles, triangles, circles, ellipses and curves; also
    `setCanvasAddress`, `bteCopyNoWait`, `bteCopyChroma`, `bteCopyRop`, `bteExpansion`,
    `setActiveWindow*`, `setMainWindowAddress` and `writeRamData`.
  - Double buffering swaps two frame buffers in the 16 MB SDRAM (canvas vs. main window
    address, `FrameBuffer.cpp`).
  - Text is drawn by BTE: chroma-key copies from a glyph atlas in SDRAM, and colour
    expansion of masks (`TextAtlas.cpp`). Strings and bitmaps also live in SDRAM
    (`VramStore`, `BitmapPool`).
- **Lua:** the firmware's own Lua 5.4 (`lib/lua`, `LUA_VERSION_NUM 504`). Its file access goes
  through the firmware's own SD layer (`luaBridge.cpp`), and the rest is plain C.
- **A syntax survey** (host `g++ -std=gnu++17 -fsyntax-only` over all 194 `.cpp` files of `src/`
  and `apps/controller/src`, with the Makefile's defines, against the unchanged `bsp/`):
  - 29 compile.
  - Almost all failures come from the Teensy core headers: `DMAChannel.h` and the register
    macros cast pointers to `uint32_t`, and `compat.h` redeclares `exp10`. These go away with a
    host BSP.
  - What remains is a short list in the application:
    - `static_assert(sizeof(Span<int>) == 5)` (a packed pointer + count, 41 files) and
      `sizeof(Control) == 32` fail on 64-bit;
    - SdFat's `File32`/`FsFile` selection;
    - `#include <String>` and `#include "String.h"` in `bsp/compat.h`, which only resolve on a
      case-insensitive file system. The firmware is developed on macOS, and `src/Lua/` has both
      `luabridge.cpp` and `luaBridge.cpp`;
    - a missing `<algorithm>` (`std::find`);
    - `random()` clashing with glibc;
    - `Print::write(int)` ambiguous (`Database.h`).

#### The shim of the MicroDexed-touch port (verified)

`~/bastel/cynthian/port` runs MicroDexed-touch, a Teensy 4 Arduino sketch, on Linux
(`README.md` "Linux desktop build", `LINUX_BUILD_PLAN.md`). It is layered:

1. the sketch;
2. `addon/arduino`: the Arduino/Teensy API on top of Circle, plus a subset of the
   Teensy 4 core in `addon/arduino/teensycore` (`Print`, `Stream`, `WString`,
   `EventResponder`, `elapsedMillis`, and `usb_midi.c` with the parser kept and the transport
   replaced by packet rings);
3. Circle (bare-metal Raspberry Pi);
4. on Linux, `host/shim/circle` (14 headers of Circle's API) and `host/core` + `host/linux`
   (about 3,500 lines).

The Electra firmware does not need layers 2–3: it brings its own Teensy core, and Circle
plays no part. What carries over is the host layer's pieces and, more than that, its method:

| Need of the Electra firmware | In the shim | Carries over |
| --- | --- | --- |
| `millis`, `micros`, `delay`, `yield`, `elapsedMillis` | `host/core/arduino_time_host.cpp` (`clock_gettime`, `nanosleep`), `teensycore/elapsedMillis.h` | directly (about 100 lines) |
| Print, Stream, WString, Arduino.h subset | `addon/arduino/include/Arduino.h`, `teensycore/*` | as a template. Electra's own `bsp/Print.cpp`, `Stream.cpp` are portable and probably compile as they are (assumed) |
| USB MIDI device, 3 cables | `teensycore/usb_midi.c`: the parser with packet rings, `usb_midi_rx_packet`/`usb_midi_tx_packet` | the pattern: keep Electra's own `usb_midi.c` parser, replace its transport with rings in the same way |
| ALSA sequencer | `host/linux/alsamidi.cpp`: a client, reader thread, `snd_midi_event` encode/decode, hotplug, `--midi-trace` | the code, with the role reversed. The shim is a USB **host** that connects to devices; the Electra is the **device**: one client "Electra Controller" with 3 duplex ports that DAWs connect to |
| USB host MIDI (2 sockets) | `host/core/hostmidi.cpp`, `hostmididevice.h`: ALSA clients appear as `USBHost_t36` MIDI devices, ports as cables; virtual devices over the control port (`vmidi`) | close to directly, if Electra's `MidiUsbHost` uses the same `MIDIDevice` API (assumed) |
| IntervalTimer, IRQ masking | "threads are cores" (`CMultiCoreSupport::ThisCore`), a timer thread for TeensyTimerTool | the idea only: the Electra has one core, so a cooperative host loop is simpler and exact (below) |
| SD card | FatFs on a disk image (`sd.cpp`, `diskio_host.cpp`) behind `SD.h` | partly: Electra uses SdFat v2 (`SdFs`, `FsFile`), so it needs its own adapter, on a host directory or on the shim's FatFs image |
| EEPROM | stub saved to a file | directly |
| Window, input | `host/linux/hostwindow.cpp` (SDL2: scaled canvas, mouse = touch, keys = encoders, F12 screenshot), `hostboard.cpp` (a virtual MCP23017 answering on the I²C shim) | the window directly (1024×600 instead of 480×320); the virtual-chip pattern for the FT5x16 and BU21072 |
| Display | `app/mdt/ili9341emu.cpp` (about 400 lines: ILI9341 command level, native fonts), `hostdisplay.cpp` (touch state, PNG screenshots with zlib) | the PNG writer and touch handling directly. The emulator is only the pattern: the RA8876 is a different and much larger chip |
| Control port | `host/core/hostcontrol.cpp`: UDP 127.0.0.1:5000, `ping`, `uptime`, `quit`, `inject …`, `touch`, `screenshot`, `vmidi …` | directly, with Electra commands |
| Headless runs and tests | `--headless`, `--no-alsa`, `--port`, `--replay`; `tools/mdtctl.py` (one UDP command); `tools/uiwalk.py` (every page, screenshots, contact sheet); `tests/host/run.sh` with `*.sim` scripts and `expect` lines | directly as tooling and as test format |
| Build, sanitizers | `host/CMakeLists.txt` + `core/HostCore.cmake` (source lists read from the Makefiles, `-include hostcompat.h`), `-DSANITIZE=ON` (ASan + UBSan), `--wrap=malloc` with a zeroing `operator new` | directly as structure |

Lessons from that port which apply here too: a 64-bit address bug (`uint32_t` holding a heap
address) and code that relies on a zeroed heap showed up only under ASan. Both kinds are
plausible in the Electra firmware (assumed): its SDRAM addresses are `uint32_t` into the chip
and fine, but host pointers are worth an ASan run.

#### What is new for the Electra

| Part | Work | Lines (assumed) |
| --- | --- | --- |
| Host BSP headers | `Arduino.h`/`core_pins.h`/`WProgram.h` replacements (pins as no-ops, `DMAMEM`/`FASTRUN` empty, `__disable_irq` as a lock), `IntervalTimer` (registered callbacks), SPI/DMA/UART stubs, `random()`, a `String.h` that is `<string.h>` | 350 |
| Timing model | `processEvents()` on the main thread; the 500 µs (and 6 µs) "ISR" called from that loop between tasks, so an ISR is as atomic for the main loop as on the chip. ALSA, the window and the control port run in their own threads and only fill and drain the USB packet rings under a lock | 100 |
| USB device MIDI | Electra's `usb_midi.c` parser with rings; 3 duplex ALSA ports `Electra Port 1`, `Electra Port 2`, `Electra CTRL` in the client `Electra Controller`; virtual cables over the control port for tests without ALSA | 350 |
| **RA8876 in software** | a host `RA8876` class with the same interface. Its state is a 16 MB SDRAM array, canvas and main-window addresses, the active window (clip), and the colour depth (16 bit). It implements the drawing primitives, `writeRam*`/`readRam*`, and the BTE copy, ROP, chroma key and colour expansion. The frame shown is the main window's buffer. `tests/fakes/RA8876` already models the memory | 1,500–2,000 |
| Storage | SdFat v2 subset (`SdFs`, `FsFile`, directories, `openNext`) on a host directory (`--card DIR`; preset slots are then plain files) | 300 |
| Touch, knobs, buttons | FT5x16 → the window's mouse and `touch` commands. The 12 pots → deltas injected into `Pot`; a later option is to synthesize the two gangs' ADC values, which would also run the decoder of `POTS.md`. BU21072 → knob touch. The 6 buttons | 300 |
| Memory checks | `_sbrk`, `RamMonitor`, the `adj_free` gate of `Presets::loadPreset`, the SRAM-bank placement → stubs that report the Teensy's sizes, so the firmware refuses the same presets it refuses on the unit | 100 |
| Firmware patches | `#ifndef ELECTRA_HOST` around the size asserts (or `-m32`), `<algorithm>`, the `Print::write` overload, the info reply's `model` as a build option (`mk1`/`mk2`) | about 20 lines in a handful of files, upstreamable |
| Window, control port, tooling | adapted from the shim: SDL window 1024×600 with knobs on keys and wheel; UDP control port with `pot N ±steps`, `potTouch N 0|1`, `button NAME`, `touch X Y`, `screenshot`, `vmidi …`, `quit`; `electra_ctl.py`, a `uiwalk`-like page walker | 600, half of it copied |
| Build | CMake reading the firmware Makefile's source lists, like `HostCore.cmake` | 150 |

Notes:

- **The 64-bit question.** Two ways. Either build for 64-bit and guard the four size asserts
  (the sizes are budgets for the Teensy's RAM, not layout contracts), or build with `-m32`. The
  second needs i386 multilib packages of ALSA, SDL2 and zlib. 64-bit with guarded asserts is
  the plan (assumed to be enough; ASan is the check).
- **Screen touch** goes through the window's mouse and the control port (the FT5x16 is polled
  every 40 ms, so a touch must be held at least that long, like the shim's 80 ms minimum).
- **The mk2 and Mini** are not covered by the mk1 firmware's screens. The mk2 differs for DBM
  only in the info reply's `model`, which is a build option. The Mini (800×480, 8 knobs with
  push, 4 pads) is a different instrument; (b) does not cover it.

#### Effort (assumed)

| | Without the shim | With the shim |
| --- | --- | --- |
| Boots headless, logs to stderr, `processEvents()` runs (BSP headers, timing, stubs, patches) | 3–4 days | 2–3 days (the survey method, `hostcompat.h`, CMake) |
| The screen: RA8876, PNG screenshot | 6–8 days | 5–7 days (only the PNG writer and touch plumbing are reused) |
| USB MIDI with 3 ALSA ports, virtual cables | 3 days | 1–2 days |
| Storage, presets in slots | 2 days | 2 days |
| Window, input, control port, tooling, `*.sim` tests | 4–5 days | 1–2 days |
| **Total to "DBM's preset runs, DBM talks to it"** | **3–4 weeks** | **2–3 weeks** |

The shim saves about a week. Most of that is infrastructure that the shim has already
debugged: the control port, headless screenshots, ALSA, the test runner, the sanitizer
build. The RA8876 is the same risk either way.

#### Fidelity

The highest available:

- the real preset parser, parameter map, Lua 5.4, components and windows;
- the real SysEx answers, including the corners `SYSEX-API.md` documents (NACK for the rest,
  transaction ids, the staged upload);
- the same repaint batching and the same RAM refusals.

The rendering is exact to the extent that the RA8876 emulation is exact. A wrong BTE detail
shows at once in the glyphs.

There is no `.eproj` guesswork either, but not because the firmware reads `.eproj`: it reads
`.epr` only. The point is that the real tools can talk to the host build. The web editor
(app.electra.one, WebMIDI on the ALSA ports) or `electraone-cli` can upload
`DrivenByMoss.eproj` the way a user does. That they accept the host build is **assumed**: they
should, if the info reply looks like a 5.0.6 mk1.

#### Maintenance

Upstream is very active: dozens of commits in October 2026, with mk2 parity as the goal. The
host build compiles `src/` and `apps/controller` from a pinned firmware checkout and replaces
only `bsp/` and the hardware libraries, so most upstream changes come for free with a bump of
the pin. What needs work after a bump:

- changes at the boundary: new RA8876 calls, a new hardware task, a new static assert;
- a failing patch in the handful of patched files.

Offering the host BSP and the patches upstream would remove most of that; Electra already
keeps a compatibility harness and Linux tooling (`tools/compat`).

#### Where it lives and license notes

The project is GPL-3.0, so the host BSP can live in our tree, for example
`support/electra_host/` (C++, CMake), with firmware-mk1 as a pinned git submodule or a sibling
checkout, as the MDT port does with its submodules. What has to be noted, per file:

- **firmware-mk1:** GPL-3.0-or-later, Electra One Project. Copied or patched files keep their
  headers.
- **The Teensy core parts** (Electra's `bsp/`, the shim's `teensycore/`): PJRC's MIT-style
  license. Keep the notices.
- **The libraries** keep their licenses: Lua (MIT), ArduinoJson (MIT), SdFat (MIT, assumed).
- **The MDT port's host code** (`~/bastel/cynthian/port/host`): the owner's own code, with no
  license file. Its license has to be stated when files are copied (§8).
- **System libraries:** the host build links SDL2 (optional, for the window), ALSA and zlib. It
  is a separate C++ program, so the Rust crate and Python stay free of new dependencies.

### (c) An emulator

QEMU has no Kinetis K66 machine. An emulator would need models for the K66 core peripherals,
the RA8876 on SPI with its BTE engine, the FT5x16 and BU21072 on I²C, the ADC with the
multiplexer, the SDHC, and above all the Kinetis USB-FS device controller with a 3-cable MIDI
function bridged to ALSA. That takes months, and it needs the same RA8876 model as (b) plus
everything else. **Rejected.**

### (d) Our QML front end over (b)

The host build runs headless and streams its frames over a UNIX socket, as the Push display
does (`display.DisplayServer`). Our QML shell shows the frames and sends touch, knob, knob-touch
and button events back:

- **The frames:** RGB565 1024×600 (1.2 MB) on each buffer swap, at most 40 a second, about
  50 MB/s over a local socket. Dirty rectangles later, if that is too much.
- **The events:** the same commands as the control port.

What the shell gives:

- `SimCommon`'s 12 `Encoder`s with touch and 6 `LedButton`s around the screen;
- the MIDI monitor (F2) on the ALSA traffic, with the decoded Electra SysEx;
- screenshots and the `run.sh` hand-over like the other simulators;
- no SDL dependency for the firmware build.

It costs about 300 lines of QML and Python on our side and 150 in the host build. The
firmware's own SDL window stays useful for quick looks and for debugging the host build alone.

For the tablet, (d) is a "remote screen". A tablet could host the real Electra this way and
look exactly like it. But it would be pixels at a fixed 1024×600, not native, scalable,
data-driven controls.

### Comparison

| | Effort | Fidelity | Maintenance | Serves the tablet | Covers mk2/Mini |
| --- | --- | --- | --- | --- | --- |
| (a) native | 1–2 weeks for the DBM subset | protocol high for the subset; pixels approximate; Lua shim or a re-written API | follows upstream by hand | **directly**: the preset model and renderer are the tablet's | yes, as data |
| (b) host firmware | 2–3 weeks with the shim (3–4 without) | exact: the real firmware; pixels as exact as the RA8876 emulation | a pin bump; the boundary rarely moves | as an oracle, not as UI | mk1; mk2 by a flag; not the Mini |
| (c) emulator | months | high, if every peripheral is right | the same as (b), plus the peripheral models | no | no |
| (d) QML over (b) | (b) + 2–3 days | as (b) | as (b) | a remote screen only | as (b) |

### Recommendation: (b) + (d) first, (a) later for the tablet

1. **(b) is the DBM test target.** It answers every SysEx the way the unit does, runs the
   preset's Lua, and loads the preset with the real parser. The question "is the simulator
   right?" goes away for the Java controller and for the Rust port. The Lua question goes
   away too: the firmware brings its own Lua 5.4.
2. **The shim brings (b)'s cost close to (a)'s:** 2–3 weeks against 1–2. The infrastructure
   (control port, headless screenshots, ALSA, `*.sim` tests, sanitizers) is already proven
   there.
3. **(d) fits it into our simulators:** our window, monitor and `run.sh` around the real
   screen.
4. **(a) comes later, when the tablet needs it,** and then with (b) as its **oracle**. The same
   SysEx/MIDI stream goes into both. The test compares the outgoing MIDI and the control state
   (names, colours, values, visibility; from the firmware through the control port or
   `controls.print`). The tablet's preset model is thus tested against the real firmware, and
   (a) may translate firmware logic where that is the shortest way to agreement.
5. **The Mini** is not covered by (b). It needs (a), or it waits.

The cost of this order: there is no QML model of the Electra until the tablet work starts. If
the tablet comes first, (a) can start in parallel with (b), and only its oracle tests wait.

**The first step** is phase 1 (§7), a day or two: a CMake build over the firmware's sources
with a stub host BSP. First every `.cpp` compiles, then it links, then `initialise()` returns
and `processEvents()` runs headless with a null display. That turns the survey's findings
(§3 (b)) into a complete list. It also shows early whether 64-bit with guarded size asserts
holds up, and whether the boot depends on hardware answers that the stubs must give. The
RA8876 work starts only after that.

## 4. Design sketch

### 4.1 The host build of the firmware ((b), `support/electra_host/`)

| Part | Content |
| --- | --- |
| `CMakeLists.txt` | the firmware's source lists (`src/`, `apps/controller`, `lib/*`), minus `bsp/` and the hardware libraries; `ELECTRA_HOST`; `-include hostcompat.h`; `SANITIZE` option |
| `bsp/` | host `Arduino.h`/`core_pins.h`, `IntervalTimer`, `SPI`/`DMAChannel`/`Uart` stubs, `usb_midi` transport rings, EEPROM file |
| `ra8876/` | the software RA8876 (`RA8876.h` interface kept) |
| `storage/` | the SdFat v2 subset on a directory |
| `input/` | FT5x16, BU21072, pots and buttons as injected events |
| `linux/` | `main.cpp` (options, threads, the loop), `alsamidi.cpp`, `hostwindow.cpp` (SDL, optional), `control.cpp` (UDP), `framesocket.cpp` (for (d)) |
| `patches/` | the few firmware patches, until upstream takes them |
| `tools/` | `electra_ctl.py` (like `mdtctl.py`), `electra_walk.py` (every page of a preset, screenshots), `run_tests.sh` with `*.sim` scripts |

Options:

- `--card DIR` (the SD card as a directory);
- `--headless`, `--no-alsa`, `--port N`;
- `--frames SOCKET` (for (d));
- `--model mk1|mk2` (the info reply);
- `--midi-trace`.

### 4.2 The QML shell ((d), `support/controller_simulator/electra_simulator/`)

- **A `SimulatorBase` subclass:** `controls` for the 12 encoders (with touch) and the 6
  buttons, and an `Image` fed from the frame socket. Mouse on the image → `touch X Y`, held
  for at least 40 ms.
- **The ALSA side:** the shell does not create MIDI ports. The host build owns `Electra
  Controller`, and DBM and Bitwig connect to it as to the unit. ALSA cannot snoop on a
  connection between two other clients, so the host build itself sends what passes its
  ports to the monitor, as a message stream on the frame socket (what `--midi-trace` logs).
- **`run.sh`:** starts the host build and the shell, and stops both.

### 4.3 The native model ((a), later)

As in the first version of this plan:

- `preset.py`, `eproj.py` (only if the editor's upload is not available);
- `parameter_map.py`, `sysex_api.py`, `lua_shim.py` (or real Lua, §8);
- `device.py`, `models.py`, and the QML page renderer.

Its tests add the oracle comparison with (b).

### 4.4 Testing

- **Headless screenshots:**
  1. `--headless --port 5100`, load the preset;
  2. `electra_ctl.py screenshot` → PNG from the emulated main window;
  3. `electra_walk.py` switches every page (`09 0A` over a virtual cable, or the `button`
     command) and writes a contact sheet, as `uiwalk.py` does for MDT.
  - The firmware's own screenshot (`7F 76` into `tmp`, fetched with `02 35`) is a second
    path that tests the file API too.
- **`*.sim` scripts** (like `tests/host/*.sim`):
  - lines that send MIDI/SysEx on a virtual cable and press, turn or touch;
  - `expect` lines on the replies (ACK, `7E 06`, CC out) and on screenshot pixels.
  - Deterministic, no ALSA (`--no-alsa`).
- **A scripted DBM conversation** (`electra_feed.py`): the Java handshake of §2.2, checking the
  replies. This is the stand-in until the Rust port exists.
- **`electraone-cli`** against the CTRL port: preset and Lua upload, commands.
- **The web editor** (Chrome WebMIDI on the ALSA ports): installing `DrivenByMoss.eproj` the
  user's way (assumed to work, §3 (b) Fidelity).
- **Java DBM in Bitwig:** two `snd-virmidi` devices (Port 1 and CTRL) connected to the host
  build's ports; select them by hand in Bitwig (the names don't match the discovery names).
- **The Rust port:**
  - `dbm_host_alsa.py --model electra` against the host build;
  - an in-process `CoreTest`: the bridge talks to the host build through the virtual cables of
    the control port. It requires no NACK and no core error, and every control operated.
- **Sanitizers:** an ASan/UBSan build runs the `*.sim` scripts.

## 5. The Rust port of the DBM controller

**Status (2026-11-22): ported** (`src/controller/electra/`, normal and Mini; doc/STATUS.md). Of
the framework pieces below, `MasterVolumeMode` turned out not to be needed (the Electra modes only
share its focused-parameter error text); the others were added as listed (the JSON reader is
`json::JsonValue`, the second port pair a further MIDI input and output of the instance). The port
was tested against the emulator of the real firmware (the electra-one project), which this plan
proposed as (b): `dbm_host_alsa.py --model electra` and
`support/controller_simulator/tests/test_electra_emulator.py`. Findings of those runs: the faders of
the DBM preset have `"relative"` messages, which the emulator reads as the mk1 firmware's `relcc`,
so the CC values from the host do not move their bars (the value texts show the values).


- **Size:** 5,441 Java lines, an estimated 3,500–4,000 lines of Rust:
  - base: surface, SysEx, PageCache, colour manager;
  - the mk1/mk2 controller: 7 modes and 38 touch-combination commands;
  - the Mini: 3 modes.
- **Framework pieces that exist:** `ButtonRowModeCommand`, `CombinedParameterProvider`,
  `Empty`/`FixedParameterProvider`, `DummyView`, `DummyMode`, the two's-complement value
  changer, `set_knob_sensitivity_is_slow`, and the Row1_1–Row6_8 button ids.
- **Framework pieces to add:**
  - **A JSON reader.** `src/json.rs` only writes, and the info and preset-list replies must be
    parsed. A minimal reader is about 300 lines, with no dependency.
  - **Moving `MasterVolumeMode` into the framework** (doc/FRAMEWORK_MAP.md says so).
  - **A play position parameter** as a parameter (MixerMode row 2, column 6). Check whether
    `PlayPositionCommand` covers it.
  - **SysEx chunk reassembly** on the CTRL input (Java concatenates chunks until `F7`), and
    ASCII fixing of text (`StringUtils.fixASCII`, `optimizeName`).
  - **A second port pair** (the definition is 2×2). Other cores already have more than one input
    or output; check the Bitwig and Python host bindings for two outputs.
- **Order of work:**
  1. surface + SysEx + online/offline + MixerMode;
  2. the other modes;
  3. touch combinations;
  4. the Mini.
- **The host build tests each step:** the handshake first, since a missing reply leaves the core
  offline and silent, like the LCXL template answer.
- **Conformance:** an `electra_one` setup is possible later. The runner plays the device into a
  virmidi port and needs `[[stub]]` replies for `02 7F`, `02 04` and `09 08`, like the LCXL's
  template echo.

## 6. What the tablet controller should take from the Electra One

**Adopt:**

- **Presets as data.** Devices, pages, control sets, groups, controls, values and messages;
  overlays for value lists. A surface is a document, not code, and an editor can write it.
- **The value model.** Separate display and MIDI ranges, sign and relative modes, acceleration,
  per-value sensitivity, formatter names, `defaultValue`. Values bind to messages through a
  parameter map, so two controls showing one parameter stay in sync.
- **Control sets.** Physical encoders (on a tablet: a strip of virtual knobs, or a connected
  MIDI encoder box) bound to the active section of the page.
- **A small live API for the host:** set a control's name, colour, visibility and value text;
  set group labels; switch page; stop/resume repaint; subscribe to events (page, control
  touch); a log channel. DBM's whole Electra driver is built on these few calls, so it is
  enough for a DAW controller.
- **Commands by name** (`COMMANDS.md`): one table shared by buttons, MIDI, host and scripts.
- **The preset lifecycle hooks and the Lua API names**, if the tablet scripts in Lua. Reuse the
  mk2 names where they fit, so Electra presets are portable.

**Do differently:**

- **Transport:** JSON in SysEx over MIDI suits a USB MIDI device. A tablet talks over the
  network: keep the same messages and semantics, but carry them over a socket (JSON lines or
  the existing DBMV/display socket style), with a MIDI binding only where a DAW needs one.
- **No 7-bit and length limits, no fixed 6×6 grid:** layouts by bounds, scaled to the screen.
  The Electra already allows free bounds, and the grid is only the editor's default.
- **Lua runtime:** one in the project (mlua in the Rust library) rather than one per frontend.
- **Pixels vs. model:** the Electra draws its own pixels from its own model. A tablet that
  only showed the firmware's frames (§3 (d)) would inherit the fixed 1024×600. The tablet keeps
  the model and renders natively, and uses the real firmware as the reference it is tested
  against.
## 7. Phases

Each step ends with tests and a commit.

| # | Step | Verified by |
| --- | --- | --- |
| 0 | Decisions (§8). The location of the host build (in our tree with firmware-mk1 as a submodule, or a fork) and the license of the copied MDT host code | the decisions are recorded in this plan |
| 1 | **The survey and the skeleton.** CMake over the firmware's sources with a stub host BSP: every `.cpp` compiles (`-fsyntax-only` first, as the MDT survey did), then it links; `initialise()` returns and `processEvents()` runs headless, logs to stderr, no display (a null RA8876), no MIDI | the build; the log shows the boot up to the main loop and the preset load from `--card DIR` |
| 2 | The RA8876 in software, the PNG screenshot over the control port (`screenshot`, `quit`) | a screenshot of the boot screen and of the compat preset (`tools/compat/presets/compat.epr`) whose text is legible; pixel checks in a `*.sim` script |
| 3 | USB MIDI: the 3 ALSA ports, virtual cables over the control port, `--midi-trace` | `*.sim`: `02 7F` → `01 7F`, `02 04` → `01 04`, `09 0A` → ACK + `7E 06`; `electraone-cli` against the CTRL port |
| 4 | Input: knobs (deltas, touch), buttons, screen touch; the SDL window (optional) | `*.sim`: a knob turn sends the preset's CC, knob touch sends `7E 0A`, a pad touch sends on/off |
| 5 | The DBM preset on the card (the editor's upload over WebMIDI, or `electraone-cli`, or an `.epr` placed by hand). The Java conversation replayed by `electra_feed.py` | the replies of §2.2 and screenshots of the 7 pages with labels, colours and group titles set |
| 6 | (d): the frame and monitor socket, the QML shell with encoders, buttons, monitor; `run.sh` | offscreen screenshot smoke test, as for the other simulators |
| 7 | Java DBM in Bitwig through snd-virmidi: online/offline, all 7 pages, touch combinations | manual run, findings in CONTROLLER_SIMULATORS.md §9/§10 |
| 8 | The Rust port (§5), step by step against the host build; `dbm_host_alsa.py --model electra`, the in-process `CoreTest` | tests in `test_midi_simulators.py`-style |
| 9 | `--model mk2`; ASan/UBSan run of the `*.sim` scripts; the patches offered upstream | the Rust and Java controllers report mk2; clean sanitizer run |
| 10 | Later, with the tablet work: (a), the native preset model and QML renderer, with oracle tests against the host build; the Mini, if wanted, in (a) | the same stream into both, equal MIDI out and control state |

## 8. Open questions for the owner

1. **Where the host build lives:** in our tree (`support/electra_host/`, firmware-mk1 as a
   pinned submodule or a sibling checkout), or as a fork of firmware-mk1 with a `host/`
   directory? Offer it upstream to Electra (Martin Pavlas is actively working on firmware-mk1)?
2. **The MDT host code:** under which license may files of `~/bastel/cynthian/port/host` be
   copied? There is no license file there.
3. **The display path:** our QML shell (d) as the default, with the SDL window optional (no
   SDL dependency unless wanted), or the SDL window first?
4. **64-bit:** guard the four size asserts for the host build (the plan), or build with `-m32`
   (needs i386 multilib packages)?
5. **Scope:** mk1 and mk2 (a flag) through (b); the Mini only later through (a), or not at
   all?
6. **The DBM preset:** is installing it with the web editor (or `electraone-cli`) into the host
   build acceptable as the way the test card is made? Otherwise an editor-exported `.epr` in
   `tests/fixtures/` is needed.
7. **The tablet direction:** should the tablet's surface format be the Electra `.epr` (with
   extensions), so that Electra presets run on the tablet, or a format of our own inspired by
   it? And Lua for (a) later: the Rust library's `mlua` (no new dependency) or `lupa`?
8. **The Rust port's place:** port the Electra One controller as the host build's first real
   client (PORTING_GUIDE order 3), or test with Java first?

## 9. Risks

- **The RA8876 emulation** is the largest piece, and an exact BTE is needed for the text
  atlases: chroma key, expansion and the colour depth. A detail wrong shows as garbled text.
  Mitigation: screenshots from step 2 on; compare against photos or a unit's `7F 76`
  screenshot if one is available.
- **The 64-bit build:** the size asserts are known. Pointers held in 32-bit fields and a
  reliance on a zeroed heap are not known but plausible (the MDT port had both). Mitigation:
  ASan/UBSan early.
- **Timing:** the host is much faster than the 180 MHz M4, so timing bugs (the 90 ms repaint,
  the DIN line rate of §5.3 in ARCHITECTURE.md) can hide. The cooperative ISR model keeps the
  ordering, not the durations.
- **Upstream churn** at the BSP boundary: a pin bump may need host work. Mitigation: upstream
  the BSP.
- **The editor and the CLI might not accept the host build** (an unknown check of serial,
  hardware revision, or USB identity). Mitigation: an exported `.epr` (question 6).
- **Silent offline.** DBM throws if the `DrivenByMoss` preset is missing from the list, and
  stays offline unless `7E 02` names its slot. The card must hold the preset, and the monitor
  must make a missing handshake obvious.
- **Version and model checks** differ between the controllers (mk1/mk2 ≥ 3.4.3, Mini ≥ 5.0.4
  and `model:"mini"`). The host build reports 5.0.6 and `mk1` (no `model`) or `mk2`.
- **Bitwig port names:** virmidi ports don't match the discovery names, so selection is manual
  (as for the other simulators).
- **Long SysEx through ALSA:** uploads (a preset is 100 KB+) arrive in fragments, and the
  firmware's partial-SysEx path has to see them as the USB stack delivers them. This is tested
  by `electraone-cli` uploads in step 3.
