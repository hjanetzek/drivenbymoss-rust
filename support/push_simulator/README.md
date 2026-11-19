# Push simulator

A desktop simulator of Ableton Push 2 and Push 3 (and, simpler, Push 1) for testing the
DrivenByMoss Push controller without the hardware. It is a PySide6 + QML application. It looks
like the Push layout (an own drawing, not Ableton's artwork) and speaks the device side of the
Push protocol over ALSA sequencer ports, with the names of the real device. The Push 2/3 display
frames come in over a UNIX socket.

Licensed under LGPLv3.

## Requirements

- Linux with the ALSA sequencer (`/dev/snd/seq`).
- Python 3.11 or newer and PySide6 (tested with 6.12). There are no other packages: MIDI goes
  through `libasound.so.2` with ctypes.

## Running it

```sh
support/push_simulator/run.sh                   # Push 2
support/push_simulator/run.sh --model push3
support/push_simulator/run.sh --model push1     # Push 1: text display, no socket
cd support/push_simulator && python3 -m push_simulator --help
```

| Option | Meaning |
| --- | --- |
| `--model push2\|push3\|push1` | the device (default `push2`) |
| `--display-socket PATH` | the display socket (default: `$DBM_PUSH_DISPLAY_SOCKET`, else `$XDG_RUNTIME_DIR/dbm-push-display.sock`) |
| `--port-style jack\|numbered` | the port names, see below |
| `--client-name`, `--live-port-name`, `--user-port-name` | override the ALSA names |
| `--connect PORT` | connect the port the controls send on to another ALSA port, both ways (name or `client:port`, repeatable), e.g. a `snd-virmidi` port for Bitwig |
| `--monitor` | open the MIDI monitor (F2 toggles it) |
| `--no-alsa`, `--no-display` | without MIDI ports or socket |
| `--quit-after S`, `--screenshot PNG`, `--size WxH` | for automated runs (`QT_QPA_PLATFORM=offscreen`) |

### With the Rust Push port in the reference host (`dbm_host`)

`tools/dbm_host_alsa.py` runs a controller of the Rust core in `bindings/python/dbm_host` (with
its simulated DAW) in real time. It connects its MIDI port 0 to the simulator and writes the
display data (`Backend::usb_bulk_out`) to the socket:

```sh
support/push_simulator/run.sh --monitor &
support/push_simulator/tools/dbm_host_alsa.py --model push2 \
    --bindings ~/bastel/github/drivenbymoss-rust-push/bindings/python \
    --library  ~/bastel/github/drivenbymoss-rust-push/target/debug/libdrivenbymoss.so
```

The display only appears once the Python binding passes the core's `usb_bulk_out` callback to the
backend. The runner's bridge has a method `usb_bulk_out(device_index, endpoint, data)` for this.
`--controller UUID` runs any other controller, which is useful for testing the runner itself.

### With Bitwig (Java DrivenByMoss or the Rust extension)

Bitwig on Linux only opens raw MIDI devices, not sequencer ports (see
`support/linux/ni_host/README.md`). Bridge the simulator through `snd-virmidi`:

```sh
sudo modprobe snd-virmidi midi_devs=1
support/push_simulator/run.sh --connect "VirMIDI 2-0"     # the name aconnect -l shows
```

In Bitwig, add the Push 2/3 controller and select the virtual raw MIDI device as input and
output. Bitwig numbers the virtual raw MIDI devices from 1: `VirMIDI 5-3` is "Virtual Raw MIDI
4/1" in Bitwig.

**The display with the Rust extension:** start Bitwig with the socket of the simulator in its
environment:

```sh
export DBM_PUSH_DISPLAY_SOCKET=$XDG_RUNTIME_DIR/dbm-push-display.sock
support/push_simulator/run.sh --model push2 --connect "VirMIDI 5-3" &
bitwig-studio
```

The extension then writes the display frames to that socket instead of the USB device (the exact
bytes of the bulk writes, as the Python hosts do), and its definition claims no USB device, so
the controller runs without a Push attached. Without a listener it drops the frames and tries to
connect again once a second. The variable must be set when Bitwig starts (the definition reads it
when Bitwig loads the extension). The Java DrivenByMoss always writes to the USB device: with it
you only get the MIDI side.

### Feeding it by hand

`tools/demo_feed.py` behaves like a host. It sends an identity request and prints the reply, sets
pad and button LEDs (one blinking pad, one pulsing pad), puts the touch strip in bar mode and
sends a few test frames over the socket (with some garbage first, to test resynchronisation).

## Protocol

| Direction | Messages |
| --- | --- |
| Pads → host | notes 36..99 (36 = bottom left) on channel 1. The velocity comes from the click height (Shift: 127, Ctrl: 40). Aftertouch: hold the right button and move up and down, or use the wheel while a pad is held. It is sent as channel pressure, or as poly pressure after SysEx `1E 01`. Push 3 MPE (`1E 02`): one channel per note (2..16), horizontal drag = pitch bend, vertical = CC 74, pressure = channel pressure |
| Buttons → host | CC on channel 1, 127 pressed, 0 released (the CCs of `PushControlSurface`) |
| Encoders → host | relative CC (1..63 clockwise, 127..65 counter-clockwise) from the mouse wheel (Ctrl: ×8) or a vertical drag. The touch note (0..7 the 8 encoders, 8 master, 9/10 the small ones) is velocity 127/0, on press, drag and wheel (released 0.5 s after the last wheel step). Push 3: jog wheel CC 70, press CC 94, tilt CC 93/95 |
| Touch strip → host | touch note 12, then pitch bend (or CC 1 with config bit 2). It returns to the centre or the bottom if the configuration asks for it |
| Host → pad LEDs | note on/off: velocity = palette index, channel 1 = steady. Channels 2-6 are a one-shot transition, 7-11 pulsing and 12-16 blinking (24th, 16th, 8th, quarter, half note, at the MIDI clock tempo, else 120 BPM), from the channel 1 colour to the new one |
| Host → button LEDs | CC, the same channels. RGB buttons (the rows above and below the display, scenes, Mute, Solo, Stop Clip, Automate, Record, Play; on Push 3 also Master, Lock and Capture) use the palette RGB. White buttons use the palette's W component (by default `index * 2`) |
| Host → touch strip | SysEx `17` configuration (host or Push controls the LEDs, bar or point, from the centre, auto-return), pitch bend on channel 1 = LED value, SysEx `19` = the 31 LEDs directly |
| SysEx (`F0 00 21 1D 01 01 …`) | `03` set palette entry, `04` get entry (reply as `ColorPaletteEntry` expects), `05` reapply, `06/07` LED brightness, `08/09` display brightness, `0A/0B` MIDI mode (Live/User/Dual: the port the controls send on), `14/15` white balance, `17/18` strip configuration, `19` strip LEDs, `1A` statistics, `1B/1C` pad parameters, `1E/1F` aftertouch mode, `20/21` velocity curve. Push 3: `26 07 xx` (MPE: per-pad pitch bend, in-tune location and width, slide height), `37 xx` (pedals, preamps, outputs), `43` pad curve |
| Identity | `F0 7E 7F 06 01 F7` → `F0 7E 01 06 02 00 21 1D 67 32 02 00 <major minor build(2) serial(5) board> F7` (Push 3: member `03 00`), the layout `PushControlSurface.handleDeviceInquiryResponse` reads. Push 1: a 35-byte reply |
| Push 1 | text display `F0 47 7F 15 18..1B 00 45 00 <68 chars> F7`, clear line `1C..1F`, ribbon mode `63`, pressure mode `5C`, pad sensitivity `5D`, Push 1 two-colour and single-colour LED values |
| MIDI clock | `F8` sets the animation tempo |

Everything received and sent is shown in the MIDI monitor, with the decoded SysEx.

### Ports

The simulator creates one ALSA client with two duplex ports:

| Model | Client | Live port | User port |
| --- | --- | --- | --- |
| Push 2 | `Ableton Push 2` | `Ableton Push 2 Live Port` | `Ableton Push 2 User Port` |
| Push 3 | `Ableton Push 3` | `Ableton Push 3 Live Port` | `Ableton Push 3 User Port` |
| Push 1 | `Ableton Push` | `Ableton Push Live Port` | `Ableton Push User Port` |

These are the names current kernels give the real device: its USB MIDI jack names. They are also
DrivenByMoss' Linux discovery name for Push 3 and its macOS names. `--port-style numbered`
produces the older kernel naming (`Ableton Push 2 MIDI 1` / `MIDI 2`). Bitwig's Linux discovery
names for Push 2 (`A2`, `A2 MIDI n`, `A2 [hw:n,0,0]`) are raw MIDI names that no sequencer port
can carry. Push 2/3 send on the Live port. After SysEx `0A 01` they send on the User port, after
`0A 02` on both. Push 1 sends on the User port, the one DrivenByMoss uses. Messages are accepted
on both ports, and replies go to the port the request came in on.

### Display socket

Per frame, the host writes exactly the USB bulk stream of Push 2 (Ableton's Push 2 display
interface manual, DrivenByMoss `PushUsbDisplay`):

- a 16-byte header: `FF CC AA 88` followed by 12 zero bytes;
- 327,680 bytes: 160 lines of 2048 bytes, each 960 pixels of 16 bits, little endian (bits 0-4 red,
  5-10 green, 11-15 blue) followed by 128 filler bytes. All of it is XORed with the repeating
  `E7 F3 E7 FF`.

The simulator resyncs on the header and drops a partial frame when the connection closes. It
accepts one connection at a time, the next one after the first closes. The header and the data
may arrive in any chunks (Java sends them as two transfers). The status bar shows the frame
count and the fps. Display brightness (`08`) dims the picture.

## Code

| File | Contents |
| --- | --- |
| `push_simulator/device.py` | `PushDevice`: protocol and state, without Qt |
| `push_simulator/layout.py` | controls of the models (MIDI numbers, labels, positions), port names |
| `push_simulator/palette.py` | the default palette (DrivenByMoss' `PushColorManager.DEFAULT_PALETTE`, so its palette check finds nothing to rewrite), Push 1 LED values |
| `push_simulator/display.py` | frame decoding (mask, BGR565, resync) and the socket server, without Qt |
| `push_simulator/midi.py` | MIDI stream parser and monitor texts |
| `push_simulator/alsa_seq.py` | ALSA sequencer client through ctypes |
| `push_simulator/qt_bridge.py`, `qml/Main.qml` | the Qt object for QML, the display image provider, the view |
| `push_simulator/main.py` | the command line and startup |
| `tools/` | `demo_feed.py` (a test host), `dbm_host_alsa.py` (the reference host on ALSA) |

## Tests

```sh
cd support/push_simulator
QT_QPA_PLATFORM=offscreen python3 -m unittest discover -s tests -v
```

- `test_device.py`: controls → MIDI, MIDI → LEDs (palette, blinking, pulsing, one-shot, MIDI
  clock tempo), palette SysEx, settings and gets, Push 3 MPE and settings, touch strip modes,
  Push 1 text, layout.
- `test_display.py`: masking, pixel format, decoding of a synthetic frame, resync after garbage
  and partial reads, sequential socket connections, the QImage conversion.
- `test_alsa.py`: an ALSA loopback between simulator ports and a test client (skipped without
  ALSA).
- `test_smoke.py`: the app offscreen, fed by `demo_feed.py`, grabbed to a PNG (in
  `PUSH_SIM_SCREENSHOT`, if set), and checked for the colours of the test frame (skipped without
  PySide6 or ALSA).

## Not covered

- Real pad physics. The velocity curve (`20`), the pad thresholds (`1B`) and the Push 3 curve are
  stored and logged but not applied to the clicked velocity.
- Pedal inputs other than the two footswitch CCs (64, 69). Push 3 audio interface, pedal CV and
  battery have no effect.
- Push 3 standalone features and the Push 3 encoder touch notes outside DrivenByMoss' use.
- Several pads at once: the mouse has one pointer.
