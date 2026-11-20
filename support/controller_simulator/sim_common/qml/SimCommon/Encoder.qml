// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

// An endless encoder (SimCommon): wheel = relative steps (Ctrl: x8), vertical drag = steps;
// press, drag and wheel touch it (sim.encoderTouch(c.note, ...); wheel touches end 500 ms after
// the last step). With the value side channel (sim.encoderState(c.note), the DBMV messages): an
// LED ring with the value, the name below and, while touched or hovered, the value text.

import QtQuick

Item {
    id: enc
    // The scale (pixels per surface unit) of the window (SimWindow.k)
    readonly property real k: Window.window && Window.window.k ? Window.window.k : 1
    property var c: parent.control
    property var st: { sim.encoderRevision; return sim.encoderState(c.note) }
    property real stepAngle: 0
    property real lastY: 0
    property bool wheelTouch: false
    property bool touched: earea.pressed || wheelTouch
    // Touched here or reported as touched by the host
    property bool lit: touched || st.touched
    readonly property real sweep: 300
    readonly property real valueAngle: -sweep / 2 + sweep * st.value
    property real angle: st.hasValue ? valueAngle : stepAngle
    onTouchedChanged: sim.encoderTouch(c.note, touched)
    onStChanged: ring.requestPaint()
    onLitChanged: ring.requestPaint()

    function turn(steps) {
        if (steps === 0) return
        sim.encoderTurn(c.cc, steps)
        stepAngle += steps * 6
    }

    // The cap, smaller than the control so that the ring fits around it
    Rectangle {
        id: cap
        anchors.centerIn: parent
        width: parent.width * 0.76
        height: width
        radius: width / 2
        color: enc.lit ? "#4b4d55" : "#33343a"
        border.color: enc.lit && enc.st.hasValue ? "#7fdcff" : "#5c5d66"
        border.width: Math.max(1, 0.35 * k)
        // The indicator: from the center to the outline, turning around the center
        Rectangle {
            width: Math.max(2, 0.7 * k)
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

    // The LED ring: many fine segments, a dim track and the lit part with a glow
    Canvas {
        id: ring
        // st.ring false: a knob without a ring (the value only turns the indicator)
        visible: enc.st.hasValue && enc.st.ring !== false
        anchors.centerIn: parent
        width: parent.width + 2 * k
        height: width
        onWidthChanged: requestPaint()
        onPaint: {
            var ctx = getContext("2d")
            ctx.reset()
            if (!enc.st.hasValue || enc.st.ring === false) return
            var cx = width / 2, cy = height / 2
            var radius = enc.width * 0.44
            var lineWidth = Math.max(1.5, enc.width * 0.075)
            var segments = 90
            var start = -enc.sweep / 2
            var step = enc.sweep / segments
            var gap = step * 0.28
            function rad(deg) { return (deg - 90) * Math.PI / 180 }
            function arc(from, to) {
                ctx.beginPath()
                ctx.arc(cx, cy, radius, rad(from), rad(to), false)
                ctx.stroke()
            }
            ctx.lineCap = "butt"
            ctx.lineWidth = lineWidth
            // The dim track
            ctx.strokeStyle = "#33444d"
            for (var i = 0; i < segments; i++)
                arc(start + i * step + gap / 2, start + (i + 1) * step - gap / 2)
            // The lit part: from the start (bipolar: the top) to the value, the last
            // segment cut at the value
            var value = start + enc.sweep * enc.st.value
            var from = enc.st.bipolar ? Math.min(0, value) : start
            var to = enc.st.bipolar ? Math.max(0, value) : value
            // A ring in "single" mode (st.dot, e.g. the APC40's LED rings): a short arc at the value
            if (enc.st.dot) {
                from = Math.max(start, value - 6)
                to = Math.min(start + enc.sweep, value + 6)
            }
            if (to - from < 0.5) {
                // At least a thin mark at the value (e.g. 0 or the centre)
                from = value - 1.5
                to = value + 1.5
            }
            ctx.strokeStyle = enc.lit ? "#d8f6ff" : "#45c8f5"
            ctx.shadowColor = enc.lit ? "#a0e8ff" : "#2a9fd0"
            ctx.shadowBlur = (enc.lit ? 1.6 : 0.9) * k
            for (i = 0; i < segments; i++) {
                var a = Math.max(from, start + i * step + gap / 2)
                var b = Math.min(to, start + (i + 1) * step - gap / 2)
                if (b > a)
                    arc(a, b)
            }
        }
    }

    Text {
        anchors.top: parent.bottom
        anchors.horizontalCenter: parent.horizontalCenter
        width: parent.width * 1.3
        horizontalAlignment: Text.AlignHCenter
        elide: Text.ElideRight
        property bool showValue: (enc.lit || earea.containsMouse) && enc.st.text.length > 0
        text: showValue ? enc.st.text : (enc.st.name.length > 0 ? enc.st.name : (enc.c.label.length > 1 ? enc.c.label : ""))
        color: showValue ? "#d8f6ff" : (enc.lit ? "#ffffff" : "#9a9aa2")
        font.pixelSize: Math.max(7, 1.8 * k)
        font.bold: enc.lit
    }
    Timer { id: wheelTimer; interval: 500; onTriggered: enc.wheelTouch = false }
    MouseArea {
        id: earea
        anchors.fill: parent
        hoverEnabled: true
        onPressed: (mouse) => { enc.lastY = mouse.y }
        onPositionChanged: (mouse) => {
            if (!pressed) return
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
