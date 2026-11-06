# -*- coding: utf-8 -*-
# ******************************************************************************
# ZYNTHIAN PROJECT: Zynthian Control Device Driver
#
# DrivenByMoss on zynthian: zynseq's transport as the core's transport
#
# Copyright (C) 2026 Hannes Janetzek
#
# ******************************************************************************
#
# This program is free software; you can redistribute it and/or
# modify it under the terms of the GNU General Public License as
# published by the Free Software Foundation; either version 2 of
# the License, or any later version.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
# GNU General Public License for more details.
#
# For a full copy of the GNU General Public License see the LICENSE.txt file.
#
# ******************************************************************************

"""zynseq's transport (JACK transport, tempo, metronome) as the core's transport (phase 2,
doc/HOST_BRIDGE_GUIDE.md §3.1, §4.2). "Play" is zynseq's transport (decided).

| Core | zynseq (libseq) |
| --- | --- |
| ``/play`` | ``getTransportState() != STOPPED`` (starting, playing or stopping) |
| ``transport play`` | ``transportStart(TRANSPORT_CLIENT_DBM)`` |
| ``transport stop``, ``stopAndRewind`` | ``transportStop(255)``: all clients, as ``transport_stop("ALL")`` intends |
| ``transport restart`` | stop, then start |
| ``/tempo``, ``tempo <bpm>``, ``tempo +|-|++|--`` | ``getTempo()``, ``zynseq.set_tempo`` (its zctrl), ±0.01 / ±1 BPM |
| ``transport tapTempo`` | ``tapTempo()`` |
| ``/numerator``, ``/denominator`` | ``getBpb()``, 4 |
| ``/click``, ``transport metronome|toggleMetronome`` | metronome mode ``TRANSPORT`` (on while the transport runs) or ``OFF`` |
| ``/click/volume``, ``transport metronomeVolume[/inc]`` | ``getMetronomeVolume``/``setMetronomeVolume`` (0..1) |

The adapter calls libseq itself with a client id: ``zynseq.transport_start(client)`` passes the
name as ``bytes`` to ``transportStart(uint8_t id)`` (analysis §9 Z9). ``getTransportState`` is
exported only since the ``dbm`` branch declares it in ``zynseq.h`` (§9 Z10); with an older
library the state is read from the exported global ``g_nTransportState``.

Not mapped (no arranger): position, loop, punch, automation, overdub, record (the audio
recorder is later), undo and redo.
"""

import ctypes

# zynseq/constants.h
STOPPED = 0
METRO_MODE_OFF = 0
METRO_MODE_TRANSPORT = 1
METRO_PLAYING_MODES = (1, 2, 3, 4)  # TRANSPORT, ON, INTRO, NO_PEEP
TRANSPORT_CLIENT_ALL = 255
# A transport client of its own (zynseq uses 1 and 2)
TRANSPORT_CLIENT_DBM = 6

TEMPO_STEPS = {"+": 0.01, "-": -0.01, "++": 1.0, "--": -1.0}
MIN_TEMPO, MAX_TEMPO = 10.0, 499.0


class ZynTransport:

    def __init__(self, zynseq):
        self.zynseq = zynseq

    @property
    def libseq(self):
        return getattr(self.zynseq, "libseq", None)

    @property
    def available(self):
        return self.libseq is not None

    # Values

    @property
    def playing(self):
        return self.transport_state() != STOPPED

    def transport_state(self):
        libseq = self.libseq
        try:
            # uint8_t: zynseq.py declares no restype for it, only the low byte is defined
            return libseq.getTransportState() & 0xFF
        except AttributeError:
            # libzynseq without getTransportState in its C interface
            return ctypes.c_uint8.in_dll(libseq, "g_nTransportState").value

    @property
    def tempo(self):
        return float(self.libseq.getTempo())

    @property
    def numerator(self):
        return max(1, int(self.libseq.getBpb()))

    @property
    def metronome(self):
        return self.libseq.getMetronomeMode() in METRO_PLAYING_MODES

    @property
    def metronome_volume(self):
        return min(1.0, max(0.0, float(self.libseq.getMetronomeVolume())))

    # Operations

    def play(self):
        self.libseq.transportStart(ctypes.c_uint8(TRANSPORT_CLIENT_DBM))

    def stop(self):
        self.libseq.transportStop(ctypes.c_uint8(TRANSPORT_CLIENT_ALL))

    def restart(self):
        self.stop()
        self.play()

    def set_tempo(self, bpm):
        self.zynseq.set_tempo(min(MAX_TEMPO, max(MIN_TEMPO, float(bpm))))

    def change_tempo(self, step):
        self.set_tempo(self.tempo + TEMPO_STEPS[step])

    def tap_tempo(self):
        self.libseq.tapTempo()

    def set_metronome(self, on):
        self.libseq.setMetronomeMode(ctypes.c_uint8(METRO_MODE_TRANSPORT if on else METRO_MODE_OFF))

    def set_metronome_volume(self, value):
        self.libseq.setMetronomeVolume(ctypes.c_float(min(1.0, max(0.0, float(value)))))
