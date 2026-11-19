# Copyright (c) 2026 Hannes Janetzek
# Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

"""What the controller simulators share: the ALSA sequencer client (ctypes), the MIDI parser,
the Qt bridge base class, the application scaffolding and the QML module ``SimCommon``."""

import os

QML_IMPORT_PATH = os.path.join(os.path.dirname(os.path.abspath(__file__)), "qml")
