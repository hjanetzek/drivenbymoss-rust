# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""The OSC commands of the scenarios (``osc = "…"`` in ``setup`` and ``before``), as model
operations of the simulated DAW.

In the Bitwig runs these commands go to the OSC controller of the Java DrivenByMoss
(``conformance_runner.send_osc``, receive port 8000), which executes them on Bitwig. Here each
address maps to a ``SimDaw`` call, with the semantics of the Java modules
(``controller/osc/module/TrackModule``, ``TransportModule``, ``DeviceModule``) and the Bitwig
implementation behind them (``bitwig/framework/daw/data/TrackImpl``, ``ChannelImpl``,
``TransportImpl``, ``ParameterPageBankImpl``):

- Track numbers are 1-based positions in the observer's track bank (``state.OscController``).
- A command without a value toggles where the Java module toggles (``isTrigger (null)`` is
  true), e.g. ``/track/1/mute``.
- Values of parameters are 0..127 and go through the takeover mode of the DAW
  (``RangedValueImpl.setValue``: ``set (value, 128)``).
- ``/track/N/monitor <v>`` always switches the monitoring on, whatever the value
  (``TrackImpl.setMonitor`` ignores it).

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
        osc.daw.application.edit()


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
        osc.daw.application.edit()


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
    device = osc.daw.cursor_device if match.group(1) == "device" else osc.daw.primary_device
    if device is not None:
        device.select_page(int(match.group(2)) - 1)


@command(r"/(device|primary)/page/(select|selected)")
def _device_page_value(osc, match, value):
    """``page/selected <n>``: the 1-based page as the value."""
    device = osc.daw.cursor_device if match.group(1) == "device" else osc.daw.primary_device
    if device is not None:
        device.select_page(int(to_number(value)) - 1)


__all__ = ["TABLE", "UnknownOscCommand", "execute", "is_trigger", "parse"]
