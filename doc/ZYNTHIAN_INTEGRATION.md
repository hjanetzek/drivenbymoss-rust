# DrivenByMoss Rust core as a Zynthian ctrldev driver: integration points

Status: 2026-11-06, second analysis, against the `nura` branch and zyncoder's C side, checked on
this PC with a Launch Control XL in desktop mode; phase 1 of the implementation is done, phase 2
started ([ZYNTHIAN_PLAN.md](ZYNTHIAN_PLAN.md)). The first analysis
(2026-11-02, read only) was written against upstream zynthian-ui `d6ed3592` without zyncoder.
The Python binding and the reference host it builds on: [PYTHON_HOST_PLAN.md](PYTHON_HOST_PLAN.md),
`bindings/python/`.

- zynthian-ui: the worktree `~/bastel/github/zynthian-dbm` (branch `dbm`, from `nura` at
  `2bb87436`). `nura` is upstream `d6ed3592` plus 205 commits. Paths below are relative to it
  unless prefixed `dbm:` or `zyncoder:`.
- zyncoder (lib_zyncore, the C MIDI router and its Python module): `~/bastel/github/zyncoder`,
  branch `nura` (`2e7490d`), linked as `zyncoder` in the worktree. Prefixed `zyncoder:`.
- drivenbymoss-rust: `~/bastel/github/drivenbymoss-rust`, prefixed `dbm:`.

Legend: **[code]** = read in the code; **[observed]** = seen on this PC (PipeWire, read-only
queries); **[unverified]** = the API exists but I did not see what it does; **[guess]** = an
inference.

What changed against the first analysis: §2 (routing, output buffering, sysex) is now read in
zyncoder's C code; §1 and §7 have nura's user-driver folder and PipeWire's port names, which break
the driver matching on nura (fixed on the `dbm` branch, §1); §9 has five new bugs. Everything the
first analysis marked [unverified] or [guess] is resolved below or still marked.

---

## 0. What nura changes against upstream

Only what matters for a ctrldev driver [code, `git diff d6ed3592 HEAD`]:

- **Desktop mode** (`ZYNTHIAN_DESKTOP=1`, `zynconf.desktop_mode`): zynthian runs as an
  application. `desktop/zynthian_desktop_env.sh` sets every directory from XDG:
  `ZYNTHIAN_CONFIG_DIR=~/.config/zynthian`, `ZYNTHIAN_MY_DATA_DIR=~/.local/share/zynthian/my-data`,
  `ZYNTHIAN_UI_DIR` = the checkout (or `/usr/share/zynthian/zynthian-ui` in the package). The
  state manager's `system_service` decorator skips starting and stopping system services
  (VNC, Bluetooth, network MIDI), power save does not run `powersave_control.sh`.
- **One place for the paths**: `zynconf/zynthian_paths.py` (`ui_dir`, `config_dir`, `my_data_dir`,
  `lib_dir`, `native_lib()`); the ctrldev manager and the state manager use it.
- **Native libraries from `ZYNTHIAN_LIB_DIR`** (`/usr/lib/zynthian` in the package):
  `libzyncore.so` (`zyncoder:zyncore.py:39-44`), the zynmixer and zynseq libraries
  (`zynthian_paths.native_lib`).
- **zyncoder with a wiringPi stand-in** (`zyncoder:wiringpi_stub/`): no GPIO, no encoders. The
  MIDI router is unchanged against upstream zyncoder (the nura commits only touch the build and
  the library path) [code, `git log` of zyncoder].
- **JACK is PipeWire** (pipewire-jack). Two consequences in `zynautoconnect`:
  - PipeWire lets only a port's owner set its aliases. nura keeps the aliases zynthian sets in
    the process (`local_port_aliases`, `zynautoconnect/zynthian_autoconnect.py:76-126`).
  - The hardware ports are named after the device, not `system:`; nura finds the audio ports by
    `is_physical` (`:1244-1249, :1449-1454`). For MIDI there was no change, see §1 "Matching".
- **User drivers** live in `$ZYNTHIAN_MY_DATA_DIR/ctrldev` when the code is read-only (§1, §7).
- **zynseq's transport** [code]: `getTransportState()` is defined in `zynseq.cpp` but missing from
  the `extern "C"` declarations of `zynseq.h`, so `libzynseq.so` exports it only C++-mangled (`nm
  -D`: `_Z17getTransportStatev`, §9 Z10); the state is visible as the exported global
  `g_nTransportState`. The Python wrapper's `transport_start/stop/toggle(client)` pass the
  client's name as `bytes` to `transportStart(uint8_t id)` (§9 Z9). zynseq.py declares the
  result types of most `uint8_t`/`bool` functions (`c_uint8`, `c_bool`), but not of
  `getTransportState`: only its low byte is defined.
- **zynseq's recording** [code]: one mode, `enableMidiRecord(bool)` / `isMidiRecord()`
  (`zynseq.h:277-285`): the notes and CCs of zynseq's MIDI input go into the selected pattern
  (`selectPattern`) of the selected sequence (`selectSequence`), in real time while the sequence
  plays, else step by step (`zynseq.cpp:287-420`). No arranger or timeline recording; the pattern
  editor GUI switches it (`zynthian_gui_pated_base.toggle_midi_record`). The other recorders are
  the SMF recorder (`state_manager.toggle_midi_record`, `libsmf`, MIDI files) and the audio
  recorder (`zynthian_audio_recorder`, `jack_capture`). The Python `zynseq.add_pattern` passes 6
  arguments to the 7 of `addPattern` (§9 Z12).
- **The Qt UI** (`zynthian_qt_main.py`, `zynqt/`, PySide6, `ZYNTHIAN_UI=qt`) runs the same state
  manager, so the same ctrldev manager and drivers. It does not register
  `SS_GUI_SHOW_MESSAGE` (only `zyngui/zynthian_gui.py:2746` does): driver messages are not shown
  in the Qt UI [code, grep]. The MIDI device options of both UIs share
  `zyngui/zynthian_midi_config_data.py` (`device_drivers`, `:321-332`).

## 1. Driver lifecycle

**Discovery** [code]
- `zynthian_ctrldev_manager` scans `ctrldev_user` first, then `ctrldev`
  (`zyngine/zynthian_ctrldev_manager.py:76-77`), once in `__init__`, which runs from
  `state_manager.start()` (`zyngine/zynthian_state_manager.py:236`). `DRIVER_DEVELOPMENT`
  (re-scan on every load) is hard-coded off (`:43`).
