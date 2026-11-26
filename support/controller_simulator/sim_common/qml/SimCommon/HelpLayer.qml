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
    // The distance of the columns and rows from the panel: a few millimetres, the free space of
    // the window stays outside the boxes, so the leader lines are short
    property real panelGap: Math.max(10, 3 * k)
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

    // As sim_common/help.py segments_cross(), lines_cross() and uncross(): neighbours on a side
    // whose leader lines (`line(slot, box)`: the polyline if the box took the slot) cross change
    // places
    function segmentsCross(a, b, c, d) {
        function side(p, q, r) { return (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0]) }
        return side(c, d, a) * side(c, d, b) < 0 && side(a, b, c) * side(a, b, d) < 0
    }

    function linesCross(first, second) {
        for (var i = 0; i < first.length - 1; i++)
            for (var j = 0; j < second.length - 1; j++)
                if (segmentsCross(first[i], first[i + 1], second[j], second[j + 1]))
                    return true
        return false
    }

    function uncross(count, line) {
        var order = []
        for (var n = 0; n < count; n++)
            order.push(n)
        function crossing(i) { return linesCross(line(i, order[i]), line(i + 1, order[i + 1])) }
        for (var pass = 0; pass < count; pass++) {
            var changed = false
            for (var i = 0; i < count - 1; i++) {
                if (!crossing(i))
                    continue
                var swap = order[i]; order[i] = order[i + 1]; order[i + 1] = swap
                if (crossing(i)) {
                    swap = order[i]; order[i] = order[i + 1]; order[i + 1] = swap
                } else {
                    changed = true
                }
            }
            if (!changed)
                break
        }
        return order
    }

    // The leader line of a box as HelpLines draws it: from the box's port (and its elbow) to the
    // control's centre; to an area along the panel's edge to the anchor and straight in
    function leader(item, port, elbow, sx, sy, scale) {
        var points = [port]
        if (elbow >= 0)
            points.push([port[0], elbow])
        var ax = sx + item.ax * scale, ay = sy + item.ay * scale
        if (!item.area) {
            points.push([ax, ay])
        } else if (item.side === "top" || item.side === "bottom") {
            var edge = item.side === "top" ? sy - 3 : sy + surface.height + 3
            var by = item.side === "top" ? sy + item.y * scale - 4 : sy + (item.y + item.h) * scale + 4
            points.push([port[0], edge], [ax, edge], [ax, by])
        } else {
            var edgeX = item.side === "left" ? sx - 3 : sx + surface.width + 3
            var bx = item.side === "left" ? sx + item.x * scale - 4 : sx + (item.x + item.w) * scale + 4
            points.push([edgeX, port[1]], [edgeX, ay], [bx, ay])
        }
        return points
    }

    function computeLayout(items, w, h, sx, sy, scale) {
        var result = new Array(items.length)
        if (!surface || items.length === 0)
            return result
        var sides = { "left": [], "right": [], "top": [], "bottom": [] }
        for (var i = 0; i < items.length; i++)
            sides[items[i].side].push(i)
        // Columns: tight to the panel, the full height of the window
        var columns = ["left", "right"]
        var columnX = {}
        for (var c = 0; c < 2; c++) {
            var column = sides[columns[c]]
            var wishes = column.map(function (index) { return sy + items[index].ay * scale - boxHeight / 2 })
            var ys = spread(wishes, boxHeight, gap / 2, gap, h - gap)
            var x = columns[c] === "left" ? Math.max(gap, sx - panelGap - sideWidth) : Math.min(w - gap - sideWidth, sx + surface.width + panelGap)
            columnX[columns[c]] = x
            var portX = columns[c] === "left" ? x + sideWidth : x
            var order = uncross(column.length, function (slot, box) {
                return leader(items[column[box]], [portX, ys[slot] + boxHeight / 2], -1, sx, sy, scale)
            })
            for (var n = 0; n < column.length; n++)
                result[column[order[n]]] = { "x": x, "y": ys[n], "w": sideWidth, "h": boxHeight }
        }
        // A corner is free for the rows when the column's boxes stay out of the rows' height
        function columnFree(side, top, bottom) {
            return sides[side].every(function (index) { return result[index].y + boxHeight + gap <= top || result[index].y - gap >= bottom })
        }
        // Rows: between the columns, two staggered rows (lane 0 next to the panel). All boxes of
        // a side are spread together at half a box apart, then alternate between the lanes: an
        // outer box's middle is above the gap between its inner neighbours, its line goes down
        // through that gap (`elbow`) and then to the control.
        var rows = ["top", "bottom"]
        for (var r = 0; r < 2; r++) {
            var row = sides[rows[r]]
            if (row.length === 0)
                continue
            var rowsTop = rows[r] === "top" ? sy - panelGap - 2 * boxHeight - gap : sy + surface.height + panelGap
            var rowsBottom = rowsTop + 2 * boxHeight + gap
            // Between the columns; a free corner up to the column's outer edge
            var left = columnFree("left", rowsTop, rowsBottom) ? columnX["left"] : columnX["left"] + sideWidth + gap
            var right = columnFree("right", rowsTop, rowsBottom) ? columnX["right"] + sideWidth : columnX["right"] - gap
            var boxWidth = Math.min(sideWidth, (right - left - (row.length - 1) * gap) / ((row.length + 1) / 2))
            var half = boxWidth / 2
            var xWishes = row.map(function (index) { return sx + items[index].ax * scale - boxWidth / 2 })
            var xs = spread(xWishes, half, gap, left, right - half)
            var innerY = rows[r] === "top" ? sy - panelGap - boxHeight : sy + surface.height + panelGap
            var outerY = rows[r] === "top" ? innerY - boxHeight - gap : innerY + boxHeight + gap
            var slots = xs.map(function (bx, m) {
                var outer = m % 2 === 1
                var y = outer ? outerY : innerY
                return { "x": bx, "y": y, "elbow": outer ? (rows[r] === "top" ? innerY + boxHeight + gap / 2 : innerY - gap / 2) : -1,
                         "portY": rows[r] === "top" ? y + boxHeight : y }
            })
            var rowOrder = uncross(row.length, function (slot, box) {
                var s = slots[slot]
                return leader(items[row[box]], [s.x + boxWidth / 2, s.portY], s.elbow, sx, sy, scale)
            })
            for (var m = 0; m < row.length; m++) {
                var slot = slots[m]
                result[row[rowOrder[m]]] = { "x": slot.x, "y": slot.y, "w": boxWidth, "h": boxHeight, "elbow": slot.elbow }
            }
        }
        return result
    }

    // The expanded entry (its full text in HelpDetail), -1 for none
    readonly property int expandedIndex: {
        for (var i = 0; i < items.length; i++)
            if (items[i].expanded)
                return i
        return -1
    }

    // The rectangle of HelpDetail: centred over the central area (the pads), at its top edge; at
    // its bottom edge if the described control is in the top part of the area, above the area if
    // the control is the area itself (the pads' box). Within the window.
    readonly property real detailWidth: {
        var c = sim.helpCentre
        if (!surface || c[2] <= c[0])
            return Math.min(480, width - 2 * gap)
        return Math.min(Math.max((c[2] - c[0]) * k - 2 * panelGap, 380), 620, width - 2 * gap)
    }

    function detailRect(item, height) {
        var c = sim.helpCentre
        var w = detailWidth
        if (!surface || !item || c[2] <= c[0])
            return Qt.rect(gap, gap, w, height)
        var x0 = surfaceX(c[0]), y0 = surfaceY(c[1]), x1 = surfaceX(c[2]), y1 = surfaceY(c[3])
        var x = Math.max(gap, Math.min((x0 + x1) / 2 - w / 2, width - gap - w))
        var margin = panelGap / 2
        // The control's rectangle(s)
        var parts = item.parts.length > 0 ? item.parts : [item]
        function hides(top) {
            return parts.some(function (p) {
                var px0 = surfaceX(p.x), py0 = surfaceY(p.y), px1 = surfaceX(p.x + p.w), py1 = surfaceY(p.y + p.h)
                return px0 < x + w && px1 > x && py0 < top + height && py1 > top
            })
        }
        var coversArea = item.area && item.w * item.h >= 0.8 * (c[2] - c[0]) * (c[3] - c[1])
        var top = y0 + margin
        if (coversArea)
            top = y0 - margin - height
        else if (hides(top))
            top = y1 - margin - height
        return Qt.rect(x, Math.max(gap, Math.min(top, height > 0 ? helpLayer.height - gap - height : top)), w, height)
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
            onHoverChanged: (inside) => {
                if (inside)
                    helpLayer.hoveredId = modelData.id
                else if (helpLayer.hoveredId === modelData.id)
                    helpLayer.hoveredId = ""
            }
            onClicked: sim.helpToggleExpanded(modelData.id)
        }
    }

    // The full text of the expanded box, over the pads; a click (or another click on its box)
    // closes it
    HelpDetail {
        id: detail
        readonly property var entry: helpLayer.expandedIndex >= 0 ? helpLayer.items[helpLayer.expandedIndex] : null
        readonly property rect place: helpLayer.detailRect(entry, height)
        visible: entry !== null
        item: entry || ({ "id": "", "label": "", "short": "", "long": "" })
        accent: helpLayer.scopeColor(entry ? entry.scope : "global")
        fontSize: helpLayer.fontSize
        x: place.x
        y: place.y
        width: helpLayer.detailWidth
        z: 20
        onClicked: if (entry) sim.helpToggleExpanded(entry.id)
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
