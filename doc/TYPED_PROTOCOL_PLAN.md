# A typed, binary protocol between the core and its hosts: plan

Status: **steps 0a, 0b, 1, 2, 3 and 5 done** (2026-11-08, branch `typed-protocol`; step 5, §6.6:
the hosts without the text path, exact doubles, the command features of a binary host from its
handled commands; Reaper stays on the text protocol, step 4 out of scope): the command table and
the address table in `src/protocol/` (`Cmd`/`CmdId`, `Msg`/`MsgId`); the binary records, the
binary C ABI next to the text one, the generated `include/dbm_protocol.h` and C++
`dbm_protocol.hpp`, `protocol.py`; the replay passes in both forms. The Bitwig extension uses the binary protocol (step 3, §6.5).
The grid ID: two slots, channel and row packed into one (§5.3 Q7, decided after step 1). What was done, the choices and the deviations: §6. The decision and the motivation are in
STATUS.md, "Next steps and open issues", "Planned: a typed protocol between core and hosts". The
current text protocol is described in [HOST_BRIDGE_GUIDE.md](HOST_BRIDGE_GUIDE.md) §2–§4 (from
the Bitwig side) and [BACKEND_PROTOCOL.md](BACKEND_PROTOCOL.md) (from the Reaper side). The CPU
numbers are from [ZYNTHIAN_CPU.md](ZYNTHIAN_CPU.md).

**Scope** (decided 2026-11-08): the Bitwig extension, the Python binding with the reference host,
the Zynthian adapter and the C++ wrapper. **Reaper is out of scope:** it is not a target of the
project, and its backend stays on the text protocol, unchanged (§3, step 4).

**Summary**
- **Size:** about 250 host → core address templates (about 420 once they are expanded per
  device, channel bank and named parameter) and about 300 core → host command forms (plus 6
  value operations). The 21 recordings use 363 and 104 of them.
- **The table** (decided 2026-11-08): Rust tables in `src/protocol/` (a `macro_rules!` table),
  which the core uses directly. A generator binary, like `dbm-catalog`, writes the definitions for
  C, C++, Python and Java from them. They are committed like `include/drivenbymoss.h`, and a test
  fails if one is stale.
- **The host API** (decided 2026-11-08): generated view wrappers in every language. A message acts
  like an object (`msg.track`, `msg.value`) over a record in the shared buffer, without copying or
  decoding it (§2.7).
- **The wire format:** fixed 24-byte little-endian records (id, op, 4 × u16 indices, an 8-byte
  inline value), with strings and lists as a padded payload after the record.
  - Host → core: one buffer per report.
  - Core → host: one buffer per call, delivered before any other callback and at the end of the
    call.
- **The first step:** the table and the generated Rust enums, with the text protocol on the wire
  unchanged (§5.4). The core then sends `Cmd` values and dispatches `Msg` values, and the text is
  produced and parsed only at the boundary. Every use becomes findable, and the compiler checks
  each one. All existing tests, replays and conformance runs stay as they are and prove the
  step.
- **CPU:** a modest gain for the Python hosts after ZYNTHIAN_CPU options 1–3. The `put` side of a
  full Zynthian pass gets about 3× cheaper, which is 8–10 % of the pass. Most of a pass is reading
  the zynthian model, which a protocol change does not touch (§4). The main payoff is
  correctness and findability; the CPU gain is a side effect.

---

## 1. Inventory

### 1.1 The current code paths

| Side | Where | What |
| --- | --- | --- |
| Host → core, C ABI | `src/ffi.rs:483` `dbm_update_model(app, const char*)` → `src/app.rs:399` `App::update_model` | Splits on `\n` and feeds each line to every running instance's `Parser::parse_line` |
| Parser | `src/parser.rs:129` `parse_line`, `:156` `parse_osc`, dispatch at `:170–305` | Hand-written `match` on path segments, about 1,490 lines of code (tests from `:1488`). Unknown paths go to `host.error("Unhandled …")`, about 60 places, and are seen only at run time |
| Parser helpers | `src/parser.rs:85–110` | `parse_double`, `parse_flag` (> 0), `parse_int` (i32 or truncated f64), `parse_limit` (`/str/<n>`), `parse_index` (digits only), `parse_color_static` `:1473` (`r g b`, `r < 0` = none) |
| Parser events | `src/parser.rs:24` `ParserEvent` | Rebind knobs, note mapping, note repeat, quantize, action selected |
| Core → host, model side | `src/daw/context.rs:340–377` `send`, `send_int`, `send_double`, `send_bool`, `send_str`, `send_strs` (suppressed while `muted`, `:213`) → `src/backend.rs:134` `Sender` → the `Backend` trait `:102` | 236 call sites in `src/daw/` and `src/framework/` (the most: `application.rs` 42, `transport.rs` 39, `clip/cursor_clip.rs` 31, `device.rs` 26, `browser.rs` 17, `parameter.rs` 14, `channel.rs` 12) |
| Command strings | built with `format!` at the call sites, e.g. `src/daw/channel.rs:560` `create_command` (`<p>/…`, `pad/<i>/…`, `layer/<i>/…`), `src/daw/parameter.rs:376` `value_address` and `:409–431` (`/inc`, `/immediately`), `:320` (`/reset`), `:335–370` (`/touch`, `/indicate`), `src/daw/slot.rs:160`, `src/daw/scene.rs:81`, `src/daw/clip/cursor_clip.rs:200–212` (`grid/<id>/…`), `src/daw/device.rs:67–90`, `:845` | No central list of commands. The processor is an enum (`src/backend.rs:12` `Processor`, 38 values; `Session`, `PlayingNotes`, `Cursor` are unused or used once); the command is free text |
| Feature gating | `src/daw/context.rs:68` `HostFeature` (9), `:176` `host_supports`, `:185` `set_host_features` | Bitset in `Shared::host_features`; the compatibility rule "paging without `/host/features` = all" |
| C ABI callbacks | `src/ffi.rs:32` `DbmCallbacks`, `:96` `impl Backend for FfiBackend` | `process_no_arg`, `_string_arg`, `_string_args`, `_int_arg`, `_double_arg`, `delay_updates`, `enable_updates`; each call allocates two `CString`s |
| Header | `build.rs` + `cbindgen.toml` → `include/drivenbymoss.h` (committed, rewritten only when changed) | cbindgen 0.29 is the only build dependency |
| Test double | `src/backend.rs:188` `BackendCall`, `:209` `RecordingBackend` | 238 assertions on `BackendCall::{NoArg,IntArg,DoubleArg,StringArg,StringArgs}` text in 34 files; 147 calls of `update_model`, `parse_line` or `feed` with text lines in the tests |
| Recorder | `backends/bitwig/.../ProtocolRecorder.java:208` `event`; `DbmExtension.java:302` records `model` before `core.updateModel` | `.rec`: `<ms>\t<kind>\t<fields>`, HOST_BRIDGE_GUIDE §8 |
| Replay | `tests/conformance_replay.rs:166` `replay`, `:245` feeds `model` text, `:108` `format_call` turns `BackendCall` back into `noArg/int/double/string/strings` text, `:70` `is_output` | Compares the outputs after each input as sets. 21 recordings (9.9 MB in `tests/conformance/*/replay/`) |

**Hosts**

| Host | Reporting (host → core) | Commands (core → host) |
| --- | --- | --- |
| Reaper (C++) | `backends/reaper/reaper_drivenbymoss/Collectors.h:20/32/40` `CollectStringValue/IntValue/DoubleValue` (compare with the last value, append `address value\n` to an `ostringstream`); `DataCollector.cpp:49` `CollectData` (86 `Collectors::Collect*` calls) plus `Track.cpp:43–95`, `Send.cpp`, `Parameter.cpp`, `Marker.cpp` | `dllmain.cpp:116–142` → `OscParser.cpp:96–233` `Process(processor, command, value)` splits the command into a deque of strings → `*Processor::Process(elements, value)` (`TrackProcessor.cpp` 751 lines, `ClipProcessor.cpp` 552, `TransportProcessor.h` 341, …) |
| Bitwig (Java over FFM) | `ModelReporter.java:103–156` `put(address, String/int/boolean/double)`, `putColor`: a `HashMap<String,String>`, doubles via `String.format(Locale.ROOT, "%.6f")`; 166 `reporter.put` calls in `BitwigModel.java`, 16 in `SpecificDevice`, 13 in `BitwigNoteClip`, 18 in `BitwigBrowser`, 11 in `EqualizerDevice`; `HOST_FEATURES` at `BitwigModel.java:83/549` | `NativeCore.java:140–159` upcalls → `CommandHandler.java:76` `handle(processor, command)`, `:123` `handle(processor, command, double)` (1,646 lines, 293 `case` labels); `:620` `reportAgain` → `ModelReporter.forgetLater(prefix)` |
| Python reference host | `bindings/python/dbm_host/reporter.py:78` `format_value`, `:171` `put` (raw-value compare first, then text compare), `:188` `put_displayed`, `:200` `take` | `bindings/python/drivenbymoss/_ffi.py:83–97` ctypes callbacks → `dbm_host/commands.py:74` `Command.parse` (splits off `/inc`, `/reset`, `/immediately`), `:139–161` `register` per processor, `:186` `handle`; `:213–217` `forget_later` by text prefix |
| Zynthian | `backends/zynthian/ctrldev/dbm_zynthian/reporter.py:82` `ZynReporter(Reporter)`, `:173` `collect`, `:336` `collect_slots`, `:471` `collect_pages`; 127 `put` calls | `dbm_zynthian/commands.py` (on `dbm_host.CommandHandler`), `forget` by text address `:156–160` |
| C++ wrapper | `bindings/cpp/include/drivenbymoss.hpp:52–78` `Backend` virtuals with `std::string` processor and command; `:247` `updateModel(std::string)` | as on the left |

### 1.2 Host → core addresses

Counted as templates: indices are placeholders, and a template shared by several owners (for
example `/<dev>/param/<i>/value` for device, primary, eq, project, track FX and master FX) counts
once. "Expanded" counts each owner separately. Line numbers are in `src/parser.rs`.

| Area | Templates | Expanded | Parser | Value types | Reaper-only |
| --- | --- | --- | --- | --- | --- |
| Transport, time, click, automation, overdub, note repeat | 48 | 48 | `parse_transport` `:417`, `parse_time` `:536`, `parse_beats` `:511`, `parse_click` `:386`, `parse_note_repeat` `:1431`, `/crossfade` `:289` | bool, int, f64 (BPM, seconds, quarters, 0..1), verbatim text, enum as int (`/automode` 0–5, `/noterepeat/mode` 0–3) | `/stop`, `/time/str`, `/time/loop/*` (6), `/beat`, `/quantize/strength`, `/click/preroll` (text) |
| Application, project, host, layout, arranger and mixer flags, record quantization, groove, cue | 50 | 50 | `:171–305`, `parse_groove` `:1449`, `parse_host_parameter` `:313` | bool, text, enum as text (`/layout` ARRANGE…, `/recordQuantization` OFF, 1/32…), name list (`/host/features`) | `/groove/active`, `/groove/amount`, `/action/select` |
| Channels: `/track/<p>`, `/fxtrack/<p>`, `/master` (fields, sends, slots, track FX head) | 54 | ≈105 | `parse_track` `:582`, `parse_effect_track` `:614`, `parse_master_track_value` `:633`, `parse_track_value_in` `:683`, `parse_send` `:854`, `parse_slot` `:821` | bool, int, f64, text, color, enum as text (`type`), list (`playingnotes`) | `depth`, `vuholdleft/right`, `overdub`, `inQuantLengthEnabled`, `inQuantResolution`, `/master/fx/param/*` |
| Scenes | 9 | 9 | `parse_scene` `:1302` | bool, int, text, color, f64 | `position`, `endPosition` |
| Devices (`device`, `primary`, `eq`, `drum`): head, siblings, drum flags, EQ bands | 19 | ≈47 | `parse_device_of` `:901`, `parse_sibling` `:1049`, `parse_drum` `:330` | bool, int, text | `/device/touchedParam` |
| Parameter banks and pages (6 owners) | 9 | 54 | `parse_device_param_value` `:1148`, `parse_track_fx_parameter` `:1086`, project `:200–237` | text, f64, int, limited text | – |
| Drum pads and layers (`drum`, `device`, `primary`) | 12 | ≈57 | `:991–1012`, `parse_device_channel` `:360` | bool, text, color, f64 | – |
| Clip and step grid | 15 | 15 | `parse_clip_value` `:1360` | bool, f64, color, int, **step** (24 numbers), **notes** list, Reaper `/clip/all` | `/clip/playposition`, `/clip/notes`, `/clip/all` |
| Browser | 24 | 24 | `parse_browser_value` `:1184`, `parse_browser_item` `:1253` | bool, int, text | `/browser/presetsfile`, `/browser/selected/index` |
| Markers | 7 | 7 | `parse_marker` `:1263` | bool, int, text, color | (`position`, `endPosition` are accepted and ignored) |
| **Total** | **≈247** | **≈416** | | | ≈30 Reaper-only |

The 21 recordings contain 2,610 `model` events with 233,610 lines (27.7 bytes per line on
average) and 363 distinct templates. By first segment: track 59, primary 43, device 37, fxtrack
28, groove 24, project 22, drum 21, browser 21, master 15, eq 13, clip 12, and 1–7 each for the
rest.

### 1.3 Core → host commands

Counted as forms: processor plus command template. A value command counts once, not once per
suffix.

| Area | Forms | Built in | Value / legacy callback |
| --- | --- | --- | --- |
| Value commands (the targets of §1.5): track/fxtrack volume and pan, send volume, master volume and pan, `param/<i>/value` (device, primary, eq, project, track), pad/layer volume and pan, cue volume/mix, groove (6 Bitwig names and 2 Reaper), crossfade, metronome volume, `master user/param/0/value`, clip accent | 14 targets × up to 6 ops | `src/daw/parameter.rs:320–431`, `src/daw/transport.rs:155/184`, `src/daw/clip/cursor_clip.rs:550/561` | double, int (`/touch`, `/indicate`), no arg (`/reset`) |
| Transport and time (`play`, `stop`, `record`, `repeat`, `tempo`, `time`, `beats`, `transport …`, `automation`, `metro_vol`, `quantize`, `noterepeat`, `undo`, `redo`, `inifile`, `action`) | ≈55 | `src/daw/transport.rs` (39 sites), `application.rs:156/164/258` | none, int, double, string (`action <id>`); `time signature` packs `num + den*100` into an int |
| Application, arranger, mixer, project, `bank`, `refresh` | ≈54 | `src/daw/application.rs`, `project.rs`, `track_bank.rs:380/391`, `bank.rs:151`, `backend.rs` `refresh` | none, int, string (`recordQuantization`) |
| Tracks and master | ≈33 | `src/daw/channel.rs`, `track.rs`, `track_bank.rs`, `model.rs:486–573/832` | none, int, double (`send/<s>/active` sends the send volume as a meaningless value), string (`name`, `color` as `RGB(r,g,b)`), strings (`addTrack`, `track_bank.rs:873`) |
| Slots and scenes | ≈22 | `src/daw/slot.rs:160–205`, `scene.rs:81–111/232`, `track_bank.rs:598` | none, string (`insertFile` path, `paste` as `"<track>/<slot>"`, `color`) |
| Devices, siblings, pads, layers, EQ | ≈38 | `src/daw/device.rs` (26 sites), `channel.rs:521/525` | none, int (`selected` 1-based, `window`, `bypass`), string (`eq band/<b>` as a stringified int, `add/0`) |
| Clip, grid, steps | ≈55 | `src/daw/clip/cursor_clip.rs` (31 sites) | none, int, double, string (`set` `"<vel> <dur>"`, `setFrom`/`update` 24 numbers, Reaper `note/<pitch>/…` 5 numbers) |
| Markers | 6 | `src/daw/marker.rs:59–155` | none |
| Browser | ≈20 | `src/daw/browser.rs:120–455` | none, int |
| Navigator | 7 | `src/framework/clip_launcher_navigator.rs:44/136` | none, int |
| Update hints (`delay_updates`, `enable_updates`) | 2 | `src/daw/context.rs:328–337` (8 sites) | processor (+ bool) |
| **Total** | **≈306** | | |

