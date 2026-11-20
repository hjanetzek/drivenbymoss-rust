# Electra One simulator: research and plan

Status: 2026-11-20. Research and planning only, no simulator code yet. This document covers the
device, what DrivenByMoss uses of it, four ways to simulate it, a recommendation, and phases.
The Electra One is a touchscreen controller driven by presets and Lua, which makes it the
closest relative of the planned tablet controller (doc/CONTROLLER_SIMULATORS.md). Section 6
looks at what the tablet should take from it.

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
`.epr` it generates from that. DrivenByMoss ships only `.eproj` files, which matters for every
simulator approach (§2.3).

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
| Lua | `08 0D` with at least `sgt(id,"text")` executed against the preset's `sgt` (§4.1) |
| Parameter map | an incoming CC on (port 1, ch 16) sets every value bound to it (cc7 and relative messages); pad on/off |
| Controls | fader (incl. relative message), relative, pad (momentary/toggle, color from `14 07`), list with overlay, group label (variant highlighted, visibility) |
| Input | 12 knobs bound by the active control set, sending relative two's-complement CCs (accelerated); knob touch → `7E 0A` (0-based pot id, control id); screen touch on a pad → `onValue`/`offValue`; 6 buttons (control set 1–3, page selection) |
| Preset loading | `.epr` files in slots; an `.eproj` → `.epr` converter for the shipped DBM files |
| Robustness | ACK/NACK with the echoed transaction id; NACK for everything unknown, logged as "unhandled" in the monitor; long SysEx reassembled from ALSA fragments |

A Rust port of the controller needs exactly the same subset.

## 3. Simulator approaches

### (a) A native re-implementation in our PySide6/QML stack

A pure Python model (`electra_simulator/device.py`, no Qt) with:

- a preset parser (`.epr`, plus an `.eproj` converter);
- the parameter map with the MIDI ↔ display translation;
- the SysEx API subset of §2.4 (later: preset/Lua upload, `08 13` commands, `14 77`, `09 0B`,
  events for control sets);
- the slot store (a directory of files, 6 banks × 12).

QML renders the controls natively: a `PageView` of `Repeater`s over the page's groups and
controls, with one component each for fader, pad, list, relative and group, the bars, 12
`Encoder`s with touch and 6 `LedButton`s from `SimCommon`.

- **Effort:** the DBM subset takes roughly 1,500–2,500 lines of Python + QML and fits the
  `midi_simulator` pattern (listeners, monitor, layout test, smoke screenshot). Envelopes,
  XY pad, vfader, knob, detail view and snapshots come later, as needed.
- **Fidelity:** protocol behaviour as specified, with pixels "in the style of" the device, not
  identical. The firmware's rendering details (fonts, the fader thin/dial variants) are
  approximated. Lua (below) is the largest fidelity gap.
- **License:** clean. It is written from the public documentation (docs.electra.one) and the
  observed protocol. Reading the GPL firmware to learn facts (message layouts, field names) is
  interoperability work. Copying code or larger tables into our LGPL tree is not, and we
  don't.
- **Lua, three options:**
  1. **None, plus a shim (stdlib only, recommended to start).** `08 0D` text is matched against
     `name(arg, …)` calls with literal arguments. A function the simulator knows (DBM's `sgt`:
     set the group label, visibility from the text length) runs natively; anything else is
     logged as unhandled Lua. The preset's own `lua` is parsed only to see which functions it
     defines. That covers DBM completely, and 50 lines do it.
  2. **`lupa`** (Python binding to Lua 5.4/LuaJIT, MIT). This gives real Lua with the API
     modules in Python, but it is a new dependency, against the project's no-deps rule, and needs
     the owner's decision.
  3. **Lua through our Rust library.** `mlua` with Lua 5.4 is already the one approved
     dependency (doc/LUA_PLAN.md). A small C ABI ("a sandboxed Lua state with host callbacks")
     in `libdrivenbymoss` would give the simulator, and later the tablet controller, real Lua
     with no new dependency. It is more work than lupa but keeps one Lua runtime in the project.
  - A tiny Lua interpreter in Python is not worth it: real presets use the whole language.

### (b) Running the real firmware on the host

Compile `src/`, `apps/controller` and the portable libraries (`lib/lua`, `ArduinoJson`,
`base64`, `CircularBuffer`, `TaskScheduler`, `arduino_midi_library`) natively against a host
BSP. Assessment from `bsp/`, `ARCHITECTURE.md` and the include graph:

