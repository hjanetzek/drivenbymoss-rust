// Copyright (c) 2026 Hannes Janetzek
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

// The window of a simulator (SimCommon): the surface with the controls of sim.controls (positions
// in surface units, scaled by k to the window), the status bar and the MIDI monitor (F2).
// componentMap maps the "kind" of a control to the Component which draws it; the component gets
// the control's dict in parent.control.

import QtQuick
import QtQuick.Controls

ApplicationWindow {
    id: window
    width: 1320
    height: 900
    visible: true
    color: "#16171a"
    title: sim.title

    property var componentMap: ({})
    property color surfaceColor: "#232428"
    property color surfaceBorder: "#3a3b40"
    property real surfaceRadius: 4
    readonly property real monitorHeight: sim.monitorVisible ? 220 : 0
    // The help boxes (key H, HelpLayer): a margin around the panel, the columns on the left and
    // right, two rows above and below
    readonly property bool helpShown: sim.helpAvailable && sim.helpVisible
    readonly property real helpSide: helpShown ? help.sideWidth + 2 * help.gap + 40 : 0
    readonly property real helpRows: helpShown ? 2 * (help.boxHeight + help.gap) + help.gap + 24 : 0
    readonly property real k: Math.min((width - 24 - 2 * helpSide) / sim.surfaceWidth,
                                       (height - monitorHeight - 56 - 2 * helpRows) / sim.surfaceHeight)
    // The surface, for items a simulator adds on it
    readonly property Item surface: surfaceItem

    Shortcut { sequence: "F2"; onActivated: sim.monitorVisible = !sim.monitorVisible }
    Shortcut { sequence: "H"; enabled: sim.helpAvailable; onActivated: sim.helpVisible = !sim.helpVisible }
    Shortcut { sequence: "G"; enabled: sim.helpAvailable; onActivated: sim.helpHideGlobal = !sim.helpHideGlobal }

    Rectangle {
        id: surfaceItem
        x: (window.width - width) / 2
        y: 10 + window.helpRows
        width: sim.surfaceWidth * window.k
        height: sim.surfaceHeight * window.k
        radius: window.surfaceRadius * window.k
        color: window.surfaceColor
        border.color: window.surfaceBorder
        border.width: Math.max(1, 0.4 * window.k)

        // The leader lines of the help boxes, under the controls
        HelpLines {
            visible: window.helpShown
            helpLayer: help
            surface: surfaceItem
        }

        Repeater {
            model: sim.controls
            delegate: Loader {
                required property var modelData
                x: modelData.x * window.k
                y: modelData.y * window.k
                width: modelData.w * window.k
                height: modelData.h * window.k
                sourceComponent: window.componentMap[modelData.kind] || null
                property var control: modelData
            }
        }
    }

    HelpLayer {
        id: help
        visible: window.helpShown
        x: 0
        y: 0
        width: window.width
        height: statusBar.y
        surface: surfaceItem
        k: window.k
    }

    StatusBar {
        id: statusBar
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: monitor.top
    }

    MidiMonitor {
        id: monitor
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        height: window.monitorHeight
    }
}
