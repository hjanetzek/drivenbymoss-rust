// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

// The full text of the expanded help box (SimCommon, see HelpLayer): one panel in a fixed place,
// centred over the pads (HelpLayer places it so that it doesn't hide the control it describes),
// with the control's name, the short text and the long text, in the colour of the scope. A click
// closes it.

import QtQuick

Rectangle {
    id: detail
    property var item: ({ "id": "", "label": "", "short": "", "long": "" })
    property color accent: "#9aa0ab"
    property real fontSize: 11
    signal clicked()

    height: content.implicitHeight + 16
    radius: 5
    color: Qt.tint("#26282e", Qt.rgba(accent.r, accent.g, accent.b, 0.12))
    opacity: 0.97
    border.color: "#ffffff"
    border.width: 2

    Rectangle {
        x: 2
        y: 2
        width: 5
        height: parent.height - 4
        radius: 2
        color: detail.accent
    }

    Column {
        id: content
        x: 14
        y: 8
        width: detail.width - 24
        spacing: 5
        Text {
            width: parent.width
            text: "<b>" + detail.item.label + "</b> · " + detail.item.short
            textFormat: Text.StyledText
            color: "#f0f0f4"
            font.pixelSize: detail.fontSize + 1
            wrapMode: Text.Wrap
        }
        Text {
            visible: detail.item.long !== ""
            width: parent.width
            text: detail.item.long
            color: "#c4c8d2"
            font.pixelSize: detail.fontSize
            wrapMode: Text.Wrap
            lineHeight: 1.1
        }
    }

    MouseArea {
        anchors.fill: parent
        onClicked: detail.clicked()
    }
}
