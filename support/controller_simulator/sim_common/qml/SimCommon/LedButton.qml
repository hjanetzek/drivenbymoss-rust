// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

// A button (SimCommon): the LED colour (sim.buttonColor(c.cc)) fills the cap, the label is
// printed on it. Left mouse button: pressed while held. Right mouse button: latches the button
// (pressed until the next right or left click), for combinations like Shift or Select + another
// button without a touchscreen; a latched button has an amber frame. Pressed on the mirrored
// hardware (sim.controlPressed): a white frame.

import QtQuick

Rectangle {
    id: btn
    // The scale (pixels per surface unit) of the window (SimWindow.k)
    readonly property real k: Window.window && Window.window.k ? Window.window.k : 1
    property var c: parent.control
    property color led: { sim.revision; return sim.buttonColor(c.cc) }
    // Held by a right click until the next click
    property bool latched: false
    // Pressed on the mirrored hardware
    property bool hwPressed: { sim.revision; return sim.controlPressed(c.cc) }
    radius: 1.0 * k
    color: Qt.rgba(0.11 + 0.89 * led.r, 0.115 + 0.885 * led.g, 0.125 + 0.875 * led.b, 1)
    border.color: latched ? "#ffb000" : (barea.pressed || hwPressed ? "#ffffff" : "#4a4b52")
    border.width: latched || hwPressed ? Math.max(2, 0.6 * k) : Math.max(1, 0.3 * k)
    Text {
        anchors.fill: parent
        anchors.margins: 0.3 * k
        text: btn.c.label
        color: (btn.led.r * 0.3 + btn.led.g * 0.59 + btn.led.b * 0.11) > 0.55 ? "#111111" : "#c8c8cc"
        font.pixelSize: Math.max(7, Math.min(2.3 * k, btn.height * 0.55))
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
        wrapMode: Text.WordWrap
        elide: Text.ElideRight
    }
    MouseArea {
        id: barea
        anchors.fill: parent
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        // The press of a click which only releases a latch (no second press)
        property bool unlatching: false
        onPressed: (mouse) => {
            if (btn.latched) {
                // Any click on a latched button releases it
                btn.latched = false
                unlatching = true
                sim.buttonRelease(btn.c.cc)
            } else if (mouse.button === Qt.RightButton) {
                btn.latched = true
                sim.buttonPress(btn.c.cc)
            } else {
                sim.buttonPress(btn.c.cc)
            }
        }
        onReleased: (mouse) => {
            if (unlatching) {
                unlatching = false
            } else if (!btn.latched) {
                sim.buttonRelease(btn.c.cc)
            }
        }
        onCanceled: {
            if (unlatching) {
                unlatching = false
            } else if (!btn.latched) {
                sim.buttonRelease(btn.c.cc)
            }
        }
    }
}
