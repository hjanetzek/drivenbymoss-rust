# Controller simulators: how they are built and what we learned

The repository has desktop simulators of controllers for testing DrivenByMoss without the
hardware. They emulate the Push 1/2/3 (`support/push_simulator`), the Maschine Mk2, JAM, Mk3 and
Mikro Mk3, the
Akai APC40 mkII, the Novation Launchpad Pro Mk3 and the Novation Launch Control XL
(`support/controller_simulator`). This guide is for whoever builds the next one, or the tablet
touchscreen controller that is meant to grow out of them. It covers the architecture, where to
find a controller's protocol, and the pitfalls we hit.

The help boxes (what each button does in the current state, texts from the core: the JAM and the Mk2)
are planned and documented in [CONTROLLER_HELP_PLAN.md](CONTROLLER_HELP_PLAN.md). Its §4 is the
mirror: a simulator shows what a real device which the core drives in-process shows (LEDs,
strips, meters, displays, presses), over the help socket; a device model takes the mirror's
"out" messages with its `receive` and names the pressed controls with `mirror_input`.

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

- **The protocol model has no Qt** (`push_simulator/device.py`, `maschine_simulator/device.py`,
  `midi_simulator/base.py` with `apc40mk2.py`, `lppromk3.py` and `lcxl.py`).
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
- **Plain MIDI controllers need almost no own code** (`midi_simulator`): `GridDevice` takes a list
  of controls, each with its message (note/CC, channel, number), its LED kind (palette RGB, single,
  none), and for encoders absolute/relative and the ring CCs. A device module adds its layout, its
  LED addressing (`led_for`: which message lights which control, on which animation channel), its
  animation channels, its SysEx and its identity reply. Its view uses only shared components
  (`Pad`, `LedButton`, `Encoder`, `Fader`). This is the template for the next pad controller.
- **The layout is data** (`layout.py` of each simulator, `build_controls` of the midi_simulator
  devices): positions, labels, MIDI numbers or template ids. QML has no per-device code for ordinary controls. A layout test checks that
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
UUIDs and discovery names the library knows. Look in particular at:

- **The bindings with their channel**, not only the numbers. The APC40's track buttons are the same
  note on the track's MIDI channel (`add_button(..., channel = i, ...)`), and so are its faders.
- **The grid translation** (`create_pad_grid ... with_translation`): grid note 36 becomes the APC40
  mkII's note 0 (bottom left, 0..39 on channel 1) or the Launchpad's programmer note 11 (row × 10 +
  column).
