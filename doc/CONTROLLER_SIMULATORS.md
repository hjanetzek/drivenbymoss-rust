# Controller simulators: how they are built and what we learned

The repository has desktop simulators of controllers for testing DrivenByMoss without the
hardware. They emulate the Push 1/2/3 (`support/push_simulator`) and the Maschine Mk2 and JAM
(`support/controller_simulator`). This guide is for whoever builds the next one, or the tablet
touchscreen controller that is meant to grow out of them. It covers the architecture, where to
find a controller's protocol, and the pitfalls we hit.

The READMEs have the user's view (options, protocol tables, how to run):
[support/push_simulator/README.md](../support/push_simulator/README.md),
[support/controller_simulator/README.md](../support/controller_simulator/README.md).

## 1. Architecture

```
 DAW / host (Bitwig, dbm_host, Reaper, zynthian)
     │  MIDI over ALSA sequencer ports            │  display frames + DBMV over a UNIX socket
     ▼                                            ▼
 sim_common.alsa_seq (ctypes, reader thread)   display.DisplayServer (thread)
     │ post_midi (queued signal)                  │ post_frame / post_encoders
     ▼                                            ▼
 device model (pure Python, no Qt) ◀──────── SimulatorBase subclass (QObject "sim")
     │ listeners / monitor                         │ properties, slots
     └──────────────────────────────────────▶ QML (SimCommon components + own ones)
```

- **The protocol model has no Qt** (`push_simulator/device.py`, `maschine_simulator/device.py`).
  It keeps the state the host sets (LEDs, displays, strips, settings) from `receive(data, port)`.
  It turns the control actions (`press_button`, `press_pad`, `turn_encoder`, ...) into MIDI through
  a `send(data, port)` callback. It tells the UI what changed through `listeners` ("leds",
  "strip", "text", ...), and every message through `monitor`. The unit tests drive it directly,
  headless and fast. Keep it that way: a tablet controller can reuse it with another UI.
- **ALSA through ctypes** (`sim_common/alsa_seq.py`): one sequencer client, ports duplex (Push) or
  one-way (Maschine, as dbm-nihost creates them). Messages are encoded with
  `snd_midi_event_encode` and sent direct to the subscribers. A thread polls the sequencer's file
  descriptors and decodes with `snd_midi_event_decode` (running status off). The project's rule is
  no new dependencies (no python-rtmidi, no mido). The precedent is
  `support/linux/ni_host/tools/dbm-nihost-bridge-test`.
- **Threads:** ALSA and the socket deliver in their own threads. `SimulatorBase` hands their data
  to the GUI thread with queued signals (`midiReceived`, `frameReceived`, `encodersReceived`). The
  model is only touched in the GUI thread. A 33 ms timer bumps `revision` when LEDs changed or
  animate.
- **The Qt object** (`sim_common/qt_base.py`, `SimulatorBase`): `controls` (a list of dicts with
  `kind`, `label`, position, and `cc`/`note` as the key of the control), `revision`,
  `padColor(note)`, `buttonColor(cc)`, the press/turn/touch slots, `encoderState(note)` for the
  value side channel, `statusText`, the log. The Maschine bridge uses the index of the control as
  its `cc`/`note`, because its template controls can be notes or CCs. The shared components don't
  care what the key means.
- **QML** (`sim_common/qml/SimCommon`): `SimWindow` draws the surface. It positions each control
  of `sim.controls` (in surface units, about millimetres) scaled by `k` to the window, picks its
  component from `componentMap` by `kind`, and adds the status bar and the MIDI monitor (F2). The
  shared components are `Pad`, `LedButton` and `Encoder` (with the LED ring of the side channel).
  A simulator adds only what is special: the Push display and touch strip, the Mk2 text displays,
  the JAM strips and meters.
- **The layout is data** (`layout.py` of each simulator): positions, labels, MIDI numbers or
  template ids. QML has no per-device code for ordinary controls. A layout test checks that
  everything fits on the surface and that no MIDI number or template id is used twice.
