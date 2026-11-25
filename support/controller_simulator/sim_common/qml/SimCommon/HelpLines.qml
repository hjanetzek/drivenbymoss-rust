// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

// The leader lines of the help boxes (SimCommon, see HelpLayer). SimWindow puts it into the panel
// as its first child, so the lines run under the controls (a line which passes a button is hidden
// by it instead of crossing its label); it covers the window (in the window's coordinates). A row
// of buttons and an area (the pads, the strips) get a bracket along their edge facing the box,
// the line starts at its middle. Colours: the scope's; hovered, pressed or expanded: white.

import QtQuick

Canvas {
    id: lines
    property Item helpLayer
    property Item surface
    x: surface ? -surface.x : 0
    y: surface ? -surface.y : 0
    width: helpLayer ? helpLayer.width : 0
    height: helpLayer ? helpLayer.height : 0

    Connections {
        target: lines.helpLayer
        function onLinesChanged() { lines.requestPaint() }
    }
    onWidthChanged: requestPaint()
    onHeightChanged: requestPaint()

    onPaint: {
        var ctx = getContext("2d")
        ctx.reset()
        var layer = helpLayer
        if (!layer || !layer.visible)
            return
        for (var pass = 0; pass < 2; pass++) {
            for (var i = 0; i < layer.items.length; i++) {
                var item = layer.items[i]
                var box = layer.boxes[i]
                if (!box)
                    continue
                var hot = item.id === layer.hoveredId || item.pressed || item.expanded
                if ((pass === 1) !== hot)
                    continue
                var port = layer.boxPort(item, box)
                var ax = layer.surfaceX(item.ax)
                var ay = layer.surfaceY(item.ay)
                ctx.strokeStyle = hot ? layer.hotColor : layer.scopeColor(item.scope)
                ctx.globalAlpha = hot ? 1.0 : 0.75
                ctx.lineWidth = hot ? 2 : 1.2
                ctx.beginPath()
                ctx.moveTo(port[0], port[1])
                if (box.elbow !== undefined && box.elbow >= 0)
                    ctx.lineTo(port[0], box.elbow)
                if (item.area) {
                    // A bracket along the edge facing the box, a little outside; the line goes
                    // to the panel's edge, along it to the anchor (a gap between the controls)
                    // and straight in
                    var x0 = layer.surfaceX(item.x), y0 = layer.surfaceY(item.y)
                    var x1 = layer.surfaceX(item.x + item.w), y1 = layer.surfaceY(item.y + item.h)
                    var d = 4, t = 4
                    var s = layer.surface
                    if (item.side === "top" || item.side === "bottom") {
                        var by = item.side === "top" ? y0 - d : y1 + d
                        var tick = item.side === "top" ? t : -t
                        var edge = item.side === "top" ? s.y - 3 : s.y + s.height + 3
                        ctx.lineTo(port[0], edge)
                        ctx.lineTo(ax, edge)
                        ctx.lineTo(ax, by)
                        ctx.moveTo(x0, by + tick)
                        ctx.lineTo(x0, by)
                        ctx.lineTo(x1, by)
                        ctx.lineTo(x1, by + tick)
                    } else {
                        var bx = item.side === "left" ? x0 - d : x1 + d
                        var tickx = item.side === "left" ? t : -t
                        var edgex = item.side === "left" ? s.x - 3 : s.x + s.width + 3
                        ctx.lineTo(edgex, port[1])
                        ctx.lineTo(edgex, ay)
                        ctx.lineTo(bx, ay)
                        ctx.moveTo(bx + tickx, y0)
                        ctx.lineTo(bx, y0)
                        ctx.lineTo(bx, y1)
                        ctx.lineTo(bx + tickx, y1)
                    }
                    ctx.stroke()
                    continue
                }
                ctx.lineTo(ax, ay)
                ctx.stroke()
            }
        }
    }
}
