// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

// The simulated Push: an own drawing after the hardware layout. The controls come from
// sim.controls (positions in surface units, see layout.py) and are scaled to the window. The
// window, pads, buttons, encoders, status bar and MIDI monitor are the shared SimCommon
// components (support/controller_simulator/sim_common/qml); the touch strip and the display
// are the Push's own.

import QtQuick
import QtQuick.Controls
import SimCommon

SimWindow {
    id: window
    title: {
        var names = { "push1": "Push 1", "push2": "Push 2", "push3": "Push 3" }
        return names[sim.model] + " Simulator"
    }
    componentMap: ({ "pad": padComponent, "button": buttonComponent, "encoder": encoderComponent,
                     "strip": stripComponent, "display": displayComponent })

    Component { id: padComponent; Pad {} }
    Component { id: buttonComponent; LedButton {} }
    Component { id: encoderComponent; Encoder {} }

    // ---------------------------------------------------------------------------------------
    // Touch strip: 31 LEDs; press/drag = touch + position (bottom 0 .. top 16383)

    Component {
        id: stripComponent
        Rectangle {
            id: strip
            radius: 1.5 * window.k
            color: "#18191c"
            border.color: sarea.pressed ? "#ffffff" : "#45464c"
            border.width: Math.max(1, 0.3 * window.k)
            Column {
                anchors.fill: parent
                anchors.margins: 1.2 * window.k
                spacing: (height - 31 * cellHeight) / 30
                property real cellHeight: height / 31 * 0.6
                Repeater {
                    model: 31
                    Rectangle {
                        required property int index
                        width: parent.width
                        height: parent.cellHeight
                        radius: height / 2
                        property real level: sim.stripLevels[30 - index]
                        color: Qt.rgba(0.2 + 0.8 * level, 0.2 + 0.8 * level, 0.22 + 0.78 * level, 1)
                    }
                }
            }
            MouseArea {
                id: sarea
                anchors.fill: parent
                function pos(my) { return Math.max(0, Math.min(1, 1 - my / height)) }
                onPressed: (mouse) => { sim.stripTouch(true); sim.stripMove(pos(mouse.y)) }
                onPositionChanged: (mouse) => sim.stripMove(pos(mouse.y))
                onReleased: sim.stripTouch(false)
            }
        }
    }

    // ---------------------------------------------------------------------------------------
    // Display: the frames of the socket (Push 2/3) or the 4 x 68 characters (Push 1)

    Component {
        id: displayComponent
        Rectangle {
            color: "#000000"
            border.color: "#55565c"
            border.width: Math.max(1, 0.4 * window.k)
            Image {
                anchors.fill: parent
                anchors.margins: parent.border.width
                visible: sim.model !== "push1"
                cache: false
                smooth: true
                fillMode: Image.Stretch
                source: "image://display/" + sim.frameRevision
            }
            Rectangle {
                anchors.fill: parent
                color: "black"
                opacity: sim.model !== "push1" ? (1 - sim.displayBrightness) * 0.85 : 0
            }
            Column {
                visible: sim.model === "push1"
                anchors.centerIn: parent
                Repeater {
                    model: sim.textLines
                    Text {
                        required property string modelData
                        text: modelData
                        color: "#9cf0ff"
                        font.family: "monospace"
                        font.pixelSize: Math.max(6, 2.6 * window.k)
                    }
                }
            }
        }
    }
}