The recordings use 104 command templates. The most frequent are the indication commands, for
example `track <n>/send/<n>/volume/indicate` 1,016 times and `device param/<n>/indicate` 456
times.

### 1.4 Indices

| Index | Domain | Base and notes | Used in |
| --- | --- | --- | --- |
| `p` track position | 0..track count | absolute, 0-based; the master has none (`MASTER_POSITION`, `context.rs`) | `/track/<p>/…`, `track <p>/…`, `fxtrack` |
| channel bank | track, fxtrack (master separate) | an enum in the first segment | `/track` vs `/fxtrack`, the `track` vs `fxtrack` processor |
| `s` send | 0..numSends | | `…/send/<s>/…` |
| `s` slot / scene | 0..scene count | absolute | `…/slot/<s>/…`, `…/clip/<s>/…`, `/scene/<s>/…`, `createClip/<slot>` |
| `i` parameter | 0..param count | flat over the pages, page = i/8; EQ: `3*band + {0,1,2}` | `/<dev>/param/<i>/…`, `param/<i>/…` |
| page | 0..page count | | `/<dev>/page/<i>/name` |
| owner (device role) | device, primary, eq, drum, project, track FX, master FX | an enum in the first segment or the processor | everything under §3.5–3.6 of the guide |
| sibling | **1-based** in reports (`/device/sibling/<n>`) and in Reaper commands (`device <i+1>/…`, `device selected <i+1>`) | the only 1-based indices | `parse_sibling` `:1049`, `device.rs:67–140` |
| pad, layer | 0..127, 0..layer count | | `/<dev>/pad/<i>/…`, `pad/<i>/…` |
| grid id | `<steps>x<rows>` from the model setup's `noteClips` | **two numbers in one segment** | `/clip/grid/<id>/…`, `clip grid/<id>/…` |
| channel, step x, row y | MIDI channel, step, row or pitch | | `…/step/<ch>/<x>/<y>`, `clearRow/<ch>/<row>`, `clearColumn/<ch>/<step>` |
| browser column, item, result, content type | 0..7, 0..n | | `/browser/column/<c>/item/<i>/…` |
| marker | absolute | | `/marker/<i>/…`, `marker <i>/…` |
| EQ band | 0..7 | | `/eq/band/<b>`, `eq band/<b>` |
| limit | 6, 8 (no index = full text) | a trailing segment | `…/str/<limit>` |
| named parameter | groove (6), cue (2), arranger flags (6), mixer flags (6) | **names, not numbers** | `/groove/<name>`, `/project/cueVolume`, `/arranger/<flag>` |
| Reaper note | pitch, channel | | `clip note/<pitch>/…` |

At most 4 indices appear together: `/clip/grid/<id>/step/<ch>/<x>/<y>`, with the grid id
counted as one.

### 1.5 Irregular forms

- **Value command suffixes** (HOST_BRIDGE_GUIDE §4.1), built in `parameter.rs`:
  - `/inc` (double, feature `valueInc`);
  - `/reset` (no arg, `valueReset`);
  - `/immediately` (double, `immediately`);
  - `/touch` (int);
  - `/indicate` (int; for volume, pan and sends gated by `indication`).

  Hosts strip them again: `Command.parse` in `dbm_host/commands.py:74`, the Bitwig extension's
  `/reset` → value path. Clip positions use `/inc` with beats, not normalized values.
- **Displayed texts:**
  - `<value>/str` and `<value>/str/6|8` on tracks, sends and parameters;
  - `/name`, `/str`, `/str/<n>` on the host parameters (`parse_host_parameter` `:313`: crossfade,
    cue, groove);
  - exceptions: `/click/volumeStr` (not `/str`); `/time/str` (formatted by the core) vs
    `/time/text` (verbatim);
  - `/time/loop/start/<anything>` is a beat text (`:564` `Some(_)`);
  - Bitwig appends `/value` to `/eq/band/<b>`, which is ignored.
- **Inconsistent field names:**
  - slot `selected` vs scene `select`;
  - `/browser/column/<c>/item/<i>/select` vs `/device/sibling/<n>/selected`.
- **Values with meaning beyond the value:**
  - `/track/offset` and `/scene/offset` (the value is ignored; the report switches on host
    paging);
  - `/track/<p>/select` (only changes count);
  - `/track/pinned` (a position or −1);
  - `/device/position` −1 = none;
  - colors with `r < 0` = none, and parse errors become gray.
- **Host-feature-dependent pairs**: the same action as two different commands (the core picks
  one, the host must know both):

  | Basic (Reaper) form | Feature form | Feature |
  | --- | --- | --- |
  | `play`, `stop`, `record` processors | `transport play\|stop\|restart\|stopAndRewind` | `transportCommands` |
  | `time 0` (int) | `transport stopAndRewind` | `transportCommands` |
  | `time loop/start\|length` (seconds) | `beats loop/start\|length` (quarters) | host step grid / Bitwig |
  | `metro_vol <0..127>`, `metro_vol +\|-` | `transport metronomeVolume[/inc]` | `transportCommands` |
  | `master user/param/0/value` | `transport crossfade` | reported `/crossfade` |
  | `groove active\|amount` | `groove <name>` | reported `/groove/<name>` |
  | `device <i+1>/bypass\|remove\|duplicate`, `device window <0\|1>` | `device sibling/<i+1>/…`, `<dev> window/toggle\|bypass\|…` | `deviceCommands` |
  | `track <p>/solo <0\|1>` | `track <p>/solo/toggle` | `soloToggle` |
  | `application layout <i>` | `application layout/-\|+` | `layoutStep` |
  | `track <p>/scrollto` | `bank track <offset>` | host paging |
  | `clip note/<pitch>/…` | `clip grid/<id>/step/…` | host step grid |
  | `track <p>/inQuantResolution`, `overdub` | `application recordQuantization`, `transport overdub/…` | reported `/recordQuantization`, `/overdub/*` |
- **Multi-value lines:**
  - colors (`r g b` in reports, `RGB(r,g,b)` in commands);
  - `playingnotes` and `/clip/notes` (`sel:mute:start:end:channel:pitch:velocity;…`,
    `src/daw/clip/note.rs:87`);
  - a step (24 space-separated numbers, `src/daw/clip/step_info.rs:377/413`);
  - `/clip/all` (Reaper: `track;count;name;selected;…`, `src/daw/model.rs:919`);
  - `/host/features` (names);
  - `track addTrack` (string list);
  - `…/set` (`"<velocity> <duration>"`), `…/paste` (`"<track>/<slot>"`);
  - `time signature` (`num + den*100`);
  - `eq band/<b>` (an int sent as a string).
- **Legacy callback kinds which differ from the value type:**
  - booleans are sent with `process_int_arg` (`Sender::bool_arg`);
  - `send/<s>/active` uses `process_double_arg`;
  - `repeat` and `toggleMultiSelect` send a constant `1`;
  - `select` always sends `1`.

  The table must keep the legacy kind per command so that the text form stays identical.
- **Found during the inventory, not in HOST_BRIDGE_GUIDE:** `/device/parameters` and
  `/drum/parameters` (the parameter page section visible, `parser.rs:342/951`) and the command
  `device parameters/toggle` (`device.rs:868`, used in the recordings). This is an example of the
  "uses are hard to find" problem.
- **Outside the command protocol (already typed C callbacks, unchanged by this plan):**
  - MIDI (`send_midi_data`, `process_midi_arg`);
  - note input filters (string list) and key and velocity tables (int arrays);
  - settings, notifications, model setup, document settings.

---

## 2. Design

### 2.1 The single source of truth

**Options**

| Option | Pro | Con |
| --- | --- | --- |
| **A. A Rust table (`macro_rules!` or `const` arrays) in `src/protocol/`** (decided) | the type checker sees it; no parser; the core uses the same table | the other languages need a generator that can read Rust: a `src/bin/` tool that links the crate (like `src/bin/dbm-catalog.rs`); staleness is caught by a test (below) |
| B. A declarative text file + `build.rs` (the first recommendation) | one file for people and generators; `build.rs` already exists and already writes a committed artifact only when it changed (`build.rs`, the header); a line-based format needs about 200 lines of parser in `build.rs`, no dependency | build.rs grows; errors in the file show as build errors (which is the point) |
| C. An IDL (protobuf, FlatBuffers, capnp) | tooling | new dependencies in every host, against the no-deps rule |

**Decision (2026-11-08): option A.** The table is Rust, in `src/protocol/`, and the core uses it
directly:
- A `macro_rules!` table, one entry per message with the same fields as the sketch below (id,
  name, text template, value type, `legacy=`, `ops=`, `feature=`, `entity=`, `detects=`),
  expands into the Rust enums (`Msg`, `Cmd`, `MsgId`, `CmdId`, the domain enums), the record
  encode/decode, the text and legacy conversions, and a `const` metadata table
  (`&[MessageInfo]`).
- The generator binary `src/bin/dbm-protocol-gen.rs` (like `dbm-catalog`) links the crate, walks
  the metadata table and writes the definitions of the other languages (the outputs below).
- **Staleness:** a test in `cargo test` runs the generator in memory and compares its output with
  the committed files, so a stale binding fails the tests (and CI) at once, not only when someone
  remembers to run the generator. The test can also print the command that updates them.
- No new dependency; `build.rs` stays as it is (cbindgen for `ffi.rs`).

The sketch below shows the fields as a declarative file; in the Rust table they are the macro's
arguments.

**The fields (sketch):** one line per message. A message family that differs
only by owner is one line with an enum index. Sketch:

```
# Index domains (enum values have stable numbers; text form = the name)
domain bank     track=0 fxtrack=1
domain dev      device=0 primary=1 eq=2 drum=3 project=4 trackfx=5 masterfx=6
domain groove   enabled=0 shuffleAmount=1 shuffleRate=2 accentAmount=3 accentRate=4 accentPhase=5
domain limit    full=0 6=6 8=8
feature valueInc=0 valueReset=1 immediately=2 indication=3 soloToggle=4 transportCommands=5 deviceCommands=6 layoutStep=7 clipLauncherNavigator=8
struct  Note    selected:bool muted:bool start:f64 end:f64 channel:u8 pitch:u8 velocity:u8
struct  Step    f64[24]                     # StepInfo::to_wire order

# dir id      name               text template                          value    options
in   0x0101  PLAY               /play                                  bool
in   0x0105  TEMPO              /tempo                                 f64
in   0x0201  TRACK_COUNT        /track/count                           int
in   0x0202  TRACK_OFFSET       /track/offset                          int      detects=hostPaging ignored_value
in   0x0210  CHANNEL_EXISTS     /{bank}/{p}/exists                     bool     entity=channel(bank,p)
in   0x0214  CHANNEL_COLOR      /{bank}/{p}/color                      color    entity=channel(bank,p)
in   0x0218  CHANNEL_VOLUME_STR /{bank}/{p}/volume/str[/{limit}]       text     entity=channel(bank,p)
in   0x0230  CHANNEL_PLAYING    /{bank}/{p}/playingnotes               Note[]   detects=playingNotes
in   0x0601  PARAM_VALUE        /{dev}/param/{i}/value                 f64      entity=param(dev,i)
in   0x0701  STEP               /clip/grid/{grid}/step/{ch}/{x}/{y}    Step|0   entity=grid(grid)
cmd  0x8201  CHANNEL_MUTE       {bank} {p}/mute                        bool     legacy=int
cmd  0x8210  CHANNEL_VOLUME     {bank} {p}/volume                      f64      ops=set,inc:valueInc,reset:valueReset,immediately:immediately,touch,indicate:indication
cmd  0x8301  TRANSPORT_PLAY     transport play                         -        feature=transportCommands
cmd  0x8302  PLAY_TOGGLE        play                                   -        basic_of=TRANSPORT_PLAY
cmd  0x8350  TIME_SIGNATURE     time signature                         Sig      legacy=int:num+den*100
cmd  0x8901  REFRESH            refresh                                -
cmd  0x8902  DELAY_UPDATES      {area}                                 -        legacy=delay
```

**Rules**
- **Ids:**
  - u16; the high bit is the direction (0 = host → core, 1 = core → host);
  - one block of 0x100 per area, so a range test finds an area;
  - ids are never renumbered or reused: a removed message stays in the file as `reserved`.
- **The text template is the text form.** Both directions of the conversion are generated from
  it (§2.6), so the table cannot disagree with the parser.
- **`entity=`** names the item a message belongs to: channel, send, slot, param, pad, layer,
  scene, marker or grid. This replaces the prefix strings of "report again" (`forgetLater("/track/3/")`,
  HOST_BRIDGE_GUIDE §6.1): `forget(Entity::Channel(Track, 3))`.
- **`detects=`** documents the progressive host features (HOST_BRIDGE_GUIDE §2 table). The
  generator also writes that table into the guide, or checks it.
- **`legacy=`** keeps the callback kind and value encoding of today's text form (§1.5, last
  points), so the text conversion is exact.
- **`ops=`** declares the value operations of a value command and the host feature of each.

**Generated outputs.** The Rust part comes from the macro (no `OUT_DIR`); the others are written
by `dbm-protocol-gen` and checked by the staleness test:

| Output | Content | Committed |
| --- | --- | --- |
| `src/protocol/` (the macro's expansion) | `enum Msg<'a>` (host → core, with typed fields, strings borrowed from the buffer), `enum Cmd` (core → host), the domain enums, `MsgId`/`CmdId` (`#[repr(u16)]`), encode/decode of records, text ↔ typed conversion, legacy `(kind, processor, command, value)` conversion, a per-id metadata table (name, template, value type, feature, entity) | the table is the source |
| `include/dbm_protocol.h` | `enum DbmMsgId`, `enum DbmCmdId`, domain enums, `DbmRecord`, value type and op constants, `DBM_PROTOCOL_VERSION`, `DBM_PROTOCOL_HASH`, `static inline` writers (`dbm_put_f64(buf, id, i0, i1, v)`, …) and a reader (`dbm_next_record`) | yes; `drivenbymoss.h` includes it (cbindgen `after_includes`) |
| `bindings/cpp/include/dbm_protocol.hpp` | `enum class` per direction, a `Writer` (std::vector<uint8_t>), a `Reader` (range-for over records), `toText`/`fromText` through the C ABI | yes |
| `bindings/python/drivenbymoss/protocol.py` | `IntEnum`s, precompiled `struct.Struct`s, `Writer` (one reusable `bytearray`), `iter_commands(memoryview)`, per-id metadata | yes |
| `backends/bitwig/src/main/java/de/mossgrabers/dbm/bitwig/protocol/Protocol.java` | `enum Msg`, `enum Cmd` with their ids, a `Writer` on a reusable `MemorySegment`, a `Reader` | yes |

The staleness test (in `cargo test`, so also in CI) keeps the committed files in step with the
table. cbindgen is not needed for these outputs; it stays for `ffi.rs`.

**In the core:**
- `Cmd` replaces the strings at the 236 send sites: `ctx.send(Cmd::ChannelMute { bank, p, value })`
  instead of `self.send_bool("mute", value)` with a prefix built elsewhere.
- `Msg` replaces the string matching of `parser.rs`. A hand-written `fn apply(model, &Msg)` has
  **no `_ =>` arm**, so a message in the table without a handler is a compile error. The
  remaining `host.error("Unhandled …")` cases are lines that are not in the table at all, from
  the text path only.

### 2.2 The binary layout

**One record** (all fields little-endian, 8-byte aligned, 24 bytes plus payload):

| Offset | Size | Field | Notes |
| --- | --- | --- | --- |
| 0 | 2 | `id` | `MsgId` or `CmdId` |
| 2 | 1 | `type` | value type: 0 none, 1 bool, 2 i32, 3 f64, 4 text, 5 color, 6 struct, 7 struct array, 8 text list, 9 feature set; redundant with the table, checked by the core (a host bug shows at once) |
| 3 | 1 | `op` | value operation of value commands: 0 set, 1 inc, 2 reset, 3 immediately, 4 touch, 5 indicate; 0 in reports |
| 4 | 8 | `idx[4]` | u16 each, unused = 0; enum domains as their numbers; the grid id as two slots (steps, rows), the channel and row of a grid step packed into one (`channel << 7 \| row`, §5.3 Q7) |
| 12 | 4 | `len` | payload bytes after the record (0 for inline values) |
| 16 | 8 | `value` | bool/i32 (as i64), f64, color (`r, g, b, valid, 0, 0, 0, 0`), feature set (u64); 0 for payload types |
| 24 | `len`, padded to 8 | payload | text: UTF-8 without NUL; struct arrays: packed little-endian structs; text list: `u32 count, (u32 len, bytes)…` |

**Why this layout**
- **Fixed-size head:** a host compares the 24-byte head (one `memcmp`, one `bytes ==`, or a
  tuple compare) for every inline value. That is about 90 % of all reports: flags, numbers,
  colors.
- **u16 indices:** they cover every domain in §1.4. Four is the most that appear together.
- **Size:** 24 bytes vs 27.7 bytes per text line on average, so a dump stays the same size.
- **Endianness:** fixed little-endian. All targets are little-endian (x86-64, aarch64 including
  the Pi and Apple Silicon); the readers use `from_le_bytes` on slices, so neither side needs
  aligned input.

**The buffer** is an 8-byte header (`u32 magic 'DBM1'`, `u16 version major`, `u16 flags`)
followed by the records. Each side skips a record with an unknown id by its `len`, and logs it
once per id.

**Batching**
- **Host → core:** one buffer per report, the binary equivalent of one `dbm_update_model`
  string. The core applies the records in order, as it does with lines today (the order rules of
  HOST_BRIDGE_GUIDE §2 stay).
- **Core → host:** the core appends commands to an instance-owned buffer and delivers it with
  one callback.
  - **when:** **before any other callback** (`send_midi_data`, note input tables, …) **and at the
    end of every `dbm_*` call**. Flushing before other callbacks keeps today's exact order of
    commands and MIDI output, while a burst of commands, for example 64 `…/indicate` on a mode
    change, costs one upcall instead of 64.
  - **why not per tick:** commands arrive during any call (HOST_BRIDGE_GUIDE §1, e.g.
    `bank track` during `update_model`), and a per-tick buffer would move them.
  - **compared with per-command typed callbacks:** these are simpler, but cost one FFI
    transition per command, which matters for ctypes (about 1–2 µs per callback) and FFM
    upcalls.

**Versioning**
- `DBM_PROTOCOL_VERSION` has a major part (layout changes, never expected) and a minor part (ids
  added).
- `DBM_PROTOCOL_HASH` is an FNV-1a hash of the normalized table text.
- New ids are added only; old ids keep their meaning.

### 2.3 The C ABI

New functions next to the existing ones. Nothing existing changes, and `DbmCallbacks` keeps its
layout: appending a field would make the core read past the end of the struct of a host built
against the old header, for example with the Bitwig extension's `DBM_DEV_LIBRARY` reload.

```c
/* Protocol identity of the library: major/minor and the table hash. */
void dbm_protocol_info(uint32_t *major, uint32_t *minor, uint64_t *hash);

/* Switch the instance's commands to the binary form. Call before dbm_start_infrastructure (its
 * first commands are refresh and bank fxtrack 0). `handled`: optional bitset over the command
 * ids the host handles (word i bit j = id 0x8000 + 64*i + j), or null for "all".
 * Returns 0 (and keeps the text callbacks) if `major` differs from the library's. */
typedef void (*DbmCommands)(void *user, const uint8_t *buffer, size_t length);
int dbm_set_command_callback(DbmApp *app, uint32_t major, DbmCommands callback, void *user,
                             const uint64_t *handled, size_t handled_words);

/* Report model values in the binary form (one buffer = one report). */
void dbm_update_model_bin(DbmApp *app, const uint8_t *buffer, size_t length);

/* Conversions for logs, recordings, tests and tools (stateless; free with dbm_free_string). */
char *dbm_messages_to_text(const uint8_t *buffer, size_t length);   /* model lines / legacy command lines */
size_t dbm_messages_from_text(const char *text, uint8_t *out, size_t capacity); /* returns the needed size */
```

- **Ownership:** the host's buffer is valid during the call; the core's command buffer is valid
  during the callback (the same rule as strings today, header comment of `cbindgen.toml`).
  Buffers stay owned by the sender and are borrowed by the receiver for the duration of the
  call; there is no shared or reference-counted ownership: it would only save one `memcpy` of
  the buffer, while the values a receiver keeps are converted to its own types anyway. Open
  zero-copy items of the binary path:
  - Python: `bindings/python/drivenbymoss/__init__.py` copies the command buffer with
    `ctypes.string_at(data, n)` before `process_commands`. A `memoryview` over the pointer
    would avoid that, with the rule that no view may escape the callback. Not done yet: the
    command objects of `dbm_host` (§6.4) are kept in `CommandHandler.log` and read their record
    lazily, so they escape the callback; with a borrowed buffer they would have to copy their
    record (24 bytes plus payload) when they are logged.
  - Java (step 3): read the command buffer directly from the native segment (FFM
    `MemorySegment` / direct `ByteBuffer`), not through a `byte[]` copy.
- **Threading:** unchanged. The calls are serialized per instance, callbacks run on the calling
  thread (or the internal timer thread without `DBM_FLAG_HOST_TICK`), and calling into the same
  instance from a callback deadlocks.
- **Mixed use:** `dbm_update_model` (text) and `dbm_update_model_bin` can be used on the same
  instance. Both end in the same `apply(&Msg)`. The command form is per instance: text callbacks
  unless `dbm_set_command_callback` succeeded.
- **The Reaper variant** `dbm_create` (several controllers per instance, `app.rs:399` feeds every
  line to every instance) works the same: one `Msg` decode, applied to every running controller.

### 2.4 Change-only reporting and dumps

The rules of HOST_BRIDGE_GUIDE §2 and §6 stay. The binary form makes them cheaper and less
error-prone:

- **Key:** `(id, idx[0..4])`, 10 bytes. In Python one int (`id | i0<<16 | i1<<32 | i2<<48 |
  i3<<64`); in Java a `long` (id and three indices) plus a fallback map for 4-index steps; in C++
  a `uint64_t` or the 12-byte head prefix. These are cheaper to hash than the address strings of
  today (`"/track/3/slot/5/playing"`).
- **Compare:** the raw value (the 8-byte inline value, or the payload bytes for texts). The host
  never formats a value. Option 3 of ZYNTHIAN_CPU (raw compare before formatting) becomes the
  only path.
- **Dense slots (optional, generated):** for the large fixed families (channel fields × tracks,
  slots, params, pads) the generator emits a slot number per `(id, indices)` within the bank
  sizes of the model setup. A host then keeps its last values in a flat array (`array('d')`,
  `double[]`, `std::vector<double>`) instead of a hash map.
- **Dumps:** `refresh` → the host sets "dump" and writes every value, as today.
- **Report again** (§6.1): `forget(entity)` with the `entity=` of the table instead of a text
  prefix. The generated metadata maps id → entity kind and which indices form the entity.
- **Diffing in the core (later, optional):** a flag on `dbm_update_model_bin` (`DBM_REPORT_FULL`:
  "this buffer is the complete state; drop the unchanged values") would let a simple host skip
  its own change tracking. It is not the default:
  - the core's setters are not all idempotent: `/project/name` schedules a note mapping update,
    and a repeated `select` used to recall views (HOST_BRIDGE_GUIDE §6.2);
  - for Python, encoding a value costs more than comparing it (§4), so the host-side compare
    stays the cheaper path.

### 2.5 Feature negotiation

- **`/host/features`** becomes the message `HOST_FEATURES` with a feature set (u64 bitset over
  the `feature` lines of the table); the text form stays the list of names. The compatibility
  rule (paging without `/host/features` = all features, `context.rs:176`) stays. `HostFeature`
  (`context.rs:68`) becomes a generated enum from the same lines, so `name()` and `lookup()` can
  no longer drift from the hosts' constants (`BitwigModel.java:83`,
  `dbm_host/commands.py:139–161`).
- **The progressive features** (detected from reports) are unchanged in behaviour. The table's
  `detects=` makes them visible in one place.
- **The handled commands:** `dbm_set_command_callback(…, handled, n)` passes a bitset of the
  command ids the host executes.
  - **Phase 1:** the core only uses it for diagnostics. It logs once, at the first send, a
    command the host declared unhandled, with its text form. Today that is "Not handled yet",
    seen only in the host's log.
  - **Later:** `host_supports(feature)` could be derived from it: a feature is supported if all
    its commands are handled. That would retire `/host/features` for binary hosts. This is an
    open question (§5.3). **Done in step 5 (§6.6)** for the command features, with "at least one
    of its commands"; the value operation features stay on `/host/features`.
- **Compile-time checks in the hosts** (the generated enums make "unsupported" a build error
  where the language allows it):

  | Host | Check |
  | --- | --- |
  | Rust core | `apply(&Msg)` and the generated `Cmd` → legacy text: exhaustive `match`, no wildcard |
  | Java (Bitwig) | `switch (cmd) { case … -> … }` as a switch expression over the generated enum: Java 21 rejects a non-exhaustive switch expression. Commands deliberately not handled are listed in one `case A, B, C -> unhandled (cmd)` arm, which documents them |
  | C++ (Reaper, wrapper) | `switch` over `enum class DbmCmdId` without `default`, built with `-Werror=switch` |
  | Python (`dbm_host`, Zynthian) | no compile time: a unit test asserts that `CommandHandler`'s table covers `protocol.CMD_IDS` minus an explicit `UNHANDLED` set; the reporter is checked against the table in the same way |

### 2.6 The text form

- **Text → typed:** generated from the templates. A trie over the path segments is built from
  the table, with each leaf holding a value parser. This replaces the hand-written path matching
  of `parser.rs` (about 1,490 lines → the generated trie plus a hand-written `apply` of roughly
  600 lines, which only calls the model setters).
- **Lenient values:** each value type parses exactly as today's helpers (`parse_flag` > 0,
  `parse_int` with the f64 fallback, `parse_color_static` with gray for errors and none for
  `r < 0`).
  - Where a field has a special failure rule today (for example
    `parse_int(value).unwrap_or(-1)` for `playingStep`, `parser.rs:1416`), the table gets an
    `on_error=` option.
  - The equivalence is checked by the existing 41 parser tests plus a generated round-trip test
    per table line.
- **Typed → text:**
  - reports: `%.6f` for doubles (as Bitwig and Python print today) **when writing the legacy
    form**;
  - recordings: the shortest round-trip form (`{}` in Rust, `Double.toString`, `repr`), so a
    replay feeds the core exactly the value the host sent. The text parser accepts both.
  - commands: the legacy `(kind, processor, command, value)` from `legacy=`. It is
    byte-identical to `tests/conformance_replay.rs:108` `format_call`, so recorded outputs
    compare unchanged.
- **The `.rec` recordings:**
  - **Existing ones** (21 files) stay as they are: a `model` event is text and is replayed
    through `dbm_update_model` → text → `Msg` → `apply`. The recorded outputs (`noArg`, `int`,
    `double`, `string`, `strings`, `delay`, `enable`) are compared with the legacy text of the
    `Cmd`s. No conversion and no re-recording is needed.
  - **New recordings by a binary host:** the recorder writes the same kinds. It converts with
    `dbm_messages_to_text` (one implementation, in the core). The format, `rec_textconv.sh` and
    the git diffs stay readable and unchanged. A later option is to move the recorder into the
    core (`DBM_RECORD_DIR` handled by `ffi.rs`), so that every host gets recordings: Python,
    Zynthian, Reaper. Today only the Bitwig extension records.
- **Logs:** "Unhandled …" and the hosts' "not handled" lists print the text form.

---

### 2.7 The host API: generated view wrappers

**Decision (2026-11-08):** every language gets generated view classes, so that a message acts
like an object while it stays a record in the shared buffer. This is the idea of
FlatBuffers/Cap'n Proto readers, generated from our own table, without their dependencies.

- **Readers** (core → host, one buffer per call, §2.3): a small view per record, whose fields
  are properties that read from the buffer when they are accessed. Nothing is copied or decoded
  up front, and a host reads only the fields it uses. One call crosses the boundary for all
  commands of the call: that matters in the hosts where a crossing is expensive (ctypes
  callbacks in Python, FFM upcalls in Java).
- **Writers** (host → core, one buffer per report): generated per-message setters that append a
  record to a reusable buffer (`w.track_volume(3, 0.5)`), and the whole report crosses in one
  call (`dbm_update_model_bin`).
- **Typed:** the view of a message has exactly that message's fields (`msg.track`, `msg.send`,
  `msg.value` with its type), and the id is an enum, so the host's dispatch is a `match`/`switch`
  over generated constants.

Per language:

| Language | Buffer | Reader | Writer |
| --- | --- | --- | --- |
| Rust (core) | `&[u8]` | `Msg<'a>` borrowing strings from the buffer (from the macro) | `Cmd` encode |
| C | `const uint8_t*` | `dbm_next_record` + `static inline` accessors | `dbm_put_*` |
| C++ | `std::span<const uint8_t>` | a range over view structs, `msg.as<DbmCmd::TrackMute>()` | `Writer` |
| Python | `memoryview` | classes with `__slots__` and properties over precompiled `struct.Struct`s, `iter_commands(buf)` yields the view of each record | `Writer` on a reusable `bytearray` |
| Java | `MemorySegment` | records or flyweight views over the segment, for exhaustive `switch` | `Writer` on a reusable segment |