- **nura:** `ctrldev_user` is `$ZYNTHIAN_UI_DIR/zyngine/ctrldev_user` if it exists (ZynthianOS
  links it to the user's folder), else `$ZYNTHIAN_MY_DATA_DIR/ctrldev`, registered as the package
  `zyngine.ctrldev_user` with that path (`setup_user_ctrldev_package`, `:46-68`, `:100-108`).
  In desktop mode that is `~/.local/share/zynthian/my-data/ctrldev`, **not** `~/.config/zynthian`.
- **`dbm` branch: `$ZYNTHIAN_CTRLDEV_PATH`** (folders separated by `:`) adds driver folders: they
  are appended to the package `zyngine.ctrldev` (`__path__`) and scanned after
  `zyngine/ctrldev`, so their modules and helper packages import as if they were installed there
  (`setup_extra_ctrldev_dirs`). The DrivenByMoss drivers are developed in the dbm repository
  (`backends/zynthian/ctrldev`) and loaded this way [observed in desktop mode].
- Every `*.py` is imported as `zyngine.<dirname>.<module>`; modules starting with `__` or
  `zynthian_ctrldev_base` are skipped (`:108-110`). Sub-folders are not scanned (only `*.py` of the
  folder), so a helper package such as `zyngine/ctrldev/mackiecontrol/` (stock) or
  `zyngine/ctrldev/dbm_zynthian/` is safe.
- **The class must have the module's name** (`getattr(module, module_name)`, `:121`): one module,
  one driver class. `available_drivers[dev_id] -> [classes]` from each class's `dev_ids`; `"*"`
  is generic (`:128-135`).

**Matching** [code]
- `dev_id = zynautoconnect.get_midi_in_devid(izmip)`: the part of the port's uid (its JACK alias 0)
  after the first `/`; None without `/` (`zynautoconnect/zynthian_autoconnect.py:333-343`).
- With jackd, `build_midi_port_name` builds the uid from the ALSA alias `in-hw-<card>-…`:
  `USB:<bus>.<ports>/<card name> IN <n>`, `<card name>` = first line of
  `/proc/asound/cardN/midi0`, `<n>` the 1-based port index (`:1501-1547`). So the dev_ids are e.g.
  `"Launch Control XL IN 1"`, `"Launchpad X IN 2"`.
- **On nura (PipeWire) this does not work** [code + observed]. pipewire-jack gives the MIDI bridge
  ports no JACK aliases (`jack.fill-aliases` is off by default; `jack.Port.aliases` was `[]` for
  every port on this PC, while PipeWire's own `port.alias` is set). The ports are named
  `Midi-Bridge:<client>:<port> (capture|playback)`, e.g.
  `Midi-Bridge:Scarlett 18i20 USB: MIDI 1 (capture)` (`pw-link -o`). `build_midi_port_name` then
  falls back to `(port.name, port.shortname)` (`:1548-1553`): the uid has no `/`, the dev_id is
  None, **no device-specific driver matches or autoloads**; only `"*"` drivers are offered
  (`zynthian_midi_config_data.device_drivers`). `dev_in_2_dev_out` (below) finds no output either.
  - **Fixed on the `dbm` branch:** `zynautoconnect/zynthian_pipewire_ports.py` builds
    `PW:<client>/<client> IN <n>` / `… OUT <n>` for these ports (`<n>` = index among the client's
    ports of that direction, in JACK's order [guess that this is ALSA's port order]), so the dev_ids
    are those of ZynthianOS (`Launch Control XL IN 1`) and the output is found.
    `build_midi_port_name` calls it for `Midi-Bridge:` ports. Side effect: the uids of hardware
    MIDI ports change on nura, so MIDI routes stored in nura snapshots by the old uid (the port name)
    are not restored (accepted; ZYNTHIAN_PLAN.md §8).
  - **The Launch Control XL has two MIDI ports** [observed: `pw-link -o/-i`, `/proc/asound/seq/clients`,
    `/proc/asound/card5/midi0`]: ALSA port 0 "Launch Control XL Launch Contro" (the controls; the
    core's Linux name), port 1 "Launch Control XL HUI". PipeWire shows them as
    `Midi-Bridge:Launch Control XL: Launch Contro (capture|playback)` and `…: HUI (…)`; JACK lists
    them in ALSA's order [observed], so they become `Launch Control XL IN 1` / `IN 2` (and `OUT 1/2`).
    On ZynthianOS the rawmidi inputs 0 and 1 give the same dev_ids. The driver takes `IN 1`, never
    HUI.
  - [observed in desktop mode] zynthian with the fix lists every hardware port with a dev_id
    (`Launch Control XL IN 1`, `… IN 2`, `Scarlett 18i20 USB IN 1`, `Virtual Raw MIDI 0-0 IN 1`, …),
    autoloads the dbm driver on zmip 7 (`IN 1`) only, and `dev_in_2_dev_out` gives zmop 7
    (`OUT 1`).
- **Matching is an exact dict lookup**, no patterns (`zynthian_ctrldev_manager.py:153-160`).
- **Autoload** when an input port appears (`zynthian_autoconnect.py:1074`), unless a ZS3 stores a
  driver for the uid. The first class in `available_drivers[dev_id]` with `get_autoload_flag()`
  wins; `"*"` drivers are never autoloaded. A `ctrldev_user` driver wins over a stock driver with
  the same dev_id (scan order).
- **Loading** (`:135-207`): `izmop = dev_in_2_dev_out(izmip)`; `driver_class(state_manager, izmip,
  izmop)`; `zmip_set_ui_midi_chans` (§2); register in `drivers[izmip]` (sysex during `init()`
  reaches it); `driver.init()`. Exceptions are logged and the driver is dropped.
- **Unloading** (`:209-234`): restores the routing (`zmip_set_ui_midi_chans(izmip, 0)`), then
  `driver.end()`. From the MIDI device options (`disable=True`, stored as `disable_ctrldev`), when
  the device disappears (`zynthian_autoconnect.py:803`) and when the state manager stops
  (`zynthian_state_manager.py:279`).
- **The output port.** `dev_in_2_dev_out` took the input uid and did `name.replace("IN", "OUT")`
  (`zynthian_autoconnect.py:260-277`), which replaces every "IN" (§9 Z1, still on nura). The `dbm`
  branch replaces only the direction of the device ID (`zynthian_pipewire_ports.dev_out_uid`).
  `idev_out` is None when no output is found; drivers must check it.

**One generic driver or one per controller?** As in the first analysis: one thin module per
controller with a shared base, because `driver_name`, `driver_description` and the autoload flag
are class attributes (`zyngine/ctrldev/zynthian_ctrldev_base.py:56-121`). The base must be named
`zynthian_ctrldev_base_*.py`. Implemented as `zynthian_ctrldev_base_dbm.py` +
`zynthian_ctrldev_dbm_launchcontrol_xl.py` (ZYNTHIAN_PLAN.md §2).

**The Maschine Mk2 through dbm-nihost** [observed: `aconnect -l`, `pw-link -i/-o`, desktop mode]:
the Mk2 has no MIDI driver on Linux; dbm-nihost (`support/linux/ni_host`) creates the ALSA
sequencer client "Maschine MK2" with the ports "Maschine MK2 In " (sic, with a space) and
"Maschine MK2 Out", PipeWire `Midi-Bridge:Maschine MK2: In (capture)` and `…: Out (playback)` (the
core's Linux names). With the PipeWire uids its dev_id is `Maschine MK2 IN 1` (output `OUT 1`).
Beside it are the device's own USB MIDI port (kernel client "Maschine Controller MK2", MIDI mode,
dev_id `Maschine Controller MK2 IN 1`) and the JAM's (`Maschine Jam - 1 IN 1`); the Mk2 driver
takes only `Maschine MK2 IN 1`. The JAM's client "Maschine Jam - 1" (ports "Maschine Jam - 1
Input"/"… Output", PipeWire `Midi-Bridge:Maschine Jam - 1: Input (capture)`) gives the dev_id
`Maschine Jam - 1 IN 1` / `OUT 1`, the JAM driver's (ZYNTHIAN_PLAN.md §7c). On ZynthianOS
(jackd) a sequencer client reaches JACK through a2jmidid (`a2j:Maschine MK2 [128] (capture): Maschine MK2 In`), whose ports have no alias
`build_midi_port_name` understands: no dev_id there either [guess; not tested, and dbm-nihost is
not packaged for ZynthianOS].

**dev_id ↔ the core's port names.** The core's `discovery.linux` input names are ALSA-seq/JACK
names: `"Launch Control XL"`, `"Launch Control XL Launch Contro"`, `"Launchpad X MIDI 2"`. The
mapping to dev_ids: `<card>` and `<card> <name of ALSA port n>` → `<card> IN <n+1>`;
`<card> MIDI <n>` → `<card> IN <n>`. The port's index cannot be read from the name, so the
generator needs each device's port list (confirmed for the Launch Control XL above; the others
[guess] until seen on hardware). The Launchpad X/Mini
MK3 cases (DBM on port 2, the stock drivers on port 1) are as in the first analysis.

**Multi-port controllers** (MCU/HUI with extenders, MPC Live/Force) still do not fit one
zmip/zmop per driver.

## 2. MIDI path

**Input** [code, zyncoder C and Python]
- In `jack_process` (`zyncoder:zynmidirouter.c:1355-1690`) every event of a device zmip
  (`ZMIP_DEV_FLAGS` = UI, FILTER, ACTIVE_CHAIN, SYSTEM, SYSTEM_RT, `zynmidirouter.h:183`) goes,
  in this order:
  1. Active sense and MIDI clock are dropped; system messages only pass with the zmip's
     `FLAG_ZMIP_SYSTEM` / `FLAG_ZMIP_SYSTEM_RT` (`:1407-1428`). **SysEx is a system message:** if
     the user switches "MIDI system messages" off for the device, the core never sees the
     controller's sysex replies (e.g. the Launch Control XL's template echo).
  2. The global MIDI filter's event map (`FLAG_ZMIP_FILTER`) can drop or remap channel messages
     **before** the UI capture (`:1448-1484`), so a user's filter rule changes what the driver sees.
  3. Messages on the master channel go to the UI only, never to chains (`:1487-1492`).
  4. **UI capture**: every message is written to the UI ring buffer (`write_zynmidi`, 32-bit
     words `izmip | status | data1 | data2`); sysex as fragments: the first word has `izmip`, `F0`
     and two bytes, the following words four data bytes each (`:1510-1550`). A sysex split over
     several JACK events of the same period is joined; one split over periods is dropped
     ("Splitted SysEx message has not end mark!").
  5. **`unroute_from_chains`**: `zmip_set_ui_midi_chans(izmip, mask)` stores a 16-bit channel mask
     (`:527-534`); after the UI capture, a *channel* message on a masked channel is not routed
     further (`:1553`). So `True` (mask `0xFFFF`) keeps all channel messages from the chains, the
     step sequencer's recording input and the MIDI-out routes. SysEx and system messages are still
     routed on, but every zmop that receives device input drops them: chain zmops have
     `FLAG_ZMOP_DROPSYS|DROPSYSEX` (`ZMOP_CHAIN_FLAGS`, `zynmidirouter.h:294`), the step zmop
     `DROPSYSEX`, and the device output zmops are not routed from device inputs by default
     (`zynmidirouter.c:1230-1248` routes devices only to the zmops below `ZMOP_CTRL`). **This
     answers the first analysis' question: the driver owns the device's MIDI completely.**
