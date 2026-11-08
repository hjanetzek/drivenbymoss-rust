# A typed, binary protocol between the core and its hosts: plan

Status: **steps 0a and 0b done** (2026-11-08, branch `typed-protocol`): the command table and
the address table in `src/protocol/`, `Cmd`/`CmdId` and `Msg`/`MsgId`; the core sends `Cmd`
values and applies `Msg` values, the text on the wire is unchanged. Next: step 1. What was done,
the choices and the deviations from this plan: §6. The decision and the motivation are in
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
| 4 | 8 | `idx[4]` | u16 each, unused = 0; enum domains as their numbers; the grid id as `steps << 8 \| rows` (both ≤ 255; checked when the model setup is created) |
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
    open question (§5.3).
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
7. ~~The grid id~~: **decided** (2026-11-08): one u16 index `steps << 8 | rows` in the binary
   form, checked (both ≤ 255) when the model setup is created (step 1). All grids of the code
   are ≤ 64 × 128. In Rust it is `protocol::GridId { steps, rows }` (u16 each), written
   `<steps>x<rows>` in the legacy text.
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

**Next: step 1** (§3, §2.2–§2.7): the binary record encode/decode in the core (from the two
tables: indices from the address fields and the command fields, values by type),
`dbm_update_model_bin`, `dbm_set_command_callback` with per-call buffering, the text
conversions, the generator binary `dbm-protocol-gen` with the staleness test, the generated
`include/dbm_protocol.h` and the C++ wrapper, and the replay's binary mode as the proof.
