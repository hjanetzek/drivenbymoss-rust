// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

// The simulated Push: an own drawing after the hardware layout. The controls come from
// sim.controls (positions in surface units, see layout.py) and are scaled to the window.

import QtQuick
import QtQuick.Controls

ApplicationWindow {
    id: window
    width: 1320
    height: 900
    visible: true
    color: "#16171a"
    title: {
        var names = { "push1": "Push 1", "push2": "Push 2", "push3": "Push 3" }
        return names[sim.model] + " Simulator"
    }

    readonly property real monitorHeight: sim.monitorVisible ? 220 : 0
    readonly property real k: Math.min((width - 24) / sim.surfaceWidth, (height - monitorHeight - 56) / sim.surfaceHeight)

    Shortcut { sequence: "F2"; onActivated: sim.monitorVisible = !sim.monitorVisible }

    Rectangle {
        id: surface
        x: (window.width - width) / 2
        y: 10
        width: sim.surfaceWidth * window.k
        height: sim.surfaceHeight * window.k
        radius: 4 * window.k
        color: "#232428"
        border.color: "#3a3b40"
        border.width: Math.max(1, 0.4 * window.k)

        Repeater {
            model: sim.controls
            delegate: Loader {
                required property var modelData
                x: modelData.x * window.k
                y: modelData.y * window.k
                width: modelData.w * window.k
                height: modelData.h * window.k
                sourceComponent: {
                    switch (modelData.kind) {
                    case "pad": return padComponent
                    case "button": return buttonComponent
                    case "encoder": return encoderComponent
                    case "strip": return stripComponent
                    case "display": return displayComponent
                    }
                    return null
                }
                property var control: modelData
            }
        }
    }

    // ---------------------------------------------------------------------------------------
    // Pad: left click = note on with the velocity from the click height (top: 127; Shift: 127,
    // Ctrl: 40), right button = note on and aftertouch from the mouse height while held, wheel
    // while held = aftertouch. Push 3 in MPE mode: horizontal drag = pitch bend, vertical = slide.

    Component {
        id: padComponent
        Rectangle {
            id: pad
            property var c: parent.control
            property int pressure: 0
            property real startX: 0
            radius: 1.2 * window.k
            color: { sim.revision; return sim.padColor(c.note) }
            border.color: area.pressed ? "#ffffff" : "#45464c"
            border.width: area.pressed ? Math.max(1, 0.5 * window.k) : Math.max(1, 0.25 * window.k)

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
    }

    // ---------------------------------------------------------------------------------------
    // Button: the LED colour fills the cap, the label is printed on it

    Component {
        id: buttonComponent
        Rectangle {
            id: btn
            property var c: parent.control
            property color led: { sim.revision; return sim.buttonColor(c.cc) }
            radius: 1.0 * window.k
            color: Qt.rgba(0.11 + 0.89 * led.r, 0.115 + 0.885 * led.g, 0.125 + 0.875 * led.b, 1)
            border.color: barea.pressed ? "#ffffff" : "#4a4b52"
            border.width: Math.max(1, 0.3 * window.k)
            Text {
                anchors.fill: parent
                anchors.margins: 0.3 * window.k
                text: btn.c.label
                color: (btn.led.r * 0.3 + btn.led.g * 0.59 + btn.led.b * 0.11) > 0.55 ? "#111111" : "#c8c8cc"
                font.pixelSize: Math.max(7, Math.min(2.3 * window.k, btn.height * 0.55))
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
    }

    // ---------------------------------------------------------------------------------------
    // Encoder: wheel = relative steps (Ctrl: x8), vertical drag = steps; press, drag and wheel
    // touch it (wheel touches end 500 ms after the last step)

    Component {
        id: encoderComponent
        Item {
            id: enc
            property var c: parent.control
            property real angle: 0
            property real lastY: 0
            property bool wheelTouch: false
            property bool touched: earea.pressed || wheelTouch
            onTouchedChanged: sim.encoderTouch(c.note, touched)

            function turn(steps) {
                if (steps === 0) return
                sim.encoderTurn(c.cc, steps)
                angle += steps * 6
            }

            Rectangle {
                anchors.fill: parent
                radius: width / 2
                color: enc.touched ? "#4b4d55" : "#33343a"
                border.color: "#5c5d66"
                border.width: Math.max(1, 0.4 * window.k)
                // The indicator: from the center to the outline, turning around the center
                Rectangle {
                    width: Math.max(2, 0.7 * window.k)
                    height: parent.height / 2 - parent.border.width
                    radius: width / 2
                    color: "#e0e0e4"
                    x: (parent.width - width) / 2
                    y: parent.height / 2 - height
                    // Its own rotation around its bottom end, the center of the encoder (in a
                    // Rotation transform, width and height would be those of the encoder item)
                    transformOrigin: Item.Bottom
                    rotation: enc.angle
                }
            }
            Text {
                anchors.top: parent.bottom
                anchors.horizontalCenter: parent.horizontalCenter
                text: enc.c.label
                color: "#9a9aa2"
                font.pixelSize: Math.max(7, 1.8 * window.k)
                visible: enc.c.label.length > 1
            }
            Timer { id: wheelTimer; interval: 500; onTriggered: enc.wheelTouch = false }
            MouseArea {
                id: earea
                anchors.fill: parent
                onPressed: (mouse) => { enc.lastY = mouse.y }
                onPositionChanged: (mouse) => {
                    var steps = Math.trunc((enc.lastY - mouse.y) / 3)
                    if (steps !== 0) { enc.turn(steps); enc.lastY -= steps * 3 }
                }
                onWheel: (wheel) => {
                    enc.wheelTouch = true
                    wheelTimer.restart()
                    var steps = wheel.angleDelta.y > 0 ? 1 : (wheel.angleDelta.y < 0 ? -1 : 0)
                    if (wheel.modifiers & Qt.ControlModifier) steps *= 8
                    enc.turn(steps)
                }
            }
        }
    }

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

    // ---------------------------------------------------------------------------------------
    // Status bar and MIDI monitor

    Rectangle {
        id: statusBar
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: monitor.top
        height: 34
        color: "#1d1e22"
        Text {
            anchors.left: parent.left
            anchors.right: monitorButton.left
            anchors.verticalCenter: parent.verticalCenter
            anchors.leftMargin: 10
            text: sim.statusText
            color: "#a8a8b0"
            font.pixelSize: 12
            elide: Text.ElideRight
        }
        Button {
            id: monitorButton
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            anchors.rightMargin: 6
            text: sim.monitorVisible ? "Hide monitor (F2)" : "MIDI monitor (F2)"
            onClicked: sim.monitorVisible = !sim.monitorVisible
        }
    }

    Rectangle {
        id: monitor
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        height: window.monitorHeight
        visible: sim.monitorVisible
        color: "#101113"
        ListView {
            id: logView
            anchors.fill: parent
            anchors.margins: 4
            anchors.rightMargin: 90
            clip: true
            model: sim.logLines
            delegate: Text {
                required property string modelData
                text: modelData
                color: modelData.indexOf("<-") >= 0 ? "#8fd18f" : (modelData.indexOf("->") >= 0 ? "#d1c08f" : "#a0a0a8")
                font.family: "monospace"
                font.pixelSize: 11
            }
            onCountChanged: positionViewAtEnd()
        }
        Column {
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.margins: 4
            spacing: 4
            Button { text: "Clear"; width: 80; onClicked: sim.clearLog() }
            Button { text: "Pause"; width: 80; checkable: true; onToggled: sim.pauseLog(checked) }
        }
    }
}
