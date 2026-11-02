# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""The reference host of DrivenByMoss: a bridge with a simulated DAW, in plain Python.

The complete, readable implementation of doc/HOST_BRIDGE_GUIDE.md (see doc/PYTHON_HOST_PLAN.md
§3): ``SimDaw`` (the DAW model, ``model``), ``Reporter`` (change-only reporting, ``reporter``),
``CommandHandler`` (core -> model, ``commands``), ``Bridge`` (connects them to a
``drivenbymoss.App``, ``bridge``). Another DAW replaces ``SimDaw`` by its own API. The package
``drivenbymoss`` (the binding) must be importable, it lives next to this one.

Usage::

    daw = SimDaw().build(tracks=[("Synth", "INSTRUMENT"), ("Bass", "AUDIO")], effect_tracks=["FX 1"], scenes=8)
    with Bridge(daw, "5AFA5045-FF92-4737-A091-5B371CD5E529", config_dir).start() as bridge:
        bridge.wait_for_startup()
        bridge.send(bytes.fromhex("B84D64"))      # MIDI from the controller
        bridge.advance(300)                        # simulated time
        print(daw.tracks[0].volume.value, bridge.midi_out[-1], bridge.commands.unhandled)
"""

from .banks import Banks
from .bridge import Bridge
from .clip import Clip, Note, NoteGrid
from .commands import Command, CommandHandler
from .device import Device, DeviceChain, RemoteControl
from .layer import Layer
from .marker import Marker, Markers
from .model import Application, Arranger, Channel, MasterTrack, Mixer, NoteInput, Scene, Send, SimDaw, Slot, Track, Transport
from .navigator import ClipLauncherNavigator
from .parameter import TAKEOVER_CATCH, TAKEOVER_IMMEDIATE, Parameter, Preferences
from .reporter import Reporter

__all__ = [
    "Application",
    "Arranger",
    "Banks",
    "Bridge",
    "Channel",
    "Clip",
    "ClipLauncherNavigator",
    "Command",
    "CommandHandler",
    "Device",
    "DeviceChain",
    "Layer",
    "Marker",
    "Markers",
    "MasterTrack",
    "Mixer",
    "Note",
    "NoteGrid",
    "NoteInput",
    "Parameter",
    "Preferences",
    "RemoteControl",
    "Reporter",
    "Scene",
    "Send",
    "SimDaw",
    "Slot",
    "TAKEOVER_CATCH",
    "TAKEOVER_IMMEDIATE",
    "Track",
    "Transport",
]
