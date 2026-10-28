import QtQuick
import qs.Commons
import qs.Ui

// Body of the Gaṇanā panel: the fullest window as a hero ring, then each
// window with a meter and a plain sentence, today's cost and low credit.
Column {
  id: root

  // The Gaṇanā Panel that owns the data.
  required property var panel

  readonly property var bar: root.panel.bar
  readonly property var data: root.panel.data
  readonly property color fg: root.panel.fg
  readonly property color dim: root.panel.dim
  readonly property string fontFamily: root.bar ? root.bar.fontFamily : Style.font.family
  spacing: Style.space(12)

  PanelHero {
    foreground: root.fg
    fontFamily: root.fontFamily
    title: "AI usage"
    meta: root.panel.error !== "" ? root.panel.error
      : !root.data ? "Reading…"
      : root.data.windows.length ? root.data.windows[0].name + " is the fullest"
      : "No plan limits seen recently"
    detail: root.data ? root.panel.money(root.data.today_cost) + " of use today at API prices" : ""
    iconComponent: Item {
      implicitWidth: Style.space(58)
      implicitHeight: Style.space(58)

      UsageRing {
        anchors.fill: parent
        lineWidth: Style.space(5)
        fraction: root.panel.fraction
        color: root.panel.levelColor(root.panel.level, Color.accent)
        trackColor: Util.alpha(root.fg, 0.1)
      }

      Text {
        anchors.centerIn: parent
        textFormat: Text.PlainText
        text: root.data && root.data.percentage !== null ? root.data.percentage + "%" : "—"
        color: root.fg
        font.family: root.fontFamily
        font.pixelSize: Style.font.subtitle
        font.bold: true
      }
    }
    trailingControl: PanelActionButton {
      iconText: "󰏌"
      tooltipText: "Open Gaṇanā"
      foreground: root.fg
      fontFamily: root.fontFamily
      onClicked: root.panel.openApp()
    }
  }

  PanelSeparator {
    foreground: root.fg
    visible: windowList.count > 0
  }

  Column {
    width: parent.width
    spacing: Style.space(10)
    visible: windowList.count > 0

    PanelSectionHeader {
      text: "LIMITS"
      foreground: root.fg
      fontFamily: root.fontFamily
    }

    Repeater {
      id: windowList
      model: root.data ? root.data.windows : []

      Column {
        required property var modelData
        width: parent.width
        spacing: Style.space(4)

        Item {
          width: parent.width
          height: name.implicitHeight

          Text {
            id: name
            textFormat: Text.PlainText
            text: modelData.name
            color: root.fg
            font.family: root.fontFamily
            font.pixelSize: Style.font.body
          }

          Text {
            anchors.right: parent.right
            textFormat: Text.PlainText
            text: Math.round(modelData.used_pct) + "%"
            color: root.fg
            font.family: root.fontFamily
            font.pixelSize: Style.font.body
            font.bold: true
          }
        }

        Rectangle {
          width: parent.width
          height: Style.space(5)
          radius: height / 2
          color: Util.alpha(root.fg, 0.1)

          Rectangle {
            width: parent.width * Math.max(0, Math.min(1, modelData.used_pct / 100))
            height: parent.height
            radius: parent.radius
            color: root.panel.levelColor(modelData.level, Color.accent)

            Behavior on width { NumberAnimation { duration: 420; easing.type: Easing.OutCubic } }
          }
        }

        Text {
          width: parent.width
          wrapMode: Text.WordWrap
          textFormat: Text.PlainText
          text: modelData.line
          color: root.dim
          font.family: root.fontFamily
          font.pixelSize: Style.font.bodySmall
        }
      }
    }
  }

  PanelSeparator {
    foreground: root.fg
    visible: root.data !== null
  }

  Text {
    width: parent.width
    visible: root.data !== null
    wrapMode: Text.WordWrap
    textFormat: Text.PlainText
    // The tooltip's last lines: today's use and any low credit.
    text: root.data ? root.data.tooltip.split("\n").slice(Math.max(1, root.data.windows.length)).join("\n") : ""
    color: root.dim
    font.family: root.fontFamily
    font.pixelSize: Style.font.bodySmall
  }
}