- **The boundary is narrow.** Only 14 files in `src/` and `apps/controller` include hardware
  headers (`Arduino.h`, `core_pins.h`, `mk66fx1m0.h`, `IntervalTimer.h`, `ADC.h`, `RA8876.h`,
  `SdFat.h`, `USBHost_t36.h`, `FT5x16.h`, `BU21072.h`); only 11 lines touch registers directly
  (IRQ masking, DWT, NVIC). There are 76k lines of application code and 68k lines of libraries.
- **The host BSP would need:**
  - an Arduino shim (`millis`, `micros`, `delay`, `elapsedMillis`, `Serial`, pins as no-ops);
  - `IntervalTimer` → call `readMidi` from the main loop, or a thread with `__disable_irq` as a
    mutex;
  - USB MIDI (`usb_midi_*`, `usbMIDI`, 3 cables, partial-SysEx callbacks) → ALSA sequencer;
  - `SdFat` → host files (`tests/fakes/SdFat.cpp` is a 163-line start);
  - the RA8876 → a software rasterizer;
  - FT5x16 → mouse, BU21072 and the pots → UI events injected at `PotBroadcaster`/`Pot` level;
  - DIN, the USB host and EEPROM → stubs or files.
- **The display is the big piece.** `Graphics`/`FrameBuffer` derive from the `RA8876` class
  (about 125 methods: primitives, the BTE copy/chroma/expansion engine, CGRAM fonts, canvas and
  layer addresses, readRAM/writeRAM into 16 MB SDRAM). Text and bitmaps live in that SDRAM and
  are blitted by the BTE. A software RA8876 is about 1,500–2,500 lines of C++, and has to be
  exact for the text atlases to come out right. `tests/fakes/RA8876` models only the memory.
- **Platform assumptions:**
  - 32-bit pointers in packed model structures (`LookupEntry` 8 bytes with a pointer, "four bytes
    an entry" indexes), so building with `-m32` (i386 multilib) is the cheapest way round;
  - the SRAM-bank and heap/stack checks (`_sbrk`, `RamMonitor`, the 62,000-byte `adj_free` gate
    in `Presets::loadPreset`) need stubs;
  - Teensy-specific `DMAChannel`/SPI code goes away with the BSP.
- **Threading** is simple: no RTOS, one cooperative scheduler plus a 500 µs timer ISR that only
  queues. A single-threaded host loop is faithful.
- **Lua** is plain Lua 5.4 C and compiles as is.
- **The tests** (`tests/`, CppUTest, macOS paths, testing a removed `extram.cpp`) are stale and
  no help.
- **Effort:** about 2–4 weeks of agent work to a window that boots the controller app, loads
  an `.epr` from a directory and talks over ALSA. The display is half of it, and the 64-bit
  issues are an unknown.
- **Fidelity:** the highest. The real parameter map, Lua, rendering and SysEx answers.
- **License:** the host port is a derivative work of GPL-3.0 code and must be GPL-3.0. It has
  to be a **separate repository** (a fork of firmware-mk1 with a `host/` BSP, ideally offered
  upstream). It cannot go into our LGPL tree. Our LGPL code talking to it over ALSA MIDI or a
  socket is communication between separate programs (aggregation), which the GPL allows.
  Starting such a project, and whether to approach Electra about upstreaming it, is the
  owner's decision.
- **Limits:** it simulates the mk1 only. The mk2 and Mini firmware is closed; their differences
  (screen size, Mini knob pushes, `model` key) would have to be patched in.

### (c) An emulator

QEMU has no Kinetis K66 machine. An emulator would need models for the K66 core peripherals,
the RA8876 on SPI with its BTE engine, the FT5x16 and BU21072 on I²C, the ADC with the
multiplexer, the SDHC, and above all the Kinetis USB-FS device controller with a 3-cable MIDI
function bridged to ALSA. Renode or a custom QEMU machine could do it in principle, but that
takes months, and every peripheral model is a fidelity risk. It brings nothing that (b) does
not bring cheaper. **Rejected.**

### (d) A hybrid: our QML front end with (b) behind a socket

The GPL host port renders into its framebuffer and streams frames over a UNIX socket, as the
Push display does (`display.DisplayServer`); the QML shell shows the image and sends touch,
knob and button events back. That gives (b)'s fidelity with our window, monitor, screenshots
and the run.sh hand-over. The cost is (b) plus about 300 lines of shell. The screen is a pixel
image, though, so nothing of it is reusable for the tablet controller, which wants native,
data-driven controls.

### Comparison

| | Effort | Fidelity | License | Serves the tablet | Covers mk2/Mini |
| --- | --- | --- | --- | --- | --- |
| (a) native | low to medium (DBM subset in 1–2 weeks) | protocol: high for the subset; pixels: approximate; Lua: shim | clean (LGPL, our code) | **directly**: the preset model and renderer are the tablet's | yes, as data (screen size, knobs) |
| (b) host firmware | high (2–4 weeks, display emulation) | highest for the mk1 | separate GPL-3.0 project | no (pixels) | mk1 only |
| (c) emulator | very high (months) | high, if every peripheral is right | separate project | no | no |
| (d) hybrid | (b) + small | as (b) | GPL part separate | no | mk1 only |

### Recommendation: (a)

1. It covers everything DBM uses (§2.4), for the Java controller and a future Rust port, at
   the lowest cost, and fits the existing simulator pattern and tests unchanged.
2. It is the **tablet controller's foundation**. A preset-driven surface model (pages, control
   sets, groups, controls, values bound to messages, host-updatable labels and colours) with
   native QML controls is exactly what the tablet needs. The Electra simulator would be its
   first user.
