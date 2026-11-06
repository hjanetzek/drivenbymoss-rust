# -*- coding: utf-8 -*-
# ******************************************************************************
# ZYNTHIAN PROJECT: Zynthian Control Device Driver
#
# DrivenByMoss on zynthian: processors as the core's devices
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

"""The processors of the active chain as the core's cursor device and its siblings (phase 2,
doc/HOST_BRIDGE_GUIDE.md §3.5, §4.6).

| Core | zynthian |
| --- | --- |
| device chain | the active chain's processors without the mixer strip (``MI``/``MR``), in chain order |
| cursor device | ``chain.current_processor`` (if it is not the mixer strip), else the first processor |
| name, bypass | ``processor.get_name()``, ``is_bypassed()``/``set_bypass()`` |
| parameter page of 8 | two controller screens of 4 (``get_ctrl_screens()``, in their order): page ``p`` = screens ``2p`` and ``2p+1`` |
| page name | the name of the first of the two screens |
| selected page | ``current_screen_index // 2``; ``parameterPage p`` selects screen ``2p`` |
| parameter value | the zctrl, normalized: by index for zctrls with ticks/labels, else linear between ``value_min`` and ``value_max`` |
| parameter text | ``get_value2label()`` |

The first instrument (``/primary``, the model setup's ``deviceIds`` ``FIRST_INSTRUMENT``) is the
active chain's first processor of the type "MIDI Synth" (the synth engine), with the same name,
bypass and pages.

Not mapped: window, expand, pinning, remove, duplicate, moving, layers, drum pads.
"""

from zyngine.ctrldev.dbm_zynthian.model import ZctrlParameter

PARAMS_PER_SCREEN = 4
SCREENS_PER_PAGE = 2
PARAMS_PER_PAGE = PARAMS_PER_SCREEN * SCREENS_PER_PAGE
MIXER_ENGINES = ("MI", "MR")
INSTRUMENT_TYPE = "MIDI Synth"


class DeviceParameter(ZctrlParameter):
    """A processor's zctrl as a normalized value; zctrls with ticks (labels, enums) by index."""

    def __init__(self, get_zctrl):
        super().__init__(get_zctrl)

    @staticmethod
    def _ticks(zctrl):
        ticks = getattr(zctrl, "ticks", None)
        return ticks if ticks and len(ticks) > 1 else None

    @staticmethod
    def _usable(zctrl):
        return zctrl is not None and not getattr(zctrl, "is_path", False) and isinstance(zctrl.value, (int, float))

    @property
    def value(self):
        zctrl = self.zctrl
        if not self._usable(zctrl):
            return 0.0
        ticks = self._ticks(zctrl)
        if ticks:
            try:
                return ticks.index(zctrl.value) / (len(ticks) - 1)
            except ValueError:
                pass
        return self._normalize(zctrl, zctrl.value)

    @property
    def default(self):
        zctrl = self.zctrl
        if not self._usable(zctrl) or zctrl.value_default is None:
            return 0.0
        return self._normalize(zctrl, zctrl.value_default)

    def set(self, value):
        zctrl = self.zctrl
        if not self._usable(zctrl):
            return False
        value = min(1.0, max(0.0, float(value)))
        ticks = self._ticks(zctrl)
        if ticks:
            zctrl.set_value(ticks[round(value * (len(ticks) - 1))])
        else:
            zctrl.set_value(zctrl.value_min + value * (zctrl.value_max - zctrl.value_min))
        return True

    def reset(self):
        zctrl = self.zctrl
        if self._usable(zctrl) and zctrl.value_default is not None:
            zctrl.reset_value()

    @property
    def name(self):
        zctrl = self.zctrl
        if zctrl is None:
            return ""
        return getattr(zctrl, "short_name", None) or zctrl.name or zctrl.symbol


class ProcessorDevice:
    """A processor as a device of the core."""

    def __init__(self, processor):
        self.processor = processor

    @property
    def name(self):
        try:
            return self.processor.get_name() or ""
        except Exception:
            return ""

    @property
    def enabled(self):
        try:
            return not self.processor.is_bypassed()
        except Exception:
            return True

    def set_enabled(self, enabled):
        self.processor.set_bypass(not enabled)

    def toggle_bypass(self):
        self.processor.toggle_bypass()

    def screens(self):
        """(name, zctrls) of the controller screens, in their order."""
        try:
            return list(self.processor.get_ctrl_screens().items())
        except Exception:
            return []

    def page_names(self):
        screens = self.screens()
        return [screens[i][0] for i in range(0, len(screens), SCREENS_PER_PAGE)]

    @property
    def selected_page(self):
        index = self.processor.get_current_screen_index()
        return index // SCREENS_PER_PAGE if index is not None and index >= 0 else -1

    def select_page(self, page):
        pages = self.page_names()
        if pages:
            page = max(0, min(int(page), len(pages) - 1))
            self.processor.set_current_screen_index(page * SCREENS_PER_PAGE)

    def zctrl(self, index):
        """The zctrl of the flat parameter index (page index // 8), None if there is none."""
        screens = self.screens()
        page, slot = divmod(index, PARAMS_PER_PAGE)
        screen = page * SCREENS_PER_PAGE + slot // PARAMS_PER_SCREEN
        if screen >= len(screens):
            return None
        zctrls = screens[screen][1]
        position = slot % PARAMS_PER_SCREEN
        return zctrls[position] if position < len(zctrls) else None

    def parameter(self, index):
        return DeviceParameter(lambda: self.zctrl(index))


class DeviceChain:
    """The processors of the active chain, the cursor device and the sibling window."""

    def __init__(self, model):
        self.model = model
        self.sibling_offset = 0
        self._devices = {}

    @property
    def chain(self):
        return self.model.chain_manager.active_chain

    def _device(self, processor):
        device = self._devices.get(id(processor))
        if device is None or device.processor is not processor:
            device = ProcessorDevice(processor)
            self._devices[id(processor)] = device
        return device

    def devices(self):
        chain = self.chain
        if chain is None:
            return []
        try:
            processors = chain.get_processors()
        except Exception:
            return []
        return [self._device(p) for p in processors if getattr(p, "eng_code", None) not in MIXER_ENGINES]

    def cursor(self):
        devices = self.devices()
        if not devices:
            return None
        current = getattr(self.chain, "current_processor", None)
        for device in devices:
            if device.processor is current:
                return device
        return devices[0]

    def primary(self):
        """The first instrument: the first "MIDI Synth" processor of the active chain."""
        for device in self.devices():
            if getattr(device.processor, "type", None) == INSTRUMENT_TYPE:
                return device
        return None

    def select(self, device):
        self.chain.set_current_processor(device.processor)

    def select_neighbour(self, direction):
        devices = self.devices()
        cursor = self.cursor()
        if cursor is None:
            return
        position = devices.index(cursor) + direction
        if 0 <= position < len(devices):
            self.select(devices[position])

    def scroll_siblings(self, direction, size):
        count = len(self.devices())
        offset = self.sibling_offset + direction * size
        if 0 <= offset < max(count, 1):
            self.sibling_offset = offset
