// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

// A pad (SimCommon): left click = note on with the velocity from the click height (top: 127;
// Shift: 127, Ctrl: 40), right button = note on and aftertouch from the mouse height while held,
// wheel while held = aftertouch; in MPE mode (sim.mpeActive()) horizontal drag = pitch bend,
// vertical = slide. Loaded by SimWindow with the control in parent.control (c.note).

import QtQuick

Rectangle {
    id: pad
    // The scale (pixels per surface unit) of the window (SimWindow.k)
    readonly property real k: Window.window && Window.window.k ? Window.window.k : 1
    property var c: parent.control
    property int pressure: 0
    property real startX: 0
    radius: 1.2 * k
    color: { sim.revision; return sim.padColor(c.note) }
    border.color: area.pressed ? "#ffffff" : "#45464c"
    border.width: area.pressed ? Math.max(1, 0.5 * k) : Math.max(1, 0.25 * k)

    function heightValue(my) { return Math.max(1, Math.min(127, Math.round(127 * (1 - my / height)))) }

    MouseArea {
        id: area
        anchors.fill: parent
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        onPressed: (mouse) => {
            var v = pad.heightValue(mouse.y)
            if (mouse.modifiers & Qt.ShiftModifier) v = 127
            else if (mouse.modifiers & Qt.ControlModifier) v = 40
            pad.startX = mouse.x
            sim.padPress(pad.c.note, v)
            if (mouse.button === Qt.RightButton) {
                pad.pressure = pad.heightValue(mouse.y)
                sim.padPressure(pad.c.note, pad.pressure)
            }
        }
        onPositionChanged: (mouse) => {
            if (sim.mpeActive()) {
                var dx = (mouse.x - pad.startX) / pad.width
                sim.padSlide(pad.c.note, Math.max(-1, Math.min(1, dx)), Math.max(0, Math.min(1, 1 - mouse.y / pad.height)))
            }
            if (pressedButtons & Qt.RightButton) {
                pad.pressure = pad.heightValue(mouse.y)
                sim.padPressure(pad.c.note, pad.pressure)
            }
        }
        onReleased: { pad.pressure = 0; sim.padRelease(pad.c.note) }
        onWheel: (wheel) => {
            if (!pressed) return
            pad.pressure = Math.max(0, Math.min(127, pad.pressure + (wheel.angleDelta.y > 0 ? 8 : -8)))
            sim.padPressure(pad.c.note, pad.pressure)
        }
    }
}
