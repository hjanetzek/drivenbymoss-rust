// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

// A button (SimCommon): the LED colour (sim.buttonColor(c.cc)) fills the cap, the label is
// printed on it.

import QtQuick

Rectangle {
    id: btn
    // The scale (pixels per surface unit) of the window (SimWindow.k)
    readonly property real k: Window.window && Window.window.k ? Window.window.k : 1
    property var c: parent.control
    property color led: { sim.revision; return sim.buttonColor(c.cc) }
    radius: 1.0 * k
    color: Qt.rgba(0.11 + 0.89 * led.r, 0.115 + 0.885 * led.g, 0.125 + 0.875 * led.b, 1)
    border.color: barea.pressed ? "#ffffff" : "#4a4b52"
    border.width: Math.max(1, 0.3 * k)
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
        onPressed: sim.buttonPress(btn.c.cc)
        onReleased: sim.buttonRelease(btn.c.cc)
        onCanceled: sim.buttonRelease(btn.c.cc)
    }
}
