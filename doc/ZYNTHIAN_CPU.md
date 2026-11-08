# zynthian drivers: CPU use of the tick (profile and options)

Measured 2026-11-08 on the Lenovo Duet (zynthian Qt UI, niri session, Nura, Python 3.14, aarch64),
idle (no input, transport stopped), with the Maschine Mk2 connected through dbm-nihost and the
Maschine JAM **not** connected.

## Findings

- The two driver threads (`dbm DrivenByMoss Maschine Mk2`, `... JAM`) use **42 % and 30 % of a
  core**; zynthian as a whole 86 %. dbm-nihost: 5 %.
- **The JAM driver runs although no JAM is connected.** dbm-nihost creates the ALSA ports of both
  devices at start (`[ni_host] ALSA ports ... Maschine Jam - 1 ...` before any device is found), so
  zynthian loads the JAM driver and starts a full controller for it; zynthian keeps sending it
  meter values (`Maschine Jam - 1 <- DAW B0 40 xx / B0 41 xx` in `dbm-nihost --log-midi`).
- **Poll and diff, every tick.** `ZynReporter.collect()` re-reads the whole model on each pass and
  `Reporter.put()` compares every value with the last one:

  | Controller | collects in 3 s | `put` per collect | changed values per collect (idle) |
  |---|---|---|---|
  | Mk2 | 53 | 923 | 0 |
  | JAM | 54 | 968 | 0 |

  About 18 collects per second (the tick is 30 ms, `TICK_SECONDS`, but a pass takes longer), so
  ~17000 `put` calls per second per controller: `format_value()`, an f-string address, a dict
  lookup and store each, for nothing.
- **Where the time goes** (sampling the Mk2 thread's Python stacks every 2 ms for 15 s, through
  `sys.remote_exec`; the thread was busy 34 % of the samples):

  | Inclusive | Function |
  |---|---|
  | 84 % | `reporter.collect` |
  | 42 % | `collect_tracks` |
  | 38 % | `collect_slots` (per track, per phrase: `play_state`, `has_content`, `slot_name` and 11 `put`) |
  | 23 % | `launcher.pattern` / `has_content` (zynseq lookups) |
  | 12 % | `collect_grids` / `clip.playing_step` |
  | 11 % | `collect_device` / `collect_pages` |
  | 10 % | `collect_clip` / `model.cursor_clip` |
  | 7 % | `zynseq.get_sequence_name` |
  | 6 % | `collect_vu` |
  | 8 % + 7 % | `take_notifications`, `tick` (the core) |

- **Native side** (`perf record -g` on both threads, 15 s): 86 % `libpython3.14` (the interpreter,
  `_PyEval_EvalFrameDefault` 36 %), 7 % musl (`memcmp`, `memcpy`, `dlsym`), **1.2 %
  `libdrivenbymoss.so`**, 0.9 % `libzynseq.so`, ctypes + libffi ~2 %. The core and zynseq are cheap;
  the cost is the Python code calling them value by value.

How to measure again (on the device, zynthian running):

```sh
# Python stacks: inject a sampler thread (scripts in the zynthian-ui session's scratchpad: a loop
# over sys._current_frames() for the "dbm ..." threads) with
sudo python3 -c "import sys; sys.remote_exec(PID, '/tmp/dbm-sampler.py')"
# native:
sudo perf record -F 499 -g -t TID1,TID2 -o /tmp/dbm.perf -- sleep 15
sudo perf report -i /tmp/dbm.perf --no-children -g none --sort dso,sym
```