- The UI ring buffer (`ZYNMIDI_BUFFER_SIZE` 16384 bytes = 4096 words, `zynmidirouter.h:483`) is
  shared by all inputs; `write_zynmidi` drops words silently when it is full (`:2191-2197`).
- The state manager's **fast thread** ("Status Manager Fast", `zynthian_state_manager.py:249-252`,
  loop `:686-695`: `zynmidi_read`, zynseq beat/progress, `sleep(0.01)`) drains it
  (`:723-760`). A short message reaches `ctrldev_manager.midi_event(izmip, ev)` as **3 bytes**
  (`ev[1:]` of the word): 2-byte messages are already zero-padded as HOST_BRIDGE_GUIDE §5 wants.
  A sysex arrives complete (`F0 … F7`). Returning True stops the processing of the event.
- `midi_event` is called only from there [code, grep of all callers]: **it runs on the fast
  thread**. The sysex reassembly has a bug (§9 Z4): the word with `F7` is processed a second time
  as an event, so a driver can get a "message" whose first byte is a data byte.

**Output** [code, zyncoder C]
- `dev_send_midi_event(idev, data, len)` → `zmop_send_midi_event(ZMOP_DEV0 + idev, …)` →
  `write_rb_midi_event`: the bytes are written into the zmop's JACK ring buffer, **all or nothing**;
  when it is full the message is dropped and "Error writing ring-buffer: FULL" goes to stderr
  (`zynmidirouter.c:1824-1836`, `:2002-2009`, `:2140-2142`).
