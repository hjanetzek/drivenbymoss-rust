# Controller simulators

Desktop simulators of controllers for testing DrivenByMoss without the hardware: PySide6 + QML,
ALSA sequencer ports through ctypes, no other packages. They are also meant as the base of a
touchscreen controller (a tablet), so they may differ from the hardware where that helps.

| Model | Package | What it speaks |
| --- | --- | --- |
| Maschine Mk2 (`mk2`) | `maschine_simulator` | the MIDI of dbm-nihost / NI's driver with the DrivenByMoss template, the two MCU text displays, the button labels, the hello/Shift handshake |
| Maschine JAM (`jam`) | `maschine_simulator` | the template MIDI, the touch strips (setup, values, dual values), the level meters, NI's Shift SysEx and hello |
| Push 1/2/3 (`push1`, `push2`, `push3`) | `support/push_simulator` | the Push protocol, the display socket and the value side channel, see [its README](../push_simulator/README.md) |

How they are built, how to find a controller's protocol, and what tripped us up along the way:
[doc/CONTROLLER_SIMULATORS.md](../../doc/CONTROLLER_SIMULATORS.md).

Licensed under LGPLv3.

## Running them

```sh
support/controller_simulator/run.sh --model mk2
support/controller_simulator/run.sh --model jam --monitor
support/controller_simulator/run.sh --model push3        # hands over to support/push_simulator/run.sh
cd support/controller_simulator && python3 -m maschine_simulator --help
```

| Option (Maschine) | Meaning |
| --- | --- |
| `--model mk2\|jam` | the device (default `mk2`) |
| `--suffix TEXT` | appended to the client and port names. Default: none, or ` (sim)` if a client with the original name already exists, e.g. dbm-nihost's (it owns `Maschine MK2` and `Maschine Jam - 1` while it runs) |
| `--client-name`, `--input-port-name`, `--output-port-name` | override the ALSA names |
| `--connect PORT` | connect the simulator to another ALSA port (name or `client:port`, repeatable): our "In" port sends to it, it sends to our "Out" port. Use it for a `snd-virmidi` port for Bitwig |
| `--detached` | start "without the device": no hello, no answer to the query (as dbm-nihost without the device) |
| `--monitor` | open the MIDI monitor (F2 toggles it) |
| `--no-alsa` | no ports |
| `--quit-after S`, `--screenshot PNG`, `--size WxH` | for automated runs (`QT_QPA_PLATFORM=offscreen`) |

### With the Rust core in the reference host

`tools/dbm_host_alsa.py` (the Push simulator's runner, which also knows the Maschines) runs a
controller of the core in `bindings/python/dbm_host` with its simulated DAW in real time:

```sh
cargo build
support/controller_simulator/run.sh --model mk2 --monitor &
support/controller_simulator/tools/dbm_host_alsa.py --model mk2 --library target/debug/libdrivenbymoss.so
```

It reads from the simulator's `... In (sim)` port and writes to `... Out (sim)`. Without the
suffix it falls back to the plain names; `--suffix` picks a suffix. The Mk2's core sends the query
at its start, the simulator answers with the hello, and the core logs "dbm-nihost sends the Shift
button, STEP LEFT is Stop".

### With Bitwig

Bitwig on Linux opens only raw MIDI devices. Connect the simulator to a `snd-virmidi` device that
dbm-nihost doesn't use (dbm-nihost `--virmidi` takes the first ones):

```sh
sudo modprobe snd-virmidi midi_devs=4
aconnect -l | grep VirMIDI                                    # e.g. VirMIDI 5-2
support/controller_simulator/run.sh --model jam --connect "VirMIDI 5-2"
```

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

### Values side channel

There is none for the Maschines. The Push gets one (`DBMV` on its display socket) because its
endless encoders show their values only on the USB display. The Maschines have no such socket:
everything goes over MIDI. The Mk2's values are in its MCU text, which the simulator shows, and the
JAM's strips already show their values with the LEDs from the core. A Mk2 knob ring would need a
new transport (a SysEx with a 7-bit encoding of the `DBMV` payload, whose UTF-8 texts have bytes
≥ 0x80) and Rust changes in the Mk2 core. The shared `Encoder` component already draws a ring
whenever `encoderState` delivers a value, so a tablet controller can add it later.

## Code

| Path | Contents |
| --- | --- |
| `sim_common/alsa_seq.py` | ALSA sequencer client (ctypes): duplex or one-way ports, reader thread, `resolve`/`connect_both` |
| `sim_common/midi.py` | MIDI stream parser, monitor texts |
| `sim_common/qt_base.py` | `SimulatorBase`: what QML uses as `sim` (log, monitor, LED revision, value side channel state, status) |
| `sim_common/app.py` | the common options, `open_ports`, the run loop with `--quit-after`/`--screenshot` |
| `sim_common/qml/SimCommon/` | the QML module: `SimWindow` (surface, status bar, monitor), `Pad`, `LedButton`, `Encoder`, `StatusBar`, `MidiMonitor` |
| `maschine_simulator/device.py` | `MaschineDevice`: protocol and state without Qt |
| `maschine_simulator/layout.py` | the controls with template ids and positions, the port names |
| `maschine_simulator/ni.py` | NI's palette, the SysEx of NI and of dbm-nihost |
| `maschine_simulator/templates.py` | the template tables (generated) |
| `maschine_simulator/qt_bridge.py`, `qml/Main.qml` | the Qt object, the view (Mk2 displays, JAM strips and meters) |
| `tools/` | `maschine_feed.py` (a test host), `gen_maschine_templates.py`, `dbm_host_alsa.py` (runs the Push simulator's runner) |

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
- `test_maschine_alsa.py`: an ALSA loopback with one-way ports named with the suffix ` (test)`.
- `test_maschine_smoke.py`: both simulators offscreen with the suffix ` (smoke)`, fed by
  `maschine_feed.py` and grabbed to PNGs (into `MASCHINE_SIM_SCREENSHOTS` if set), checked for the
  fed colours; and that `run.sh` hands Push models to the Push simulator.
