// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

// The status bar (SimCommon): sim.statusText and the button of the MIDI monitor.

import QtQuick
import QtQuick.Controls

Rectangle {
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