- The ring buffer has `JACK_MIDI_BUFFER_SIZE` = 16384 bytes (`zynmidirouter.h:411`), 16383 usable
  (JACK's ring buffer keeps one byte free [JACK API]). **No message can be larger**; a Fire OLED
  frame or an LCD sysex fits easily.
- Every JACK period, `jack_process` takes all messages out of the ring (`:1693-1709`), parses them by
  status byte (a sysex byte by byte up to `F7`, `populate_midi_event_from_rb`, `:1279-1336`) and
  writes them at the last frame of the period with `jack_midi_event_write`. If the JACK port buffer
  is full the message is lost (stderr "Error writing jack midi output event!"). The size of a JACK
  MIDI port buffer under PipeWire [unverified]. So the throughput is "whatever fits into one port
  buffer per period"; at 256 frames / 48 kHz that is one flush every 5.3 ms.
- **`dev_send_*` return nothing** (`int` functions without `return`, `:2140-2170`, §9 Z6): the
  Python side gets an undefined value and cannot detect a drop.
- **One writer per device.** The JACK ring buffer is safe for one writer and one reader; ctypes
  releases the GIL during the call, so two Python threads writing to the same device at once can
  interleave bytes [code + JACK ring buffer semantics]. The dbm driver writes only from its tick
  thread.
- [observed] **PipeWire's MIDI bridge stopped delivering to the Launch Control XL** during the
  session: after three runs in which the device answered the core's template selection, neither
  zynthian nor a minimal JACK client (`jack.Client`, one sysex to
  `Midi-Bridge:Launch Control XL: Launch Contro (playback)`) got an answer, and `aseqdump` on the
  device's ALSA port showed nothing from the device, while `amidi -p hw:5,0,0` (raw ALSA, the
  same sysex) got the echo every time. A PipeWire problem on this host [cause unverified; PipeWire
  was not restarted].
- The 50 ms sleep after each sysex in the stock Launchpad X driver
  (`zyngine/ctrldev/zynthian_ctrldev_launchpad_x.py:51`) is not needed by the router [guess: it is
  for the device's mode switch]; the core sends its LED batches without sleeping.

**Injecting notes into the chains** (note inputs, `process_midi_arg`) [code]
- `ui_send_note_on/off(chan, …)`, `ui_send_midi_event` write to the zmip `ZMIP_FAKE_UI`
  (`zynmidirouter.c:1957-1975`). Its flags are only `DIRECTIN` (`zynmidirouter.h:188`): no active
  chain translation, so the event goes, on its MIDI channel, to every chain zmop with that channel;
  all chain zmops route from `ZMIP_FAKE_UI` (`zynmidirouter.c:1238-1241`). **This answers the first
  analysis' question:** `ui_send_*` reaches all chains on the channel. To play the active chain,
  send on `chain_manager.get_active_chain().midi_chan`. Note-on/off from the UI are not dropped by
  `DROPNOTE` (`:1649`).
- `write_zynmidi(uint32)` writes into the **UI** ring buffer (`:2191`), as in 2020: the event comes
  back to `zynmidi_read`, it does not reach the chains. The stock Launchkey Mini MK4 driver passes a
  `bytes` object to it "to route to chains" (`zyngine/ctrldev/zynthian_ctrldev_launchkey_mini_mk4_37.py:243`):
  ctypes then passes a pointer as the 32-bit word (§9 Z8).
- **Implemented for the Maschine Mk2** (the dbm drivers' note input): the driver applies the core's
  note-input filters and key/velocity tables in `midi_event`, on the fast thread, and sends the
  note at once with `ui_send_note_on/off` on the active chain's channel; the core gets the message
  through the queue as before (for its LEDs). Latency [measured for the Python part, reasoned for
  the rest]: dbm-nihost (HID report → ALSA sequencer → PipeWire bridge, not measured) + the fast
  thread's poll (0–10 ms, `sleep(0.01)` between reads) + 35–45 µs in the driver (measured in
  desktop mode; 3 µs in the test without zynthian) + the next JACK period of the router (0–5.3 ms
  at 256 frames/48 kHz) = up to about 15 ms on top of dbm-nihost, 5–8 ms on average. Not waiting
  for the 30 ms tick saves up to 30 ms. Lower would need a real-time path (a midiproc, below) or
  a shorter fast-thread poll. A real-time midiproc (`zynthian_ctrldev_base.py:167-212`) stays
  the option for low-latency playing (Launchpad, phase 3), with the problems of the first analysis
  (the driver then sees the midiproc's output; a second port needs a zynautoconnect change).

## 3. Timing and threading

- **Threads, no asyncio** [code]: the state manager's slow (0.2 s) and fast (10 ms) threads, the
  signal manager's `SIGNAL_QUEUE` thread (`zyngine/zynthian_signal_manager.py:224-240`, started on
  import), the GUI thread (Tk or Qt), zynautoconnect's threads.
- **`midi_event` runs on the fast thread** [code, §2]. Signal callbacks run on `SIGNAL_QUEUE`
  (queued) or on the sender's thread (`zynsigman.send`, e.g. every `zynmixer.set_*`,
  `zynlibs/zynmixer/zynmixer.py:292` etc.).
- **No periodic hook for 30 ms** (`add_slow_update_callback` checks once per second;
  `remove_slow_update_callback` is broken, §9 Z2).
- `get_state` is called from whatever thread saves the ZS3/snapshot, `set_state` from the one
  loading it, `sleep_on/off` from the slow thread's power save (`zynthian_state_manager.py:930-956`)
  [code for the calls; the threads: by the callers].
- **Decision (implemented):** the driver owns one thread which makes **all** `dbm_*` calls
  (`zyngine/ctrldev/zynthian_ctrldev_base_dbm.py`). `midi_event` only queues and returns True;
  `get_state`, `set_state`, `sleep_on/off` and `refresh` only set flags or read a cache. Each
  pass waits on an `Event` (30 ms timeout, woken by MIDI), then: document settings to apply, dump
  request, MIDI → `dbm_on_midi_event`, `dbm_set_time` (real time since the controller started),
  `dbm_tick`, report → `dbm_update_model`, notifications → `SS_GUI_SHOW_MESSAGE`.
- **GIL** [code for the binding; ctypes semantics]: ctypes releases the GIL during the call and the
  callback trampolines take it back, so the core's mutex cannot deadlock against the GIL; with one
  dbm thread no Python lock is held across a `dbm_*` call. The commands change zctrls on the tick
  thread, which fires non-queued signal handlers there (as the stock drivers do from the fast
  thread). The Python cost per tick on a Pi or the Duet [unverified, phase 4].
