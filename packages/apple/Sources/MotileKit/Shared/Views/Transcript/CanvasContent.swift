import Foundation

#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// A scene laid out at a column's width with what the client sets around it: where the scene
/// lies, its legend and its tooltips. Pure and safe on any thread, so the transcript builds it
/// while it measures the row.
struct CanvasContent {
    let scene: CanvasScene
    let width: CGFloat
    /// The view's size: the column's width and the scene and its legend tall with a legend, the
    /// scene's own size without one.
    let size: CGSize
    /// Where the scene's top left corner is in the view.
    let origin: CGPoint
    let legend: [Mark]
    /// A tooltip for each hit, in the scene's points.
    let tips: [Tip]

    /// A dot and the words beside it.
    struct Mark {
        let dot: CGRect
        let color: CanvasColor
        let texts: [(text: NSAttributedString, frame: CGRect)]
    }

    struct Tip {
        let frame: CGRect
        let texts: [(text: NSAttributedString, frame: CGRect)]
        let dots: [(frame: CGRect, color: CanvasColor)]
    }

    init(scene: CanvasScene, width: CGFloat) {
        self.scene = scene
        self.width = width
        tips = scene.hits.map { Self.tip($0.tip) }
        guard let legend = scene.legend, !legend.entries.isEmpty else {
            size = scene.size
            origin = .zero
            self.legend = []
            return
        }
        guard legend.isPie else {
            let marks = Self.chartLegend(legend, width: width, top: scene.height + Legend.chartGap)
            let bottom = marks.map { $0.texts.map(\.frame.maxY).max() ?? $0.dot.maxY }.max() ?? scene.height
            size = CGSize(width: width, height: ceil(bottom))
            origin = .zero
            self.legend = marks
            return
        }
        let left = legend.beside ? scene.width + Legend.pieGap : 0
        let columnWidth = max(0, width - left)
        let column = Self.pieLegend(legend, width: columnWidth)
        guard legend.beside else {
            let top = scene.height + Legend.pieGap
            self.legend = column.marks.map { $0.moved(by: CGPoint(x: 0, y: top)) }
            size = CGSize(width: width, height: ceil(top + column.height))
            origin = .zero
            return
        }
        let height = max(scene.height, column.height)
        self.legend = column.marks.map { $0.moved(by: CGPoint(x: left, y: ((height - column.height) / 2).rounded())) }
        size = CGSize(width: width, height: ceil(height))
        origin = CGPoint(x: 0, y: ((height - scene.height) / 2).rounded())
    }

    /// How tall a scene is in a column `width` wide, with its legend.
    static func height(_ scene: CanvasScene, width: CGFloat) -> CGFloat {
        CanvasContent(scene: scene, width: width).size.height
    }

    /// The hit under a point of the view.
    func hit(at point: CGPoint) -> Int? {
        scene.hit(at: CGPoint(x: point.x - origin.x, y: point.y - origin.y))
    }

    // MARK: Legends

    private enum Legend {
        static let chartGap: CGFloat = 10
        static let itemGap: CGFloat = 16
        static let lineGap: CGFloat = 6
        static let dot: CGFloat = 8
        static let chartDotGap: CGFloat = 7
        static let pieGap: CGFloat = 24
        static let rowGap: CGFloat = 9
        static let pieDotGap: CGFloat = 8
        static let shareGap: CGFloat = 8
    }

    private static func chartLegend(_ legend: CanvasLegend, width: CGFloat, top: CGFloat) -> [Mark] {
        let lineHeight = TextStyle.caption.lineHeight
        var (x, y) = (CGFloat(0), top)
        return legend.entries.map { entry in
            let text = TextStyle.caption.text(entry.label, color: Theme.mutedForeground)
            let words = ControlText.width(text)
            let itemWidth = Legend.dot + Legend.chartDotGap + words
            if x > 0, x + itemWidth > width {
                x = 0
                y += lineHeight + Legend.lineGap
            }
            let dot = CGRect(x: x, y: y + ((lineHeight - Legend.dot) / 2).rounded(), width: Legend.dot, height: Legend.dot)
            let frame = CGRect(x: x + Legend.dot + Legend.chartDotGap, y: y, width: words, height: lineHeight)
            x += itemWidth + Legend.itemGap
            return Mark(dot: dot, color: entry.color, texts: [(text, frame)])
        }
    }