Example (Python host):

```python
for msg in protocol.iter_commands(buf):      # one crossing for the whole call
    match msg.id:
        case Cmd.TRACK_MUTE:
            chain(msg.track).mute = msg.value
        case Cmd.TRACK_VOLUME:
            chain(msg.track).volume = msg.value
```

Example (writing a report):

```python
w = protocol.Writer()                        # reused for every report
w.track_volume(3, 0.5)
w.track_name(3, "Synth")
app.update_model_bin(w.buffer())
```

Notes:
- A view is valid only during the callback (the buffer belongs to the core), as the strings of
  the text callbacks today. A host which keeps a message copies it (`msg.copy()`).
- Python cost: a view object per record is cheap compared with formatting and parsing text, but
  for the hottest paths the generator can also emit a flat dispatch (`id, i0, i1, value =
  unpack_from(...)`) without objects. Measure both in step 2.
- The test that every message id has a handler (Python, §2.5) runs over the generated metadata.

## 3. Migration

The text protocol stays until the last host has moved. Each step is proven by the test suites
that already exist, run unchanged.

| Suite | What it proves | Command |
| --- | --- | --- |
| Rust unit tests | parser behaviour (41 parser tests, 147 text feeds), every command's text (238 `BackendCall` assertions) | `cargo test` |
| Replay of the 21 recordings | the outputs after every input as sets, LED state vs Java at the end markers | `cargo test --test conformance_replay` |
| Python reference host conformance | 195 scenarios of 4 setups against the Java results (`*-java.ref.json`) | `cd bindings/python && python3 -m conformance --all`; `bindings/python/tests` (81 tests) |
| Zynthian | the drivers against the stand-ins, `test_conformance_lcxl.py`, `test_cpu.py` | `python3 backends/zynthian/tests/run_tests.py` |
| C++ wrapper | `bindings/cpp/tests/run_tests.sh` | |
| Bitwig conformance (needs Bitwig, the `conformance-runner` agent) | Java vs Rust on the real DAW | `backends/bitwig/tools/conformance_all.sh` |
| Reaper | no automated tests; by hand | |

**The order**