- `zynthian_ctrldev_base.py:34` calls `mp.set_start_method('fork')` at import. The core has no
  thread of its own (`DBM_FLAG_HOST_TICK`), so a forked midiproc child is safe as long as it never
  calls the library [guess].

## 4. The DAW model mapping

Zynthian's main objects: `state_manager.chain_manager` (chains), `zynmixer_chan` and
`zynmixer_bus`, `zynseq`, `audio_recorder`. Values are `zynthian_controller` objects (zctrls:
`value`, `value_min/max/default`, `set_value`, `nudge`, `toggle`, `reset_value`,
`get_value2label`, `zyngine/zynthian_controller.py:400-616`); `set_value` sends through the
engine (`zynthian_engine_audio_mixer.send_controller_value` → `zynmixer.set_*`) and then the
signal `SS_ZYNMIXER_SET_VALUE`. There is no general change signal on a zctrl, so the adapter
**polls and diffs** every tick (the reference `Reporter` does the same), reading the zctrls (what
the GUI shows) rather than zynmixer.

| Core area | zynthian, read | Commands | Gaps |
| --- | --- | --- | --- |
| **Tracks** | The chains without chain 0 and the mixbus chains, in `chain_manager.chains` order (= display order: `get_chain_index`, `move_chain`, `zyngine/zynthian_chain_manager.py:522-590`). Name `chain.get_title()`. Type: MIDI chain → `INSTRUMENT`, else `AUDIO`. `canHoldNotes` = `is_midi()`, `canHoldAudio` = `is_audio()` (has a mixer strip). Selection = `chain_manager.active_chain` | `select` → `set_active_chain_by_id` (`:979`), `name` → `set_title` | No groups, depth, colors, activation; `addTrack`, `remove`, `duplicate`, move not mapped |
| **Volume, pan, mute, solo, arm** | The strip zctrls `level` (0..1 linear gain, 1.0 = 0 dB, default 0.8), `balance` (−1..1), `mute`, `solo`, `record` (`zyngine/zynthian_engine_audio_mixer.py:60-160`) | `set_value` on them; pan = `2v−1`; `/inc` adds to the normalized value; `/reset` = `reset_value` | A MIDI chain has no strip (`zynmixer_proc` None) |
| **Texts** | Volume `20·log10(level)` dB, as the mixer GUI (`zyngui/zynthian_gui_mixer.py:1468`); pan as the reference host | – | – |
| **VU** | `lib_zynmixer.getDpm(chan, leg)` in dBFS (−200 floor, `zynlibs/zynmixer/mixer.c:1024-1030`), −60..0 dB as 0..1 | – | The GUI's DPM array is filled by the GUI (`update_dpm_states`); the adapter calls `getDpm` itself |
| **Master** | Chain 0 ("Main"), its strip is the main mixbus `MR` channel 0 | As tracks | Main `solo` clears all solos (`zynthian_engine_audio_mixer.py:274-281`) |
| **Effect tracks, sends** | The mixbus chains (`is_mixbus()`, `MR`) except 0 = `/fxtrack/…`; send `s` of a track = zctrl `send_<mixbus chain id>_level` of its strip, in the mixbus order (`zynthian_chain_manager.py:1302-1376`) | `send/<s>/volume` | `send/<s>/active` (a toggle) has no counterpart, only pre/post (`send_<id>_mode`) |
| **Scenes / slots** | zynseq phrases × chain MIDI channels, as in the first analysis; the play states below | implemented (ZYNTHIAN_PLAN.md §3.3, §3.6) | names, colors, alternative quantization |
| **Devices / parameters** | the active chain's processors without the mixer strip; `get_ctrl_screens()` (4 zctrls per screen, two per core page) | implemented in phase 2 (ZYNTHIAN_PLAN.md §3.2) | window, expand, pin, layers, drum pads |
| **Transport / tempo** | `libseq.getTransportState()` (§0, §9 Z10), `getTempo`, `getBpb`, the metronome mode and volume | implemented in phase 2: play = zynseq's transport (decided) | position, loop, punch, automation, record |
| **Step grid** | zynseq patterns, as first analysis | as first analysis | phase 3 |
| **Browser** | the cursor device's processor: `get_bank_list()` (rows `[id, msb, name, …]`, section titles with id `None`, the favourites row `*FAVS*` first if there are favourites), `load_preset_list()` (of `bank_info`, or the favourites with `show_fav_presets`; favourites marked "❤ " in the titles), `set_bank(i, set_engine=False)` (the bank without the engine, as the bank screen), `set_preset`, `preload_preset`/`restore_preset` (the preview) | implemented (ZYNTHIAN_PLAN.md §3.7); zynthian's preset view by the CUIAs `CHAIN_CONTROL`, `SHOW_SCREEN preset`, `SELECT`, `BACK` | the device browser, sub-folder banks, hit counts |
| **Markers** | – | – | not mapped |

**zynseq's patterns, as the step grid needs them** [code: `zynlibs/zynseq/zynseq.h`, `zynseq.cpp`,
`pattern.cpp`; checked in desktop mode]:
- A pattern has `getStepsPerBeat()` steps per beat (1, 2, 3, 4, 6, 8, 12 or 24; the pattern
  editor's setting) and `getBeatsInPattern()` beats. An event (`StepEvent`, packed to 30 bytes,
  the same layout as zynseq.py's `event_data`) has an integer `position` in steps, a fractional
  `offset` within the step, a `duration` in steps, velocity, `play_chance`, `play_freq` (0 = never
  plays) and stutter values; no MIDI channel, no pan, gain, timbre or pressure.
- The editing functions (`addNote`, `removeNote`, `clearNotes`, `setNoteVelocity`,
  `setNotePlayChance`, `setNotePlayFreq`, `transpose`, `getEventDataAt`, `getStepsPerBeat`,
  `getPatternPlayhead`) work on the **globally selected pattern** (`selectPattern`, which creates
  a pattern that does not exist), the one the pattern editor shows; `getPatternIndex` returns −1
  without one.
- One note per key and step: `addNote` deletes the overlapping notes of its key (and inherits
  their chance, frequency and stutter, `Pattern::addEvent`), `removeNote` and `getNoteData` address
  a note by its integer step and key.
- No `setNoteDuration`; `setNoteData` copies the event from the second value byte on, so not the
  duration either: a duration changes by `addNote` again.
- The edits mark the pattern modified; the pattern editor polls `isPatternModified()` in
  `refresh_status` and redraws. That call reads **and clears** one global flag: another reader
  would take the editor's redraw away.
- `getPatternPlayhead()` is the play position of the C side's selected sequence (`g_nScene`,
  `g_nPhrase`, `g_nSequence`, set by `selectSequence`) in steps of the selected pattern.
