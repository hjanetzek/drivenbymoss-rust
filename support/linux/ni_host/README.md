# dbm-nihost: NI Maschine Mk2 and JAM on Linux

On Windows and macOS, NI's driver turns the USB HID of a Maschine into MIDI ports. There is no
such driver on Linux. `dbm-nihost` does its job: it drives the **Maschine Mk2** and the
**Maschine JAM** over hidraw and creates ALSA sequencer ports with NI's names:

| Device | DAW input | DAW output |
| --- | --- | --- |
| Maschine Mk2 | `Maschine MK2 In` | `Maschine MK2 Out` |
| Maschine JAM | `Maschine Jam - 1 Input` | `Maschine Jam - 1 Output` |

The MIDI follows the DrivenByMoss NI Controller Editor templates (`resources/Maschine Mk2`,
`resources/Maschine Jam`). DrivenByMoss (the Rust port, or the Java version) therefore works with
these ports as with NI's driver. Background and design: `doc/NI_HOST_LINUX_PLAN.md`.

There is also a library, `libdbm_nihost.so`, with the same host for a process which drives the
devices itself instead of going through the ports: zynthian's Maschine drivers load it (see
below and `doc/ZYNTHIAN_INTEGRATION.md` §10).

## Build and run

Needs CMake, a C++17 compiler and the ALSA headers (`alsa-lib`); without them only the library and
the tools are built.

```
cmake -S support/linux/ni_host -B build-nihost
cmake --build build-nihost
build-nihost/dbm-nihost            # both devices; --mk2 / --jam for one, --log-midi to log the MIDI
```

The devices can be plugged in and out while it runs. Access to `/dev/hidraw*` needs the udev rule
`packaging/70-dbm-nihost.rules`.

## Install

```
cmake -S support/linux/ni_host -B build-nihost -D CMAKE_INSTALL_PREFIX=$HOME/.local
cmake --build build-nihost
cmake --install build-nihost
```

This installs:

- `bin/dbm-nihost`.
- `lib/dbm-nihost/libdbm_nihost.so` and `include/dbm_nihost.h`, the library.
- `share/systemd/user/dbm-nihost.service`, a systemd user service that runs `dbm-nihost --virmidi`.
- `share/dbm-nihost/`: the udev rule and the `snd-virmidi` setup. These belong in `/etc` and need
  root:

```
sudo cp ~/.local/share/dbm-nihost/70-dbm-nihost.rules /etc/udev/rules.d/
sudo udevadm control --reload && sudo udevadm trigger
sudo cp ~/.local/share/dbm-nihost/modprobe-dbm-nihost.conf /etc/modprobe.d/dbm-nihost.conf        # Bitwig only
sudo cp ~/.local/share/dbm-nihost/modules-load-dbm-nihost.conf /etc/modules-load.d/dbm-nihost.conf
```

Then start the service, and on every login from then on:

```
systemctl --user daemon-reload
systemctl --user enable --now dbm-nihost
journalctl --user -u dbm-nihost -f              # the log
```

Only one process can drive a device: dbm-nihost and the library take an exclusive lock on the
hidraw node, the second one counts the device as absent. Where zynthian drives the Maschines
in-process, the service must not run (`systemctl --user disable --now dbm-nihost`; with the
nura package of an older version, whose symlink in `/usr/lib/systemd/user/default.target.wants`
`disable` can't remove, `systemctl --user mask --now dbm-nihost`).

## The library (in-process)

`include/dbm_nihost.h`: `dbm_nihost_open(DBM_NIHOST_MK2 | DBM_NIHOST_JAM)` starts a thread which
looks for the device once a second and translates its HID reports, `dbm_nihost_read` returns the
MIDI to the DAW (each message with 2 bytes length; it waits up to a timeout, for a thread of the
caller), `dbm_nihost_send` takes one MIDI message from the DAW, `dbm_nihost_flush` writes the
changed LEDs and displays (e.g. on every tick), `dbm_nihost_close`. The MIDI is that of the
ports, including the DrivenByMoss messages below. No callbacks, no ALSA. The zynthian drivers use
it through ctypes (`backends/zynthian/ctrldev/dbm_zynthian/nihost.py`); test devices
(`dbm_nihost_open_test`) stand in for the hardware in the tests. Without CMake (e.g. on the Duet):

```
cd support/linux/ni_host
g++ -std=c++17 -O2 -fPIC -shared -fvisibility=hidden -Isrc -Iinclude -pthread -o libdbm_nihost.so \
    src/log.cpp src/hidraw_device.cpp src/maschine_jam.cpp src/maschine_mk2.cpp src/palette.cpp \
    src/text_display.cpp src/jam_host.cpp src/mk2_host.cpp src/c_api.cpp
```

## Use in a DAW

### Reaper

Reaper opens the sequencer ports through PipeWire's MIDI bridge, named
`Midi-Bridge:Maschine MK2: In (capture)` and so on. The Rust port finds them by these names.

### Bitwig Studio

Bitwig only opens raw MIDI devices (`/dev/snd/midiC*D*`), not sequencer ports. The kernel's
virtual MIDI driver bridges the two:

- `snd-virmidi` with `midi_devs=2` creates two raw MIDI devices, each also a sequencer client
  (`Virtual Raw MIDI N-0`, `N-1`). The files in `packaging/` load it at boot; for a quick test,
  `sudo modprobe snd-virmidi midi_devs=2`.
- `dbm-nihost --virmidi` connects its ports to them: the 1st to the Mk2, the 2nd to the JAM (in this
  order, for the enabled devices). It checks every 2 seconds, so the module can also be loaded
  after the start.
- In Bitwig, select `Virtual Raw MIDI/1` (input and output) for the Mk2 and `Virtual Raw MIDI 2/1`
  for the JAM. Bitwig lists the 16 subdevices of each device; they all carry the same MIDI.
- Do not select the Mk2's own `Maschine Controller MK2 MIDI 1`, which disturbs the HID connection.
- The Java DrivenByMoss refuses the Maschine on Linux unless its Linux check is removed
  (`MaschineControllerSetup.init`, `MaschineJamControllerSetup.init`).

## Tools and tests

- `nihost-probe` prints the decoded HID events of both devices and lights a test pattern.
- `nihost-test` (also `ctest`) tests the translation with synthetic HID reports and MIDI;
  `nihost-c-api-test` the library's C interface.
- `tools/gen_template_map.py <repository root> src/template_map.h` regenerates the template
  tables from the NI templates. `tests/ni_templates.rs` in the Rust crate checks them against the
  Maschine surfaces.
- Debugging the path to zynthian through dbm-nihost's ports, not used by the in-process drivers (installed to `share/dbm-nihost/tools`; Python with the module
  `jack`, on zynthian `PYTHONPATH=/usr/share/zynthian/python`). zynthian reaches the ports through
  PipeWire's MIDI bridge as JACK ports, not over ALSA directly:
  - `dbm-nihost-jack-probe [--client "Maschine Jam - 1"] [--listen]` connects to the bridge ports
    of a device as zynthian does, sends DrivenByMoss' query and prints what arrives (the hello).
    `--listen` only prints, e.g. while pressing pads.
  - `dbm-nihost-bridge-test` sends SysEx messages of every length from its own ALSA client through
    the bridge: when these arrive but dbm-nihost's do not, the bridge's state for dbm-nihost's
    client is the problem, not the messages.
  - The ALSA side, bypassing the bridge: `aseqdump -p 'Maschine MK2:0'` and
    `aseqsend -p 'Maschine MK2:1' F0 7D 44 42 4D 11 F7`.