- **The blink mode of the pad grid** (`BlinkMode`): `Channels` sends the blink colour on an
  animation channel (the APC: 11 slow, 13 fast); `Collect` gathers the blinking pads and sends
  them as SysEx (the Launchpad's LED update, types 01/02); `Emulated` resends the colours.
- **The colours are indices**: `colors.rs` maps colour IDs to the device's palette indices, but
  holds no RGB for most of them (Java registers only a few, for its own simulator window). The
  device's palette has to come from elsewhere. The APC40 mkII, the Launchpads and the Push 1
  share one 128-colour velocity palette, which Java has as `PushColorManager.DEFAULT_PALETTE`.
- **The startup and exit messages** in `create`/`init`/`exit` (the APC's mode SysEx, the
  Launchpad's programmer mode and its reset on exit, the Launch Control XL's template selection).
- **What the core waits for.** The Launch Control XL core selects a template, but binds its
  controls only when the device reports the template change (`handle_sysex`). A simulator that
  doesn't answer looks dead: nothing reacts, and no LED is lit. The conformance scenarios show such
  answers as `[[stub]]` entries (`reply = "echo"`).
- **Read the formula, not the pattern of the inputs.** The Launch Control XL's knobs send
  consecutive CCs per row (13..20, 29..36, 49..56), but their LEDs are addressed column by column:
  `set_knob_led_color` sends the note `13 + row + 16 × column` (13, 29 .. 125 for the first row). The
  simulator's first guess (13..20 per row) lit the wrong knobs; the monitor's "no LED" lines for the
  core's messages showed it.

**dbm-nihost** (`support/linux/ni_host`) for the NI controllers: its README ("What is emulated",
"DrivenByMoss messages"), `src/midi.h` (the handshake SysEx), `src/mk2_host.cpp` and
`src/jam_host.cpp` (which template control a button sends, which MIDI lights which LED, the strip
drawing), `src/text_display.cpp` (MCU text and button labels), `src/template_map.h` (generated
from the DrivenByMoss NI Controller Editor templates in Java's resources). The Maschine simulator
generates its own table from that header (`tools/gen_maschine_templates.py`), and a test keeps
the two in sync.

**NI Controller Editor templates** (`resources/Maschine */DrivenByMoss.ncm*` of the Java
DrivenByMoss) are the protocol of the devices behind NI's driver. Only the active page of each
paged section counts, and an element can be `<disabled/>`: NI's driver sends nothing for it.
`support/linux/ni_host/tools/gen_template_map.py` ignores that flag (for the Mk2 and the JAM it
only matters for the JAM's knob touch `CapBrowse`, which dbm-nihost sends anyway); the Maschine
simulator's generator reads it and lists the disabled controls (`MK3_DISABLED`).

**Then watch the real conversation:** run the core against the simulator with
`dbm_host_alsa.py --verbose`. Open the simulator's MIDI monitor (F2), which decodes the SysEx it
knows and marks the rest "unhandled", and read the core's log lines. `aseqdump -p <client>:<port>`
shows a port from outside. Whatever arrives as "unhandled" or "unknown SysEx" is the next thing to
implement.

**Keep the tables in sync** with what the core uses, mechanically where possible:

- Generate tables from a source file and test that they still agree (the Maschine templates from
  dbm-nihost's header, the velocity palette from Java's `PushColorManager`).
- Tag each simulated control with the name of the Rust constant it stands for and test the values
  against `surface.rs` (the APC40), or collect the numbers a Rust definition assigns and compare the
  sets (the Launchpad Pro Mk3's `set_button` calls, its scene controls, track buttons and extra
  buttons).
- **Run the real core in-process** (`tests/test_midi_simulators.py`, `CoreTest`): a `dbm_host`
  `Bridge` with the simulated DAW, its MIDI output fed into the device model and the model's
  output into the bridge, no ALSA and no Qt. Then require that the model understood every message
  the core sent (`device.unknown` is empty), operate every control, and require that the core
  logged no error ("CC .. should be handled in framework" for an unbound CC). This catches
  wrong numbers, channels and message kinds in both directions within a second. It needs the
  built library (`cargo build`) and is skipped without it.

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
- **Several ports, one of them used.** The Launchpad Pro Mk3 has three (`LPProMK3 MIDI`, `DIN`,
  `DAW`); the core opens only `LPProMK3 MIDI` (programmer mode), not the DAW port that Ableton
  uses. The simulator creates all three so that a host's port list looks like the device's, and
  logs what arrives on the others.
- **Name lookup by substring.** `find_port` takes an exact match first, then the first port whose
  name contains the searched one. So `APC40 mkII MIDI 1` also finds `APC40 mkII MIDI 1 (sim)`. Give
  tests their own suffix, and never rely on the substring match to tell two simulators apart.

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
- **Extend shared components by optional keys**, so the existing simulators stay untouched: the
  `Encoder` draws a short arc for `st.dot` (the APC40's "single" ring mode), and no ring for
  `st.ring === false` (the Launch Control XL: the value only turns the indicator, and the knob's LED
  is a separate small `LedButton` without a message). The Push's state has no
  `dot` key, and an absent key is `undefined`, which is false.
- **Don't name an id `layer`.** Inside an `Item` (a delegate, a `Rectangle`), `layer.x` is the
  item's own `layer` group property (`layer.enabled`, `layer.effect`), not the outer item with
  that id: every binding read `undefined` (the help layer's boxes had no size). The id of the
  help layer is `helpLayer`.
- **QML warnings and `console.log` went nowhere** on this desktop (Qt logs to the journal when
  stderr is not a terminal). `QT_FORCE_STDERR_LOGGING=1` brings them to stderr; set it when an
  offscreen run shows a broken window without a message.
- **Give a delegate component's `var` properties defaults** (`property var box: ({...})`): they
  are `undefined` until the delegate assigns them, and a binding which throws then is not
  re-evaluated reliably.
- **Faders are not motorized:** `Fader` shows the value the simulator sent last
  (`sim.faderValue`) and follows `sim.revision`; the bridge bumps the revision when a fader moved.

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
- A runner killed with SIGINT can lose its output: printed into a pipe, Python's stdout is
  block-buffered, and a `KeyboardInterrupt` that hits inside a ctypes callback of the core is
  "ignored" by ctypes, so the loop runs on and the process ends without flushing. Read the
  simulator's monitor or use `--verbose` into a file; the core was fine.
- Thin drawings (the LED rings) can fall between sampled pixels: the APC40 smoke test scans every
  pixel of the top of the window for the ring colour instead of every third one.
- `MIDI_SIM_SCREENSHOTS` keeps the APC40/Launchpad PNGs. Their smoke test starts the simulators
  through the Push simulator's `run.sh`, which also tests the hand-over.
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

`--model push1|push2|push3|mk2|jam|mk3|mikromk3|apc40mk2|lppromk3|lcxl|electra|electramini`, `--controller UUID` for others, `--bindings` and `--library`
for another checkout (e.g. a port in a worktree), `--verbose` prints the MIDI. `--host-pages`: the
simulated DAW provides parameter pages (zynthian-like, `dbm_host/host_page.py`,
HOST_PAGE_PLAN.md); the JAM shows them with Select + CONTROL, and SIGUSR1 / SIGUSR2 to the runner
move the selection of the host's simulated screen (printed as `host screen: …`). `--snapshots
dense|sparse`: the simulated DAW keeps snapshots (`dbm_host/snapshots.py`, HOST_BRIDGE_GUIDE.md
§4.14); the JAM shows them with Select + SONG.

**The Electra One** has no simulator of ours: the emulator of its real firmware (the electra-one
project, `build/electra`, an ALSA client "Electra Controller") stands in for it. The runner then
has two ports, for Port 1 (the controls) and CTRL (the SysEx API), and adds a small project (a
synth with remote controls, an EQ+, clips, scenes, markers):

```sh
build/electra --headless --port 5402 --card /tmp/card --load data/DrivenByMoss.eproj --slot 1 &   # in electra-one
support/controller_simulator/tools/dbm_host_alsa.py --model electra --library target/debug/libdrivenbymoss.so \
    --electra-client 130     # the emulator's client number, from its log (if several run)
tools/electra_ctl.py --port 5402 screenshot /tmp/mixer.png                                         # in electra-one
```

`support/controller_simulator/tests/test_electra_emulator.py` runs the same headless, over the
emulator's virtual cables (`--no-alsa`), and is skipped without the emulator.

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

**The `run.sh` hand-over** goes both ways: `support/push_simulator/run.sh` passes every non-Push
model to `support/controller_simulator/run.sh`, which passes `push*` back and starts
`maschine_simulator` or `midi_simulator` for the others. A new model needs a case in the
controller simulator's `run.sh` only. Unknown models end up in `maschine_simulator`, whose
argument parser rejects them, so there is no loop.

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
- The APC40 mkII has no handshake: the core sends its mode SysEx (Ableton Live mode) and starts.
  The Launchpad Pro Mk3 is asked for its identity (the core logs the firmware version from it),
  switched out of the DAW mode and into programmer mode, and reset on exit.

## 9. Per-device notes

- **APC40 mkII:** the knobs are endless encoders that send absolute values. The host's ring CC
  (the knob's own CC) sets the LED ring and the knob's internal value, so turning continues from
  the DAW's value. The ring mode CC selects off, single (a dot), volume (a bar from the left) or pan
  (a bar from the top). The pads and the scene buttons are RGB with the Push-like animation
  channels; the other buttons have single LEDs (0 off, 1 on, 2 blinking). The crossfader assignment
  has two colours. The core's constant names follow the APC40 mk1, and its mk2 branch rebinds them,
  e.g. `APC_BUTTON_CLIP_TRACK` (0x3A) is "◀ Device" on the mkII and `APC_BUTTON_SEND_C` (0x5A) is
  Metronome. The simulator labels the buttons as printed on the mkII and keeps the Rust names as
  `const` for the sync test.
- **Maschine Mk3 and Mikro Mk3:** no dbm-nihost, so the simulator is NI's driver: one duplex
  port with the driver's Windows name (`Maschine MK3 Ctrl MIDI`, also the core's Linux discovery
  name), NI's "return from host" on start, NI's Shift SysEx with the device's USB ID (16 00; the
  Mikro has no Shift in MIDI mode, the core uses Stop), the MIDI of the DrivenByMoss NI templates.
  The Mk3 shows the MCU text like the Mk2 but gets no button labels (those are dbm-nihost's). The
  template disables the Mk3's touch strip touch (and defines it as poly aftertouch while the core
  binds CC 2), so with NI's driver the core never sees it; the simulator follows the template
  and offers `--send-disabled`.
- **Launch Control XL:** everything depends on the template. Each of the 16 templates has its own
  MIDI channel (user 1-8 on 1-8, factory 1-8 on 9-16) and its own LED memory, so the simulator keeps
  the LEDs per template and shows the active one. The core selects factory template 1 at its start,
  and the simulator answers with the template change like the device. The UI's selector row stands
  in for the device's Templates button. The LEDs are two-colour (red and green, 0..3 each, plus
  copy/clear flags in bits 2-3): there is no palette, so `base_rgb` decodes the velocity. Flashing
  is the Launchpad-S style double buffering (CC 0 = 0x28, LEDs with flags 8), which the core
  doesn't use. The knobs are pots, not encoders: the simulator keeps their absolute value and sends
  it.
- **Launchpad Pro Mk3:** programmer mode only. Static colours come as notes/CCs on channel 1, and
  the core sends blinking pads as the SysEx LED update (its grid uses `BlinkMode::Collect`):
  flashing entries carry colour B (the blink colour) before colour A (the steady one). The device
  also understands flashing/pulsing on channels 2/3 and RGB entries, which the core doesn't use.
  The logo is LED 99. Pads are released with velocity 0 (note on). The core has no fader-mode
  setup for the Pro Mk3: its faders are drawn on the pads.

## 10. Java bugs found through the simulators

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
- **Maschine Mk3 touch strip touch:** the DrivenByMoss NI template disables the touch of the Mk3's
  strip (`TouchstripCap`, and as poly aftertouch on note 2), but Java and the core bind it as CC 2
  and need it for the pitch modes: the pitch bend returns to the centre only on the touch's
  release. With NI's driver the release never comes, so the pitch stays where the finger left it.
  The Mikro Mk3's template has the touch (CC 2). Fix candidates: enable the touch as CC 2 in the
  Mk3 template, or reset on a timeout in the core. Not changed.
- **APC40 mkII and Launchpad Pro Mk3:** the in-process core run found no unknown message and no
  core error. One observation, not checked on hardware: the core (like Java) lights a pressed APC
  button with `BUTTON_STATE_HI` = 2, which is "blinking" for the mkII's single LEDs, so pressed
  buttons blink instead of lighting brighter.

## 11. The conformance scenarios and the simulators

The Launch Control XL is a conformance setup (`tests/conformance/launchcontrol_xl/`). Could a
simulator replay those scenarios instead of the VirMIDI runner? Not as a replacement:

- The runner already plays the device. It sends the scenario's MIDI into a `snd-virmidi` port that
  the Bitwig extensions use, answers like the device (the `[[stub]]` entries), and records what the
  extensions send back. Its expectations are Bitwig's state (`expect.bitwig`, from OSC), so it
  needs Bitwig and the fixture project, and it compares the Java version with the Rust one. A
  simulator on the same port would be a second device: it would answer the template selection a
  second time and could only replace what the runner does already.
- Headless replays of the recordings against the core exist too (`cargo test --test
  conformance_replay`), without MIDI ports. The simulators' in-process core run
  (`tests/test_midi_simulators.py`) is similar but uses dbm_host's simulated DAW, not the fixture
  project, so it can't check the scenarios' Bitwig expectations.

Where a simulator could help:

- **As the runner's device model:** the runner's stub and its LED bookkeeping could use the Qt-free
  device model (`midi_simulator.lcxl.LaunchControlXl`). It answers the template selection,
  decodes the two-colour LEDs per template, and reports unknown messages, which would make the LED
  expectations readable ("Mute red" instead of `98 6A 0F`).
- **As a viewer:** a simulator that only listens, started with `--no-alsa` and fed from a
  recording, would show a run, which helps with debugging differences. It needs a "passive" mode
  that doesn't answer. The mirror (CONTROLLER_HELP_PLAN.md §4) is that mode for a live device:
  `--help-file` with `{"mirror": ...}` and `{"midi": "out", ...}` lines replays a recording.

## 12. Checklist for the next simulator

1. Collect the protocol (§2): discovery names, buttons/encoders/pads with their MIDI, LED
   feedback, SysEx in both directions, startup messages, handshakes.
2. Write the model without Qt and its unit tests first, against the messages the Java or Rust
   code builds (copy them from there, not from the manual).
3. Write the layout as data, and reuse `SimWindow`, `Pad`, `LedButton`, `Encoder` and `Fader`. Add
   components only for what is special. A plain MIDI pad controller is a new `midi_simulator`
   device module (a `GridDevice` subclass) plus a case in `run.sh`.
4. Create the ports with the device's names, made overridable, with a suffix when the name is
   taken. Add `--connect` for Bitwig.
5. Add an ALSA loopback test and an offscreen smoke test with a feeder, and look at the PNG.
6. Add the sync tests (§2): generated tables or constant checks, and the in-process core run.
   Run the core against it with `dbm_host_alsa.py` and read the monitor for "unhandled".
7. Document the protocol table and the gaps in the simulator's README, and add what you learned
   here.

**Latched buttons:** `SimCommon/LedButton.qml` latches a button on a right click (pressed until the next click, amber frame), so combinations like Shift or Select + another button work with one mouse pointer. New simulators get it for free by using `LedButton`.
