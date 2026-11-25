// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

// One help box (SimCommon, see HelpLayer): the label of the control and the short text (2 lines);
// expanded (item.expanded) the full text, larger and on top of the neighbours, growing from its
// place towards the panel and kept inside `bounds`.

import QtQuick

Rectangle {
    id: helpBox
    property var item: ({ "id": "", "label": "", "short": "", "long": "", "side": "left", "pressed": false, "expanded": false })
    property var box: ({ "x": 0, "y": 0, "w": 0, "h": 0 })
    property bool hovered: false
    // The colour of the scope
    property color accent: "#9aa0ab"
    property real fontSize: 11
    property rect bounds
    readonly property bool expanded: item.expanded === true
    signal clicked()
    signal hoverChanged(bool inside)

    readonly property real expandedWidth: Math.min(Math.max(box.w, 380), bounds.width)
    readonly property real expandedHeight: Math.min(content.implicitHeight + 10, bounds.height)

    width: expanded ? expandedWidth : box.w
    height: expanded ? expandedHeight : box.h
    x: {
        if (!expanded)
            return box.x
        var wanted = item.side === "right" ? box.x + box.w - width : (item.side === "left" ? box.x : box.x + box.w / 2 - width / 2)
        return Math.max(bounds.x, Math.min(wanted, bounds.x + bounds.width - width))
    }
    y: {
        if (!expanded)
            return box.y
        var wanted = item.side === "bottom" ? box.y + box.h - height : (item.side === "top" ? box.y : box.y + box.h / 2 - height / 2)
        return Math.max(bounds.y, Math.min(wanted, bounds.y + bounds.height - height))
    }
    z: expanded ? 10 : (hovered ? 5 : 1)
    radius: 4
    // A tint of the scope's colour on the dark background, its colour on the frame and the bar
    color: Qt.tint(expanded ? "#2c2f36" : "#222429", Qt.rgba(accent.r, accent.g, accent.b, 0.10))
    border.color: hovered || expanded ? "#ffffff" : (item.pressed ? "#ffffff" : Qt.rgba(accent.r, accent.g, accent.b, 0.55))
    border.width: hovered || expanded || item.pressed ? 2 : 1

    Rectangle {
        x: 1
        y: 1
        width: 4
        height: parent.height - 2
        radius: 2
        color: helpBox.accent
    }

    Column {
        id: content
        x: 9
        y: 4
        width: helpBox.width - 15
        spacing: 3
        Text {
            width: parent.width
            text: "<b>" + helpBox.item.label + "</b> · " + helpBox.item.short
            textFormat: Text.StyledText
            color: "#e8e8ee"
            font.pixelSize: helpBox.fontSize
            wrapMode: Text.Wrap
            maximumLineCount: helpBox.expanded ? 6 : 2
            elide: Text.ElideRight
            lineHeight: 0.95
        }
        Text {
            visible: helpBox.expanded && helpBox.item.long !== ""
            width: parent.width
            text: helpBox.item.long
            color: "#b8bcc6"
            font.pixelSize: helpBox.fontSize
            wrapMode: Text.Wrap
        }
    }

    MouseArea {
        anchors.fill: parent
        hoverEnabled: true
        onClicked: helpBox.clicked()
        onEntered: helpBox.hoverChanged(true)
        onExited: helpBox.hoverChanged(false)
    }
}