- The Python `zynseq.state` (the launcher GUI's pads) changes only with `refresh_state()`.
- `Pattern::setStepsPerBeat` does not move the events (§9 Z13); the GUI warns that changing it
  "may alter timing".

**zynseq's launcher, as the clip launcher needs it** [code: `zynlibs/zynseq/constants.h`,
`zynseq.cpp` `setPlayState`/`togglePlayState`, `sequence.cpp`, `sequencemanager.cpp`]:
- The play states: STOPPED 0, PLAYING 1, STARTING 2, STOPPING 3 (at the end of the sequence),
  FORCED_STOP 4, STOPPING_SYNC 5 (at the next sync point), and for a phrase launcher (channel
  32) CHILD_PLAYING 6 (some of its sequences play, the launcher not), CHILD_STOPPING 8 (setting
  it sets the playing children to STOPPING). Bit 0 = sounding, except 6 and 8.
- `togglePlayState`: STOPPED → STARTING, STARTING → STOPPED, PLAYING → STOPPING_SYNC (a phrase
  launcher: CHILD_STOPPING), STOPPING/STOPPING_SYNC → PLAYING, CHILD_PLAYING → CHILD_STOPPING →
  STARTING; FORCED_STOP stays. A sequence with repeat 0 (disabled) stops its group instead.
- `setPlayState` (C API): STARTING/PLAYING start zynseq's transport; STOPPING becomes STOPPED
  when nothing plays; the sequence manager stops the other sequences of the group (= the
  channel; phrase launchers are group 32): queued (STOPPING) if the new state is STARTING, at
  once if it is PLAYING. STARTING on a phrase launcher sets its children (with repeat) to STARTING.
  `MODE_END_IMMEDIATE` (the sequence's mode) turns a queued stop into STOPPED.
- `insertPhrase(scene, phrase)` inserts an empty phrase at the index, every MIDI sequence with an
  empty pattern of the default length; `duplicatePhrase(scene, phrase)` inserts the copy at the
  same index (the original, with its play state, moves one down), the patterns copied;
  `removePhrase` calls `stop()` (all sequences STOPPED, the event schedule cleared: §9 Z14).
  zynseq.py's wrappers (`insert_phrase`, `duplicate_phrase`, `remove_phrase`) also update clippy
  and the Python state; `remove_phrase` refuses to remove the last phrase.
- `clearPattern(pattern)`, `copyPattern(source, destination)` (notes and length) work on pattern
  ids, not the selected pattern.

**The first instrument (`/primary`) without layers and pads** [code, core]: the layer modes of the
core use the first instrument's layer bank only if `uses_layers` = `!hasDrumPads && hasLayers`
(`src/framework/mode/parameter_mode.rs:349`), else its drum pad bank. Until the host reports pads,
that bank holds the core's own 128 existing pads ("Drum 36", …, `src/daw/device.rs:320-345`). A
host whose instruments have neither must therefore report the pads as absent
(`/primary/pad/<i>/exists 0`); then the Launch Control XL's Layer Mix finds no item: the Track
Focus and Track Control rows and the knob LEDs stay dark, no layer command is sent, Track Select
next stays lit (the pad bank keeps its count of 128; there is no `pad/count` address), and the Mute
mode still sends `primary pad/<n>/mute` for an absent pad [observed in the adapter's tests and the
conformance run of layer_mix.toml].

**What the Maschine Mk2's display modes read** [code: `src/controller/maschine/mk/mode/`;
observed in desktop mode]: the track modes (volume, panorama, sends) the names of the tracks of the
bank page (shortened to 6 characters) and the value texts (`…/str/8`); the parameters mode the
cursor device's 8 parameter names and value texts (`/device/param/<i>/name`, `value/str/8`) and
"Please select a device..." without one; notifications with the device name ("Selected device:")
and the page name ("Page:"); the labels of the buttons above the displays (the core's
`F0 7D 44 42 4D 10` sysex, which dbm-nihost draws). Paging in the parameters mode: PAGE LEFT/RIGHT
= the previous/next device, the encoder (cursor) up/down = the parameter pages; in the track
modes the encoder up/down = the track bank page. The Mk2 does not use `/primary` (its `deviceIds`
are empty); it wants a drum device (`/drum`, not reported) and the focused parameter.

**Gaps the Mk2 showed** (fixed): without `/device/pinned` the core takes the cursor device as
pinned, as Reaper's, and lights the Pin button; without `/track/pinned` the same for the track;
without `/repeat` the loop LED is never written. All three are now reported (0 / −1 / 0: zynthian
has no pinning and no loop). Empty slots are reported without color.

**Reuse of the reference host — decided** (ZYNTHIAN_PLAN.md §3): `ZynBridge(Bridge)`,
`ZynReporter(Reporter)`, `ZynCommandHandler(CommandHandler)` reuse the loop, the change tracking,
the dumps and the report again after value commands, the command parsing and dispatch, the
unhandled-command log and the host features; the collectors and handlers are zynthian's. The
`SimDaw` object graph is not imitated. `dbm_host.Bridge` got two class attributes for this
(`reporter_class`, `command_handler_class`).

**Host features (phase 1):** `/track/offset`, `/fxtrack/offset` and `/host/features valueInc
valueReset immediately soloToggle`; all chains reported flat with absolute positions.

## 5. State and settings

- **Per-ZS3 driver state** [code]: `get_state_drivers()` → `zs3[…]["midi_capture"][uid]["ctrldev_state"]`
  (`zynthian_state_manager.py:2080`), restored by `set_state_drivers` on ZS3 load (`:2207`) after
  loading the stored driver (`:2163-2165`). There is no snapshot-level driver state.
- **Decided: the core's document settings belong to the snapshot, not the ZS3.** The dbm driver
  returns `{"dbm_document": <formatted document settings>}` (cached by the tick thread once per
  second), so every ZS3 stores the current settings; `set_state` applies them only while the
  state manager is busy with `"load snapshot"` (`load_snapshot` recalls the snapshot's first ZS3
  inside `start_busy("load snapshot")` … `end_busy`, `:1106-1265`); recalling a ZS3 leaves them.
