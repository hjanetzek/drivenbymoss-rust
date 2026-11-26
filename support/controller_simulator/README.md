# Controller simulators

Desktop simulators of controllers for testing DrivenByMoss without the hardware: PySide6 + QML,
ALSA sequencer ports through ctypes, no other packages. They are also meant as the base of a
touchscreen controller (a tablet), so they may differ from the hardware where that helps.

| Model | Package | What it speaks |
| --- | --- | --- |
| Maschine Mk2 (`mk2`) | `maschine_simulator` | the MIDI of dbm-nihost / NI's driver with the DrivenByMoss template, the two MCU text displays, the button labels, the hello/Shift handshake |
| Maschine JAM (`jam`) | `maschine_simulator` | the template MIDI, the touch strips (setup, values, dual values), the level meters, NI's Shift SysEx and hello |
| Maschine Mk3 (`mk3`) | `maschine_simulator` | what NI's driver sends with the DrivenByMoss template: 8 touch-sensitive knobs, the 4-D encoder (turn, touch, push, tilt), the touch strip, NI's Shift SysEx and "return from host", the MCU text displays |
| Maschine Mikro Mk3 (`mikromk3`) | `maschine_simulator` | the same for the Mikro: the navigation encoder, the touch strip with touch, no knobs, displays, group buttons or Shift |
| Akai APC40 mkII (`apc40mk2`) | `midi_simulator` | Ableton Live mode: RGB clip pads and scene buttons with the animation channels, single LEDs, the knobs with their LED rings, faders, crossfader |
| Novation Launchpad Pro Mk3 (`lppromk3`) | `midi_simulator` | programmer mode: pads with aftertouch, all buttons, LEDs by note/CC with flashing and pulsing, the SysEx LED update with RGB, the logo |
| Novation Launch Control XL (`lcxl`) | `midi_simulator` | the 16 templates with the template-change handshake and a template selector, 24 knobs with their two-colour LEDs, 8 faders, 24 buttons with LEDs, flashing, the SysEx LED update |
| Push 1/2/3 (`push1`, `push2`, `push3`) | `support/push_simulator` | the Push protocol, the display socket and the value side channel, see [its README](../push_simulator/README.md) |

How they are built, how to find a controller's protocol, and what tripped us up along the way:
[doc/CONTROLLER_SIMULATORS.md](../../doc/CONTROLLER_SIMULATORS.md).

Licensed under LGPLv3.

## Running them

With the Rust core and the help boxes in one go (Maschine models; builds the library if missing,
both stop together):

```sh
support/controller_simulator/run_with_core.sh                 # JAM; the real one if plugged in (mirrored in the simulator)
support/controller_simulator/run_with_core.sh --sim           # the core drives the simulator
support/controller_simulator/run_with_core.sh --model mk2 --no-help
support/controller_simulator/run_with_core.sh --model push3   # the Push 3 simulator (--sim only), its display
```

The simulators alone:

```sh
support/controller_simulator/run.sh --model mk2
support/controller_simulator/run.sh --model jam --monitor
support/controller_simulator/run.sh --model mk3
support/controller_simulator/run.sh --model mikromk3
support/controller_simulator/run.sh --model apc40mk2
support/controller_simulator/run.sh --model lppromk3
support/controller_simulator/run.sh --model lcxl
support/controller_simulator/run.sh --model push3        # hands over to support/push_simulator/run.sh
cd support/controller_simulator && python3 -m maschine_simulator --help
cd support/controller_simulator && python3 -m midi_simulator --help
```

`support/push_simulator/run.sh` hands all non-Push models over to this `run.sh`.

| Option (Maschine) | Meaning |
| --- | --- |
| `--model mk2\|jam` | the device (default `mk2`) |
| `--suffix TEXT` | appended to the client and port names. Default: none, or ` (sim)` if a client with the original name already exists, e.g. dbm-nihost's (it owns `Maschine MK2` and `Maschine Jam - 1` while it runs) |
| `--client-name`, `--input-port-name`, `--output-port-name` | override the ALSA names |
| `--connect PORT` | connect the simulator to another ALSA port (name or `client:port`, repeatable): our "In" port sends to it, it sends to our "Out" port. Use it for a `snd-virmidi` port for Bitwig |
| `--detached` | start "without the device": no hello, no answer to the query (as dbm-nihost without the device) |
| `--send-disabled` | Mk3: also send the controls the NI template disables (the touch strip touch, as CC 2, which the core binds) |
| `--monitor` | open the MIDI monitor (F2 toggles it) |
| `--no-alsa` | no ports |
| `--quit-after S`, `--screenshot PNG`, `--size WxH` | for automated runs (`QT_QPA_PLATFORM=offscreen`) |
| `--help-socket PATH`, `--show-help`, `--hide-global`, `--help-file JSONL` | the help boxes (below) |
| `--tap LABEL`, `--latch LABEL`, `--action-delay S` | for automated runs: tap / hold a button after S seconds (default 1.5) |