- **The app scaffolding** (`sim_common/app.py`): the common options (`--connect`, `--monitor`,
  `--no-alsa`, `--quit-after`, `--screenshot`, `--size`), `open_ports`, and `run` (QML engine
  with the `SimCommon` import path, the screenshot timer, cleanup).

## 2. Finding the protocol

Start with what DrivenByMoss sends and expects, not with the vendor's manual. The simulator has
to satisfy the host, and the manual describes more than the host uses.

**Java DrivenByMoss** (`~/bastel/github/DrivenByMoss`, the main reference;
DrivenByMoss4Reaper only adds the Reaper glue):

| Class | What it tells |
| --- | --- |
| `*ControllerDefinition` | the MIDI discovery names per OS (`getMidiDiscoveryPairs`), USB vendor/product IDs |
| `*ControllerSetup` | `registerTriggerCommands` (each `addButton`: id, label, CC/note), `registerContinuousCommands` (encoders, faders, touch notes, `BindType`), `startup` (what is sent at the start: device inquiry, modes, palette), the setting observers (which SysEx a setting sends) |
| `*ControlSurface` | the SysEx builders, `handleMidi` (what it ignores), the SysEx parser (`handleSysEx`: which replies it expects) |
| `*ColorManager` | the palette, which buttons are monochrome, the colour indices |
| `*Display`, `*UsbDisplay` | text display SysEx or the USB frame format |
| `PadGridImpl` and subclasses | how pads are lit (note/velocity, the blink channels) |

