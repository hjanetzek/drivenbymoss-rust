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
