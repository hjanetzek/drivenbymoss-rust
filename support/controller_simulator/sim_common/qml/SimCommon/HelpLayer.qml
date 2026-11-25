// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

// The help boxes (SimCommon, doc/CONTROLLER_HELP_PLAN.md): one box per control with help
// (sim.helpItems), on the side of the panel the control is nearest to, with a leader line to it.
// Along a side the boxes keep the order of their controls and are pushed apart so that they don't
// overlap (the lines don't cross). Left and right: a column each; top and bottom: two staggered
// rows. A click on a box (or pressing its control, the host reports it) shows the full text, on
// top of the neighbours; hovering a box highlights its control and line. The boxes and lines
// are coloured by the scope of their text (global, view, mode, modifier); a row of buttons is
// one box whose line starts at a bracket along the row. The lines are drawn by HelpLines, under
// the controls.
//
// The layer fills the window (its id is not `layer`: inside an Item that names the item's own
// `layer` group property); `surface` is the panel item, `k` its scale (pixels per surface unit).

import QtQuick

Item {
    id: helpLayer
    property Item surface
    property real k: 1
    // The geometry the window reserves for the boxes (see SimWindow)
    property real sideWidth: 230
    property real boxHeight: 34
    property real gap: 6
    property real fontSize: 11
    property var items: sim.helpItems
    property string hoveredId: ""
    // The collapsed rectangles of the boxes, by the index in items
    property var boxes: computeLayout(items, width, height, surface ? surface.x : 0, surface ? surface.y : 0, k)

    function surfaceX(x) { return surface.x + x * k }
    function surfaceY(y) { return surface.y + y * k }

    // As sim_common/help.py spread(): the least squares fit of the starts to the wishes, at
    // least `spacing` apart, the order kept, within [low, high] if they fit
    function spread(desired, size, spacing, low, high) {
        var step = size + spacing
        var count = desired.length
        var blocks = []
        for (var i = 0; i < count; i++) {
            blocks.push([desired[i] - i * step, 1])
            while (blocks.length > 1 && blocks[blocks.length - 2][0] / blocks[blocks.length - 2][1] > blocks[blocks.length - 1][0] / blocks[blocks.length - 1][1]) {
                var last = blocks.pop()
                blocks[blocks.length - 1][0] += last[0]
                blocks[blocks.length - 1][1] += last[1]
            }
        }
        var positions = []
        for (var b = 0; b < blocks.length; b++)
            for (var n = 0; n < blocks[b][1]; n++)
                positions.push(blocks[b][0] / blocks[b][1] + positions.length * step)
        if (count * step - spacing > high - low)
            return positions.map(function (p, index) { return low + index * step })
        for (var f = 0; f < count; f++)
            positions[f] = Math.max(positions[f], low + f * step)
        for (var r = 0; r < count; r++)
            positions[r] = Math.min(positions[r], high - (count - r) * step + spacing)
        return positions
    }

    function computeLayout(items, w, h, sx, sy, scale) {
        var result = new Array(items.length)
        if (!surface || items.length === 0)
            return result
        var sides = { "left": [], "right": [], "top": [], "bottom": [] }
        for (var i = 0; i < items.length; i++)
            sides[items[i].side].push(i)
        // Columns: the full height of the window
        var columns = ["left", "right"]
        for (var c = 0; c < 2; c++) {
            var column = sides[columns[c]]
            var wishes = column.map(function (index) { return sy + items[index].ay * scale - boxHeight / 2 })
            var ys = spread(wishes, boxHeight, gap / 2, gap, h - gap)
            var x = columns[c] === "left" ? gap : w - gap - sideWidth
            for (var n = 0; n < column.length; n++)
                result[column[n]] = { "x": x, "y": ys[n], "w": sideWidth, "h": boxHeight }
        }
        // Rows: between the columns, two staggered rows (lane 0 next to the panel). All boxes of
        // a side are spread together at half a box apart, then alternate between the lanes: an
        // outer box's middle is above the gap between its inner neighbours, its line goes down
        // through that gap (`elbow`) and then to the control.
        var left = sideWidth + 2 * gap
        var right = w - sideWidth - 2 * gap
        var rows = ["top", "bottom"]
        for (var r = 0; r < 2; r++) {
            var row = sides[rows[r]]
            if (row.length === 0)
                continue
            var boxWidth = Math.min(sideWidth, (right - left - (row.length - 1) * gap) / ((row.length + 1) / 2))
            var half = boxWidth / 2
            var xWishes = row.map(function (index) { return sx + items[index].ax * scale - boxWidth / 2 })
            var xs = spread(xWishes, half, gap, left, right - half)
            var innerY = rows[r] === "top" ? sy - gap - boxHeight : sy + surface.height + gap
            var outerY = rows[r] === "top" ? innerY - boxHeight - gap : innerY + boxHeight + gap
            for (var m = 0; m < row.length; m++) {
                var outer = m % 2 === 1
                var elbow = outer ? (rows[r] === "top" ? innerY + boxHeight + gap / 2 : innerY - gap / 2) : -1
                result[row[m]] = { "x": xs[m], "y": outer ? outerY : innerY, "w": boxWidth, "h": boxHeight, "elbow": elbow }
            }
        }
        return result
    }

    // The end of the line at the box: the middle of the edge facing the panel
    function boxPort(item, box) {
        if (item.side === "left")
            return [box.x + box.w, box.y + box.h / 2]
        if (item.side === "right")
            return [box.x, box.y + box.h / 2]
        if (item.side === "top")
            return [box.x + box.w / 2, box.y + box.h]
        return [box.x + box.w / 2, box.y]
    }

    // The colours of the scopes (Okabe-Ito hues, told apart also with a colour vision
    // deficiency): what a text depends on
    readonly property var scopeColors: ({ "global": "#9aa0ab", "view": "#56b4e9", "mode": "#e69f00", "modifier": "#cc79a7" })
    readonly property color hotColor: "#ffffff"
    function scopeColor(scope) { return scopeColors[scope] || scopeColors["global"] }

    // Repaints the leader lines (HelpLines, drawn under the controls by SimWindow)
    signal linesChanged()
    onBoxesChanged: linesChanged()
    onHoveredIdChanged: linesChanged()

    // The highlight of the control(s) of the hovered (or expanded) box
    Repeater {
        model: helpLayer.items
        delegate: Item {
            id: highlight
            required property var modelData
            readonly property bool hot: modelData.id === helpLayer.hoveredId || modelData.expanded
            visible: hot
            Repeater {
                model: highlight.modelData.parts.length > 0 ? highlight.modelData.parts : [highlight.modelData]
                delegate: Rectangle {
                    required property var modelData
                    x: helpLayer.surfaceX(modelData.x) - 2
                    y: helpLayer.surfaceY(modelData.y) - 2
                    width: modelData.w * helpLayer.k + 4
                    height: modelData.h * helpLayer.k + 4
                    color: "transparent"
                    border.color: helpLayer.hotColor
                    border.width: 2
                    radius: 3
                }
            }
        }
    }

    // Hover areas over the controls (they don't take the clicks)
    Repeater {
        model: helpLayer.items
        delegate: Item {
            required property var modelData
            x: helpLayer.surfaceX(modelData.x)
            y: helpLayer.surfaceY(modelData.y)
            width: modelData.w * helpLayer.k
            height: modelData.h * helpLayer.k
            HoverHandler {
                onHoveredChanged: {
                    if (hovered)
                        helpLayer.hoveredId = modelData.id
                    else if (helpLayer.hoveredId === modelData.id)
                        helpLayer.hoveredId = ""
                }
            }
        }
    }

    Repeater {
        model: helpLayer.items
        delegate: HelpBox {
            required property var modelData
            required property int index
            item: modelData
            accent: helpLayer.scopeColor(modelData.scope)
            box: helpLayer.boxes[index] || { "x": 0, "y": 0, "w": 0, "h": 0 }
            hovered: modelData.id === helpLayer.hoveredId
            fontSize: helpLayer.fontSize
            bounds: Qt.rect(helpLayer.gap, helpLayer.gap, helpLayer.width - 2 * helpLayer.gap, helpLayer.height - 2 * helpLayer.gap)
            onHoverChanged: (inside) => {
                if (inside)
                    helpLayer.hoveredId = modelData.id
                else if (helpLayer.hoveredId === modelData.id)
                    helpLayer.hoveredId = ""
            }
            onClicked: sim.helpToggleExpanded(modelData.id)
        }
    }

    Text {
        visible: helpLayer.items.length === 0
        anchors.horizontalCenter: parent.horizontalCenter
        y: helpLayer.gap
        text: sim.helpStatus
        color: "#a8a8b0"
        font.pixelSize: 12
    }
}