**The Rust core** (`src/controller/<name>/`): `definition.rs` (discovery pairs, UUID),
`surface.rs` (the MIDI numbers as constants), `setup.rs` (startup, SysEx handling, additions not
in Java such as the Mk2's dbm-nihost handshake). `drivenbymoss.list_controllers()` lists the
UUIDs and discovery names the library knows.

**dbm-nihost** (`support/linux/ni_host`) for the NI controllers: its README ("What is emulated",
"DrivenByMoss messages"), `src/midi.h` (the handshake SysEx), `src/mk2_host.cpp` and
`src/jam_host.cpp` (which template control a button sends, which MIDI lights which LED, the strip
drawing), `src/text_display.cpp` (MCU text and button labels), `src/template_map.h` (generated
from the DrivenByMoss NI Controller Editor templates in Java's resources). The Maschine simulator
generates its own table from that header (`tools/gen_maschine_templates.py`), and a test keeps
the two in sync.

**Then watch the real conversation:** run the core against the simulator with
`dbm_host_alsa.py --verbose`. Open the simulator's MIDI monitor (F2), which decodes the SysEx it
knows and marks the rest "unhandled", and read the core's log lines. `aseqdump -p <client>:<port>`
shows a port from outside. Whatever arrives as "unhandled" or "unknown SysEx" is the next thing to
implement.

## 3. Ports, names and discovery

Names are where most of the time went.

- **Kernel vs. sequencer names.** A USB MIDI device appears twice: as raw MIDI devices
  (`/dev/snd/midiC<card>D<n>`, which Bitwig on Linux names after the card, e.g. `A2` for Push 2,
  `A2 [hw:1,0,0]`, `A2 MIDI 1`) and as a sequencer client. Current kernels name the sequencer
  ports from the USB jack names (`Ableton Push 2 Live Port`); older ones number them
  (`Ableton Push 2 MIDI 1`). A simulator can only create sequencer ports, so raw-MIDI discovery
  names (Java's Push 2 Linux pairs) can never match. The Push simulator defaults to the jack names
  and has `--port-style numbered`.
- **PipeWire's MIDI bridge** shows the sequencer ports to JACK clients (Reaper, zynthian) under
  other names: `Midi-Bridge:Maschine MK2: In (capture)` / `Out (playback)`. Zynthian derives its
  device IDs from the client name (`Maschine Jam - 1 IN 1`). The core's discovery pairs include
  the bridge names.
- **Bitwig opens only raw MIDI devices.** Bridge with `snd-virmidi`: every virtual raw MIDI device
  is also a sequencer client (`Virtual Raw MIDI 5-3`, port `VirMIDI 5-3`). `--connect "VirMIDI 5-3"`
  connects the simulator to it both ways. Bitwig lists the devices numbered from 1 per card, so
  `VirMIDI 5-3` shows as "Virtual Raw MIDI 4/1". dbm-nihost `--virmidi` takes the first virmidi
  devices: pick a free one (`aconnect -l` shows which are connected). `connect_both` treats an
  existing connection (EBUSY) as success.
- **Duplicate client names.** ALSA allows two clients with the same name. While dbm-nihost runs
  it owns `Maschine MK2` and `Maschine Jam - 1`. A second client with that name confuses every
  lookup by name (`find_port` takes the first match). The Maschine simulator appends ` (sim)` when
  the name is taken, `--suffix` sets it, and the tests use ` (test)` and ` (smoke)`. Never stop or
  disturb a running dbm-nihost or a user's simulator in tests. The Push tests likewise use their
  own names.
- **Port direction.** NI's driver and dbm-nihost create a read-only port the DAW reads from
  ("In"/"Input") and a write-only one it writes to ("Out"/"Output"). The Push has duplex ports
  (Live, User). The DAW-side name is from the DAW's view: "Maschine MK2 In" is where the device's
  MIDI comes out.
- **Which port sends.** Push 2/3 send on the Live port (on the User port after SysEx `0A 01`,
  both after `0A 02`). DrivenByMoss uses the User port of the Push 1. Replies go to the port the
  request came in on.

## 4. The display socket and the side channel

**The Push 2/3 display:** the host writes exactly what would go to the USB bulk endpoint, back to
back, on a UNIX stream socket (`--display-socket`, `DBM_PUSH_DISPLAY_SOCKET`, default
`$XDG_RUNTIME_DIR/dbm-push-display.sock`):

- the 16-byte header `FF CC AA 88` + 12 zero bytes;
- 327,680 bytes: 160 lines × 2048 bytes (960 pixels of 16-bit little endian, bits 0-4 red, 5-10
  green, 11-15 blue, then 128 filler bytes), all XORed with `E7 F3 E7 FF`.

The reader resyncs on the header, unmasks with one big-integer XOR (fast enough in pure Python),
and makes a `QImage` as `Format_RGB16` with red and blue swapped (`rgbSwapped`). It accepts any
chunking, because Java and the core write the header and the image as two transfers. A partial
frame at disconnect is dropped. The next connection is accepted after the previous one closed.
`tools/demo_feed.py` sends test frames with some garbage first.

**The value side channel (`DBMV`)**, HOST_BRIDGE_GUIDE §5a: data that the hardware doesn't have,
such as the Push encoders' values, names and touch state, which the simulator draws as LED rings.
It travels on the same socket, between frames: `"DBMV"`, version, kind, a 3 × 7-bit length,
payload. **No message may contain `0xFF`.** All numbers are 7-bit and UTF-8 has no `0xFF`, so a
message can never contain the frame header, and a reader that only knows frames skips it like
garbage. Conversely, a "message" with an `0xFF` in it is garbage (or a frame starts), and the
reader skips one byte and scans again. The core builds the data only while a host registered
`dbm_set_simulator_callback`, and hosts register it only when their USB data goes to the socket,
so a real device never sees it. The socket writers keep the last message and resend it after a
reconnect, because the core sends only changes.

The Maschines don't use it. They have no socket, the Mk2's values are in its MCU text, and the
JAM strips get their values over MIDI. A MIDI transport would need a SysEx with a 7-bit encoding
of the payload (its UTF-8 texts have bytes ≥ 0x80). The shared `Encoder` already draws a ring
whenever `encoderState` has a value.

**UNIX socket paths are limited** to about 108 bytes (`sun_path`, including the terminating NUL).
A path in a deep scratch directory fails with "AF_UNIX path too long" (the simulator shows "can't
listen on ..." in the status bar). Use `$XDG_RUNTIME_DIR` or another short directory, also in
tests (`tempfile.mkdtemp(dir=os.environ.get("XDG_RUNTIME_DIR"))`).

## 5. QML and PySide6 gotchas

- **`Rotation` scope.** Inside a `transform: Rotation { origin.x: width / 2 }`, `width` and
  `height` are not the rotated item's: the `Rotation` has no such properties, so they resolve to
  an outer item (in the Push encoder, the encoder itself). The indicator turned around the wrong
  point. Use the item's own `rotation` with `transformOrigin: Item.Bottom` (or qualify with an
  id).
- **A notify signal of a base class crashes PySide6 (6.12).** A `Property` in a subclass declared
  with `notify=Base.someChanged` segfaults when QML reads it. Each class declares its own signals
  for its properties. `SimulatorBase.tick_dirty` is the hook to emit them.
- **The root object type depends on imports.** `engine.rootObjects()[0]` came back as a plain
  `QWindow` (no `grabWindow`) in the Maschine simulator, but as a `QQuickWindow` in the Push
  simulator, which imports `PySide6.QtQuick` for its image provider. `sim_common.app.run` imports
  `PySide6.QtQuick` for this reason.
- **Slot calls are not tracked by bindings.** `color: sim.padColor(c.note)` would never update. Make
  the binding depend on a property: `color: { sim.revision; return sim.padColor(c.note) }`, and
  bump `revision` on change (and on every tick while something animates).
- **Data to loaded components:** the `Loader` in `SimWindow` carries `property var control`, and the
  component reads `parent.control`. A shared component file has no `window` id, so the scale
  comes from `Window.window.k`, guarded because `Window.window` is null while the item is created.
- **A `Canvas` repaints only on `requestPaint()`.** The encoder ring requests it when its state,
  touch or size changes.
- **The MIDI monitor list** is only updated while it is visible (a busy host sends thousands of
  messages).
- **Shared QML module:** `SimCommon` is a directory with a `qmldir`, added with
  `engine.addImportPath(sim_common.QML_IMPORT_PATH)`, and imported as `import SimCommon`.

## 6. Offscreen testing and screenshots

- Unit tests drive the model without Qt. ALSA loopback tests create the simulator's ports and a
  test client with ctypes (skipped without `/dev/snd/seq`), always with their own names.
- Smoke tests start the app with `QT_QPA_PLATFORM=offscreen` and `--quit-after S --screenshot PNG`.
  The window is grabbed (`grabWindow`) just before quitting. A feeder (`demo_feed.py`,
  `maschine_feed.py`) plays the host over ALSA and the socket while the app runs. The test then
  looks for the fed colours in the PNG, with a tolerance: `LedButton` blends the LED colour into
  its cap, so a red button is `(255, 29, 32)`, not `(255, 0, 0)`.
- Set `PUSH_SIM_SCREENSHOT` / `MASCHINE_SIM_SCREENSHOTS` to keep the PNGs, and look at them. Most
  layout and drawing bugs (the indicator rotating around the wrong centre, overlapping buttons)
  were only visible there.
- For a screenshot with the real core, start the simulator with `--quit-after`, then the runner,
  and let the screenshot happen while the runner still runs. When the runner stops, the core shuts
  down and clears the LEDs and displays.

## 7. Driving a simulator

**The reference host on ALSA** (`support/push_simulator/tools/dbm_host_alsa.py`, also reachable as
`support/controller_simulator/tools/dbm_host_alsa.py`): runs one controller of the core in
`bindings/python/dbm_host` with its simulated DAW, in real time. It connects to the simulator's
ports and, for the Push, writes the display (`usb_bulk_out`) and the side channel to the socket.

```sh
cargo build
support/controller_simulator/run.sh --model jam --monitor &
support/controller_simulator/tools/dbm_host_alsa.py --model jam --library target/debug/libdrivenbymoss.so
```

`--model push1|push2|push3|mk2|jam`, `--controller UUID` for others, `--bindings` and `--library`
for another checkout (e.g. a port in a worktree), `--verbose` prints the MIDI.

**Bitwig with the Rust extension:**

```sh
export DBM_PUSH_DISPLAY_SOCKET=$XDG_RUNTIME_DIR/dbm-push-display.sock   # Push: frames to the simulator
export DBM_DEV_LIBRARY=$PWD/target/debug/libdrivenbymoss.so             # use the checkout's library
support/push_simulator/run.sh --model push2 --connect "VirMIDI 5-3" &
bitwig-studio
```

- `DBM_PUSH_DISPLAY_SOCKET` must be set when Bitwig starts. The extension then writes the USB data
  (and the side channel, unless `DBM_PUSH_SIM_VALUES=0`) to the socket and claims no USB device.
- `DBM_DEV_LIBRARY` makes the extension load that library instead of the bundled one, and restart
  when it is rebuilt.
- In Bitwig, select the virtual raw MIDI device as the controller's input and output (see §3 for
  the numbering).
- The Java DrivenByMoss always writes the display to the USB device: with it you only get the
  MIDI side. It refuses the Maschines on Linux (its Linux check), and it knows neither the
  Maschine hello nor the Mk2's Shift SysEx.

## 8. Handshakes and device presence

A host that waits for its device needs to be told the device is there. The simulators send what
the real path sends:

- The Push answers the identity request. DrivenByMoss reads the firmware version from the reply
  and checks the palette entry by entry (SysEx `04`, rewriting differing ones with `03` and
  re-applying with `05`). Starting with DrivenByMoss' own palette keeps that check quiet.
- The Maschines (as dbm-nihost does) send their hello on start and answer the query
  `F0 7D 44 42 4D 11 F7` with it. The Mk2's hello (`... 11 01 F7`) also tells the core that Shift
  is sent, so STEP LEFT becomes Stop. The JAM's hello is NI's "return from host". On quit they
  send gone (`... 13 F7`), which stops the zynthian drivers until the next hello. `--detached`
  starts without the device, to test a host's waiting.

## 9. Java bugs found through the simulators

- **Push 3 pad curve without its command byte:** `PushControlSurface.sendPadSensitivityPush3`
  builds an array with the command `0x43` but sends the bare curve, so the device gets
  `F0 00 21 1D 01 01 <128 curve bytes> F7`. The simulator showed it as an "unhandled" SysEx. The
  Rust port sends the command byte (commit 206ff7d). The simulator still recognises the headerless
  form and logs it as such.
- **Push 2 discovery on Linux uses raw MIDI names** (`A2`, `A2 MIDI n`, `A2 [hw:n,0,0]`). They
  match Bitwig's raw devices, but no sequencer port can carry them. This is a pitfall rather than a
  bug: hosts that list sequencer ports must offer manual selection or the jack names.
- **Not bugs, but invisible on hardware:** Java logs dbm-nihost's hello and gone as "Unhandled
  Sysex message", because only the Rust core knows them.

## 10. Checklist for the next simulator

1. Collect the protocol (§2): discovery names, buttons/encoders/pads with their MIDI, LED
   feedback, SysEx in both directions, startup messages, handshakes.
2. Write the model without Qt and its unit tests first, against the messages the Java or Rust
   code builds (copy them from there, not from the manual).
3. Write the layout as data, and reuse `SimWindow`, `Pad`, `LedButton` and `Encoder`. Add
   components only for what is special.
4. Create the ports with the device's names, made overridable, with a suffix when the name is
   taken. Add `--connect` for Bitwig.
5. Add an ALSA loopback test and an offscreen smoke test with a feeder, and look at the PNG.
6. Run the core against it with `dbm_host_alsa.py` and read the monitor for "unhandled".
7. Document the protocol table and the gaps in the simulator's README, and add what you learned
   here.
