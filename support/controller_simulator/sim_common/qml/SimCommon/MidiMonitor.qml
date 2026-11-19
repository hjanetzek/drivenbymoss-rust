// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

// The MIDI monitor (SimCommon): sim.logLines, received messages green, sent ones yellow.

import QtQuick
import QtQuick.Controls

Rectangle {
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
