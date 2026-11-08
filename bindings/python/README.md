# DrivenByMoss: Python binding

A pure Python binding (ctypes) of the C interface in [`include/drivenbymoss.h`](../../include/drivenbymoss.h),
for Python hosts such as Zynthian. It mirrors the C++ wrapper in [`bindings/cpp`](../cpp):
`drivenbymoss.App` is the instance, `drivenbymoss.Backend` the callbacks as overridable methods.

Licensed under LGPLv3.

## Requirements

- Python 3.11 or newer, no other packages.
- The native library `libdrivenbymoss.so` (`.dylib`, `drivenbymoss.dll`), built from this
  repository:

  ```sh
  cargo build            # target/debug
  cargo build --release  # target/release
  ```

The library is searched in this order: the path in the environment variable `DBM_LIBRARY`, then
`target/debug` and `target/release` of the repository which contains the package.

## Usage

```python
import drivenbymoss

class MyBackend(drivenbymoss.Backend):
    def get_midi_inputs(self):
        return [drivenbymoss.MidiDevice(3, "Launchpad X LPX MIDI Out")]
    def get_midi_outputs(self):
        return [drivenbymoss.MidiDevice(5, "Launchpad X LPX MIDI In")]
    def open_midi_input(self, device_id):
        return True
    def open_midi_output(self, device_id):
        return True
    def send_midi_data(self, device_id, data):
        ...  # write the bytes to the MIDI output
    def process(self, processor, command, value):
        ...  # execute a command of the core in the DAW, e.g. ("transport", "play", None)

# ini_path contains DrivenByMoss4Reaper.config (CONTROLLER_INSTANCE0=...) and the controller configs
with drivenbymoss.App(MyBackend(), ini_path, "1.0/linux-x86_64") as app:
    app.start_infrastructure()
    while running:
        for device_id, data in received_midi():
            app.on_midi_event(device_id, data)
        app.tick()                                 # every 30ms
        app.update_model("track/0/name Bass\n")    # changes of the DAW, '<address> <value>' lines
```

`App.create_controller(backend, uuid, config_path)` runs exactly one controller on the MIDI
ports 0 (as for Bitwig); `drivenbymoss.list_controllers()` lists the UUIDs. `get_model_setup()`
returns the model setup of the running controller as a dict, `get_settings()`,
`set_setting()`, `take_setting_changes()` and the document settings calls serve hosts with
their own settings UI. See [doc/BACKEND_PROTOCOL.md](../../doc/BACKEND_PROTOCOL.md) for the
call sequence and the model data.

## The binary protocol

`drivenbymoss/protocol.py` is generated from the tables of the core (`cargo run --bin
dbm-protocol-gen`; a Rust test fails if it is stale, doc/TYPED_PROTOCOL_PLAN.md §2.7): the IDs
and index domains as `IntEnum`s (`Msg`, `Cmd`, `ChKind`, `Processor`, …), the metadata
(`MSG_INFO`, `CMD_INFO`: name, template, index slots, value kind), a `Writer` with one method
per report and a view class per command.

```python
from drivenbymoss import protocol

class MyBackend(drivenbymoss.Backend):
    def process_commands(self, buffer):            # one buffer per call into the library
        for c in protocol.iter_commands(buffer):   # or iter_flat: (id, op, i0, i1, i2, i3, value)
            if c.id == protocol.Cmd.CHANNEL_MUTE:
                ...                                # c.ch_kind, c.ch_index, c.value

app.use_binary_commands()                          # before start_infrastructure
w = protocol.Writer()                              # reused: w.clear()
w.channel_volume(protocol.ChKind.TRACK, 3, 0.5)
app.update_model_bin(w)
```

Without an override, `process_commands` converts the commands to their text and calls the
`process_*` methods, so a text backend works unchanged. `use_binary_commands(handled=[...])`
passes the handled command IDs (the library logs the others once) and returns False if the
library has another major protocol version. `messages_to_text`/`messages_from_text` convert
buffers for logs and tests; `protocol_info()` is the library's (major, minor, hash).

## Threading and callbacks

- The App is created with `DBM_FLAG_HOST_TICK`: the library starts no timer thread, the host
  calls `tick()` from its own loop. The backend methods are therefore only called on the thread
  which calls a method of the App, while the GIL is held.
- All calls on one App are serialized. A backend method must not call methods of the same App:
  the library would deadlock, the binding raises a `RuntimeError` instead.
- Exceptions raised by backend methods are caught and logged with the logger `drivenbymoss`;
  they never cross into the library. A failing `open_midi_*` reports failure.
- The arguments of the backend methods are copies and can be kept.
- The log of the library goes to `Backend.log`; the default passes it to the logger
  `drivenbymoss` (INFO). The library writes nothing to stderr itself, except the message of a
  caught panic.

## Tests

```sh
cargo build
python3 -m unittest discover -s bindings/python/tests
```

The reference host (`dbm_host`) reports and receives the commands in the binary protocol by
default; `DBM_HOST_PROTOCOL=text` selects the text protocol, `DBM_HOST_PROTOCOL=check` the binary
one with every report record compared with the core's conversion of the text line (the dual
run). The suites pass in all three.

## Conformance in process

`conformance` runs the scenario files of `tests/conformance/<setup>/` against the reference host
(`dbm_host`, a simulated DAW) and compares the results with those of the Java DrivenByMoss on
Bitwig (doc/PYTHON_HOST_PLAN.md §4). From `bindings/python`, after `cargo build`:

```sh
python3 -m conformance launchcontrol_xl track_mix   # one file (a name of the setup or a path)
python3 -m conformance launchcontrol_xl             # all files of a setup
python3 -m conformance --all                        # all setups; -q: only the status lines
python3 -m conformance.extract [setup ...]          # the references from target/conformance/*-java.json
```

It reuses the functions of `backends/bitwig/tools/conformance_runner.py` (scenario format, LED
state, expectations, comparison, Flexi mapping) and the controller settings of the Bitwig
profiles (`~/.local/share/dbm-conformance/profiles/<setup>/config`, or `DBM_PROFILE_DIR`; read
only). A file fails on a difference, a failed expectation, an unhandled command or a core error;
keys of the Java run which the state view does not model yet are counted as "not modelled".
The references are `tests/conformance/<setup>/replay/<run>-java.ref.json`, extracted from clean
Bitwig runs together with the replays.
