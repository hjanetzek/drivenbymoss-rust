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

## Threading and callbacks

- The App is created with `DBM_FLAG_HOST_TICK`: the library starts no timer thread, the host
  calls `tick()` from its own loop. The backend methods are therefore only called on the thread
  which calls a method of the App, while the GIL is held.
- All calls on one App are serialized. A backend method must not call methods of the same App:
  the library would deadlock, the binding raises a `RuntimeError` instead.
- Exceptions raised by backend methods are caught and logged with the logger `drivenbymoss`;
  they never cross into the library. A failing `open_midi_*` reports failure.
- The arguments of the backend methods are copies and can be kept.

## Tests

```sh
cargo build
python3 -m unittest discover -s bindings/python/tests
```