| Option (APC40 mkII, Launchpad Pro Mk3, Launch Control XL) | Meaning |
| --- | --- |
| `--model apc40mk2\|lppromk3\|lcxl` | the device (default `apc40mk2`) |
| `--suffix TEXT` | appended to the client and port names, e.g. ` (sim)` next to a real device |
| `--port-style jack\|numbered` | Launchpad: `Launchpad Pro MK3 LPProMK3 MIDI`/`DIN`/`DAW` (current kernels) or `Launchpad Pro MK3 MIDI 1`/`2`/`3`; Launch Control XL: `Launch Control XL Launch Contro` or `Launch Control XL MIDI 1` |
| `--port-name`, `--client-name` | override the name of the port the core uses, of the client |
| `--channel-pressure` | send channel instead of poly aftertouch |
| `--connect PORT`, `--monitor`, `--no-alsa`, `--quit-after`, `--screenshot`, `--size` | as above |

### With the Rust core in the reference host

`tools/dbm_host_alsa.py` (the Push simulator's runner, which also knows the Maschines) runs a
controller of the core in `bindings/python/dbm_host` with its simulated DAW in real time:

```sh
cargo build
support/controller_simulator/run.sh --model mk2 --monitor &
support/controller_simulator/tools/dbm_host_alsa.py --model mk2 --library target/debug/libdrivenbymoss.so
```

For the APC40 mkII, the Launchpad Pro Mk3 and the Launch Control XL it connects to their duplex
port (`APC40 mkII MIDI 1`, `Launchpad Pro MK3 LPProMK3 MIDI`, `Launch Control XL Launch Contro`,
first with ` (sim)`):

```sh
support/controller_simulator/run.sh --model apc40mk2 --suffix " (sim)" &
support/controller_simulator/tools/dbm_host_alsa.py --model apc40mk2 --library target/debug/libdrivenbymoss.so
```

For the Maschines it reads from the simulator's `... In (sim)` port and writes to `... Out (sim)`. Without the
suffix it falls back to the plain names; `--suffix` picks a suffix. The Mk2's core sends the query
at its start, the simulator answers with the hello, and the core logs "dbm-nihost sends the Shift
button, STEP LEFT is Stop".

### The help boxes

With the Rust core in the reference host the Maschine simulators show what each button does in
the current state (view, mode, held Shift / Select): key **H** or the "Help" checkbox in the status
bar. The texts come from the core (`dbm_take_control_help`, doc/CONTROLLER_HELP_PLAN.md); so far
the JAM, the Mk2 and the Push 1/2/3 have them (the Push simulator: support/push_simulator). A click on a box, or pressing its button, shows the full text.

```sh
support/controller_simulator/run.sh --model jam --show-help &
support/controller_simulator/tools/dbm_host_alsa.py --model jam --library target/debug/libdrivenbymoss.so
```

The simulator listens on `--help-socket`, else `$DBM_SIM_HELP_SOCKET`, else
`$XDG_RUNTIME_DIR/dbm-sim-help.sock` (not if another simulator listens there); the host writes to
`--help-socket`, `$DBM_SIM_HELP_SOCKET`, or the default path if it exists (`--no-help`: none).
`--help-file` shows saved entries without a host (tests, screenshots).

### The mirror of a real device

When the core drives a real Maschine JAM or Mk2 in-process (`run_with_core.sh --hw`, the default
while it is plugged in), the simulator mirrors it over the help socket: the LEDs, the JAM's touch
strips (setup, values, dual values) and level meters, the Mk2's displays, and the presses,
touches and moves on the hardware as a white frame around the control (its help box expands).
A simulator which connects later gets the complete state (the core refreshes it). The status bar
says "Mirror: Maschine JAM (hardware), clicks act on the core": clicks in the simulator act on the
core as the device's input, so it works as a second surface of the same instance;
`--no-mirror-input` ignores them. Details: doc/CONTROLLER_HELP_PLAN.md §4.

The boxes are coloured by what their text depends on: grey global, blue the view, orange the mode
(strips, Page ◂▸), purple a held modifier (Shift, Select, ...); the status bar shows the legend with
the active view and mode. Key **G** (the "Globals" checkbox, `--hide-global`) hides the global
boxes: what remains changes with the view, the mode or the modifiers. A row of buttons with one
function (the JAM's scenes, the group buttons, the Mk2's buttons above the displays and the
encoder's ◂ ▸) is one box with a bracket along the row; the pads, the JAM's strips and the Mk2's
knobs are one box each (their details in the full text).

Each control's box goes to the side of the panel it is on (left / right of the pads, else above or
below them), or the side its layout names (`help_side`). The Mk2: the upper left block and the
master section left, Control, Step and the buttons above the displays on top, the knobs and the
column left of the pads right (their lines pass under the pads instead of across the left half's),
the transport's upper row left, its lower row and the pads below. The columns and rows sit a few
millimetres from the panel whatever the window's size. A click on a box (or its control's press)
shows the full text in a panel over the pads; a click on the panel closes it. Neighbours whose lines would cross change places (columns and
the staggered rows), and the rows above / below the panel use the corners the columns leave free.

### The layouts

`maschine_simulator/layout.py` places the controls in surface units of about a millimetre. The
JAM (`_jam()`, 370 × 312) and the Mk2 (`_mk2()`, 321 × 262) are drawn from top view photos of the
devices (the photo's pixels times 0.264 / 0.172 mm; the logo strip cut off), with the main labels
of the hardware; NI's small secondary labels (the Maschine software's Shift functions: MIDI, SAVE,
TAP, UNDO, ...) are left out, the help boxes take that role. The Mk2's arrows under BROWSE are
drawn but send nothing (they are not in the template in the MIDI mode); its encoder push is a
small button under the encoder. The Mk3 and the Mikro Mk3 are still the own drawings.

### With Bitwig

Bitwig on Linux opens only raw MIDI devices. Connect the simulator to a `snd-virmidi` device that
dbm-nihost doesn't use (dbm-nihost `--virmidi` takes the first ones):

```sh
sudo modprobe snd-virmidi midi_devs=4
aconnect -l | grep VirMIDI                                    # e.g. VirMIDI 5-2
support/controller_simulator/run.sh --model jam --connect "VirMIDI 5-2"
```

The APC40 mkII and the Launchpad Pro Mk3 go the same way: `--connect "VirMIDI 5-2"`, then add the
Akai APC40 mkII or Novation Launchpad Pro Mk3 controller in Bitwig. Both the Java DrivenByMoss and
the Rust extension have these controllers; neither has been tried with these simulators yet.

In Bitwig, add the Maschine JAM (or Mk2) controller of the Rust extension and select "Virtual Raw
MIDI 3/1" as input and output. Bitwig numbers the devices from 1, so `VirMIDI 5-2` is the third.
The simulator sends its hello after connecting. A Bitwig that was started earlier then refreshes
the JAM, and the Mk2 core switches to the real Shift button. The Java DrivenByMoss refuses the
Maschines on Linux unless its Linux check is removed, and it knows neither the hello nor the Mk2's
Shift SysEx (it logs them as unhandled and uses STEP LEFT as Shift).

### Zynthian

The zynthian Maschine drivers run either in-process (dbm-nihost's library over hidraw, no ports)
or through dbm-nihost's ports, which PipeWire's MIDI bridge shows as `Maschine Jam - 1 IN 1` /
`Maschine MK2 IN 1`. On the port path they wait for the hello and stop at gone. The simulator
speaks the same handshake, so it can stand in for dbm-nihost there. It needs the original names
(`--suffix ""`, with dbm-nihost stopped), because the device IDs come from the client name.

## The Maschine protocol

**Button combinations without a touchscreen:** a right click latches a button (amber frame): it stays pressed until the next click on it. Example on the JAM: right click Select, click SONG, click Select again (the surface link mode).

| Direction | Messages |
| --- | --- |
| Buttons, pads → DAW | the template's CC or note on channel 1 (on 127, off 0; a note off as `80 nn 00`). Mk2: the pads are notes 36..51 (pad 1 at the bottom left, velocity from the click height, right button or wheel: poly aftertouch, the template's "Pressure"), the 7th display button is note 28. JAM: the 8×8 pads are notes 92 (top left) .. 43 (bottom right) |
| Encoders → DAW | relative CC in two's complement (1 = +1, 127 = -1): Mk2 knobs CC 70..77, the master encoder CC 7 (press: CC 8); JAM knob CC 57 (touch CC 59, press CC 58) |
| JAM strips → DAW | touch CC 20..27 (127/0), then the value CC 8..15 (0..127) |
| Shift → DAW | Mk2: `F0 7D 44 42 4D 12 01/00 F7` (DrivenByMoss SysEx); JAM: NI's `F0 00 21 09 15 00 4D 50 00 01 4D 01/00 F7`. The simulator lights it itself, like dbm-nihost |
| DAW → LEDs | on channel 1 (Mk2; the JAM ignores the channel): pads by note, groups (Mk2 CC 100..107, JAM CC 70..77) and JAM scenes (CC 80..87) by CC, value = NI's palette index (hue × 4 + level; 72..79 greys and white). The other buttons are lit with their own CC (or note) as a brightness 0..127 |
| DAW → Mk2 displays | MCU text `F0 00 00 66 <dev> 12 <position> <chars> F7` (2 lines × 56; each display shows 28 characters of both lines), button labels `F0 7D 44 42 4D 10 <position> <56 chars> F7` above them |
| DAW → JAM strips | NI's strip setup `<header> 05 <type colour> × 8 F7` (single, dot, pan, dual), dual values `<header> 04 <8 values> F7`, the values CC 8..15; drawn as dbm-nihost draws them (11 LEDs) |
| DAW → JAM meters | CC 64 (left) and 65 (right), 8 LEDs each. dbm-nihost doesn't show them yet |
| Handshake | on start the simulator is "attached": Mk2 hello `F0 7D 44 42 4D 11 01 F7` (Shift is sent), JAM hello NI's "return from host" `F0 00 21 09 15 00 4D 50 00 01 46 01 F7`. The query `F0 7D 44 42 4D 11 F7` gets the hello (no answer with `--detached`), and on quit the simulator sends gone `F0 7D 44 42 4D 13 F7` |

The ports are as dbm-nihost and NI's driver create them: one client (`Maschine MK2`,
`Maschine Jam - 1`) with a read-only port the DAW reads from (`Maschine MK2 In`,
`Maschine Jam - 1 Input`) and a write-only port it writes to (`Maschine MK2 Out`,
`Maschine Jam - 1 Output`).