(py-spy 0.3.14 from Alpine doesn't support Python 3.14.)

## Options, roughly in order of gain per effort

### Python (no new native code)

1. **No controller without its device.** dbm-nihost already sends the hello SysEx
   (`F0 7D 44 42 4D 11 01 F7`) when it finds a device; let it send a "gone" message when the
   device disappears, and keep the driver dormant (no bridge, `sleeping`) until the hello. That
   halves the CPU use with one device, and also fixes the black displays/LEDs after a dbm-nihost
   restart or a replug: on hello, re-send the whole state (`dump_all` / forget the last values).
   Workaround meanwhile: `dbm-nihost --mk2` (or `--jam`) when only one device is used.
2. **Two rates.** Collect what changes by itself every tick (transport position, playing step,
   VU, playing notes), the rest (tracks, channels, sends, slots, device pages, names) at a few Hz,
   or only when zynthian signals a change (zynsigman: chain and processor changes, zynseq state,
   controller value changes). With the slots and pages out of the fast path, ~80 % of the pass is
   gone.
3. **Cheaper `put`.** Compare the raw value before `format_value()` (keep the last raw value),
   and build the per-track / per-slot address prefixes once instead of f-strings per call.
4. **Cache slot data.** `has_content` and `slot_name` change only when sequences are edited:
   cache them per (phrase, channel) and invalidate on zynseq edits; read the play states of a
   bank in one pass.

### Native code

5. **One zynseq call per bank** (C++, libzynseq; upstreamable to zynthian): a function that
   fills a buffer with the play state, "has content" and a name change counter of every
   sequence of a bank, replacing ~3 ctypes calls and their Python wrappers per slot per pass.
6. **zynseq data read by the core.** Slots, clip, playing step, transport and VU come from
   libzynseq / zynmixer (C APIs): the Rust core could read them itself (dlopen libzynseq, poll and
   diff in Rust), leaving Python only the zynthian objects (chains, processors, controllers:
   names, channels, sends, device pages), which change rarely and fit the slow rate of option 2.
   That moves the hot ~60 % (slots, grids, clip) into Rust, at the price of the core depending on
   zynseq's API.
7. **Diff in Rust.** Pass the collected values in bulk (one buffer per collect) and compare in the
   core: removes `put`/`format_value`, but not the reads; less gain than 2 or 6.

Recommendation: 1 and 2 first (Python, no API changes, the biggest part of the gain), then 3/4;
native code (5 or 6) only if the remaining cost still matters, 5 being the smaller step.

## Done (2026-11-08): options 1–3

### 1. No controller without its device

- **dbm-nihost** (`mk2_host`, `jam_host`, `daemon.cpp`; the messages in `src/midi.h` and the README):
  - Before: the Mk2 sent its hello `F0 7D 44 42 4D 11 01 F7` when attached, on `dawConnected`
    (`--virmidi`) and as the answer to the query `F0 7D 44 42 4D 11 F7`, the last two **also
    without the device**. The JAM sent no dbm hello, only NI's "return from host" when attached,
    and ignored the query.
  - Now: the query is answered only while the device is attached (the Mk2 with its hello, the JAM
    with "return from host", which its core already handles: a forced flush); `dawConnected` says
    hello only with the device. New: **gone** `F0 7D 44 42 4D 13 F7` when the device disappears
    (unplugged, or dbm-nihost stops: the hidraw thread reports the detach at its stop too). `13` is
    free: the core uses `10` (labels), `11` (query, hello), `12` (Shift); `01`–`03` belong to the
    conformance runner. The core doesn't know `13`: on Bitwig an unplug logs "Unhandled Sysex
    message" once.
  - Tests in `tests/host_test.cpp` (`testMk2Presence`, `testJamPresence`).
- **The Maschine drivers** (`nihost_hello` in the Mk2 and JAM modules, the logic in
  `zynthian_ctrldev_base_dbm.py`: `query_device`, `take_midi`, the loop): the query once at the
  start, dormant (no bridge, no core, MIDI dropped) until the hello, dormant again after gone.
  A hello which answers a query (the driver's or the Mk2 core's own, seen in `send_midi`) goes to
  the core, so the Mk2's real Shift button is still announced; a hello which answers none (a replug,
  a restarted dbm-nihost) restarts the controller, which sends all LEDs and displays again (the
  black displays after a dbm-nihost restart). The LCXL and Launchpad drivers are unchanged
  (`nihost_hello = None`). Tests: `test_driver.py` `NihostDeviceTest`.
- With an older dbm-nihost: the Mk2 driver runs as before (the old host answers always), the JAM
  driver waits for a replug of the JAM.

### 2. Two rates

`dbm_zynthian/reporter.py`, `bridge.py`, `commands.py`:

- **Fast, every pass:** `collect_transport`, `collect_meters` (the VU of the tracks and the master
  of the last full pass, the playing notes of the cursor track), `collect_playing_steps`.