3. It avoids the GPL question entirely and covers the mk1, mk2 and Mini as data.
4. (b)/(d) remain a possible later **reference oracle**: a separate GPL project that answers
   "does the real firmware do the same?" for the protocol corners. Worth it only if (a)'s
   behaviour is ever in doubt, and only with the owner's go-ahead.

Lua: start with the shim (stdlib only). If real Lua is wanted, prefer option 3 (through the Rust
library's `mlua`) over `lupa`, because it adds no dependency and the tablet controller would
script through the same runtime as the Lua controllers.

## 4. Design sketch for (a)

### 4.1 Package and modules

`support/controller_simulator/electra_simulator/`, next to `maschine_simulator`, using
`sim_common`:

| Module | Content |
| --- | --- |
| `preset.py` | the `.epr` model (Preset, Page, Device, Overlay, Group, Control, Value, Message), the slot geometry, validation |
| `eproj.py` | `.eproj` → `.epr` conversion (schemaVersion 2 tiles; 3 with `layouts` for the Mini) |
| `parameter_map.py` | (device, type, number) → MIDI value and bound values; MIDI ↔ display translation (ranges, sign modes, relative modes, acceleration) |
| `sysex_api.py` | parse/build the Electra SysEx, transaction ids, ACK/NACK, events, subscriptions |
| `lua_shim.py` | `08 0D` call matcher, the known functions (`sgt`), detection of the preset's defined functions |
| `device.py` | the device: ports, slots, the loaded preset, active page and control set, knob → control binding, `receive(data, port)`, `turn_knob`, `touch_knob`, `press_button`, `touch_control`, listeners, monitor, `unknown` |
| `models.py` | mk1 / mk2 / Mini: screen, knobs (with push on the Mini), buttons, info JSON |
| `qt_bridge.py`, `main.py`, `qml/` | `SimulatorBase` subclass and the QML page renderer |

Notes:

- Slots live in a directory (`--slots DIR`, default a temp copy). `--preset FILE[@bank:slot]`
  preloads the DBM preset.
- The info reply follows `--model mk1|mk2|mini` and `--firmware 5.0.6`, so DBM's version checks
  (`300400300`, `500000400`) can be tested both ways.
- Long SysEx: `sim_common.alsa_seq` decodes event by event. Large uploads (a preset is
  100 KB+) arrive in fragments and must be reassembled until `F7`, and the 64 KB encoder buffer
  is too small for sending a preset back. DBM itself sends only short messages.

### 4.2 Testing

- **Unit tests** (headless): the converter against the shipped `.eproj` (307 tiles → 252
  controls + 55 groups, unique ids, pages from `slotId // 72`, bounds on the grid). Each SysEx
  command has a test, and so do the parameter map translations.
- **A scripted host** (`tools/electra_feed.py`): replays the Java conversation of §2.2 over ALSA
  and checks the replies. This is the stand-in until the Rust port exists.
- **The official client as a cross-check:** `electraone-cli` (GPL, run as an external program)
  can upload presets, Lua and run commands against the simulator's CTRL port, which tests the
  upload paths against a "complete host of the protocol". The web editor (Chrome WebMIDI on the
  ALSA ports) is a further check, if it accepts a simulated device.
- **Java DBM in Bitwig:** two `snd-virmidi` devices (Port 1 and CTRL), `--connect` both; select
  them by hand in Bitwig (the names don't match the discovery names).
- **The Rust port:** `dbm_host_alsa.py --model electra` and an in-process `CoreTest` (bridge ↔
  device model, `device.unknown` empty, every control operated, no core errors).
- **A smoke test:** offscreen screenshot after the feeder has set labels, colours and group
  titles; check the colours in the PNG.

## 5. The Rust port of the DBM controller

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
- **The simulator tests each step:** the handshake first, since a missing reply leaves the core
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

## 7. Phases

Each step ends with tests and a commit.

| # | Step | Verified by |
| --- | --- | --- |
| 0 | Decisions (§8). Get an `.epr` the editor generated from `DrivenByMoss.eproj` (export from app.electra.one, or fetch from a unit with `02 01`) as the reference for the converter | the file is in `tests/fixtures/` |
| 1 | `preset.py` + `eproj.py`: parse `.epr`, convert the two DBM `.eproj` | unit tests: counts, ids, pages, bounds; equal to the editor's `.epr` where it is available |
| 2 | `sysex_api.py` + `device.py` headless: info, preset list, switching, `14 07`, `14 0E`, `14 79`, `7F 7A`, `7F 7D`, `08 0D` with the `sgt` shim, ACK/NACK, NACK for the rest; the parameter map for CC | unit tests, plus `electra_feed.py` replaying the Java conversation |
| 3 | ALSA ports (`Electra Controller`: Port 1, Port 2, CTRL), long SysEx reassembly, the MIDI monitor with decoded Electra SysEx | ALSA loopback test |
| 4 | QML: page view (fader, relative, pad, list, group), bars, 12 touch encoders, 6 buttons, control sets, page selection; `--model mk1` | offscreen screenshot smoke test with the feeder |
| 5 | Java DBM in Bitwig through snd-virmidi: online/offline, all 7 pages, touch combinations | manual run, findings in CONTROLLER_SIMULATORS.md §9/§10 |
| 6 | The Rust port (§5), step by step against the simulator; `dbm_host_alsa.py --model electra`, the in-process `CoreTest` | tests in `test_midi_simulators.py`-style |
| 7 | mk2 (`model:"mk2"`) and Mini (800×480, no screen touch, 8 knobs with push and touch, 4 buttons, the schemaVersion 3 `layouts`), the Rust Mini controller | unit + smoke tests per model |
| 8 | Optional, per decision: real Lua (via the Rust library, or lupa); more control types (knob, vfader, envelopes, XY, textBox); the detail view; preset/Lua upload so the CLI and editor can write the simulator | per feature |
| 9 | Optional, separate GPL repository: the host port of firmware-mk1 as a reference oracle | out of this repository |

## 8. Open questions for the owner

1. **The GPL host port:** never, later as an oracle (a separate GPL-3.0 repository), or ask
   Electra (Martin Pavlas, who is actively working on firmware-mk1) whether they would take a
   host BSP upstream?
2. **Lua:** shim only (stdlib); a C ABI to the Rust library's `mlua` (no new dependency); or
   `lupa` (a new Python dependency)?
3. **Scope:** mk1 first and the mk2 as a flag (identical apart from `model`), with the Mini
   later? Or all three from the start?
4. **The reference `.epr`:** can you export the editor's `.epr` for `DrivenByMoss.eproj` (and
   the Mini's)? Without it the converter reproduces the editor's derivation of bounds and pages
   by inference.
5. **The Rust port's place:** port the Electra One controller as the simulator's first real
   client (PORTING_GUIDE order 3), or keep the simulator for the tablet work and Java tests
   first?
6. **The tablet direction:** should the tablet's surface format be the Electra `.epr` (with
   extensions), so that Electra presets run on the tablet, or a format of our own inspired by
   it?

## 9. Risks

- **The `.eproj` → `.epr` derivation** is undocumented: tile bounds, pages from `slotId`,
  `relative` messages, group mapping and the Mini's `layouts`. A wrong guess shows as wrong
  positions or controls that don't react. Mitigation: the reference export (question 4).
- **Silent offline.** DBM throws if the `DrivenByMoss` preset is missing from the list and stays
  offline unless `7E 02` names its slot. The simulator must preload it, and the monitor must
  make a missing handshake obvious.
- **Version and model checks** differ between the controllers (mk1/mk2 ≥ 3.4.3, Mini ≥ 5.0.4
  and `model:"mini"`). The defaults must satisfy them; tests cover both sides.
- **Bitwig port names:** virmidi ports don't match the discovery names, so selection is manual
  (as for the other simulators).
- **Long SysEx through ALSA:** fragmentation and the 64 KB buffer only matter for uploads, which
  DBM does not do.
- **Fidelity drift:** the native renderer won't match the firmware's pixels; the vendor
  documentation and the mk1 firmware may disagree on details (e.g. the `14 79` flag set). Where
  they disagree, follow the firmware for the mk1 and record the difference.
- **License hygiene:** keep firmware code out of the LGPL tree. Implement from the public docs
  and observed behaviour, and cite the facts used.