## What is emulated

- **Input:** buttons, pads (Mk2 with velocity), encoders (relative, two's complement), the JAM
  matrix, touch strips with touch, the JAM's Shift (as NI's sysex) and the Mk2's Shift (as a
  DrivenByMoss sysex, see below; NI's MIDI mode does not send it, so with NI's driver
  DrivenByMoss uses STEP LEFT as Shift).
- **Output:** button LEDs, pad, group and scene colours (NI's palette, hue × 4 + level), the JAM
  strip modes from the `STRIP_SETUP` sysex, and the Mk2 displays (Mackie MCU text, 2 × 56
  characters split across both displays; above them the labels of the 8 buttons over the
  displays, see below).
- **Not yet:** the Mk2 pad pressure as poly aftertouch, the JAM level meters and footswitch.

### DrivenByMoss messages

Not part of NI's MIDI mode: system exclusive messages with the header `F0 7D 44 42 4D` (`7D` is
the ID for non-commercial use, then "DBM"). NI's driver ignores them. The Mk2 uses all of them,
the JAM the query and "gone".

| Message | Direction | Meaning |
|---|---|---|
| `F0 7D 44 42 4D 10 <position> <56 characters> F7` | DAW → device | the labels of the 8 buttons above the displays (8 cells of 7 characters), drawn above the MCU lines |
| `F0 7D 44 42 4D 11 F7` | DAW → device | the query whether the device is there: DrivenByMoss's Mk2 asks at its start whether the host sends the Shift button; the zynthian drivers of both Maschines ask at their start. No answer while the device isn't connected |
| `F0 7D 44 42 4D 11 01 F7` | device → DAW | the Mk2's hello: it is there and the host sends the Shift button. The answer to the query, also sent when the Mk2 is attached and, while it is, when `--virmidi` connects the ports (for a DAW which was started before dbm-nihost). DrivenByMoss then uses the Shift button, and STEP LEFT is only Stop |
| `F0 00 21 09 15 00 4D 50 00 01 46 01 F7` | device → DAW | the JAM's hello: NI's "return from host" (the core refreshes the device with it), sent when the JAM is attached and as the answer to the query |
| `F0 7D 44 42 4D 13 F7` | device → DAW | the device is gone (unplugged, or dbm-nihost stops): the zynthian drivers stop their controller until the next hello. DrivenByMoss's core doesn't know it (on Bitwig it logs "Unhandled Sysex message" when a device is unplugged) |
| `F0 7D 44 42 4D 12 <01 down, 00 up> F7` | device → DAW | the Shift button; the Shift LED is lit by dbm-nihost |

`01` and `02` (the markers of the conformance runner) and `03` (its restart) are used by the
conformance tests of DrivenByMoss, not by dbm-nihost.

## Origin

The HID protocol classes come from KICK's `kick_controllers` module (same author, relicensed to
LGPL-3.0). Their tables, the font and part of the palette come from
[cabl](https://github.com/shaduzlabs/cabl), MIT license, see `LICENSE-cabl.txt`.