- **Global settings**: the core's `<model>.config` in `$ZYNTHIAN_CONFIG_DIR/ctrldev/dbm/`
  (desktop: `~/.config/zynthian/ctrldev/dbm/`), as the Mackie driver keeps its YAML in
  `$ZYNTHIAN_CONFIG_DIR/ctrldev` (`zyngine/ctrldev/zynthian_ctrldev_mackiecontrol.py:49-50`).
- **No generic options UI** for drivers (the "Controller Options" only have the scroll modes, with
  the bug Z3). A settings UI needs a zynthian patch (phase 4).

## 6. UI feedback

- `zynsigman.send_queued(S_GUI, SS_GUI_SHOW_MESSAGE, message=…)` shows a text in the Tk UI's title
  bar; used for `dbm_take_notifications`. Not shown by the Qt UI (§0).
- Displays the core drives over MIDI go through `dev_send_midi_event` (§2: ≤ 16383 bytes per
  message, dropped when the ring is full). Maschine Mk2/JAM are HID; they need `dbm-nihost`.

## 7. Packaging

- **nura** builds APKBUILDs with pmbootstrap for Alpine edge, aarch64 (the Duet) and x86_64 (tested
  in a rootfs) (`nura/README.md`, `docs/nura-packaging-guide.md`). zynthian-ui installs to
  `/usr/share/zynthian/zynthian-ui` (read-only), the native libraries to `/usr/lib/zynthian`,
  bundled Python packages to `/usr/share/zynthian/python`; the user's data under `~/.config` and
  `~/.local/share`. Not ZynthianOS/Debian as the first analysis guessed for nura.
- **Native library**: `libdrivenbymoss.so` for `aarch64-unknown-linux-musl` and
  `x86_64-unknown-linux-musl` (musl, not glibc). The default feature `lua` builds Lua's C source
  (mlua `vendored`), which needs a C compiler for the target; in an APKBUILD on Alpine that is
  the native gcc, in a cross build a musl cross compiler. See ZYNTHIAN_PLAN.md §5.
- **Python**: the binding is pure ctypes, Python ≥ 3.11 (Alpine edge has 3.12+ [guess]). The
  driver finds the packages on `sys.path`, `$DBM_PYTHON_DIR` or `/usr/share/drivenbymoss/python`
  and the library through `$DBM_LIBRARY` (default `/usr/lib/drivenbymoss/libdrivenbymoss.so`)
  (`zyngine/ctrldev/dbm_zynthian/__init__.py`).
- **Where the driver sources live** (decided): in the dbm repository, `backends/zynthian/` (GPL,
  as zynthian); the zynthian-ui `dbm` branch has only the changes to zynthian itself.
- **Where the drivers are installed** (decided: they ship in the drivenbymoss package): nura's user driver
  folder is per user (`$ZYNTHIAN_MY_DATA_DIR/ctrldev`, `~/.local/share/…`), which a package
  cannot fill, and a system `zyngine/ctrldev_user` folder would hide it (the manager takes
  `ctrldev_user` from the UI folder when it exists, §1). The package therefore installs the
  modules into the stock folder `/usr/share/zynthian/zynthian-ui/zyngine/ctrldev/` (the
  zynthian-ui package owns the folder, the drivenbymoss package its own files). On ZynthianOS the
  same files go to `zyngine/ctrldev_user/` (linked to the user's folder). Scanned only at UI
  start. See ZYNTHIAN_PLAN.md §5.

## 8. Recommendation

Unchanged: start with the **Novation Launch Control XL** (no stock driver, single port, a
conformance target, exercises the mixer), then a Launchpad on zynseq. The driver base is no longer
a sketch: `zyngine/ctrldev/zynthian_ctrldev_base_dbm.py` (ZYNTHIAN_PLAN.md §2).

**Main risks** (updated)
1. **Port names on nura/PipeWire** (§1): without the `dbm` branch's zynautoconnect fix no driver
   matches. The fix changes the uids of hardware MIDI ports.
2. **MIDI output**: per message ≤ 16383 bytes, dropped silently (no return value) when the ring is
   full; per JACK period limited by the port buffer [unverified]. LED storms are small; measure on
   the device.
3. **Note-input latency** (0–40 ms via Python, phase 3).
4. **Model mismatch** (no arranger, loop, markers, groups, chain colors; launch is a toggle; play
   is ambiguous; 4 parameters per screen).
5. **Python cost at 33 Hz** on the Duet/Pi [unverified].
6. **The user's MIDI filter and "system messages" switch** (§2) change or drop the controller's
   input before the driver.
7. Thread safety of zynthian's objects from the tick thread (as the stock drivers from the fast
   thread).

**Conflicts with stock drivers**: as the first analysis (APC40 mkII same dev_id; Launchpad X/Mini
MK3/Pro on another port of the same device; the generic Mackie driver). The Launchpads: the stock
drivers keep autoloading on the DAW port, the DrivenByMoss drivers (MIDI port) are picked by the
user and unload the stock driver of the same device (ZYNTHIAN_PLAN.md §7b).

## 9. Zynthian bugs found

Checked in the code of `nura` (zynthian-ui `2bb87436`) and zyncoder `nura` (`2e7490d`); not
reported upstream yet. "nura" says whether the bug is still there; "dbm" whether the `dbm` branch
fixes it.

