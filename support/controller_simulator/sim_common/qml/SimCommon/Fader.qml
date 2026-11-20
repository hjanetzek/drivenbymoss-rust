// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

// A fader (SimCommon): vertical, or horizontal when wider than high (a crossfader). Press and drag
// set the value 0..127 (sim.faderMove(c.cc, value)); the cap shows sim.faderValue(c.cc). The
// faders are not motorized: the value is the one the simulator sent last.

import QtQuick

Item {
    id: fader
    readonly property real k: Window.window && Window.window.k ? Window.window.k : 1
    property var c: parent.control
    readonly property bool horizontal: width > height
    property int value: { sim.revision; return sim.faderValue(c.cc) }
    readonly property real capSize: (horizontal ? width : height) * 0.14

    // The slot
    Rectangle {
        anchors.centerIn: parent
        width: fader.horizontal ? parent.width : Math.max(2, parent.width * 0.18)
        height: fader.horizontal ? Math.max(2, parent.height * 0.18) : parent.height
        radius: Math.min(width, height) / 2
        color: "#111214"
        border.color: "#3e3f45"
    }
    // The cap
    Rectangle {
        width: fader.horizontal ? fader.capSize : parent.width
        height: fader.horizontal ? parent.height : fader.capSize
        x: fader.horizontal ? (parent.width - width) * fader.value / 127 : 0
        y: fader.horizontal ? 0 : (parent.height - height) * (1 - fader.value / 127)
        radius: 0.8 * fader.k
        color: area.pressed ? "#d8d8dc" : "#a8a9ae"
        border.color: "#2a2b30"
        Rectangle {
            anchors.centerIn: parent
            width: fader.horizontal ? Math.max(1, 0.3 * fader.k) : parent.width * 0.8
            height: fader.horizontal ? parent.height * 0.8 : Math.max(1, 0.3 * fader.k)
            color: "#202125"
        }
    }
    Text {
        anchors.top: fader.horizontal ? parent.bottom : undefined
        anchors.bottom: fader.horizontal ? undefined : parent.top
        anchors.horizontalCenter: parent.horizontalCenter
        text: fader.c.label
        color: "#9a9aa2"
        font.pixelSize: Math.max(7, 1.8 * fader.k)
    }
    MouseArea {
        id: area
        anchors.fill: parent
        function valueAt(mouse) {
            var f = fader.horizontal ? mouse.x / width : 1 - mouse.y / height
            return Math.round(Math.max(0, Math.min(1, f)) * 127)
        }
        onPressed: (mouse) => sim.faderMove(fader.c.cc, valueAt(mouse))
        onPositionChanged: (mouse) => sim.faderMove(fader.c.cc, valueAt(mouse))
        onWheel: (wheel) => sim.faderMove(fader.c.cc, Math.max(0, Math.min(127, fader.value + (wheel.angleDelta.y > 0 ? 2 : -2))))
    }
}
