# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""The OSC commands of the scenarios (``osc = "…"`` in ``setup`` and ``before``), as model
operations of the simulated DAW.

In the Bitwig runs these commands go to the OSC controller of the Java DrivenByMoss
(``conformance_runner.send_osc``, receive port 8000), which executes them on Bitwig. Here each
address maps to a ``SimDaw`` call, with the semantics of the Java modules
(``controller/osc/module/TrackModule``, ``TransportModule``, ``DeviceModule``, ``LayoutModule``,
``ClipModule``) and the Bitwig
implementation behind them (``bitwig/framework/daw/data/TrackImpl``, ``ChannelImpl``,
``TransportImpl``, ``ParameterPageBankImpl``):

- Track numbers are 1-based positions in the observer's track bank (``state.OscController``).
- A command without a value toggles where the Java module toggles (``isTrigger (null)`` is
  true), e.g. ``/track/1/mute``.
- Values of parameters are 0..127 and go through the takeover mode of the DAW
  (``RangedValueImpl.setValue``: ``set (value, 128)``).
- ``/track/N/monitor <v>`` always switches the monitoring on, whatever the value
  (``TrackImpl.setMonitor`` ignores it).
- The device commands act on the observer's cursor device and first instrument, which follow
  the selection (``OscController.cursor_device``), the layers on its page of 8.

