// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

// The simulated Maschine Mk2 or JAM: pads, buttons, encoders, window, status bar and MIDI monitor
// are the shared SimCommon components; the Mk2's text displays and the JAM's touch strips and
// level meters are drawn here. The controls come from sim.controls (layout.py), identified by
// their key (cc/note).

import QtQuick
import QtQuick.Controls
import SimCommon

SimWindow {
    id: window
    componentMap: ({ "pad": padComponent, "button": buttonComponent, "encoder": encoderComponent,
                     "display": displayComponent, "strip": stripComponent, "meter": meterComponent })

    Component { id: padComponent; Pad {} }
    Component { id: buttonComponent; LedButton {} }
    Component { id: encoderComponent; Encoder {} }

    // ---------------------------------------------------------------------------------------
    // Mk2 display: the button labels and the two MCU lines (28 characters of each on a display)

    Component {
        id: displayComponent
        Rectangle {
            id: disp
            property var c: parent.control
            property var lines: { sim.textRevision; return sim.displayLines(c.index) }
            color: "#05080a"
            border.color: "#55565c"
            border.width: Math.max(1, 0.4 * window.k)
            Column {
                anchors.centerIn: parent
                spacing: 0.8 * window.k
                Text {
                    text: disp.lines[0]
                    color: "#8fb8c8"
                    font.family: "monospace"
                    font.pixelSize: Math.max(6, 3.0 * window.k)
                }
                Repeater {
                    model: 2
                    Text {
                        required property int index
                        text: disp.lines[index + 1]
                        color: "#e8f4ff"
                        font.family: "monospace"
                        font.pixelSize: Math.max(7, 6.0 * window.k)
                    }
                }
            }
        }
    }

    // ---------------------------------------------------------------------------------------
    // JAM touch strip: 11 LEDs in NI's palette colours (the strip setup of the core); press and
    // drag = touch and value (0 bottom .. 127 top)

    Component {
        id: stripComponent
        Rectangle {
            id: strip
            property var c: parent.control
            property var colours: { sim.stripRevision; return sim.stripColours(c.index) }
            radius: 1.5 * window.k
            color: "#18191c"
            border.color: sarea.pressed ? "#ffffff" : "#45464c"
            border.width: Math.max(1, 0.3 * window.k)
            Column {
                anchors.fill: parent
                anchors.margins: 1.2 * window.k
                property real cellHeight: height / 11 * 0.7
                spacing: (height - 11 * cellHeight) / 10
                Repeater {
                    model: 11
                    Rectangle {
                        required property int index
                        width: parent.width
                        height: parent.cellHeight
                        radius: height / 3
                        property color led: strip.colours[10 - index]
                        color: Qt.rgba(0.13 + 0.87 * led.r, 0.13 + 0.87 * led.g, 0.15 + 0.85 * led.b, 1)
                    }
                }
            }
            MouseArea {
                id: sarea
                anchors.fill: parent
                function pos(my) { return Math.max(0, Math.min(1, 1 - my / height)) }
                onPressed: (mouse) => sim.stripMove(strip.c.index, pos(mouse.y))
                onPositionChanged: (mouse) => sim.stripMove(strip.c.index, pos(mouse.y))
                onReleased: sim.stripTouch(strip.c.index, false)
                onCanceled: sim.stripTouch(strip.c.index, false)
            }
        }
    }

    // ---------------------------------------------------------------------------------------
    // JAM level meter: 8 LEDs (CC 64 left, 65 right)

    Component {
        id: meterComponent
        Rectangle {
            id: meter
            property var c: parent.control
            property int lit: { sim.stripRevision; return sim.meterLeds(c.index) }
            color: "#18191c"
            radius: 1 * window.k
            Column {
                anchors.fill: parent
                anchors.margins: 0.8 * window.k
                property real cellHeight: height / 8 * 0.7
                spacing: (height - 8 * cellHeight) / 7
                Repeater {
                    model: 8
                    Rectangle {
                        required property int index
                        width: parent.width
                        height: parent.cellHeight
                        property bool on: 7 - index < meter.lit
                        color: on ? (index < 2 ? "#ff5040" : (index < 4 ? "#ffd040" : "#50e060")) : "#2a2b30"
                    }
                }
            }
        }
    }
}