| # | Where | Bug | Effect | nura | dbm |
| --- | --- | --- | --- | --- | --- |
| Z1 | `zynautoconnect/zynthian_autoconnect.py:272` (`dev_in_2_dev_out`) | `name.replace("IN", "OUT")` replaces every "IN" of the uid | A device whose name contains "IN" gets no output ("APC MINI IN 1" → "APC MOUTI OUT 1") | present | fixed (`dev_out_uid`) |
| Z2 | `zyngine/zynthian_state_manager.py:707-717` (`remove_slow_update_callback`) | The loop variable `cb` shadows the argument; `cb.remove(cb)` on the entry | No callback is ever removed; `add_slow_update_callback` can add one twice | present | – |
| Z3 | `zyngui/zynthian_gui_midi_config.py:346` | `set_scroll_mode(cal)` instead of `val` | Setting a driver's scroll mode always fails (NameError, logged) | present | – |
| Z4 | `zyngine/zynthian_state_manager.py:737-748` (`zynmidi_read`) | The inner sysex loop `break`s without `i += 1` on the word with `F7` | That word is processed again as an event: `izmip` = a data byte, the "message" starts with a data byte; it may reach another device's driver. A sysex whose `F7` is in its first word (≤ 2 data bytes) swallows the next event | present | dbm driver drops messages without status byte |
| Z5 | `zyncoder:zynmidirouter.h:176-177` | `FLAG_ZMIP_UI` and `FLAG_ZMIP_FILTER` are both `1` | Every zmip with the UI flag is filtered and vice versa (e.g. the SMF player input `ZMIP_SEQ_FLAGS` = UI goes through the MIDI filter) [effect: guess at the intent] | present | – |
| Z6 | `zyncoder:zynmidirouter.c:2140-2170` (`dev_send_*`), `:1992-1998` (`ui_send_all_notes_off*`) | `int` functions without `return` | Undefined return value; callers cannot see a full ring buffer | present | – |
| Z7 | `zyngine/ctrldev/zynthian_ctrldev_base.py:127-129` (`send_sysex_universal_inquiry`) | `if self.idev_out > 0` | TypeError when there is no output (None); the device in output slot 0 gets no inquiry | present | – |
| Z8 | `zyngine/ctrldev/zynthian_ctrldev_launchkey_mini_mk4_37.py:243` | `lib_zyncore.write_zynmidi(ev)` with `ev` a `bytes` object "to route to chains" | ctypes passes a pointer as the 32-bit word into the UI ring buffer: the keyboard notes never reach the chains, `zynmidi_read` gets a garbage event | present | – |
| Z9 | `zynlibs/zynseq/zynseq.py:632-648` (`transport_start/stop/toggle`) | The client name is passed as `bytes` to `transportStart(uint8_t id)` (no argtypes) | The id is the low byte of a pointer; `1 << id` with ids up to 255 overflows the 32-bit client mask; a stop with another pointer does not clear the start's bit, so the transport may not stop [effect: from the code] | present | dbm adapter calls libseq with a fixed id |
| Z10 | `zynlibs/zynseq/zynseq.h` (the `extern "C"` block) | `getTransportState()` is not declared | `libzynseq.so` exports only the mangled C++ name: no C/ctypes caller can read the transport state | present | fixed (declared in `zynseq.h`) |
| Z11 | `zyngui/zynthian_gui_base.py:544` (`on_title_timeout`), `zyngui/zynthian_gui_mixer.py:1394` (`clear_tempo_highlight`) | `threading.Timer` callbacks call Tk | After the Tk main loop has ended (exit) they raise "main thread is not in main loop" [observed at exit in desktop mode] | present | – |
| Z12 | `zynlibs/zynseq/zynseq.py:574-576` (`add_pattern`) | Calls `addPattern(chan, sequence, track, time, pattern, force)`: 6 arguments for `addPattern(scene, phrase, sequence, track, position, pattern, force)` | The arguments are shifted: the arranger GUI's call (`zyngui/zynthian_gui_arranger.py:752`, `add_pattern(scene, sequence, track, time, pattern)`) reaches the C function as phrase = sequence, sequence = track, track = time, position = pattern, pattern = 0, force undefined [from the code, not run] | present | dbm adapter calls libseq |
| Z13 | `zynlibs/zynseq/pattern.cpp:665-692` (`Pattern::setStepsPerBeat`) | `float fScale = …` inside the `else` declares a new variable; the outer `fScale` stays 1.0 | Changing the steps per beat does not rescale the events' positions and durations: the notes move in time (the GUI warns "may alter timing") | present | not used by the dbm drivers |
| Z14 | `zynlibs/zynseq/zynseq.cpp` `stop()` (called by `removePhrase`) | `g_mSchedule.map.clear()` drops all scheduled events, also the note-offs of notes already sent | Removing a phrase while sequences play may leave hanging notes [from the code, not run] | present | – |
| Z15 | `zynlibs/zynseq/` `SequenceManager` (`getSequence` and the scene and phrase lists) | No lock: loading a snapshot replaces the scene and phrase lists while other threads read them | A crash (segfault in `SequenceManager::getSequence`, through `getPlayState`/`getSequenceName`) when another thread reads zynseq during a snapshot load: three crashes of the Qt UI on the Duet (2026-11-07), from the DrivenByMoss drivers' tick thread. The drivers now pause while `state_manager.is_busy()`; a tick which started just before the load can still collide. Full fix: a lock around scene and phrase changes in zynseq (upstream candidate) | present | dbm drivers pause while busy |
| Z16 | PipeWire's ALSA sequencer MIDI bridge (PipeWire 1.6.11 on the Duet, not zynthian code) | After dbm-nihost was restarted (or the Mk2 dropped out and came back) while PipeWire and zynthian ran, the bridge kept the port "Midi-Bridge:Maschine MK2: In (capture)" and the ALSA subscription 128:0 → PipeWire, but forwarded no events from it to JACK | The Mk2 driver stayed dormant: dbm-nihost answered the query (seen with `aseqdump`), the hello never reached zynthian; the JAM of the same dbm-nihost worked [2026-11-08, Duet]. The same happened to the JAM after it dropped out and came back at 18:07:21 (around a zynthian start): dbm-nihost still answered over ALSA, nothing arrived through the bridge; the Mk2 had dropped out at 17:52 before its port got stuck. Likely trigger: a device dropping out and returning while dbm-nihost keeps its ALSA ports connected through the bridge (not reproduced on purpose yet). Related: the driver asks dbm-nihost only once at start (`loop()`, `query_device`); a lost answer leaves it dormant until a spontaneous hello, which is why unplugging/replugging was needed to bring the devices up. Diagnosed with `dbm-nihost-jack-probe` and `dbm-nihost-bridge-test` (support/linux/ni_host/tools). Workaround: restart dbm-nihost (`systemctl --user restart dbm-nihost`), then zynthian. Further observation [2026-11-08, Duet, dbm-nihost log]: both devices dropped out about 20 s after a zynthian start (zynthian 18:10:04, Mk2 gone 18:10:24, JAM gone 18:10:28); the JAM's hidraw node came back before udev had set its permissions ("Permission denied" at 18:10:30, opened at 18:10:31), so the devices really re-enumerate on USB, they are not only read errors. PipeWire had been restarted just after dbm-nihost (18:09:33 vs. 18:09:24). Neither the dropouts' cause nor the bridge getting stuck after one was reproduced on purpose. Not pursued further: zynthian is to use the Maschines through the HID library in-process, without dbm-nihost and PipeWire's bridge, which removes this path for zynthian | present | workaround |
| N1 | nura: `zynautoconnect/zynthian_autoconnect.py:1463-1553` (`build_midi_port_name`) under PipeWire | No JACK aliases on PipeWire's MIDI bridge ports → uid without `/` | No dev_id: no device-specific driver matches or autoloads, `dev_in_2_dev_out` finds no output (§1) | present | fixed (`zynthian_pipewire_ports.py`) |
