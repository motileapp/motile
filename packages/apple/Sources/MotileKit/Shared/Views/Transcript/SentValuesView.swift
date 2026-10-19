import Foundation

#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// What programs sent with a message, as core names it: the chip's label and the values it opens.
struct SentValues: Equatable {
    struct Entry: Equatable {
        let name: String
        let value: String
    }

    let label: String
    let entries: [Entry]

    init?(json: JSON?) {
        guard let json else { return nil }
        label = json.string("label")
        entries = json.objects("entries").map { Entry(name: $0.string("name"), value: $0.string("value")) }
    }
}

/// The chip at the end of a message's bubble that opens its values.
final class SentValuesChip: ControlView {
    static let size = ControlSize.small
    static let gap: CGFloat = 8

    private var values: SentValues?
    private var panel: ProgramPopoverPanel?

    init() {
        super.init(nil, symbol: .braces, variant: .outline, size: Self.size) {}
        surface = .secondary
        action = { [weak self] in self?.toggle() }
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    static func width(_ values: SentValues) -> CGFloat {
        ControlView.width(title: values.label, symbol: true, size: size)
    }

    func show(_ values: SentValues?) {
        guard values != self.values else { return }
        self.values = values
        title = values?.label
        isHidden = values == nil
        panel?.close()
        panel = nil
    }

    private func toggle() {
        if let panel {
            panel.close()
            self.panel = nil
            return
        }
        guard let values else { return }
        let panel = ProgramPopoverPanel(anchor: self) { [weak self] in self?.panel = nil }
        self.panel = panel
        panel.show(values)
    }
}

/// A message's values in Motile's popover: each name over its value.
final class SentValuesPanel: FlippedView {
    private static let padding = (ends: CGFloat(12), sides: CGFloat(14))
    private static let most: CGFloat = 320
    private static let entryGap: CGFloat = 8
    private static let nameGap: CGFloat = 2

    private let face = SurfaceView()
    private var lines: [TextBlockView] = []

    override init(frame: CGRect) {
        super.init(frame: frame)
        face.fill = Surface.popover.platform
        face.stroke = Surface.popover.platformBorder
        face.radius = Radius.lg
        face.drop = (.md, .shadow)
        addSubview(face)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    /// Shows the values and answers how large the panel is.
    func show(_ values: SentValues, window: CGSize) -> CGSize {
        let room = min(Self.most, window.width - 24) - 2 * Self.padding.sides
        let texts = values.entries.flatMap { entry in
            [
                ControlText.wrapped(entry.name, font: Theme.smallFont, lineHeight: scaled(18), color: Theme.mutedForeground),
                ControlText.wrapped(entry.value, font: .ui(13), lineHeight: scaled(20), color: Theme.foreground),
            ]
        }
        let width = min(room, texts.map(ControlText.width).max() ?? 0)
        while lines.count < texts.count {
            let line = TextBlockView()
            addSubview(line)
            lines.append(line)
        }
        var y = Self.padding.ends
        for (index, line) in lines.enumerated() {
            line.isHidden = index >= texts.count
            guard index < texts.count else { continue }
            if index > 0 { y += index.isMultiple(of: 2) ? Self.entryGap : Self.nameGap }
            let height = ControlText.height(texts[index], width: width)
            line.text = texts[index]
            line.frame = CGRect(x: Self.padding.sides, y: y, width: width, height: height)
            y += height
        }
        return CGSize(width: width + 2 * Self.padding.sides, height: y + Self.padding.ends)
    }

    override func layoutNow() {
        face.frame = bounds
    }
}
