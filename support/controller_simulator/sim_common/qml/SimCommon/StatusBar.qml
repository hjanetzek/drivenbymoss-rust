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
        anchors.right: legend.visible ? legend.left : helpBox.left
        anchors.verticalCenter: parent.verticalCenter
        anchors.leftMargin: 10
        text: sim.statusText
        color: "#a8a8b0"
        font.pixelSize: 12
        elide: Text.ElideRight
    }
    // The legend of the help boxes' colours, the names of the active view and mode
    Row {
        id: legend
        visible: sim.helpAvailable && sim.helpVisible
        anchors.right: globalBox.left
        anchors.verticalCenter: parent.verticalCenter
        anchors.rightMargin: 10
        spacing: 12
        Repeater {
            model: [
                { "scope": "view", "text": "View: " + (sim.helpViewName || "-") },
                { "scope": "mode", "text": "Mode: " + (sim.helpModeName || "-") },
                { "scope": "modifier", "text": "Shift / Select / held" },
                { "scope": "global", "text": "Global" }
            ]
            delegate: Row {
                required property var modelData
                spacing: 5
                Rectangle {
                    width: 12
                    height: 12
                    radius: 2
                    anchors.verticalCenter: parent.verticalCenter
                    color: ({ "global": "#9aa0ab", "view": "#56b4e9", "mode": "#e69f00", "modifier": "#cc79a7" })[modelData.scope]
                }
                Text {
                    text: modelData.text
                    color: "#d0d0d6"
                    font.pixelSize: 12
                    anchors.verticalCenter: parent.verticalCenter
                }
            }
        }
    }
    CheckBox {
        id: globalBox
        visible: sim.helpAvailable && sim.helpVisible
        anchors.right: helpBox.left
        anchors.verticalCenter: parent.verticalCenter
        text: "Globals (G)"
        palette.windowText: "#c8c8cc"
        checked: !sim.helpHideGlobal
        onToggled: sim.helpHideGlobal = !checked
    }
    CheckBox {
        id: helpBox
        visible: sim.helpAvailable
        anchors.right: monitorButton.left
        anchors.verticalCenter: parent.verticalCenter
        anchors.rightMargin: 6
        text: "Help (H)"
        palette.windowText: "#c8c8cc"
        checked: sim.helpVisible
        onToggled: sim.helpVisible = checked
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