The values are typed as the runner sends them (``osc_monitor.build_message``: an integer, else
a float, else a string). An address the table does not know raises ``UnknownOscCommand``; the
runner counts these as unhandled commands.
"""

import re
from collections.abc import Callable

from osc_monitor import build_message, parse_message

from .state import OscController


class UnknownOscCommand(Exception):
    """An OSC command which the table does not map (yet)."""


def is_trigger(value) -> bool:
    """AbstractModule.isTrigger: no value or a number > 0."""
    return value is None or (isinstance(value, (int, float)) and not isinstance(value, bool) and value > 0)


def to_number(value) -> float:
    if not isinstance(value, (int, float)):
        raise UnknownOscCommand(f"not a number: {value!r}")
    return float(value)


def parse(text: str) -> tuple[str, object]:
    """Split ``/address value`` and type the value as the runner sends it; None without a
    value."""
    parts = text.split()
    ((address, values),) = parse_message(build_message(parts[0], parts[1:]))
    return address, (values[0] if values else None)


Handler = Callable[[OscController, re.Match, object], None]
TABLE: list[tuple[re.Pattern, Handler]] = []


def command(pattern: str) -> Callable[[Handler], Handler]:
    def register(handler: Handler) -> Handler:
        TABLE.append((re.compile(pattern), handler))
        return handler
    return register


def execute(osc: OscController, text: str) -> None:
    """Execute one command, e.g. ``/track/1/select 1``."""
    address, value = parse(text)
    for pattern, handler in TABLE:
        match = pattern.fullmatch(address)
        if match is not None:
            handler(osc, match, value)
            return
    raise UnknownOscCommand(address)


def set_value(osc: OscController, parameter, value) -> None:
    """A parameter value 0..127 with the takeover mode (``set (value, 128)``); a change is an
    edit of the project."""
    before = parameter.value
    parameter.set(to_number(value) / 127.0)
    if parameter.value != before:
        osc.daw.application.edit_value(("value", id(parameter)), lambda v: setattr(parameter, "value", v), before, parameter.value)


def set_flag(current: bool, value) -> bool:
    """A switch: toggled without a value, else set (``isTrigger``)."""
    return not current if value is None else is_trigger(value)


# ---------------------------------------------------------------------------------------------
# Transport (TransportModule)


@command(r"/play")
def _play(osc, match, value):
    t = osc.daw.transport
    if is_trigger(value) != t.playing:
        t.toggle_play()


@command(r"/stop")
def _stop(osc, match, value):
    """Stop if playing, else stop and rewind."""
    t = osc.daw.transport
    if t.playing:
        t.stop()
    else:
        t.stop_and_rewind()


@command(r"/(repeat|punchIn|punchOut|click|click/ticks)")
def _transport_switch(osc, match, value):
    t = osc.daw.transport
    attribute = {"repeat": "loop", "punchIn": "punch_in", "punchOut": "punch_out", "click": "metronome", "click/ticks": "metronome_ticks"}[match.group(1)]
    setattr(t, attribute, set_flag(getattr(t, attribute), value))


@command(r"/click/volume")
def _click_volume(osc, match, value):
    set_value(osc, osc.daw.transport.metronome_volume, value)


@command(r"/tempo/raw")
def _tempo(osc, match, value):
    t = osc.daw.transport
    before = t.tempo
    t.set_tempo(to_number(value))
    if t.tempo != before:
        osc.daw.application.edit_value("tempo", t.set_tempo, before, t.tempo)


@command(r"/automationWriteMode")
def _automation_write_mode(osc, match, value):
    """TransportImpl.setAutomationWriteMode (``AutomationMode.valueOf``): Read and Trim/Read
    switch the arranger and launcher automation writing off, the others on, Latch Preview is
    Latch."""
    if value is None:
        return
    mode = str(value).strip().lower()
    t = osc.daw.transport
    if mode in ("read", "trim_read"):
        t.automation_arranger = t.automation_launcher = False
    elif mode in ("write", "touch", "latch", "latch_preview"):
        t.automation_arranger = t.automation_launcher = True
        t.automation_mode = "latch" if mode == "latch_preview" else mode
    else:
        raise UnknownOscCommand(f"automation write mode {value}")


# ---------------------------------------------------------------------------------------------
# Tracks (TrackModule)


@command(r"/track/bank/page/([+-])")
def _track_bank_page(osc, match, value):
    osc.scroll_page(1 if match.group(1) == "+" else -1)


def _bank_track(osc: OscController, number: str):
    track = osc.track(int(number))
    if track is None:
        # TrackModule addresses the empty item of the bank; Bitwig ignores the command
        return None
    return track


@command(r"/track/([1-9]\d*)/(select|selected)")
def _track_select(osc, match, value):
    track = _bank_track(osc, match.group(1))
    if track is not None and is_trigger(value):
        track.select()


@command(r"/track/([1-9]\d*)/(mute|solo|recarm|activated)")
def _track_switch(osc, match, value):
    track = _bank_track(osc, match.group(1))
    if track is None:
        return
    name = match.group(2)
    if name == "mute":
        track.set_mute(set_flag(track.mute, value))
    elif name == "solo":
        track.set_solo(set_flag(track.solo, value))
    elif name == "recarm":
        track.set_arm(set_flag(track.arm, value))
    else:
        # TrackImpl.setIsActivated (isTrigger (value)): no toggle
        track.activated = is_trigger(value)


@command(r"/track/([1-9]\d*)/monitor")
def _track_monitor(osc, match, value):
    track = _bank_track(osc, match.group(1))
    if track is None:
        return
    if value is None:
        track.monitor_mode = "OFF" if track.monitoring else "ON"
    else:
        track.monitor_mode = "ON"


@command(r"/track/([1-9]\d*)/autoMonitor")
def _track_auto_monitor(osc, match, value):
    track = _bank_track(osc, match.group(1))
    if track is None:
        return
    auto = set_flag(track.monitor_mode == "AUTO", value)
    track.monitor_mode = "AUTO" if auto else "OFF"


@command(r"/track/([1-9]\d*)/(volume|pan)")
def _track_value(osc, match, value):
    track = _bank_track(osc, match.group(1))
    if track is not None:
        set_value(osc, track.volume if match.group(2) == "volume" else track.pan, value)


@command(r"/track/([1-9]\d*)/send/([1-9]\d*)/volume")
def _track_send(osc, match, value):
    track = _bank_track(osc, match.group(1))
    index = int(match.group(2)) - 1
    if track is not None and index < len(track.sends):
        set_value(osc, track.sends[index].volume, value)


@command(r"/track/([1-9]\d*)/clip/([1-9]\d*)/(select|selected)")
def _track_clip_select(osc, match, value):
    track = _bank_track(osc, match.group(1))
    index = int(match.group(2)) - 1
    if track is not None and index < len(track.slots):
        track.slots[index].select()


@command(r"/master/(mute|solo)")
def _master_switch(osc, match, value):
    master = osc.daw.master
    if match.group(1) == "mute":
        master.set_mute(set_flag(master.mute, value))
    else:
        master.set_solo(set_flag(master.solo, value))


@command(r"/master/(volume|pan)")
def _master_value(osc, match, value):
    master = osc.daw.master
    set_value(osc, master.volume if match.group(1) == "volume" else master.pan, value)


# ---------------------------------------------------------------------------------------------
# Devices (DeviceModule)


@command(r"/(device|primary)/page/([1-9]\d*)(/(select|selected))?")
def _device_page(osc, match, value):
    """``page/<n>`` (and ``page/<n>/selected``: the number counts) selects the n-th page of the
    page bank (``ParameterPageBankImpl.selectPage``)."""
    device = osc.cursor_device if match.group(1) == "device" else osc.primary_device
    if device is not None:
        device.select_page(int(match.group(2)) - 1, cursor=match.group(1) == "device")


@command(r"/(device|primary)/page/(select|selected)")
def _device_page_value(osc, match, value):
    """``page/selected <n>``: the 1-based page as the value."""
    device = osc.cursor_device if match.group(1) == "device" else osc.primary_device
    if device is not None:
        device.select_page(int(to_number(value)) - 1, cursor=match.group(1) == "device")


@command(r"/layout/(arrange|mix|edit|play)")
def _layout(osc, match, value):
    """LayoutModule: the panel layout."""
    osc.daw.application.panel_layout = match.group(1).upper()


@command(r"/clip/stopall")
def _clip_stop_all(osc, match, value):
    """ClipModule ``stopall``: stop the clips of all tracks (``sceneBank.stop``)."""
    osc.daw.stop_all_clips()


@command(r"/(device|primary)/param/([1-9]\d*)/value")
def _device_parameter(osc, match, value):
    """``param/<n>/value``: the n-th parameter of the selected page of the observer's parameter
    bank (DeviceModule.parseDeviceValue, ``IParameterBank.getItem``: the page follows the page
    selected in the DAW)."""
    cursor = match.group(1) == "device"
    device = osc.cursor_device if cursor else osc.primary_device
    number = int(match.group(2))
    if device is None or number > 8:
        return
    parameter = device.parameter(device.selected_page * 8 + number - 1, osc.daw.preferences, cursor)
    if parameter is not None:
        set_value(osc, parameter, value)


@command(r"/device/layer/([1-9]\d*)/(select|selected|mute|solo|volume|pan)")
def _device_layer(osc, match, value):
    """The layers of the observer's cursor device, page of 8 at offset 0
    (DeviceModule.parseDeviceLayerValue): ``select`` selects whatever the value, ``mute`` and
    ``solo`` toggle without a value, volume and panorama are 0..127."""
    device = osc.cursor_device
    number = int(match.group(1))
    layers = device.layer_list() if device is not None else []
    if number > 8 or number > len(layers):
        return
    layer = layers[number - 1]
    name = match.group(2)
    if name in ("select", "selected"):
        layer.select()
    elif name == "mute":
        layer.set_mute(set_flag(layer.mute, value))
    elif name == "solo":
        layer.set_solo(set_flag(layer.solo, value))
    else:
        set_value(osc, layer.volume if name == "volume" else layer.pan, value)


__all__ = ["TABLE", "UnknownOscCommand", "execute", "is_trigger", "parse"]