| Step | Content | Proof | Effort |
| --- | --- | --- | --- |
| **0a. Table + generator + `Cmd`** (the recommended first step) | the Rust table for the commands in `src/protocol/` (the macro, Rust only at first), `Cmd` and the legacy conversion; `ctx.send(Cmd)` at the 236 sites; `Sender`/`Backend` take `&Cmd` and `FfiBackend` formats the legacy text. No change on the wire, in the C ABI or in the hosts | `cargo test` (all 238 `BackendCall` assertions unchanged, because `RecordingBackend` records the legacy text), replay unchanged | 4–6 days (table ≈2, generator ≈2, call sites ≈2) |
| **0b. `Msg` + `apply`** | the address part of the table; text → `Msg` (generated trie), hand-written exhaustive `apply` replacing `parser.rs`'s matching; `ParserEvent` stays | the 41 parser tests and 147 text feeds unchanged, replay, Python conformance `--all`, Zynthian tests (all hosts still send text) | 6–8 days |
| **1. Binary ABI in the core + the C header and C++ wrapper** | record encode/decode, `dbm_update_model_bin`, `dbm_set_command_callback` with per-call buffering, the conversions, `dbm_protocol.h`, `dbm_protocol.hpp` | **the replay in a binary mode**: each `model` text is converted to binary and fed through `dbm_update_model_bin`; the commands come through the binary callback, converted to legacy text, compared with the recording. All 21 recordings must pass in both modes, which shows that the binary path changes nothing. Plus round-trip tests text → bin → text for every table line, and the C++ wrapper tests on the typed API | 4–5 days |
| **2. Python: binding, `dbm_host`, Zynthian** | `protocol.py`; `Reporter.put(id, idx, value)` writing records; `last_values` keyed by int; `CommandHandler` dispatch on `CmdId` (replacing `Command.parse`'s suffix splitting with `op`); `forget(entity)`; Zynthian's `ZynReporter` prefixes become prebuilt keys | **dual run**: a check mode in which the reporter builds both forms and asserts `dbm_messages_to_text(bin) == text` every pass, run over all 195 scenarios and the Zynthian tests; then conformance `--all` and the Zynthian suite on the binary path alone. Tests that read `reporter.last_values["/track/4/name"]` (Zynthian tests) use a text view (`reporter.text_snapshot()`) | 5–7 days |
| **3. Bitwig (Java over FFM)** | `Protocol.java`; `ModelReporter` on a reusable `MemorySegment` (no `String.format`); `CommandHandler` as switch expressions over `Cmd` (the 293 `case` labels become enum cases); `reportAgain` by entity; `ProtocolRecorder` via `dbm_messages_to_text` | the dual-run check of step 2 in the extension (dev mode); recordings made before and after the change replay the same; the Bitwig conformance runs (all setups) must give the same results as before. Note the rounding risk (§5.1) | 6–8 days |
| ~~4. Reaper (C++)~~ | **Out of scope** (decided 2026-11-08): Reaper is not a target of the project (unless a Reaper license comes with a reason to port to it). The Reaper backend stays on the text protocol as it is, unchanged; the core keeps the text path for it (the legacy conversion of `Cmd` and the text parser feeding `Msg`) | – | – |
| **5. Retire the text path in the hosts** | remove `format_value`, the text `put`s and the text command handlers from the hosts. In the core, keep `dbm_update_model` and the text callbacks: they are cheap to keep and serve logs, recordings and simple hosts | everything above | 1–2 days |
| **6. Subscriptions (planned)** | the core tells the host which values it currently needs (`SUBSCRIBE`, §6.7); declared per mode/view, traced model reads as a check in tests; the hosts collect/observe only the subscribed sections | everything above; Zynthian first (`test_cpu.py`), then Bitwig (`subscribe()`/`unsubscribe()`); conformance must not change | to estimate after a Zynthian prototype |

**Total:** about 27–38 days without the Reaper step (out of scope). Steps 0a and 0b are useful on
their own and carry no risk to the hosts. Steps 2 and 3 are independent of each other once step 1
is done. The text path stays in the core for the Reaper backend (and for logs, recordings and
replays).

---

## 4. Performance

**Measured on this host** (x86-64, Python 3.14.9), 1,000 values per pass,
`/track/<n>/slot/<n>/playing`-like keys:

| Operation | Today (text) | Binary records | Factor |
| --- | --- | --- | --- |
| idle pass: compare with the last value, nothing changed (`Reporter.put` with option 3's raw compare vs list index compare) | 108 ns/value | 38 ns/value (dense slot) | 2.8× |
| full pass: format every value (`f"{a} {v:.6f}"` + join vs `struct.pack_into` of a 24-byte record) | 355 ns/value | 168 ns/value | 2.1× |
| bulk write of 1,000 doubles into a ctypes/array buffer | – | 64–73 ns/value element-wise, 17 ns/value as a slice assignment | |

**One full Zynthian pass** (the Mk2 against the stand-ins of `test_cpu.py`: 8 chains, 8 phrases,
idle, forced full): 1,330 µs for 1,585 `put`s.
- Under cProfile, `put` is about 22 % inclusive, but the profiler inflates function calls.
- Without the profiler: 1,585 × 108 ns ≈ 170 µs ≈ 13 % of the pass.
- The rest is reading the zynthian model: `collect_slots`, `collect_pages`, zctrl lookups,
  `displayed`, `_normalize`, `limit_text`.

**Estimate for the Duet** (ZYNTHIAN_CPU.md "Measured on the Duet": Mk2 driver 10.3 % of a core
idle, 4 full and about 29 fast passes per second):
- Binary reporting removes about 60–70 % of the `put` cost: ≈ 8–10 % of a full pass and less of
  a fast pass. That is **about 0.5–1 percentage point of a core per controller.**
- Building and diffing the strings was the big item in the original profile (17,000 `put`s per
  second). Options 1–3 already removed most of it.
- Command delivery is negligible either way: a few to a few hundred commands per second. Batching
  saves the ctypes upcalls of the indication bursts.
- The core's text parsing was 1.2 % of the native profile. Binary decode removes most of it.

**After the change, the cost is still in:**
1. reading the DAW model value by value in Python (zynseq and zctrl lookups): the
   event-driven collection per subtree (ZYNTHIAN_CPU "Next", postponed) and options 4–6 address
   this, not the protocol;
2. the Python loop overhead per value, about 40 ns even for an unchanged one. Only dense slot
   arrays filled in bulk (slice assignment, 17 ns/value), or values read natively (option 6),
   go below that;
3. ctypes per call, about 1–2 µs per `dbm_*` call: unchanged, a few per tick.

**Python techniques for the binary form:**
- one preallocated `bytearray`, written with precompiled `struct.Struct.pack_into`;
- handed to the core zero-copy as `(ctypes.c_char * n).from_buffer(buf)`;
- commands read with `memoryview` slices and `unpack_from` (records of variable length rule out
  `struct.iter_unpack` across the buffer, but the inline head can be read with one
  `unpack_from` per record);
- last values in a `dict[int, …]`, or `array('d')` per family for the dense slots.

**Other hosts:** not measured.
- **Bitwig:** `String.format("%.6f")` per double and `HashMap<String,String>` per report are the
  known expensive parts of `ModelReporter`, so it probably gains more than Python. Profile it with
  `backends/bitwig/tools/bitwig_profile.sh` before and after step 3.
- **Reaper:** formats with `ostringstream`, which is cheaper but still a string per value.

The CPU gain is real but modest for the Python hosts. It is largest for dumps (2× cheaper after a
replug or a `refresh`) and for Bitwig. The decisive gains are the compile-time checks and the
findability.

---

## 5. Risks, open questions, effort, first step

### 5.1 Risks

- **Rounding:** the text hosts send `%.6f`, a binary host sends the exact double. Values close to
  a threshold can then behave differently: the VU clip border `16241/16384`, `value * 16383`
  rounding for displays and LEDs, takeover comparisons. This is more faithful to the Java
  version (which sees Bitwig's exact values), but it can change a conformance result.
  - **Mitigation:** in steps 2 and 3, first round to 6 decimals in the writer (transition mode)
    to prove equivalence with the dual run and the conformance suites; drop the rounding as a
    separate, reviewable change.
- **Lenient parsing:** the per-field failure rules (`unwrap_or(0)`, `unwrap_or(-1)`,
  `unwrap_or(true)`, ignore) must be reproduced exactly in the generated text → `Msg` path. A
  missed rule changes only malformed input, but the replays would show it only if a recording
  contains such a line.
  - **Mitigation:** the `on_error=` column, plus a test that feeds every table line with an
    invalid value through the old and the new parser while the old one still exists (0b).
- **Order:** batching commands per call is designed to keep today's order (flush before other
  callbacks). A host that today relies on executing a command *during* `update_model`, before
  the remaining lines are parsed, would not notice: the core does not read host state back
  during a call.
- **The size of the table:** about 550 messages written by hand. Errors are caught by the
  round-trip tests and the existing suites, but writing it is the largest single task of step 0.
  The Reaper-only forms and the edge cases of §1.5 each need a decision when they are written:
  keep, rename or drop.
- **The generator writes into `include/`, `bindings/` and `backends/`.** It runs by hand (or in
  CI); the staleness test makes a forgotten run fail the tests instead of shipping stale
  bindings. A normal build never writes these files, so a read-only checkout builds.
- **The ABI between the Bitwig extension and a reloaded library** (`DBM_DEV_LIBRARY`):
  `dbm_set_command_callback` refuses a different major version and the extension falls back to
  text. That fallback needs a test.
- **The no-deps rule** holds: the table is a `macro_rules!` macro, the generator is a plain Rust
  binary of the crate, and the bindings use
  only `struct`/`ctypes` (Python), FFM (Java) and the standard library (C++).

### 5.2 Effort

Step 0 takes 10–14 days, step 1 4–5, steps 2, 3 and 5 12–18. **In total about 27–38 days.** The
Reaper step (8–12 days, the least certain one) is out of scope: Reaper stays on the text
protocol.

### 5.3 Open questions for the decision

1. ~~Where the table lives~~: **decided**: Rust tables in `src/protocol/`, used by the core, and a
   generator binary for the other languages (§2.1).
2. ~~Commands~~: **decided**: one buffer per call, read through generated view wrappers in every
   language (§2.7).
3. ~~The handled-commands bitset~~: **decided**: for binary hosts it eventually replaces
   `/host/features` (§2.5). First it diagnoses (phase 1); the text hosts (Reaper) keep
   `/host/features` and the progressive detection.
4. ~~The Reaper-only forms~~: **decided**: Reaper is out of scope and stays on the text protocol.
   The Reaper-only forms (≈30 addresses, ≈25 commands, §1.2/§1.5) stay in the table only as far
   as the core's text path for the Reaper backend needs them (their legacy text), not in the
   typed host API; unused ones (e.g. `cursor`, `Processor::Session/PlayingNotes/Cursor`) can be
   dropped when the table is written.
5. ~~Recordings by binary hosts~~: **decided**: the `.rec` format stays text (through
   `dbm_messages_to_text`), and the recorder moves into the core, so that every host can record
   (today only the Bitwig extension's `ProtocolRecorder` does).
6. ~~Doubles in new recordings~~: **decided** (2026-11-08): the exact shortest round-trip form
   (`{}` in Rust). Old recordings keep `%.6f`; the text parser accepts both. Applies when the
   recorder moves into the core (step 1 or later); step 0a writes no recordings.
7. ~~The grid id~~: **decided** (2026-11-08, revised after step 1): the grid takes **two index
   slots** (steps, rows, u16 each), and the channel and the row/note of a grid step share **one
   slot** (`channel << 7 | row`: channel 4 bits, row 7 bits), so every record keeps 4 indices. The
   first decision (one slot `steps << 8 | rows`, all grids ≤ 255) did not hold: the raindrops view
   has a 1536 × 128 grid (§6.3). In Rust the grid is `protocol::GridId { steps, rows }`, written
   `<steps>x<rows>` in the legacy text; the packed fields are `GridChannel` and `GridRow`
   (adjacent in the braces of the entry, so the order of the slots differs from the text).
8. ~~In-core diffing~~: **decided** (2026-11-08): not now. The host-side compare stays (§2.4).
9. ~~The inconsistent names~~: **decided** (2026-11-08): normalized in the typed names only; the
   text form stays as it is, so the 21 recordings stay valid. In step 0a the command names are
   uniform already (e.g. `SlotAction::Select`, `SceneAction::Select`, `MarkerAction::Select`);
   the address names (slot `selected` vs scene `select`, `/click/volumeStr`, `/time/str` vs
   `/time/text`) are normalized in `Msg` in step 0b.

### 5.4 The recommended first step

**Step 0a: the table and the generated `Cmd` enum, with the text protocol unchanged on the wire.**

1. Write the command half of the Rust table in `src/protocol/` from §1.3: about 306 forms, each
   with its `legacy=` kind, `ops=`, `feature=` and `entity=`.
2. The macro expands it (Rust only at first): `CmdId`, `Cmd` with typed fields, the metadata
   table, `Cmd::legacy() -> (kind, processor, command, value)`. The generator binary and the
   other languages follow with step 1.
3. Change `Sender`/`Backend` to take `&Cmd`. `FfiBackend` and `RecordingBackend` format the
   legacy text, so all hosts, `BackendCall` assertions and recordings stay byte-identical.
4. Migrate the 236 send sites (`grep` for `.send(`, `send_int(`, … in `src/daw`, `src/framework`)
   to `ctx.send(Cmd::…)`. `channel.rs:560` `create_command` and `parameter.rs:376`
   `value_address` become `Cmd` constructors.
5. Add a test that every `CmdId` has a legacy form and that the legacy text parses back to the
   same `Cmd` (needed for step 1).

What it brings at once:
- every command use is findable (`grep Cmd::ChannelMute`);
- a misspelt command no longer compiles;
- the table documents the protocol;
- the hosts are not touched.

Step 0b (`Msg` + `apply`) follows with the same "nothing changes on the wire" property. Step 1
then follows, with the binary replay mode as its proof.

---

## 6. Implementation status

### 6.1 Step 0a (done 2026-11-08)

**What is there**
- `src/protocol/macros.rs`: the `commands!` macro (`macro_rules!`, no dependency). An entry is
  `Name = 0x8301 { field: Type, … } @ <processor> [ <segments> ] => <value>, feature = F;`.
  It expands into `Cmd<'a>`, `CmdId` (`#[repr(u16)]`, `CmdId::ALL`, `info()`), `Cmd::id`,
  `Cmd::legacy()`, `Cmd::from_legacy()` and the metadata table `COMMANDS: &[CmdInfo]` (name,
  processor, template such as `{ch}/send/{send}/volume`, fields with their Rust types, value,
  feature).
- `src/protocol/cmd.rs`: the table, **236 entries** in blocks of 0x100 per area (0x81
  transport … 0x8B note repeat/groove; the block list is in the file's header). With the action
  domains below they cover all ≈300 command forms of §1.3.
- `src/protocol/legacy.rs`: the legacy form (`Legacy { processor, command, arg }`, `Arg::{None,
  Int, Double, Str, Strs, Delay, Enable}`) and the traits which convert the typed fields:
  `Field` (path segments), `ProcField` (a field selecting the processor), `Value` (the value
  with its legacy callback kind, e.g. `bool` → `process_int_arg` 0/1), `ConstValue` (`One`:
  the constant 1 of `repeat` and `toggleMultiSelect`; `Delay`).
- `src/protocol/types.rs`: the domains (`domain!`: stable number = discriminant, text form =
  name) and the structured types: `Chan` (master, track/fxtrack position, drum pad, layer: the
  former `Channel::create_command`), `ValueOp` (set, inc, reset, immediately, touch, indicate:
  the suffixes of §1.5), `Rgb` (`RGB(r,g,b)`), `Signature` (`num + den*100`), `IntText` (the EQ
  band type as a string), `SlotRef` (`<track>/<slot>`), `GridNote` (`<velocity> <duration>`),
  `GridId`, `Pos1` (a 0-based index written 1-based: the Reaper device commands and the
  siblings).
- `Backend::send_command(&Cmd)` replaces the seven text methods (`process_no_arg` …
  `enable_updates`); `Sender::send(&Cmd)`; `DawContext::send(Cmd)` (suppressed while muted, as
  before). `FfiBackend` and `RecordingBackend` turn the `Cmd` into its legacy form, so the C
  callbacks, the `BackendCall` assertions and the recordings are unchanged. The C ABI and
  `include/drivenbymoss.h` are unchanged.
- All send sites in `src/daw`, `src/framework` and `src/midi.rs` send `Cmd` values (about 265
  calls; `src/controller` and the Lua layer `src/script` never sent commands directly, they
  call the model). The string helpers are gone: `Channel::create_command` became
  `Channel::address() -> Chan`, `Parameter::value_address` became `value_cmd(op) -> Cmd`, the
  helpers of the slots, scenes, markers, siblings, the browser columns, the step grid
  (`send_grid*`, `step_path`) and the transport take typed values.
- Tests (`src/protocol/cmd.rs`): every `CmdId`, with 8 samples per ID (every sample of every
  field type is used), has a legacy form which `Cmd::from_legacy` parses back to the same `Cmd`;
  IDs and names are unique and have the direction bit; some fixed legacy texts; the
  `GrooveParam` names equal `HOST_GROOVE_PARAMETERS`.

**Choices and deviations from the plan**
- **Families as domains:** commands which differ only by a verb are one entry with a domain
  field, as the plan does for owners: `SlotAction` (8 verbs), `SceneAction` (7),
  `MarkerAction` (3), `DeviceAction` (4, the device itself and the siblings), `Dir` (`-`/`+`),
  `TempoStep`, `NavigatorTarget`, `StepAttr` (22 step attributes), `GrooveParam`, `CueParam`.
  The other commands have one entry each (e.g. the 22 application, 7 arranger and 6 mixer
  toggles), so a use is found by `grep Cmd::ApplicationToggleMixer`.
- **Owners are `Processor` for now:** fields such as `dev` (`device`, `primary`, `drum`, `eq`,
  `project`, `track`), `bank` (`track`, `fxtrack`, `scene`) and `area` (delay/enable updates)
  use the legacy `Processor` enum. Narrower domains can replace them when the binary form needs
  the numbers (step 1). `Processor::Session` is not unused as §1.1 says: the scene bank sends
  `enable_updates(session)`. No processor was dropped.
- **`Cmd<'a>` borrows its strings** (`&'a str`, `Vec<&'a str>`) instead of owning them, so a
  send site allocates no more than before.
- **Text values kept as text** where the core formats them for Reaper or the step grid: the
  Reaper note commands (`ClipNote*`, `<position> <length> <velocity> <channel> <muted>`) and the
  step text of `GridStepSetFrom`/`GridStepUpdate` (`StepInfo::to_wire`). The `Step` struct of
  §2.1 comes with the binary form (step 1).
- **No `entity=`, `ops=`, `detects=` columns yet:** `entity=` belongs to the reports and
  "report again" (step 0b/2), `detects=` to the reports (0b); the ops of a value command are
  those of its `ValueOp` value. `feature=` is in the table (documentation; the call sites check
  the features as before).
- **Decoding** (`from_legacy`) tries the entries with a fixed processor first, so an owner field
  (any processor) does not shadow them (`clip pinned` vs `<dev> pinned`). `Chan` only accepts the
  processors of channels.
- **Merged forms:** `track indicate` (clip launcher feedback) and `drum|primary indicate` (pad
  indication) are the same form: one entry `BankIndicate`. `time <double>` and `time 0` (int)
  are two entries (`TimeSet`, `TimeSetInt`).
- **Found while writing the table:** the touch and indicate of a device parameter are
  `param/<i>/touch|indicate`, not the value command plus a suffix, and they have no track prefix
  (track FX parameters: `track param/<i>/touch` but `track <t>/param/<i>/value`); entries
  `ParamTouch`, `ParamIndicate` keep that.
- **The generator binary** (`dbm-protocol-gen`) is left for step 1, as §5.4 says; nothing
  outside the core needs the table yet.

**Proof:** `cargo test` (all `BackendCall` assertions unchanged), `cargo test --test
conformance_replay`, the Python binding tests and conformance `--all`, the Zynthian tests and
the C++ wrapper tests pass unchanged; clippy is clean with and without `lua`.

### 6.2 Step 0b (done 2026-11-08)

**What is there**
- `src/protocol/msg.rs`: the address table, **222 entries** in blocks of 0x100 per area (0x01
  transport … 0x0B note repeat/groove, the block list is in the file's header), expanded by the
  `messages!` macro (`src/protocol/macros.rs`). An entry is
  `Name = 0x0301 { field: Type, … } [ <segments> ] => value = rule;`: literals, `("lit")` for an
  optional literal (`/eq/band/<b>[/value]`: Reaper sends it without, Bitwig and Python with) and
  fields. It expands into `Msg<'a>` (strings borrowed from the line), `MsgId`
  (`#[repr(u16)]`, high bit clear), `MsgId::decode`, `MsgId::patterns`, `Msg::to_text` and
  the metadata table `MESSAGES: &[MsgInfo]` (name, template such as `/{ch}/volume/str/{limit}`,
  fields, value field, rule).
- The address fields (`msg.rs`): `Ch` (`track/<p>`, `fxtrack/<p>`, `master`), `Dev` (`device`,
  `primary`, `eq`), `ParamOwner` (a device, `project`, `track/fx`, `master/fx`), `PageOwner`,
  `PadRef` (`drum/pad/<i>`, `<dev>/pad/<i>`, `<dev>/layer/<i>`), `HostParam` (`crossfade`,
  `project/cueVolume|cueMix`, `groove/<name>`), `BrowserItem` (a column item or a result),
  `ArrangerFlag`, `MixerFlag`; `Option<usize>` for the optional `/str/<limit>`; `Pos1` for the
  1-based siblings; `GridId`.
- `src/protocol/text.rs`:
  - the `Seg` trait (a typed address field: write, read, its segment patterns);
  - the `Trie` over the path segments (literal, digits, `<n>x<m>`), built once from the
    patterns of the table (`OnceLock`); a test checks that no path leads to two entries;
  - the value rules (`rules::f64`, `int`, `flag`, `dflag`, `iflag`, `int_or(-1)`, `color`,
    `color_logged`, `channel_type`, …): each keeps the lenient parsing and the failure rule of
    the former parser (drop the message, or a default), which is the `on_error=` of §2.6;
  - `TextValue`: the text of a typed value (doubles in their shortest exact form, §5.3 Q6).
- `src/parser.rs`: `Parser::parse` = `Msg::from_text` + `Parser::apply`, an exhaustive `match`
  over `Msg` without a wildcard arm (about 1,490 lines of path matching → about 600 lines of
  `apply` and helpers). `ParserEvent` and the 41 parser tests are unchanged. A line which is not
  in the table logs `Unhandled OSC address: <address> <value>`; a known address with an invalid
  value is ignored, as before.
- `src/parser_old.rs` (test only): the former parser and the **differential test**: every
  model line of the 21 recordings (in order) and every table line (6 samples per entry, each also
  with the values `x`, ``, `-1`, `0.5`, `2`, `1 2`, `300 -2 7` and a note list, and without the
  leading `/`) give the same log lines, events and commands with both parsers. Remove it once
  the table is stable (with step 1 or 2).
- `Model::note_clip_mut(steps, rows)` (the grid lookup without formatting the grid ID).

**Choices and deviations from the plan**
- **The trie is built at run time** from the patterns which the macro generates (no build
  script, no generated source), once per process. A path leads to one entry; the entry's
  generated decoder reads the typed fields and the value.
- **Lines not in the table** (§2.1: "the remaining Unhandled cases are lines that are not in
  the table at all"): the former parser accepted some malformed forms silently or partly, e.g.
  extra segments (`/play/x 1` set the play state, `/track/3/volume/x` was ignored),
  `/time/loop/start/<anything>` (now only `beat` and `str`), a missing index
  (`/clip/grid/<id>/step/x/x/x`), unknown arranger/mixer flag names (stored before, unused), and
  `/device/band/…` or `/primary/sibling/…` with malformed indices. These now log one
  "Unhandled OSC address" error and change nothing. No host sends them (the differential test
  over all recordings, the Python conformance `core_errors` check and the Reaper collectors).
- **Invalid values with the "drop" rule** drop the whole message before the model is touched;
  the former parser sometimes looked up the addressed item first (e.g. `/track/5/exists x`
  created track 5 as a default item). Valid values behave exactly as before.
- **The value types are typed** where the former parser converted at once: flags are `bool`
  with their rule (`dflag`: drop if not a number; `flag`: false; `iflag`: an int above 0;
  `int_nonzero` for bypass, …), `ChannelType` and `RecordQuantization` are enums, colors are
  `Option<[f64; 3]>` (None for "no color" and for an error; `apply` uses gray for both, as
  before), notes `Vec<Note>`, a step `Option<StepInfo>`. Kept as text in 0b: `/host/features`
  (the bitset of §2.5 comes with the binary form), `/clip/all` (Reaper), the layout.
- **Normalized names** (§5.3 Q9): `SlotSelected`/`SceneSelected` (text `selected`/`select`),
  `MetronomeVolumeStr` (`/click/volumeStr`), `TimeStr` (formatted by the core) vs `TimeText`
  (by the host), `ItemSelected` (`…/item/<i>/select`), `DeviceSiblingSelected`.
- **Accepted and ignored** entries for forms which hosts send or which the former parser
  explicitly accepted: `/marker/<i>/position|endPosition`, `/browser/selected/name`,
  `/<dev>/pad/count`, `/drum/pad/count`, and the value of `/track/offset`, `/fxtrack/offset`,
  `/scene/offset`.
- **`/action/select`** is in the table (`ActionSelect`); `parse` returns its event without
  `after_parse`, as before.
- **Speed:** the Mk2 recordings (16,700 lines, release build) take 504 ns per line against 457 ns
  before (+10 %, most of it is the model's work); the trie uses std's `HashMap`.

**Proof:** `cargo test` (the 41 parser tests and all text feeds unchanged, the differential
test, the round trip of every table entry through its text), `cargo test --test
conformance_replay`, the Python binding tests and conformance `--all` (195 the same, 0 core
errors), the Zynthian tests and the C++ wrapper tests pass unchanged; clippy is clean with and
without `lua`.

### 6.3 Step 1 (done 2026-11-08)

**What is there**
- `src/protocol/bin.rs`: the layout of §2.2 (header `DBM1` + major version + flags, 24-byte
  records, payload padded to 8), `Writer` (reused buffer), `records()` (an iterator which checks
  the header and the lengths), `BinError` (header, truncated, unknown ID, value).
  - `BinIdx`: the index encoding of every address and command field (in the order of the
    table): numbers as u16, `i32` indices as i16, `Option<usize>` limit (0 = full text), `Pos1`
    0-based, `GridId` two slots (steps, rows; originally `steps << 8 | rows`, see the grid ID below), domains by their number, `Processor` by its
    position in `Processor::ALL` (append only), and the composite fields with several slots:
    `Ch` (kind, pos), `Chan` (kind, processor, index), `PadRef` (kind, dev, index), `HostParam`
    (kind, param), `BrowserItem` (kind, column, item); `ParamOwner`/`PageOwner` one number.
  - `BinValue`: bool, i32, f64 inline; text and text lists (`u32 count, (u32 len, bytes)…`) in
    the payload; colors inline `r, g, b, valid`; notes as a struct array (32 bytes: `f64 start,
    f64 end, i32 channel, i32 pitch, i32 velocity, u32 flags`); a step as 24 doubles
    (`StepInfo::to_values`, no payload = empty step); `ChannelType`/`RecordQuantization` by
    number; `ValueOp` = the op byte plus a double, a bool or nothing; `Signature`/`SlotRef` as an
    inline pair of i32; `GridNote`/`StepValue` as 16-byte payload structs.
  - `to_text`/`from_text`: the conversions; commands as legacy lines
    `<kind>\t<processor>\t<command>[\t<value>…]` (the kinds of the recordings, fields escaped as
    there).
- The macros generate `Msg::encode`/`decode_record`, `Cmd::encode`/`decode_record`,
  `MsgId`/`CmdId::from_u16`, `index_slots()` and `value_type()`. To tell the indices from the
  value, **the value field moved out of the braces** in both tables (`=> value: Type [= rule]`;
  the constant values are `= One`/`= Delay`).
- `protocol_hash()`: FNV-1a over ID, name, template, index slots and value type of every entry.
- The C ABI (`src/ffi.rs`, header by cbindgen, which now includes `dbm_protocol.h`):
  `dbm_protocol_info`, `dbm_set_command_callback` (major check, the optional handled bitset),
  `dbm_update_model_bin`, `dbm_messages_to_text`, `dbm_messages_from_text`; `DbmCallbacks` is
  unchanged. `FfiBackend` buffers the commands in the binary mode and flushes them before every
  other callback and at the end of every call (`Inner::with_app`, so also after the timer's
  tick). Phase 1 of the handled bitset: a command the host does not handle is logged once per ID
  (`Command not handled by the host: <legacy line>`).
- `App::update_model_bin` (unknown IDs and bad values are logged once per ID and skipped),
  `Parser::apply_msg` (shared by both forms).
- The generator: `src/protocol/gen.rs` (the generators, `OUTPUTS`, the staleness test
  `the_generated_bindings_are_up_to_date`) and `src/bin/dbm-protocol-gen.rs` (writes the files
  which changed). Outputs: `include/dbm_protocol.h` (IDs with their template, index slots and
  value type as comments, the domains as enums, `DbmRecord`, `DbmWriter` with `dbm_put_*`,
  `dbm_check_header`/`dbm_next_record`) and `bindings/cpp/include/dbm_protocol.hpp` (C++14:
  `enum class Msg/Cmd`, `Record`, a range-for `Reader`, a `Writer` with one method per report,
  e.g. `w.channelVolume (DBM_CH_KIND_TRACK, 3, 0.5)`, and a view per command,
  `r.as<cmd::ChannelMute> ().ch_index ()`).
- The C++ wrapper (`drivenbymoss.hpp`): `Backend::processCommands`, `App::useBinaryCommands`,
  `App::updateModelBin`, `messagesToText`/`messagesFromText`.
- Tests: the record round trip of every message and command (8 samples each), text → record →
  text for every table line, ≤ 4 index slots per ID, unknown IDs, truncation and wrong types;
  `tests/ffi.rs` `binary_protocol` (major check, buffering, the handled bitset, the
  conversions); the C++ wrapper test on the typed API (writer, reader, views, C writer);
  **the replay's binary mode** (`replay_recorded_sessions_binary`: every model text converted
  with `bin::from_text` and fed through `update_model_bin`, every command encoded and decoded
  again before its legacy text is compared): all 21 recordings pass in both modes.

**Choices and deviations from the plan**
- **`GridStepAttribute`** had five index fields (grid, channel, step, note, attribute), one more
  than the record holds. The attribute became part of the value (`StepValue { attr, value }`,
  in the legacy text still the last segment), so the four indices stay.
- **Per-type stable numbers** instead of a `domain` line per index in the table: the domains
  of 0a/0b already have stable discriminants; `Processor`, `ChannelType` and
  `RecordQuantization` use the order of their `ALL` lists, which are append only now (noted at
  the lists). The generator lists them all in the C header.
- **The binary replay is in-process**: `RecordingBackend::binary` encodes and decodes every
  command, `App::update_model_bin` takes the converted reports. The C ABI itself (buffering,
  flush order, callbacks) is tested in `tests/ffi.rs` and by the C++ wrapper test.
- **The replay's temp dirs** are per mode (the text and the binary replay run in parallel and
  shared one directory per recording, which made a run flaky).
- **Colors** are bytes in the binary form: the text form accepts fractional values (none of the
  hosts sends them); the record rounds them.
- **`dbm_messages_to_text`** writes doubles in their shortest form (`{}`); the text hosts send
  `%.6f`. Equal values, different text: the dual run of step 2 compares the parsed values or
  rounds (§5.1).
- Not yet: Python and Java outputs of the generator (steps 2 and 3); the recorder in the core
  (§5.3 Q5, later); `host_supports` from the handled bitset (later, §2.5).
- **The grid ID** (decided after step 1: option 1). §5.3 Q7 decided one index `steps << 8 | rows`
  "checked when the model setup is created, all grids are ≤ 64 × 128". That does not hold: the
  raindrops view (`abstract_raindrops_view.rs`, `MAX_STEPS = 1536`, as in Java) creates a
  **1536 × 128** note clip, which is in the model setup's `noteClips` of the Launchpads and the
  Maschine JAM, so Bitwig and the Python host report `/clip/grid/1536x128/…`. A check in
  `Model::note_clip` showed it at once (5 core errors in the JAM conformance, 20 Zynthian
  failures) and was removed again. Today `BinIdx for GridId` clamps to 255, so the binary form
  would turn that grid into `255x128` (no recording contains it, so the replays pass). The
  options:
  1. two slots (steps, rows) and the channel and the row/note of the grid steps packed into one
     slot (`channel << 8 | row`): stays stateless and in the layout, but needs a binary-only
     combined field (channel and note are not adjacent in the text);
  2. the grid as its position in the model setup's `noteClips`: one small slot, but the
     conversions (`dbm_messages_to_text`/`from_text`) need the model setup;
  3. a wider record (more or wider indices): a layout change for everything;
  4. `steps` in 11 bits and the rows as a code of a fixed set (1, 8, 16, 32, 64, 128, …).
  **Decided: option 1** (done 2026-11-08, after step 1). `BinIdx for GridId` writes two slots
  (steps, rows; the clamp to 255 is gone). The channel and the row/note of the grid step entries
  (`GridStep`, `GridStepToggle` … `GridStepAttribute`) are the newtypes `GridChannel` and
  `GridRow`: `GridChannel` opens a slot with `channel << 7` (clamped to 0–15), `GridRow` adds the
  low 7 bits (clamped to 0–127) to the slot before it (`IdxOut::or_last`, `IdxIn::last`). They
  are adjacent in the braces (`{ grid, channel, note, step }`), so the slots are
  `grid_steps, grid_rows, channel<<7|note, step`, while the text keeps `…/step/<channel>/<step>/<note>`.
  The metadata of the slots (`index_slots()`) is now a list of parts per slot (`IdxPart`: name,
  shift, bits, signed); the generators write accessors per part (C++: `channel ()`, `note ()`;
  signed slots return `int16_t`) and writer parameters per part. Test:
  `large_grids_and_packed_channel_and_row` (1536 × 128, channel 15, row 127, message and
  command); 1536 × 128 is also a sample of every round trip.
- `src/parser_old.rs` (the differential test of 0b) is still there.

**Proof:** `cargo test` (incl. the round trips and the staleness test), `cargo test --test
conformance_replay` (both modes), the Python binding tests and conformance `--all` (195 the same),
the Zynthian tests and the C++ wrapper tests; clippy clean with and without `lua`.

**Next: step 2** (§3): the generated `bindings/python/drivenbymoss/protocol.py` (a third output
of `gen.rs`) with the view wrappers of §2.7, the Python binding on the binary ABI
(`_ffi.py`: `dbm_set_command_callback`, `dbm_update_model_bin`, the conversions), `dbm_host`'s
`Reporter` writing records (keyed by `(id, idx)`) and `CommandHandler` dispatching on `CmdId`
(the ops instead of the suffix splitting), the Zynthian adapter, and the dual-run check.

### 6.4 Step 2 (Python hosts done for the hot paths: parts 1–5, see the hand-over at the end)

**Decided (2026-11-08): the goal is binary messages everywhere.** No performance work on the
transition paths: neither the text protocol, nor the text ↔ binary conversions, nor
backward-compatible fallbacks need to be fast; they only need to be correct while they exist.
Optimize only the binary paths (keys, records, views, typed dispatch). Transitional costs such as
the rounding to 6 decimals for the byte-equal dual run are accepted until the text path is
removed.

**Done**
- `bindings/python/drivenbymoss/protocol.py`, the third output of `dbm-protocol-gen` (in the
  staleness test): the value types and ops, the domains, `Msg`/`Cmd` as `IntEnum`s, the metadata
  (`MSG_INFO`/`CMD_INFO`: name, template, slots with their parts, record type, value kind;
  `KINDS`), the runtime (`src/protocol/python_runtime.py`, included verbatim: `Record`,
  `iter_records`/`iter_commands`, `iter_flat`, `_WriterBase` with `put_*`, `key()`), a `Writer`
  with one method per report (`w.channel_volume(kind, pos, value)`) and a view class per ID
  (`CmdChannelMute`: `ch_kind`, `ch_processor`, `ch_index`, `value`; `MSG_VIEWS`, `CMD_VIEWS`).
  The value kind per ID comes from the Rust type of the value field (`py_kind` in `gen.rs`):
  none, bool, i32, f64, text, text_list, color (`(r, g, b)` 0–255 or None), notes, step (24
  doubles or None), value_op (double/bool/None by the record type, the op in `op`), pair,
  int_double.
- The binding (`drivenbymoss/__init__.py`, `_ffi.py`): `protocol_info()`,
  `App.use_binary_commands(handled=None)` (refuses another major version, logs another hash),
  `Backend.process_commands(buffer)` (the default converts to the legacy text and calls the
  `process_*` methods, `dispatch_legacy_line`), `App.update_model_bin(writer | bytes)` (a Writer
  or bytearray without a copy), `messages_to_text`, `messages_from_text`. The command buffer is
  copied (`ctypes.string_at`) before the backend sees it, so views stay valid.
- Tests (`bindings/python/tests/test_binding.py`): the same session in text and binary gives the
  same commands and MIDI; the typed views; the writer against the text conversions.

- **The reports of `dbm_host` on the binary path** (`dbm_host/reporter.py`): `Reporter.put(where,
  value)` takes an address or a key (`protocol.key(id, i0, i1, i2, i3)`, one int). The
  protocol is selectable per reporter (`protocol_=`) or by `DBM_HOST_PROTOCOL` = `text`,
  `binary` (the default) or `check`:
  - text: unchanged (lines, last values by address, `format_value`);
  - binary: everything keyed by int. An address is resolved to its key once by the core
    (`address_key`: `dbm_messages_from_text` of the address with a dummy value; the reverse,
    `key_address`, by `dbm_messages_to_text`), cached for the process. The value is normalized
    by the kind of the ID (bool: `> 0`, i32, f64 rounded to 6 decimals, text with line breaks
    replaced, color tuple) and written into a `protocol.Writer`; a value of another type, the
    ChannelType names, the playing notes and the grid steps are converted by the core from their
    text (`_convert`, cached), so the record is exactly the text's. Messages without a value
    (`/track/offset`) are reported when their text changes. Addresses which are not in the table
    would go as text lines (`UNKNOWN_ADDRESSES`; none in the suites);
  - check (the **dual run**): binary, and every record is compared with
    `dbm_messages_from_text(<address> <format_value(value)>)`, i.e. the text line of the text
    protocol converted by the core; a difference raises. **Decision:** the comparison is in the
    binary form, not in the text: the shortest-form doubles of `dbm_messages_to_text` never
    meet the `%.6f` text, and the writer rounds to 6 decimals (§5.1 transition mode), so the
    text line and the record carry the same double, byte for byte.
  - `collect()`/`take()` still return text (in binary the text of the written records, for the
    tests); the bridges call `collect_pass()` and `take_buffer()` and send one
    `update_model_bin` per pass (`Bridge.send_report`). `last_values` is a live read-only view
    by address (text), so the tests which read `reporter.last_values["/track/4/name"]` run
    unchanged. `ZynReporter.collect` became `collect_pass`.
- **The commands on the binary path:** the bridges (`Bridge.start`, so also `ZynBridge`) call
  `use_binary_commands()` unless the protocol is text. The commands arrive as one buffer per
  call; `Backend.process_commands`' default converts the buffer to legacy lines with one
  `dbm_messages_to_text` and calls `process` → `CommandHandler.handle` as before. **Not done:
  the `CmdId` dispatch** (below).
- The collectors still put by address (prebuilt strings): the binary path costs a bit more than
  the text path so far (below). Moving the collectors to keys is the next part.
- All suites pass in all three protocols: the Python tests (85), conformance `--all` (195 the
  same), the Zynthian tests (204); `test_host.ReporterTest.test_binary_reports_equal_the_text`
  runs the dual run in the default suite.

**Measured** (x86-64, Python 3.14, 1,000 records, `bench_protocol.py`-like loops):

| | ns per record/value |
| --- | --- |
| read commands, views (`iter_commands`, 2 attributes) | 874 |
| read commands, flat (`iter_flat`, tuple unpack) | 425 |
| read commands, text (split the legacy line, reference) | 453 |
| write f64, `put_f64` | 321 |
| write f64, the generated method | 334 |
| write f64, `f"{a} {v:.6f}"` (reference) | 412 |

The views cost about twice the flat form (a Python object and a property call per field); a
tuple subclass with `itemgetter` properties measured 700. Commands are few (§4), so the views
stay the API, `iter_flat` the hot path. Writing a record costs about as much as formatting a
line: the gain of the reporter has to come from the compare before the write (§2.4), which
both forms share.

**The Zynthian idle pass** (`test_cpu.py`, Mk2, 300 ticks, 227 puts per tick, mean of the
collection; noisy, ±10 %): text 176–189 µs (= before the change: the text path is the old code),
binary 204–224 µs, check 199 µs (the collectors by address).

**`Reporter.put` per value** (1,000 `/track/<t>/send/<s>/volume` doubles, `bench_put.py`):

| | idle (unchanged) | full (dump) |
| --- | --- | --- |
| text, by address (before) | 142 ns | 704 ns |
| binary, by address | 246 ns | 1,456 ns |
| binary, by key | 169 ns | 1,376 ns |

So far the binary reporter is **slower** than the text one: by address it pays the address →
key lookup on top, and a written value costs the normalization (`round(v, 6)` alone is about
240 ns, the transition rounding of §5.1), two dicts (`last_raw` and the normalized last value),
the writer call and the list of changed keys. The idle pass by key is at the text's level, not
at the 38 ns of §4's dense slots. The gains of §4 need the next part.

**Part 4: the reporter on keys** (done)
- `Reporter.put(k, value)`: **one dict** (`Reporter.last`: key → the value as put). An
  unchanged value costs one `dict.get`, one compare and one type check; nothing else is on that
  path (no type test of the key: an address is never in `last`, so it falls through to the
  slow path, which resolves it to its key once and compares again). A changed value: the per-ID
  `_Spec` (normalizer, writer by key, the position of the slot "limit"), looked up once per ID
  for the process; the normalized value is written with the new `Writer.put_key_*` (the four
  indices packed as one u64, the key is not unpacked). The second compare (is the new value
  another form of the same record?) uses the normalizer for flags, ints, texts and colors (`1`
  and `True`), and the plain number for doubles: **decision:** a changed double is reported even
  when the transition rounding (`round(v, 6)`) gives the same record; that saves the second
  `round` per change, and in the final form (exact doubles) every change of a double is a change
  anyway. No `changed` list: `take()` (tests, logs) reads the keys back from the written records.
  The text protocol uses the same dict (the canonical key of an address); it formats the value
  and the last value when the raw value changed. `last_values`/`snapshot()` format the kept
  values (`format_value`), so the text the tests read is the text protocol's.
- `protocol.py`'s runtime (`python_runtime.py`): `Writer.put_key_none|bool|i32|f64|text|color`,
  `put_key(k, value)`, `KEY_PUTTERS`, `key_id`, `key_indices`, `record_key(head)`.
- **The collectors on keys:** `dbm_host/reporter.py` (all collectors) and
  `dbm_zynthian/reporter.py` (all, plus `browser.py`'s `report_session` and `add_chain.py`):
  `M` (the `Msg` IDs as plain ints: a plain int is a faster dict key than the IntEnum member,
  25 ns per lookup), key bases `channel_key(ChKind.TRACK, p)` (`| index << 48` for a send or
  slot), `grid_key(grid)`, `owner << 16 | index << 32` for parameters, `PadKind << 16 | Dev << 32
  | index << 48` for pads and layers; `put_displayed(k, …)` puts the shortened texts by the
  slot "limit" of the ID. The ZynReporter caches key tuples instead of address strings (slots,
  parameters, pads). `forget` takes a key (exact) or a text prefix (by the addresses of the
  keys, transition); the playing notes and the Zynthian "add chain" selection forget by key.
- The text dump of the reference host (5,671 lines, every collector, markers, EQ, primary) is
  byte-identical before and after; every address of it is its key's address in the core's text
  form, so the text protocol's lines do not change.

**Measured after part 4** (`bench_put.py`: 1,000 `/track/<t>/send/<s>/volume` doubles):

| | idle (unchanged) | full (dump) |
| --- | --- | --- |
| text, by address | 337 ns | 1,069 ns |
| text, by key | 127 ns | 905 ns |
| binary, by address | 359 ns | 1,489 ns |
| binary, by key | 126 ns | 1,291 ns |

(By address both protocols now pay the address → key resolution: addresses are the transition
path.) A written double costs about 1.3 µs: `round(v, 6)` (270 ns, transition), the writer call
(300 ns, mostly `pack_into`'s call overhead), the spec lookup and the dict. The Zynthian idle
pass (`test_cpu.py`, Mk2, 300 ticks, 227 puts per tick, mean of 3 runs each):

| | before part 4 | after part 4 |
| --- | --- | --- |
| text | 191–195 µs | 178–180 µs |
| binary | 215–217 µs | 178–180 µs |
| check | 215–218 µs | 178–179 µs |

The binary pass got 17 % cheaper and is now level with the text pass; the text pass got a bit
cheaper too (its collectors are the same key collectors). Most of the remaining pass is reading
the zynthian model (§4); the idle puts are about 127 ns × 227 ≈ 29 µs of it.

**Part 5: the commands by `CmdId`** (done)
- `CommandHandler.process_commands(buffer)` (`Bridge.process_commands` overrides the binding's
  default): for each record of `protocol.iter_commands`, the typed handler of its ID
  (`CommandHandler.typed`, `register_typed`); the IDs without one, and the records a typed
  handler leaves to the text (it returns `FALLBACK` before doing anything), go through their
  legacy text (`handle_text`: the core's conversion of that record, `drivenbymoss.legacy_commands`,
  then `handle` as before; the transition path).
- **The hooks stay:** a typed handler resolves its item from the view (`ChanKind`, `Processor`,
  the signed index) and calls the same hooks as the text handlers with constant `sub` lists:
  `typed_master`/`typed_track` → `channel_command`, `track_command` (→ `send_command`,
  `value_command`, `set_value`); `typed_param` → `parameter_command` (the reference host) or
  `value_command` (Zynthian). `ZynCommandHandler` overrides `typed_master`, `typed_track` (its
  track list with the "add chain" placeholder) and `typed_param` (cursor device and the first
  instrument); its text overrides (`channel_command`, `set_value`, `track_command`, …) serve
  both forms. `handle`'s Zynthian override became `before_command` (called before every command
  of both forms: the pattern is read again, the next pass is full).
- **`ViewCommand`** (a `Command` subclass): `reset`, `increment`, `immediately` from the record's
  op, the value (a flag as 1/0, a double, None) read once from the record; `processor`,
  `full_command`, `command` from the core's legacy text of the record, converted only when they
  are read (logs, the not handled list, `commands.log` readers). Report again: the typed handlers
  call `forget_later` with the prefix of their item (still text prefixes, below).
- **Typed so far:** `REFRESH`; `CHANNEL_MUTE`, `_SOLO`, `_SOLO_TOGGLE`, `_SELECT`, `_VOLUME`,
  `_PAN`, `_SEND_VOLUME`, `TRACK_REC_ARM` of the master, tracks and effect tracks (pads and layers
  through the text); `PARAM_VALUE`, `_TOUCH`, `_INDICATE` of the cursor device (and of the first
  instrument on Zynthian). Everything else goes through the text, processor by processor.
- **The dual run of the commands:** with `DBM_HOST_PROTOCOL=check` every typed command is compared
  with its legacy text (`_check_typed`: number, flag, text, reset, inc, immediately).
- **The coverage test of §2.5** (`dbm_host/coverage.py`; `test_host.CommandCoverageTest`,
  Zynthian `test_commands.py`): a record of every `CmdId` with plausible indices and values
  (a track, the cursor device, a 16 × 8 grid; `COMMAND_OVERRIDES` for the few which need other
  values) goes through `process_commands` of a fresh handler; the IDs which end unhandled must be
  exactly the handler's `UNHANDLED_COMMANDS` (reference host 70 of 236, Zynthian 152).
  `handled_commands()` (all IDs minus that set) is passed to `use_binary_commands`, so the core
  logs a command the host does not execute once, with its text. Known imprecision: Zynthian's
  set assumes zynseq; without it the transport commands are declared handled but are not (phase
  1 is diagnostics only).
- A test each (reference host, Zynthian) runs the same commands as text and as records and
  compares the model, the log and the report-again state.

**Measured** (1,000 `track <n>/volume` set commands on the reference host, `process_commands`):
typed 6.7 µs per command, all through the text fallback 20.6 µs, `handle` with the text
directly 6.0 µs (without the ctypes callback and the line split which the text protocol pays
per command). Most of it is the handler itself (the undo edit of `set_value`, the report again);
the typed dispatch (view, `ViewCommand`, the resolution) costs about as much as `handle`'s
`Command.parse`. The dispatch is not on the Zynthian idle path (no commands when idle).

**Hand-over: what the next run starts with** (step 2: the Python side is done for the hot paths)
1. **More typed handlers, processor by processor** (both hosts): transport (`TRANSPORT_*`,
   `PLAY_TOGGLE`, tempo), `BANK_SCROLL`, slots and scenes, device actions, the clip and grid
   commands. Each moved ID drops out of the fallback; when a processor has no text-only IDs left
   its text handler can go (the text protocol of the Python hosts is a transition path, §6.4
   "Decided").
2. **Report again by key:** `forget_later` still takes text prefixes, applied through the
   addresses of the keys (`key_address`, cached). `forget(entity)` by key ranges (the IDs of a
   channel family with `ch_kind`/`ch_pos` in slots 0/1, a parameter by owner/index) would remove
   the last string work after a command.
3. **The rounding:** dropping `round(v, 6)` in the reporter (§5.1, a separate change; the check
   protocol then compares parsed values with a tolerance of 5e-7) saves about 270 ns per written
   double.
4. **Zero-copy command buffer** (§2.3 note): a `memoryview` over the core's buffer needs
   `ViewCommand` to copy its record when it is logged.
5. **Step 3 (Bitwig/Java) needs the user:** real Bitwig runs and `mvn install` of the extension.
   Start from `Protocol.java` (generated), the `Writer` for the reports (`BitwigModel`'s
   `ModelReporter` by key, a `long` key plus a fallback map for 4-index steps, §2.4), the
   command buffer read from the native segment, and the exhaustive `switch` over `Cmd` (§2.5).
   Reaper stays on the text protocol (out of scope).

### 6.5 Step 3: the Bitwig extension (done 2026-11-08)

**User decisions (2026-11-08)**
1. The step-2 choices stay: a changed double is always reported (also when the 6-decimal
   rounding gives the same record); the coverage test is a snapshot list of unhandled commands.
2. Bitwig conformance runs and the installation of the Rust extension may be done by the agent
   (`conformance_all.sh`; never kill a Bitwig it did not start).
3. Conformance replays may be re-recorded and committed on the branch.
4. The rounding to 6 decimals stays until step 5, so step 3 stays byte-comparable with the text
   path.
5. Bitwig switches to binary in one go, no per-handler text fallback in Java; a text mode only as
   a debug switch until step 5.

**Done**
- `dbm-protocol-gen` writes `backends/bitwig/src/main/java/de/mossgrabers/dbm/bitwig/Protocol.java`
  (the fourth output, covered by the staleness test): the constants, the domains (with the text
  names of `ChannelType` and `RecordQuantization`), the report IDs (`Protocol.Msg`) with their
  metadata (type, name, template, slots), the commands as an enum (`Protocol.Cmd`: ID, template,
  slots, value type and kind), and the runtime (`src/protocol/java_runtime.java`, included
  verbatim): `Protocol.key (id, i0, i1, i2)` (a `long`), a flyweight `View` over a
  `MemorySegment` with an accessor per index field of the commands (generated: `chKind ()`,
  `send ()`, `channel ()` of a packed grid slot, … by the ID of the record), and a `Writer` on a
  growing native segment (`putBool/Int/Double/Text/Color/Doubles/Notes` by key and fourth slot).
- `NativeCore`: `dbm_protocol_info` (another major version or a library without the binary ABI
  stops the extension with an error; another hash is logged), `dbm_set_command_callback` (an
  upcall with the buffer read in place, `reinterpret (length)`, valid only during the call),
  `dbm_update_model_bin` (`invokeExact`), `dbm_messages_to_text` / `_from_text`.
- `ModelReporter` on keys: the key (ID and three slots) plus the fourth slot (only
  `SendVolumeStr` and `GridStep` use it) in one open addressing table (`long` keys, the raw
  value as a `long`, texts/notes/steps as objects); no fallback map is needed. One buffer of
  records per report → `dbm_update_model_bin`. Doubles rounded as `String.format ("%.6f")` +
  `Double.parseDouble` (the exact semantics of the old Java text, which rounds the shortest
  decimal form half-up; a value with at most 6 decimals takes a fast path). The channel types
  and the record quantization are written as their numbers, the playing notes as notes, the
  steps of a grid as the 24 values; a value without a direct form goes through the core's
  conversion of its text (none in the extension today). Report again by entity
  (`Family.CHANNEL`, `PARAM`, `PAD`, `HOST_PARAM`, `OWNER` from the slot names of the metadata)
  instead of address prefixes, with the same rules as before (only the commands with a number
  value; not the effect tracks).
- `DBM_HOST_PROTOCOL` (the name of the Python hosts): `binary` (default), `check` (every record
  compared with the core's conversion of the text line which the text protocol reports; a
  difference is logged, the counts at exit), `text` (the text protocol: the address of a key
  from the core, the commands of the text callbacks converted by the core and executed by the
  same typed handler; a debug switch until step 5).
- All collectors on keys (`BitwigModel`, `SpecificDevice` with a `Dev` code instead of a prefix,
  `EqualizerDevice`, `BitwigBrowser`, `BitwigNoteClip`). No string work per tick on the binary
  path except the texts themselves.
- `CommandHandler`: `processCommands (MemorySegment)` with one exhaustive `switch` over
  `Protocol.Cmd` (`case null` makes the statement exhaustive: a new command is a compile error);
  the 16 commands of Reaper in one case and in `UNHANDLED` (metro_vol, the clip note commands,
  inQuantResolution, recordClip, pin, overdub, the master crossfader, groove active/amount),
  passed to the core as the handled bitset. The sub-handlers of the browser, the navigator and
  the note clip got typed methods.
- The recorder writes the core's text of the records (`dbm_messages_to_text`; the doubles in
  the shortest form, the command lines as before). The conformance probe uses the text mode
  without a core (its own address formatter for the clip and the grid).
- `ProtocolTests` (`src/test/java`, a main class run by `maven-antrun-plugin` in the test phase:
  no test framework is in the local repository; `-DskipTests` skips it): the command coverage
  of §2.5 (a record of every command on stand-ins of the Bitwig API made with
  `java.lang.reflect.Proxy`; the unhandled ones must be exactly `UNHANDLED`), the handled
  bitset, the writer (also when it grows) and the view, `round6` against the text (300,000
  values), and with `target/debug/libdrivenbymoss.so` the dual run of every report ID with the
  value forms of the extension (1,635 records) and report again.

**Deviations from the old text handler** (all where the old one ended "not handled"; found by
the differential run below): a parameter value command with touch or indicate (`…/value/touch`)
now touches/indicates; `browser column/<n>/reset` now selects the first item (the old handler
stripped `/reset` as a value operation); a send volume touch now touches the send (the old
handler set the volume to the touch flag; not sent by the tested controllers).

**Verification**
- A differential run (scratch, not committed): the old extension (63cfa3d) and the new one on
  the same deterministic stand-ins of the Bitwig API (values by the object path, every void call
  logged), with a rich model setup: the full dump (4,564 lines) is identical in the text
  mode, and for 904 command samples (8 samples of every `CmdId`, the legacy lines of the core)
  the Bitwig API calls and the values reported again are identical except the 7 lines of the
  deviations above. The same run in the check mode: 13,919 records equal to the text.
- It found one bug before any Bitwig run: the writer evaluated its segment before growing it.
- **Bitwig conformance** (Bitwig 5.3.13, all four setups, the installed extension of this step,
  `conformance_all.sh` with `JAVA_PORT="VirMIDI 0-2" RUST_PORT="VirMIDI 0-1"`: snd-virmidi was
  card 0 after a reboot): maschine_mk2 27/27, launchcontrol_xl 27/27, maschine_jam 41/41,
  generic_flexi 100/100 the same, as the last committed runs; all replays re-recorded (the
  recordings now hold the core's text of the records). Maschine Mk2 also with
  `DBM_HOST_PROTOCOL=check`: 27/27 the same, 38,383 records compared with the text in Bitwig, 0
  differences. Note for runs from a worktree: the Flexi profile loads the mapping file of the
  main checkout, so the runner needs `DBM_FLEXI_MAPPING=<main checkout>/tests/conformance/generic_flexi/flexi-mapping.properties`
  (without it both sides fail the same 106 expectations).

**Measured** (x86-64, JDK 27, 1,000 send volumes per pass, after warm-up):

| | ns per value |
| --- | --- |
| binary, unchanged | 6 |
| binary, changed double (with the 6-decimal rounding, transition) | 260–310 |
| binary, changed double without the rounding (exact, step 5) | 23 |
| text (old, address strings and `%.6f`), unchanged | 250–330 |
| text (old), changed | 270–350 |

The rounding costs a format and a parse per changed double; dropping it in step 5 makes a
changed double about 10 times cheaper.

**Hand-over for step 5** (retire the text path in the hosts)
1. **Bitwig:** drop `ModelReporter.round6` (a changed double then costs about 23 ns instead of
   about 280 ns); the check mode then compares parsed values with a tolerance of 5e-7 (or goes,
   with the text mode). Remove `Mode.TEXT`, the text callbacks of `DbmExtension`
   (`processNoArg` … `dispatchText`) and `ModelReporter.convert` (unused by the collectors).
   The probe (`ProbeExtension`) uses the text mode without a core and needs its lines: keep a
   small text formatter for it, or move the probe to the core's conversion.
2. **Zero copy:** the Java command buffer is already read in place (`reinterpret`); Python still
   copies it (§2.3 note).
3. **Report again** still follows the old rules of the text prefixes (only commands with a
   number value, not the effect tracks); with the text gone these rules can be revisited.
4. **The handled bitset** is diagnostics only (phase 1 of §2.5); deriving `host_supports` from
   it would retire `/host/features` for the binary hosts.
5. The recordings: the extension writes the core's text of the records (shortest doubles); the
   replays read both forms.

### 6.6 Step 5: the text path retired in the hosts (done 2026-11-08)

**User decisions (2026-11-08)**
1. Start step 5 now. **Drop the 6-decimal rounding** (Java `round6`, Python `round(v, 6)`): the
   doubles are exact (the shortest form in the text conversions and the recordings). The check
   modes go together with the text mode.
2. **Replace `/host/features` by the handled-commands bitset where it fits** (§2.5 "Later"):
   `host_supports(feature)` of a binary host comes from the bitset it passes to
   `dbm_set_command_callback`. What does not map cleanly stays (and is documented); text hosts
   keep the compatibility behaviour (Reaper stays on the text protocol; the core keeps
   `dbm_update_model` and the text callbacks, §3). The coverage tests and the handled sets are
   made accurate, since they now drive behaviour (Zynthian without zynseq: no transport).
   Controllers must behave the same in the conformance runs.
3. The earlier decisions stay: a changed double is always reported; the coverage tests are
   snapshot lists; Reaper stays on text; no new dependencies.

**What replaced `/host/features`, and what remains of it**
- **Command features** (`soloToggle`, `transportCommands`, `deviceCommands`, `layoutStep`,
  `clipLauncherNavigator`: the `feature =` of the commands in `src/protocol/cmd.rs`;
  `HostFeature::is_command_feature`, a test keeps it in step with the table): a binary host has
  a feature if it **handles at least one of its commands** (`daw::context::command_features`;
  `Backend::command_features`, set by `dbm_set_command_callback`; null = all commands). Their
  names in a binary host's `/host/features` are ignored.
  - **Why "at least one", not "all":** the commands of a feature are alternative forms of the
    basic commands, and a host handles those its DAW has: zynthian handles `transport play`,
    `stop`, `restart`, the metronome and its volume, but has no punch, fill or metronome ticks.
    With "all" zynthian would lose `transportCommands` and get `play`/`stop` toggles it does not
    handle, a behaviour change. "At least one" gives exactly the features the hosts reported
    before (checked for the reference host, zynthian with and without zynseq, and Bitwig).
  - The gates stay per feature, not per command: a feature also switches model semantics (the
    Reaper punch mode, the default metronome volume, the core's own clip navigation), which a
    per-command test would mix.
- **Value operation features** (`valueInc`, `valueReset`, `immediately`, `indication`): stay on
  `/host/features` for every host. They are the operation of a value command (the `op` byte of
  the record), not commands of their own, so a bitset of command IDs cannot express them
  (`indication` also covers `track indicate` and the indication of volume, pan and sends, the
  latter only ops).
- **Progressive features** (detected from reports: host paging, VU hold, the step grid, …):
  unchanged, they are not tied to commands.
- **Text hosts** (Reaper): unchanged; `/host/features` lists all their features, and a paging
  host without it supports all (the compatibility rule of `context.rs`).
- `dbm_set_command_callback` can be **called again** (it keeps the buffered commands and the
  warnings): Bitwig and the Python hosts pass their handled set again once the model setup is
  known, since the navigator's commands are handled only if the controller wants the navigator
  (Bitwig: `CommandHandler.handledBitset (withNavigator)`; before the model setup without them).
- The recordings get a **`handled` event** (the hex words of the bitset; Bitwig records it before
  the start and again with the model setup); the replay sets them on the `RecordingBackend`
  (`handled`), so a replay derives the same features. Recordings without it name all features
  in `/host/features`, as before.

**Done**
- **Core** (`807b1c2`): `HostFeature::is_command_feature`, `command_features(handled)`,
  `Backend::command_features` (`FfiBackend`: a `Cell` set by `dbm_set_command_callback`;
  `RecordingBackend.handled`), `host_supports` as above; the replay's `handled` event; tests
  (`parser::host_features_of_a_binary_host`, the second call in `tests/ffi.rs`).
- **Python hosts** (`266c4c4`): see the deviations below for the details.
  - The reporter writes the exact doubles; `DBM_HOST_PROTOCOL`, the text protocol, the check
    mode, `format_value`, the text lines, `UNKNOWN_ADDRESSES` and the core's conversion of
    values (`_convert`) are gone. The playing notes are written as notes (tuples), the grid steps
    as their 24 values (`Note.wire_values`, `NoteGrid.step_values`); a value of a wrong type
    raises `TypeError`. The texts for the tests and logs (`take`, `collect`, `last_values`,
    `snapshot`) are the core's text of the records (shortest doubles; a message without a value
    such as `/track/offset` has no text value).
  - Every command by `CmdId`: the typed handlers of all IDs the hosts execute (transport,
    banks, channels with pads and layers, slots, scenes, devices, EQ, clip, grid, browser,
    navigator, markers, note repeat, application, arranger, mixer, project). `handle`,
    `handle_text`, `Command.parse`, `FALLBACK` and the processor handlers on text are gone.
    The channel commands keep their hooks with constant sub lists (`CHANNEL_SUBS`; the send or
    slot index as an int), so the Zynthian overrides stay; the browser commands of zynthian get
    a typed `BrowserCommand` (browser.py, add_chain.py). The update hints (`delay`, `enable`)
    are ignored without a log entry, as before.
  - `handled_commands()` = the IDs with a handler (plus the ignored hints), passed again after
    `configure`. Zynthian registers the transport and the clip launcher only with zynseq; the
    reference host the navigator only if the controller wants it. `/host/features` reports
    `OP_FEATURES`. The coverage tests: the snapshot `UNHANDLED_COMMANDS` (the setup of the test),
    and the empirical run (a record of every ID on a fresh handler) equals the handled set, per
    setup (Zynthian with and without zynseq; the browser's `replace/device` and the Reaper form
    of the bypass are handled but need a device in the stand-ins).
  - **Report again by entity** (§2.4): `report_again_entity(view)` gives `("ch", ChKind,
    position)`, `("param", ParamOwner, index)` or `("pad", PadKind, Dev | None, index)`;
    `Reporter.forget(entity)` matches the families of the table (the IDs whose first slots are
    `ch_kind, ch_pos` / `owner, param` / `pad_kind, pad_dev, pad_index`). The rules are the old
    ones of the text prefixes (every command with a number value or none of a channel, a slot,
    a parameter of a device or the project, every command to the master), so the reports again
    are the same; the rules themselves were not revisited (they decide what the controllers see
    after a command, and the conformance results depend on them).
  - **Zero copy:** the binding passes a `memoryview` of the core's buffer
    (`ctypes.cast(data, POINTER(c_uint8 * n))`, no `string_at`) and releases it after the call,
    so a view which escaped raises instead of reading freed memory. `ViewCommand` copies its
    record (24 bytes plus payload) and reads its value during the call; its `processor`,
    `full_command`, `command` (the log, `commands.log` readers, the not handled list) are the
    core's text of the copied record.
  - `dbm_host.coverage.legacy_record` / `run_command`: a command in the legacy text form,
    converted by the core, for the tests (a line not in the table raises).
- **Bitwig** (`270a9ae`): `ModelReporter` without `Mode`, `round6`, `convert`, the text lines and
  the check; a value of a wrong type is logged once per ID. `DbmExtension` without the text
  callbacks and `dispatchText`; `NativeCore` without them (null in `DbmCallbacks`) and without
  `updateModel`. The handled bitset with the navigator (see above) and the `handled` events.
  `BitwigModel` reports `/host/features valueInc valueReset immediately indication`.
  **The probe** (`ProbeExtension`) has no core: it keeps a small formatter of its own
  (`probeLines`: the records of the clip and the grid as the text protocol had them, doubles
  with 6 decimals, the steps as `StepInfo::to_wire`, only changed texts). The core's conversion
  was not an option: the probe runs without a core, and its lines must stay those of the
  committed Java reference runs. `ProtocolTests`: the core reads every record of every report
  ID (1,634 records of 222 IDs), exact doubles, the probe's lines, the handled bitset with and
  without the navigator.
- `src/parser_old.rs` removed (`5be3c53`): the plan removes it once the table is stable.

**Deviations from the old text handlers** (Python; all found by the differential run, none in
the conformance scenarios)
- A combination which the core never sends but the text path happened to map onto another
  command (a device command with a track or the master as its device, e.g. `track pinned` from
  `DEVICE_PINNED {dev: TRACK}`) is no longer executed; the differential run uses only the core's
  samples, where these did not change the result.
- Zynthian without zynseq: a slot command on the "add chain" placeholder is now logged as not
  handled (before: ignored); `send/<s>/active` on the placeholder likewise (it is not handled on
  any track).
- The log of a command keeps the core's text; its `number` of a pair or a step attribute is
  0 (the typed value is a tuple), and `browser column/<c>/reset` no longer has the reset flag.

**Verification**
- A **differential run** (scratch, not committed): the old Python handlers (117cdfb) and the new
  ones on the core's samples of every `CmdId` (1,306 commands: 16 per ID, plus the 16 × 8 grid,
  track 1 and slot 0 variants), each on a fresh host, comparing the unhandled list, the log,
  the reports after the command and the reports again 400 ms later; for the reference host,
  zynthian with zynseq and without. Only the cosmetic log fields above differ.
- `cargo test` (443) and `--no-default-features` (414), clippy 0 warnings both ways, `cargo
  build --release`; the Python tests (87), conformance `--all` (195 the same), the Zynthian tests
  (206), the C++ wrapper tests, the Maven build with `ProtocolTests`.
- **Bitwig conformance** (Bitwig 5.3.13, the installed extension of this step, `conformance_all.sh`
  from the worktree with `DBM_FLEXI_MAPPING`, VirMIDI card 5): maschine_mk2 27/27,
  launchcontrol_xl 27/27, maschine_jam 41/41 the same; generic_flexi 99/100, the rerun 100/100
  (the difference: Bitwig's order of the browser's location filters, `osc:/browser/filter/1/item/10..16/name`
  in `flexi.modes.browser-discard`, not an output of the extension). 20 of the 21 replays
  recorded again (exact doubles, the `handled` events); the Flexi `modes` replay keeps its
  recording of step 3 and still passes, both replay modes.

**Measured**
- Zynthian idle pass (`test_cpu.py`, Mk2, 300 ticks, 227 puts per tick, 3 runs): 170–173 µs
  (step 2, part 4: 178–180 µs binary; the rounding is not on the idle path, the gain is the
  simpler `_changed`).
- Python `Reporter.put`, 1,000 changed send volumes per pass: 864 ns per changed double (step 2,
  part 4: 1,291 ns with `round(v, 6)`), 137 ns unchanged (126 ns).
- Java: not measured again; step 3 measured a changed double at 23 ns without the rounding
  (260–310 ns with it, §6.5).

**What remains**
- Report again: the rules of the text prefixes (Python: also the effect tracks and every
  command with a number value; Java: only the value commands, not the effect tracks) differ
  between the hosts; unifying them is a behaviour change for the controllers and needs Java
  references.
- The binding's `Backend.process_commands` default (the legacy text and the `process_*`
  methods) stays for simple Python backends; the text callbacks of the C ABI stay for Reaper.
- The value operation features stay on `/host/features`; an op bitset (per value command) would
  retire it, but would need a new argument of the C ABI.

### 6.7 Step 6 (planned 2026-11-08): subscriptions — the core says what it needs

**Goal:** the host collects and observes only the values the controller currently uses (the
displayed mode/view plus what the controller logic always needs), instead of everything that
`ModelSetup` allows. `ModelSetup` (`src/daw/model_setup.rs`) is already a static subscription (bank
sizes, pages, parameters, `wants_*`); step 6 adds the dynamic part that follows modes and views.

**Collecting what is needed**
- **Declared per mode/view (the source):** e.g. a parameter mode declares the cursor device's
  parameter page, a session view the slots of its bank window, the browser mode the browser. Explicit,
  cheap and testable; fits the generic modes/views of doc/CONTROLLER_DSL_PLAN.md, which can carry the
  declaration.
- **Traced model reads (the check):** count the model sections the controller reads (the ~20 section
  accessors: `current_track_bank`, `cursor_device`, `browser`, `note_clip`, `scene_bank`,
  `transport`, …) while it updates LEDs/displays and handles input; in tests and debug builds, compare
  with the declaration to catch drift. Not the source: per field it would mean wrapping hundreds of
  getters, and a trace of the display updates misses what input handling reads (a pad press reads
  "track exists"/"selected") and has nothing for the first frame of a new mode.
- **Always subscribed:** what the controller logic needs regardless of the display: transport state,
  track selection/existence for bank navigation, the project name (note mapping), the values from
  which the core detects features (§2.5).
- **Several surfaces:** the union of their sets.

**Protocol**
- A command `SUBSCRIBE` (core → host) with the change of the set: sections (groups of `MsgId`s) on/off,
  plus index ranges where the bank windows don't already limit them (visible note clip steps, browser
  columns). The ranges are mostly given already: the host reports only the bank windows.
- Sent on mode/view changes; the host answers a newly subscribed section with its values at once
  (like a refresh of that section). Hosts that don't handle `SUBSCRIBE` (handled bitset, §2.5) keep
  collecting everything — unchanged behaviour.

**Risk: latency on mode switches (conformance).** Java keeps all observers active, so a newly shown
mode has its data at once; with subscriptions the first frame can be empty or stale until the host
answers. Mitigations: the immediate answer above; unsubscribe only after a delay (seconds), so
switching back and forth stays instant; keep the sections of neighbouring modes (the next mode on the
same button) subscribed. The conformance runs must not change.

**Gains**
- **Zynthian (most):** the polling collectors skip unsubscribed sections; the polling was the CPU
  problem (doc/ZYNTHIAN_CPU.md). An alternative to the postponed event-driven collection, and mostly
  core work, so less affected by zynthian changing.
- **Bitwig:** `subscribe()`/`unsubscribe()` on banks, the cursor device, the browser, … save Bitwig's
  own observer work.
- **The Python reference host:** little.

**First step (prototype):** the declarations for the modes/views of one controller (Maschine Mk2 or
JAM), `SUBSCRIBE` in the core, the Zynthian collectors skipping unsubscribed sections, measured with
`backends/zynthian/tests/test_cpu.py`; the traced-reads check in the tests.