The tables come from dbm-nihost (`support/linux/ni_host/src/template_map.h`, generated from the
DrivenByMoss NI templates). `tools/gen_maschine_templates.py` turns them into
`maschine_simulator/templates.py`, and a test checks that the two agree.

### The Mk3 and the Mikro Mk3

There is no dbm-nihost for them; the simulator emulates NI's driver (Windows/macOS) directly.

| Direction | Messages |
| --- | --- |
| Port | one duplex port named as NI's driver names its in- and output on Windows: `Maschine MK3 Ctrl MIDI` (client `Maschine MK3`), `Maschine Mikro MK3`. These are the core's (and Java's) discovery names for Linux too. On macOS NI's driver names them `Maschine MK3 Virtual Input`/`Output` |
| Controls → DAW | the MIDI of the DrivenByMoss NI templates (`resources/Maschine Mk3/DrivenByMoss.ncm3`, `resources/Maschine Mikro Mk3/DrivenByMoss.ncmm3` of the Java DrivenByMoss): the same CCs as the core's `surface.rs` (the Mk2's template was made to match them). Mk3: knobs CC 70..77 (relative) with touch CC 60..67, the 4-D encoder CC 7 (relative), touch CC 9, push CC 8, tilt up/right/down/left CC 30..33, the touch strip CC 1, Button 7 a note (28). Mikro: the navigation encoder CC 7/8/9, the touch strip CC 1 with touch CC 2 |
| Touch strip touch (Mk3) | **disabled** in the Mk3 template (and as poly aftertouch on note 2, while the core binds CC 2): NI's driver sends no touch for the Mk3's strip. The simulator does the same; `--send-disabled` sends CC 2 |
| Shift | Mk3: NI's SysEx `F0 00 21 09 16 00 4D 50 00 01 4D 01/00 F7` (Maschine+ 18 20); the Mikro has none in MIDI mode, the core uses Stop as Shift |
| Hello | NI's "return from host" (`... 46 01 F7`) on start, as NI's driver sends it when it enters MIDI mode; no "gone", and dbm-nihost's query is ignored |
| DAW → LEDs | pads (notes 36..51), groups (Mk3, CC 100..107): NI's palette; the other buttons with their own CC as a brightness; the touch strip's LEDs with CC 1 (a bar) |
| DAW → displays (Mk3) | the MCU text (2 × 56 characters, half of each line on a display), as for the Mk2; no button labels (they are dbm-nihost's) |

`tools/gen_maschine_templates.py` reads these two templates from a DrivenByMoss checkout (the
parser of `support/linux/ni_host/tools/gen_template_map.py`, plus the `<disabled/>` flag), and
a test keeps them in sync with the checkout and with the core's CC constants.

### Values side channel

There is none for the Maschines. The Push gets one (`DBMV` on its display socket) because its
endless encoders show their values only on the USB display. The Maschines have no such socket:
everything goes over MIDI. The Mk2's values are in its MCU text, which the simulator shows, and the
JAM's strips already show their values with the LEDs from the core. A Mk2 knob ring would need a
new transport (a SysEx with a 7-bit encoding of the `DBMV` payload, whose UTF-8 texts have bytes
≥ 0x80) and Rust changes in the Mk2 core. The shared `Encoder` component already draws a ring
whenever `encoderState` delivers a value, so a tablet controller can add it later.

## The APC40 mkII protocol

| Direction | Messages |
| --- | --- |
| Startup (DAW → device) | the mode SysEx `F0 47 7F 29 60 00 04 41 08 02 01 F7` (0x41: Ableton Live mode); the simulator shows the mode |
| Buttons → DAW | notes on 127, released with note off. Track buttons (clip stop 0x34, select 0x33, A/B 0x42, activator 0x32, solo 0x31, arm 0x30) on the track's channel 1..8, the others on channel 1; the footswitch is CC 0x40 |
| Pads → DAW | notes 0..39 on channel 1 (0 bottom left, 39 top right) with velocity; the hardware has no aftertouch |
| Knobs → DAW | track knobs CC 0x30..0x37 and device knobs CC 0x10..0x17 absolute (0..127, continuing from the value the DAW set); tempo CC 0x0D and cue level CC 0x2F relative (1 = +1, 127 = -1) |
| Faders → DAW | CC 7 on the track's channel, master CC 14, crossfader CC 15 |
| DAW → LEDs | pads and scene buttons (0x52..0x56): velocity = index of the 128-colour palette, channel 1 steady, 2..6 one-shot, 7..11 pulsing, 12..16 blinking (24th .. half note at 120 BPM; the core uses 11 and 13). Single LEDs: 0 off, 1 on, 2 blinking; A/B: 1 A (yellow), 2 B (orange) |
| DAW → rings | the knob's CC = ring value (also the knob's new value), CC 0x38..0x3F (track) / 0x18..0x1F (device) = ring mode: 0 off, 1 single (a short arc), 2 volume (from the left), 3 pan (from the top); drawn by the shared `Encoder` |
| Identity | `F0 7E 7F 06 01 F7` → `F0 7E 00 06 02 47 29 00 19 00 00 01 00 00 F7` (the core doesn't ask) |

## The Launch Control XL protocol

| Direction | Messages |
| --- | --- |
| Port | `Launch Control XL Launch Contro`: the kernel's port name (the USB jack name, cut to 31 characters), one of the core's two Linux discovery names (the other is `Launch Control XL`) |
| Templates | 16 templates: user 1-8 on MIDI channels 1-8, factory 1-8 on channels 9-16. Every control sends on the channel of the active template, and the LEDs are set on it; each template keeps its own LEDs |
| Startup (DAW → device) | the template selection `F0 00 20 29 02 11 77 08 F7` (factory template 1). The device switches and answers with the same message, and only then the core binds its controls to channel 9 and lights the LEDs. Factory templates 1-5 are the core's Mixer, Equalizer, Layer Mixer, Drum Sequencer and Note Sequencer; the others are a dummy mode |
| Template change (device → DAW) | the same message, sent when the template changes on the device; the simulator's selector row (U1-U8, F1-F8) does this. On the device: Templates + a Track Focus/Control button |
| Knobs → DAW | absolute CC: Send A 13..20, Send B 29..36, Pan 49..56; the indicator shows the knob's position (no ring, the device has none) |
| Faders → DAW | CC 77..84 |
| Buttons → DAW | Track Focus notes 41..44, 57..60, Track Control notes 73..76, 89..92, Device/Mute/Solo/Record Arm notes 105..108 (on 127, off: note off 0); Send ▲/▼ and Track ◀/▶ CC 104..107 (127/0) |
| DAW → LEDs | two-colour red/green: velocity = red (bits 0-1) + flags (bits 2-3: 4 copy, 8 clear) + 16 × green (bits 4-5), each 0..3, 12 = off. Knob LEDs: notes 13 + row + 16 × column (first row 13, 29 .. 125); the buttons: their own note or CC. CC 0 = 0 resets the template's LEDs; CC 0 = 0x28 switches flashing on, and LEDs written with the flags 8 (clear without copy) flash |
| DAW → LED SysEx | `F0 00 20 29 02 11 78 <template> <index> <value> F7`: index 0-23 knobs (by row), 24-31 Track Focus, 32-39 Track Control, 40-43 Device/Mute/Solo/Record Arm, 44-47 ▲▼◀▶ (the core doesn't use it) |
| Identity | `F0 7E 00 06 02 00 20 29 61 00 00 00 00 00 01 00 F7` (the core doesn't ask) |

## The Launchpad Pro Mk3 protocol

| Direction | Messages |
| --- | --- |
| Ports | `LPProMK3 MIDI` (the core's: Linux discovery `Launchpad Pro MK3 LPProMK3 MIDI` or `Launchpad Pro MK3 MIDI 1`), `LPProMK3 DIN` and `LPProMK3 DAW` (logged only) |
| Startup (DAW → device) | the identity request, `F0 00 20 29 02 0E 10 00 F7` (DAW mode off, standalone), `... 0E 01 F7` (programmer mode); on exit `... 0E 00 F7`. Layout select `00 <layout>` is shown too |
| Pads → DAW | notes 11..88 (row × 10 + column, 11 bottom left) with velocity, released with velocity 0; poly aftertouch (or channel aftertouch with `--channel-pressure`) |
| Buttons → DAW | CC 127/0: top row 91..98 (◀ ▶ Session Note Chord Custom Sequencer Projects), left column 90 (Shift), 80, 70, 60 .. 10, scenes 89..19, the bottom rows 101..108 (track select) and 1..8 (Record Arm .. Stop Clip) |
| DAW → LEDs | note or CC with the palette index: channel 1 static, 2 flashing (between the static colour and the new one), 3 pulsing |
| DAW → LED SysEx | `F0 00 20 29 02 0E 03 <entries> F7`, entries type 0 static (index, colour), 1 flashing (index, colour B, colour A), 2 pulsing (index, colour), 3 RGB (index, r, g, b 0..127); index 99 is the logo |
| Identity | `F0 7E 00 06 02 00 20 29 23 01 00 00 00 05 03 01 F7`; the core logs "Firmware version: 531" |

Both use the 128-colour velocity palette shared by Push 1, the APC40 mkII and the Launchpads
(`sim_common/palettes.py`, generated from Java's `PushColorManager.DEFAULT_PALETTE` by
`tools/gen_palettes.py`).

## Code

| Path | Contents |
| --- | --- |
| `sim_common/alsa_seq.py` | ALSA sequencer client (ctypes): duplex or one-way ports, reader thread, `resolve`/`connect_both` |
| `sim_common/midi.py` | MIDI stream parser, monitor texts |
| `sim_common/qt_base.py` | `SimulatorBase`: what QML uses as `sim` (log, monitor, LED revision, value side channel state, status) |
| `sim_common/app.py` | the common options, `open_ports`, the run loop with `--quit-after`/`--screenshot` |
| `sim_common/qml/SimCommon/` | the QML module: `SimWindow` (surface, status bar, monitor), `Pad`, `LedButton`, `Encoder` (value ring or LED ring, "single" mode as a short arc), `Fader`, `StatusBar`, `MidiMonitor` |
| `sim_common/palettes.py` | the 128-colour velocity palette (generated) |
| `sim_common/help.py`, `qml/SimCommon/HelpLayer.qml`, `HelpBox.qml` | the help boxes: the socket, the model, the layout and drawing |
| `maschine_simulator/device.py` | `MaschineDevice`: protocol and state without Qt |
| `maschine_simulator/layout.py` | the controls with template ids and positions, the port names |
| `maschine_simulator/ni.py` | NI's palette, the SysEx of NI and of dbm-nihost |
| `maschine_simulator/templates.py` | the template tables (generated) |
| `maschine_simulator/qt_bridge.py`, `qml/Main.qml` | the Qt object, the view (Mk2 displays, JAM strips and meters) |
| `midi_simulator/base.py` | `GridDevice`: controls with their message and LED kind, LED animations, absolute/relative encoders with rings, faders, without Qt |
| `midi_simulator/apc40mk2.py`, `lppromk3.py`, `lcxl.py` | the devices: controls and layout, LED addressing, animation channels, SysEx, identity |
| `midi_simulator/qt_bridge.py`, `qml/Main.qml` | the Qt object and the view (only shared components) |
| `tools/` | `maschine_feed.py`, `midi_feed.py` (test hosts), `gen_maschine_templates.py`, `gen_palettes.py`, `dbm_host_alsa.py` (runs the Push simulator's runner) |

The Push simulator uses `sim_common` too. Its `alsa_seq` and `midi` modules re-export the shared
ones, and its package puts `support/controller_simulator` on `sys.path`.

## Tests

```sh
cd support/controller_simulator
QT_QPA_PLATFORM=offscreen python3 -m unittest discover -s tests -v
```

- `test_maschine.py`: controls → template MIDI (buttons, the Mk2's note button, Shift, encoders,
  pads with pressure, JAM strips), LED feedback (palette, brightness, channel rules), the Mk2
  displays, the JAM strip types and meters, the handshake, the layout, and that `templates.py`
  agrees with dbm-nihost's `template_map.h`.
  The Mk3 and the Mikro Mk3: the return from host, NI's Shift, the knobs with touch, the 4-D and
  navigation encoders, the touch strip (with and without the disabled touch), the LEDs and the
  display, and the sync with the NI templates and the core's CC constants.
- `test_maschine_core.py`: the real core in-process for all four models (dbm_host's bridge, no
  ALSA): everything the core sends is understood, every control works without a core error
  (skipped without the built library).
- `test_maschine_alsa.py`: an ALSA loopback with one-way ports (Mk3: one duplex port) named with
  the suffix ` (test)`.
- `test_maschine_smoke.py`: the four Maschine simulators offscreen with the suffix ` (smoke)`, fed by
  `maschine_feed.py` and grabbed to PNGs (into `MASCHINE_SIM_SCREENSHOTS` if set), checked for the
  fed colours; and that `run.sh` hands Push models to the Push simulator.
- `test_midi_simulators.py`: the APC40 mkII and Launchpad Pro Mk3 protocol (controls, LEDs and
  animations, rings, SysEx, identity), the sync with the Rust sources (the APC constants in
  `surface.rs`, the Launchpad Pro Mk3 buttons, header, program mode and port names in
  `definition.rs`) and the Java palette, and a run of the real core in-process: everything the
  core sends must be understood, and every control is operated without a core error (skipped
  without `target/debug/libdrivenbymoss.so`).
  The Launch Control XL: the template handshake and selector, the controls on the template's
  channel, the two-colour LEDs per template, the SysEx LED update, flashing and reset, and the sync
  with `src/controller/launchcontrol` (the constants, the template header, the knob LED formula,
  the UUID, the discovery name and the startup template).
- `test_midi_alsa.py`: an ALSA loopback with ` (test)` names.
- `test_help.py`: the help boxes: the layout along a side, the crossing lines changing places, the
  model (sides, expanding on a press), the socket, the JAM's and the Mk2's mapping of the core's
  entries (every entry of the real core in the states every button makes goes to a control; the
  Mk2's sides, rows and areas; its layout keeps the template ids), the boxes drawn offscreen, the
  host's writer.
- `test_midi_smoke.py`: all three offscreen with ` (smoke)` names, started through the Push `run.sh`,
  fed by `midi_feed.py`, grabbed to PNGs (into `MIDI_SIM_SCREENSHOTS`), checked for the fed colours
  and the LED rings.