- **Slow part** in subtrees (`SUBTREES`: host, tracks, mixer, slots, transport, device, clip,
  browser), each callable alone (`collect_subtrees(set)`); for now a full pass collects all.
- **A full pass** after a command of the core (`ZynCommandHandler.handle` → `request_full`), a
  change signal of zynthian (registered by `ZynBridge.start`, see below), a dump (`refresh`, the
  driver's chain/snapshot refresh), a due "report again" of a value command (300 ms), else every
  `SLOW_INTERVAL_MS` = 250 ms. The order of the lines in a full pass is unchanged.
- **Signals used** (`bridge.change_signals`, each with the subtrees it affects; direct callbacks
  that only set the reporter's flag): `S_MIXER/SS_ZYNMIXER_SET_VALUE` (level, balance, mute, solo,
  record arm, also from zynthian's UI) → mixer; `S_PROCESSOR/SS_PROCESSOR_BYPASS`,
  `SS_PROCESSOR_CTRL_SCREENS` → device; `S_CHAIN_MAN/SS_SET_ACTIVE_CHAIN`, `SS_ADD_CHAIN`,
  `SS_REMOVE_CHAIN`, `SS_REMOVE_ALL_CHAINS`, `SS_MOVE_CHAIN` → tracks, mixer, slots, device, clip,
  browser; `SS_RENAME_CHAIN` → tracks; `SS_ADD_PROCESSOR`, `SS_REMOVE_PROCESSOR` → device, browser;
  `S_STEPSEQ/SS_SEQ_PLAY_STATE` → slots; `SS_SEQ_STATE`, `SS_SEQ_SELECT_PHRASE` → slots, clip;
  `SS_SEQ_TEMPO`, `SS_SEQ_METRO` → transport; `SS_SEQ_TIMESIG` → transport, clip;
  `S_STATE_MAN/SS_LOAD_ZS3` → all. Not used: `SS_SEQ_BEAT` (every beat), `SS_AUDIO_RECORDER_ARM`
  (defined, never sent; the arm comes with zynmixer's `record`).
- **No signal** (the 250 ms pass covers them): the send levels (zynmixer's `set_send` sends none),
  the parameter values of the processors (also when changed in zynthian's UI or by MIDI learn),
  the sequences' names and note edits in the pattern editor (`libseq.addNote` directly), the
  position in zynthian's preset and Add Chain views (the browser), the play states between
  zynseq's `update_state` calls (the status thread, every 200 ms; then `SS_SEQ_PLAY_STATE`).
- Tests: `test_rates.py`. The model tests which change the stand-ins directly and call `collect`
  ask for a full pass (`request_full`) in their `report()` helper: the rate is the only difference;
  the stand-ins now send zynthian's signals (`zynstubs.send_signal`: zynmixer's for the mixer
  strips, the chain manager's for add/remove/active chain; delivered at once). No comparison was
  changed; the conformance runs (`test_conformance_lcxl.py`) pass unchanged.

### 3. A cheaper `put`

- `dbm_host.Reporter.put` compares the raw value (same type and equal) before formatting it
  (`last_raw`, only immutable values; `forget` drops it too); `put_displayed` builds its `/6`, `/8`
  addresses once. The reference host's output is identical (all reports of `bindings/python/tests`
  compared before/after).
- `ZynReporter` builds the track, effect track and scene prefixes, the slot addresses, the parameter
  addresses of the pages, the 128 `/primary/pad` addresses and the playing step addresses once.

### Numbers

`backends/zynthian/tests/test_cpu.py`: the Mk2 against the stand-ins (8 chains, 8 phrases, a synth
with 4 pages), idle, 300 ticks of 30 ms on simulated time, x86 (this host, not the Duet):

| | `put` per tick | `collect()` mean | median |
|---|---|---|---|
| before | 1585 | 1.52 ms | 1.51 ms |
| option 3 only | 1585 | 1.13 ms | 1.13 ms |
| options 2 + 3 | 227 (1585 in a full pass, ~60 in a fast one) | 0.17 ms | 0.05 ms (fast pass) |

33 full passes in 300 ticks (one per 250 ms). Per controller: ~9x less time in `collect()`; with
option 1 the JAM driver costs nothing without a JAM. On the Duet the profile had 18 collects per
second at ~923 puts (each pass overran the 30 ms tick); expected now: up to ~33 cheap fast passes
and 4 full ones per second (not measured there yet).

To check on the Duet: the method above (`sys.remote_exec` sampler, `perf`); expected: the JAM
thread idle without a JAM, the Mk2 thread well below its 42 %.

## Next: event-driven collection per subtree (plan)

Signals mark subtrees dirty and only the dirty subtrees are collected; polling stays only for the
continuous values (transport position and play state, playing step, VU, playing notes) plus a slow
safety-net pass (every few seconds instead of 250 ms).

- **The adapter:** `ZynReporter.dirty` already collects the subtrees per signal and
  `collect_subtrees` collects a set; the step is `collect_subtrees(dirty)` instead of all, then
  finer keys (per chain for the mixer and sends from the signal's `chan`/`mixbus`, per phrase or
  bank for the slots from `SS_SEQ_PLAY_STATE`'s `phrase`/`chan`). Commands of the core: dirty
  subtrees from the command's processor (`track`, `device`, `clip`, …) instead of all.
  The "report again" (300 ms) of a command: the subtree of its prefix.
- **zynthian additions** (zynthian-ui, branch `dbm`):
  - **zctrl value change signal**: in `zynthian_controller.set_value` / `send_value`
    (`zyngine/zynthian_controller.py`), e.g. `S_PROCESSOR/SS_ZCTRL_SET_VALUE` (the Launchkey Mini
    Mk4 driver already refers to a `SS_ZCTRL_SET_VALUE` which the signal manager lacks) with the
    zctrl. Care for MIDI rates (a CC stream calls `set_value` per message): coalesce, e.g. send only
    on the first change after the last delivery (a dirty flag per zctrl reset by the receiver) or
    queued and deduplicated per zctrl. Small: ~20 lines in the controller and the signal manager,
    plus the constant. Covers the processor parameters, and with it the send levels.
  - **Send levels**: zynmixer's `set_send` (`zynlibs/zynmixer/zynmixer.py`) without
    `SS_ZYNMIXER_SET_VALUE`; one `zynsigman.send` there, as for `level` (a few lines), or covered by
    the zctrl signal.
  - **zynseq content, names, note edits**: `SS_SEQ_STATE` comes only from `refresh_state` (phrases
    inserted/removed/duplicated, channels enabled, a state loaded), not from note edits (the pattern
    editor calls `libseq.addNote`/`removeNote` directly) nor from renaming a sequence. Either a
    change counter per pattern / per bank in libseq (C++: incremented in the pattern's
    add/remove/clear/copy, the sequence name; one getter; ~50 lines, also usable by option 5) which
    the fast pass polls cheaply, or a signal from the pattern editor's edit paths (Python, several
    places in `zyngui/zynthian_gui_patterneditor.py`/`zynthian_pated_*.py`, easy to miss one).
    The counter is the safer one.
  - **The browser / Add Chain views**: a signal when the selection of zynthian's list views changes
    (`zyngui` selector screens, `select()`); small, but per UI (Tk and Qt). Until then the browser
    stays on the safety-net pass while it is open (it is cheap while closed).
  - The play state latency (zynseq's `update_state` every 200 ms in the status thread) stays as in
    zynthian's own UI.

## Also seen

- A freeze (LEDs frozen, no button reports, black displays after restarting dbm-nihost): the Mk2
  sent only its pad pressure stream (report type 0x20, ~750/s) and no button reports (0x01), and
  dbm-nihost wrote nothing to the device. A fresh dbm-nihost (as a separate user unit with
  `--log-midi`) has run stable since; the old instance had been running for hours through several
  replugs. Unresolved; if it happens again, `journalctl --user -u nihost-debug` (or the service with
  `--log-midi`) has the MIDI traffic before it.
- After a dbm-nihost restart the displays and LEDs stay black until zynthian sends something new:
  dbm-nihost clears them at start (reports 0x80-0x82 and 0xE0), and the driver doesn't know about
  the restart (the ALSA ports stay) — see option 1.
- The Mk2's pad pressure stream (~750 reports/s) costs dbm-nihost ~5 % of a core while idle.
