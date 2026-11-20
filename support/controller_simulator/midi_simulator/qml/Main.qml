// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

// The plain MIDI controllers (APC40 mkII, Launchpad Pro Mk3): only shared SimCommon components,
// the controls come from sim.controls (the device module's build_controls).

import QtQuick
import SimCommon

SimWindow {
    id: window
    componentMap: ({ "pad": padComponent, "button": buttonComponent, "encoder": encoderComponent,
                     "fader": faderComponent })

    Component { id: padComponent; Pad {} }
    Component { id: buttonComponent; LedButton {} }
    Component { id: encoderComponent; Encoder {} }
    Component { id: faderComponent; Fader {} }
}