    private static func pieLegend(_ legend: CanvasLegend, width: CGFloat) -> (marks: [Mark], height: CGFloat) {
        let lineHeight = TextStyle.small.lineHeight
        var y: CGFloat = 0
        let marks = legend.entries.map { entry in
            let share = entry.share.map { TextStyle.share.text($0, color: Theme.foreground) }
            let shareWidth = share.map(ControlText.width) ?? 0
            let labelLeft = Legend.dot + Legend.pieDotGap
            let labelWidth = max(1, width - labelLeft - (share == nil ? 0 : shareWidth + Legend.shareGap))
            let label = TextStyle.small.text(entry.label, color: Theme.foreground)
            let labelHeight = max(lineHeight, ControlText.height(label, width: labelWidth))
            var texts: [(text: NSAttributedString, frame: CGRect)] = [(label, CGRect(x: labelLeft, y: y, width: labelWidth, height: labelHeight))]
            if let share { texts.append((share, CGRect(x: width - shareWidth, y: y, width: shareWidth, height: lineHeight))) }
            let dot = CGRect(x: 0, y: y + ((lineHeight - Legend.dot) / 2).rounded(), width: Legend.dot, height: Legend.dot)
            y += labelHeight + Legend.rowGap
            return Mark(dot: dot, color: entry.color, texts: texts)
        }
        return (marks, max(0, y - Legend.rowGap))
    }

    // MARK: Tooltips

    private enum TipMetrics {
        static let vertical: CGFloat = 10
        static let horizontal: CGFloat = 12
        static let least: CGFloat = 110
        static let rowGap: CGFloat = 3
        static let dot: CGFloat = 8
        static let dotGap: CGFloat = 6
        static let valueGap: CGFloat = 8
    }

    private static func tip(_ tip: CanvasTip) -> Tip {
        let lineHeight = TextStyle.caption.lineHeight
        let title = TextStyle.tipTitle.text(tip.title, color: Theme.foreground)
        let rows = tip.rows.map { row in
            (
                row: row, name: row.name.isEmpty ? nil : TextStyle.caption.text(row.name, color: Theme.mutedForeground),
                value: TextStyle.readout.text(row.value, color: Theme.foreground)
            )
        }
        let rowWidths = rows.map { row in
            let lead = row.row.color == nil ? 0 : TipMetrics.dot + TipMetrics.dotGap
            let name = row.name.map { ControlText.width($0) + TipMetrics.valueGap } ?? 0
            return lead + name + ControlText.width(row.value)
        }
        let inner = max(TipMetrics.least - 2 * TipMetrics.horizontal, ([ControlText.width(title)] + rowWidths).max() ?? 0)
        let width = inner + 2 * TipMetrics.horizontal
        var texts: [(text: NSAttributedString, frame: CGRect)] = [(title, CGRect(x: TipMetrics.horizontal, y: TipMetrics.vertical, width: inner, height: lineHeight))]
        var dots: [(frame: CGRect, color: CanvasColor)] = []
        var y = TipMetrics.vertical + lineHeight
        for row in rows {
            y += TipMetrics.rowGap
            var x = TipMetrics.horizontal
            if let color = row.row.color {
                dots.append((CGRect(x: x, y: y + ((lineHeight - TipMetrics.dot) / 2).rounded(), width: TipMetrics.dot, height: TipMetrics.dot), color))
                x += TipMetrics.dot + TipMetrics.dotGap
            }
            if let name = row.name { texts.append((name, CGRect(x: x, y: y, width: ControlText.width(name), height: lineHeight))) }
            let valueWidth = ControlText.width(row.value)
            texts.append((row.value, CGRect(x: width - TipMetrics.horizontal - valueWidth, y: y, width: valueWidth, height: lineHeight)))
            y += lineHeight
        }
        let height = y + TipMetrics.vertical
        let frame = CGRect(x: (tip.x - width / 2).rounded(), y: tip.y.rounded(), width: ceil(width), height: ceil(height))
        return Tip(frame: frame, texts: texts, dots: dots)
    }
}

extension CanvasContent.Mark {
    fileprivate func moved(by offset: CGPoint) -> CanvasContent.Mark {
        CanvasContent.Mark(
            dot: dot.offsetBy(dx: offset.x, dy: offset.y), color: color,
            texts: texts.map { ($0.text, $0.frame.offsetBy(dx: offset.x, dy: offset.y)) })
    }
}

/// The text styles of `crates/openui/LAYOUT.md` the client sets around a scene, enlarged on iOS
/// as all of Motile's text is.
private enum TextStyle {
    case caption, tipTitle, readout, small, share

    var font: PlatformFont {
        switch self {
        case .caption: Theme.smallFont
        case .tipTitle: .ui(12, weight: .semibold)
        case .readout: .uiDigits(12, weight: .medium)
        case .small: .ui(13)
        case .share: .uiDigits(13, weight: .medium)
        }
    }

    var lineHeight: CGFloat {
        switch self {
        case .caption, .tipTitle, .readout: scaled(18)
        case .small, .share: scaled(20)
        }
    }

    func text(_ string: String, color: PlatformColor) -> NSAttributedString {
        ControlText.wrapped(string, font: font, lineHeight: lineHeight, color: color)
    }
}
