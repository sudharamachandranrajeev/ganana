import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui

// Gaṇanā bar widget: a ring with the fullest AI plan window, and a panel with
// every window, its reset time, the pace and today's cost. The data comes
// from `ganana bar --format json`, which reads the app's database.
Panel {
  id: root
  moduleName: "ganana.usage"
  ipcTarget: "ganana.usage"

  readonly property string command: setting("command", "ganana")
  readonly property string labelMode: setting("label", "percent")
  readonly property color fg: bar ? bar.foreground : Color.foreground
  readonly property color dim: Qt.darker(fg, 1.4)

  // Last summary from `ganana bar`; null until the first read.
  property var data: null
  property string error: ""

  readonly property string level: data ? data.level : "idle"
  readonly property real fraction: data && data.percentage !== null ? data.percentage / 100 : 0

  function levelColor(level, base) {
    if (level === "critical") return Color.urgent
    if (level === "warning") return Qt.tint(base, Util.alpha(Color.urgent, 0.6))
    return base
  }

  function money(usd) {
    return "$" + Number(usd || 0).toFixed(2)
  }

  function refresh() {
    if (!barProc.running) barProc.running = true
  }

  function openApp() {
    Quickshell.execDetached([root.command])
    close()
  }

  function apply(text) {
    try {
      data = JSON.parse(text)
      error = ""
    } catch (e) {
      error = text.trim() || "Gaṇanā did not answer"
    }
  }

  onOpenedChanged: if (opened) refresh()

  implicitWidth: button.implicitWidth
  implicitHeight: button.implicitHeight

  Component.onCompleted: refresh()

  Process {
    id: barProc
    command: [root.command, "bar", "--format", "json"]
    stdout: StdioCollector { waitForEnd: true; onStreamFinished: root.apply(text) }
    onExited: function(exitCode) {
      if (exitCode !== 0 && root.error === "") root.error = "Could not run " + root.command
    }
  }

  Timer {
    interval: root.opened ? 5000 : 30000
    running: true
    repeat: true
    onTriggered: root.refresh()
  }

  // ---------- Bar button ----------

  Row {
    id: button
    spacing: 0

    readonly property string label: {
      if (!root.data || root.labelMode === "none") return ""
      if (root.labelMode === "cost") return root.money(root.data.today_cost)
      return root.data.percentage !== null ? root.data.percentage + "%" : ""
    }
    readonly property string tooltip: root.error !== "" ? root.error : (root.data ? root.data.tooltip : "Gaṇanā")

    function press(b) {
      if (b === Qt.MiddleButton) root.openApp()
      else if (b === Qt.RightButton) root.refresh()
      else root.toggle()
    }

    BarIconButton {
      id: ringButton
      bar: root.bar
      tooltipText: button.tooltip
      slotSize: button.label !== "" ? Style.bar.iconCanvas + Style.space(10) : Style.bar.iconSlot
      onPressed: function(b) { button.press(b) }

      iconComponent: Item {
        UsageRing {
          anchors.fill: parent
          anchors.margins: 1
          lineWidth: Math.max(2, Style.space(2.2))
          fraction: root.fraction
          color: root.levelColor(root.level, root.fg)
          trackColor: Util.alpha(root.bar ? root.bar.barForeground : Color.foreground, 0.22)
        }
      }
    }

    WidgetButton {
      bar: root.bar
      visible: text !== "" && !ringButton.vertical
      text: button.label
      tooltipText: button.tooltip
      horizontalMargin: 2
      fontSize: Style.font.bodySmall
      onPressed: function(b) { button.press(b) }
    }

    Item {
      visible: button.label !== "" && !ringButton.vertical
      width: Style.space(6)
      height: 1
    }
  }

  // ---------- Panel ----------

  KeyboardPanel {
    id: panel
    anchorItem: button
    owner: root
    bar: root.bar
    open: root.opened
    focusTarget: keyCatcher
    contentWidth: panel.fittedContentWidth(Style.space(400))
    contentHeight: panel.fittedContentHeight(content.implicitHeight)

    PanelKeyCatcher {
      id: keyCatcher
      anchors.fill: parent
      onActivateRequested: root.openApp()
      onCloseRequested: root.close()
      onTabRequested: function(direction) { root.switchPanel(direction) }
      onTextKey: function(t) { if (t === "r") root.refresh() }

      UsageContent {
        id: content
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        panel: root
      }
    }
  }
}
