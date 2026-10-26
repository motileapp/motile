import Foundation
import QuartzCore

#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// The text of the controls the transcript draws itself, measured as it is drawn, on any thread.
enum ControlText {
    /// One line in the font, cut short at its end where it doesn't fit.
    static func line(_ text: String, font: PlatformFont, color: PlatformColor) -> NSAttributedString {
        let style = NSMutableParagraphStyle()
        style.lineBreakMode = .byTruncatingTail
        return NSAttributedString(string: text, attributes: [.font: font, .foregroundColor: color, .paragraphStyle: style])
    }

    /// Lines `lineHeight` apart that wrap at any width. `TextBlockView` centres each on its line.
    static func wrapped(
        _ text: String, font: PlatformFont, lineHeight: CGFloat, color: PlatformColor, struck: Bool = false
    ) -> NSAttributedString {
        let style = NSMutableParagraphStyle()
        style.minimumLineHeight = lineHeight
        style.maximumLineHeight = lineHeight
        var attributes: [NSAttributedString.Key: Any] = [.font: font, .foregroundColor: color, .paragraphStyle: style]
        if struck { attributes[.strikethroughStyle] = NSUnderlineStyle.single.rawValue }
        return NSAttributedString(string: text, attributes: attributes)
    }

    /// How far a line of the font is raised in a taller line to stand in its middle: the text
    /// system puts all the room over it.
    static func lift(_ font: PlatformFont, lineHeight: CGFloat) -> CGFloat {
        max(0, ((lineHeight - (font.ascender - font.descender)) / 2).rounded())
    }

    /// Where the first baseline of text set with `wrapped` is drawn.
    static func baseline(_ font: PlatformFont, lineHeight: CGFloat) -> CGFloat {
        lineHeight + font.descender - lift(font, lineHeight: lineHeight)
    }

    /// The lift of text set with `wrapped`.
    static func lift(_ text: NSAttributedString) -> CGFloat {
        guard text.length > 0, let font = text.attribute(.font, at: 0, effectiveRange: nil) as? PlatformFont,
            let style = text.attribute(.paragraphStyle, at: 0, effectiveRange: nil) as? NSParagraphStyle, style.maximumLineHeight > 0
        else { return 0 }
        return lift(font, lineHeight: style.maximumLineHeight)
    }

    static func width(_ text: NSAttributedString) -> CGFloat {
        ceil(text.bounds(width: .greatestFiniteMagnitude).width)
    }

    static func height(_ text: NSAttributedString, width: CGFloat) -> CGFloat {
        guard text.length > 0 else { return 0 }
        return ceil(text.bounds(width: max(1, width)).height)
    }
}

/// Text the view draws itself: wrapped at its width, or on one line cut short.
final class TextBlockView: FlippedView {
    var text = NSAttributedString() { didSet { redraw() } }
    var oneLine = false { didSet { redraw() } }

    override init(frame: CGRect) {
        super.init(frame: frame)
        #if os(iOS)
        isUserInteractionEnabled = false
        registerForTraitChanges([UITraitUserInterfaceStyle.self]) { (view: Self, _: UITraitCollection) in view.redraw() }
        #endif
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    #if os(macOS)
    override func hitTest(_ point: NSPoint) -> NSView? { nil }
    #endif

    override func draw(_ rect: CGRect) {
        let lift = ControlText.lift(text)
        let area = CGRect(x: 0, y: -lift, width: bounds.width, height: bounds.height + lift)
        guard oneLine else { return text.draw(with: area, options: [.usesLineFragmentOrigin, .usesFontLeading], context: nil) }
        text.drawTruncated(in: area)
    }
}

/// The fill of a field on a surface: the input colour, or on a card, which is as dark, its box.
func fieldFill(_ surface: Surface) -> PlatformColor {
    surface == .card ? surface.platform(.box) : Theme.input
}

/// `ActionButton` for the views the transcript draws itself: words, a symbol or both, in any
/// variant and size, lit under the pointer and while pressed. One that `fills` takes the width it
/// is given, with what it says in its middle.
class ControlView: FlippedView {
    let size: ControlSize
    var variant: ButtonVariant { didSet { paint() } }
    var title: String? { didSet { paint(); relayout() } }
    var symbol: Symbol? { didSet { showMark() } }
    /// A Lucide icon by its character in the font, which the apps' own `Symbol` may not have.
    var glyph: String? { didSet { showMark() } }
    var tint: PlatformColor? { didSet { paint() } }
    var selected = false { didSet { paint() } }
    var surface = Surface.background { didSet { paint() } }
    var enabled = true {
        didSet {
            opacity = enabled ? 1 : Opacity.disabled.fixed
            paint()
        }
    }
    var help: String? { didSet { describeSelf() } }
    /// Its words are what it shows while nothing is chosen, in a field's placeholder colour.
    var placeholder = false { didSet { paint() } }
    var fills = false
    /// Its words start at its start and its chevron stands at its end, as a select's do.
    var opens = false {
        didSet {
            chevron.isHidden = !opens
            describeSelf()
            relayout()
        }
    }
    var action: () -> Void

    private let face = SurfaceView()
    private let words = TextBlockView()
    private let mark = SymbolView()
    private let chevron = SymbolView()
    private var lit = false

    init(
        _ title: String?, symbol: Symbol? = nil, help: String? = nil, variant: ButtonVariant = .secondary, size: ControlSize = .regular,
        action: @escaping () -> Void
    ) {
        self.title = title
        self.symbol = symbol
        self.help = help
        self.variant = variant
        self.size = size
        self.action = action
        super.init(frame: .zero)
        words.oneLine = true
        chevron.image = .symbol(.chevronDown, size: size.chevron, trimmed: true)
        chevron.tint = Theme.mutedStrongerForeground
        chevron.isHidden = true
        for view in [face, mark, words, chevron] as [PlatformView] { addSubview(view) }
        showMark()
        focusable = true
        onPress = { [weak self] _ in self?.press() }
        onHover = { [weak self] point in self?.light(point != nil) }
        paint()
        describeSelf()
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    static func font(_ size: ControlSize) -> PlatformFont { .ui(size.textSize, weight: .medium) }

    private var hasMark: Bool { symbol != nil || glyph != nil }

    private func showMark() {
        mark.isHidden = !hasMark
        if let symbol {
            mark.show(symbol, size: size.symbol)
        } else if let glyph {
            mark.image = .lucide(glyph, side: size.symbolSide)
        }
        relayout()
    }

    private var wordless: Bool { (title ?? "").isEmpty && !opens }

    /// How wide it is when nothing cuts its words short.
    static func width(title: String?, symbol: Bool, size: ControlSize, opens: Bool = false) -> CGFloat {
        let words = title.map { ControlText.width(ControlText.line($0, font: font(size), color: Theme.foreground)) } ?? 0
        guard words > 0 || opens else { return size.height }
        var width = size.padding - (symbol ? size.symbolOutset : 0) + words + size.padding
        if symbol { width += size.symbolSide + size.gap }
        if opens { width += size.gap + PlatformImage.symbol(.chevronDown, size: size.chevron, trimmed: true).size.width - size.chevronOutset }
        return max(size.height, ceil(width))
    }

    var naturalWidth: CGFloat { Self.width(title: title, symbol: hasMark, size: size, opens: opens) }

    override var focusShape: (rect: CGRect, radius: CGFloat) { (face.frame, size.radius) }

    /// How far around it a finger still presses it.
    private var reach: CGRect {
        let around = max(0, (Platform.minimumPress - size.height) / 2)
        return bounds.insetBy(dx: wordless ? -around : 0, dy: -around)
    }

    override func takesPress(at point: CGPoint) -> Bool {
        enabled && reach.contains(point)
    }

    #if os(iOS)
    override func point(inside point: CGPoint, with event: UIEvent?) -> Bool {
        reach.contains(point) || super.point(inside: point, with: event)
    }
    #endif

    override func layoutNow() {
        let height = size.height
        let top = ((bounds.height - height) / 2).rounded()
        face.frame = CGRect(x: 0, y: top, width: bounds.width, height: height)
        let side = size.symbolSide
        guard !wordless else {
            mark.frame = CGRect(x: ((bounds.width - side) / 2).rounded(), y: top + ((height - side) / 2).rounded(), width: side, height: side)
            return
        }
        let text = words.text
        let textWidth = ControlText.width(text)
        let chevronWidth = opens ? chevron.image?.size.width ?? 0 : 0
        let leading = size.padding - (hasMark ? size.symbolOutset : 0)
        let trailing = size.padding - (opens ? size.chevronOutset : 0)
        let content = (hasMark ? side + size.gap : 0) + textWidth + (opens ? size.gap + chevronWidth : 0)
        var x = opens || !fills ? leading : max(leading, ((bounds.width - content) / 2).rounded())
        if hasMark {
            mark.frame = CGRect(x: x, y: top + ((height - side) / 2).rounded(), width: side, height: side)
            x += side + size.gap
        }
        let end = bounds.width - trailing - (opens ? chevronWidth + size.gap : 0)
        let textHeight = ControlText.height(text, width: .greatestFiniteMagnitude)
        words.frame = CGRect(x: x, y: top + ((height - textHeight) / 2).rounded(), width: max(0, min(textWidth, end - x)), height: textHeight)
        let chevronX = opens ? bounds.width - trailing - chevronWidth : words.frame.maxX + size.gap
        chevron.frame = CGRect(x: chevronX, y: top, width: chevronWidth, height: height)
    }

    private func light(_ lit: Bool) {
        self.lit = lit
        paint()
    }

    private func paint() {
        let look = ControlLook(variant: variant, size: size, selected: selected, lit: lit && enabled, surface: surface)
        face.fill = look.platformFill
        face.stroke = look.platformBorder
        face.radius = size.radius
        let ink = placeholder ? Theme.mutedStrongerForeground : (tint.flatMap { look.takesTint ? $0 : nil } ?? look.platformForeground)
        words.text = ControlText.line(title ?? "", font: Self.font(size), color: ink)
        mark.tint = ink
    }

    func press() {
        guard enabled else { return }
        action()
    }

    private func describeSelf() {
        describe(title.flatMap { $0.isEmpty ? nil : $0 } ?? help ?? "", role: opens ? .popUp : .button)
        tip = help
    }
}

/// `Select` for the views the transcript draws itself: the chosen option, or the placeholder, at
/// its start and its chevron at its end. A press offers the options under it, with a check by the
/// chosen one: a menu on the Mac, a sheet on iOS.
final class SelectView: ControlView {
    var options: [String] { didSet { show() } }
    var chosen: Int? { didSet { show() } }
    var placeholderText: String { didSet { show() } }
    var onSelect: (Int) -> Void

    init(options: [String], chosen: Int?, placeholder: String = "Choose", size: ControlSize = .regular, onSelect: @escaping (Int) -> Void) {
        self.options = options
        self.chosen = chosen
        placeholderText = placeholder
        self.onSelect = onSelect
        super.init(nil, size: size) {}
        opens = true
        fills = true
        action = { [weak self] in self?.offer() }
        show()
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    private func show() {
        let index = chosen.flatMap { options.indices.contains($0) ? $0 : nil }
        title = index.map { options[$0] } ?? placeholderText
        placeholder = index == nil
    }

    private func offer() {
        let choices = options.indices.map { index in
            Choice(options[index], chosen: index == chosen) { [weak self] in self?.onSelect(index) }
        }
        Choice.offer(choices, under: self)
    }
}

/// `Segmented` for the views the transcript draws itself: a short row of choices of which one is
/// on, in a track bordered on the background. The arrow keys move the choice.
final class SegmentedView: FlippedView {
    let size: ControlSize
    var options: [String] { didSet { build() } }
    var selection: Int { didSet { paint() } }
    var fills = false { didSet { relayout() } }
    var enabled = true {
        didSet {
            opacity = enabled ? 1 : Opacity.disabled.fixed
            paint()
        }
    }
    var onSelect: (Int) -> Void

    private static let edge = Segmented<Int>.inset + Segmented<Int>.border
    private static let half = Segmented<Int>.inset / 2
    private let track = SurfaceView()
    private var segments: [(light: SurfaceView, words: TextBlockView, area: FlippedView)] = []
    private var hovered: Int?

    init(_ options: [String], selection: Int, size: ControlSize = .regular, onSelect: @escaping (Int) -> Void) {
        self.options = options
        self.selection = selection
        self.size = size
        self.onSelect = onSelect
        super.init(frame: .zero)
        track.fill = Theme.background
        track.stroke = Theme.border
        track.radius = size.radius + Segmented<Int>.inset + Segmented<Int>.border
        addSubview(track)
        focusable = true
        onKey = { [weak self] key in self?.key(key) ?? false }
        build()
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    static func height(_ size: ControlSize) -> CGFloat { size.height + 2 * edge }

    private static func segmentWidth(_ title: String, size: ControlSize, index: Int, count: Int) -> CGFloat {
        let words = ControlText.width(ControlText.line(title, font: ControlView.font(size), color: Theme.foreground))
        return words + 2 * size.padding + (index == 0 ? edge : half) + (index == count - 1 ? edge : half)
    }

    /// How wide it is at its own width.
    static func width(_ options: [String], size: ControlSize) -> CGFloat {
        options.indices.reduce(0) { $0 + segmentWidth(options[$1], size: size, index: $1, count: options.count) }
    }

    override var focusShape: (rect: CGRect, radius: CGFloat) { (track.frame, track.radius) }

    private func build() {
        for segment in segments { segment.area.removeFromSuperview() }
        segments = options.indices.map { index in
            let area = FlippedView()
            let light = SurfaceView()
            light.radius = size.radius
            let words = TextBlockView()
            words.oneLine = true
            area.addSubview(light)
            area.addSubview(words)
            area.onPress = { [weak self] _ in self?.choose(index) }
            area.onHover = { [weak self] point in self?.hover(index, point != nil) }
            area.describe(options[index], role: .radio)
            addSubview(area)
            return (light, words, area)
        }
        paint()
        relayout()
    }

    override func layoutNow() {
        track.frame = bounds
        let natural = options.indices.map { Self.segmentWidth(options[$0], size: size, index: $0, count: options.count) }
        let share = fills && !natural.isEmpty ? bounds.width / CGFloat(natural.count) : 0
        var x: CGFloat = 0
        for (index, segment) in segments.enumerated() {
            let width = fills ? share : natural[index]
            segment.area.frame = CGRect(x: x, y: 0, width: width, height: bounds.height)
            let leading = index == 0 ? Self.edge : Self.half
            let trailing = index == segments.count - 1 ? Self.edge : Self.half
            let lit = CGRect(x: leading, y: Self.edge, width: max(0, width - leading - trailing), height: size.height)
            segment.light.frame = lit
            let textHeight = ControlText.height(segment.words.text, width: .greatestFiniteMagnitude)
            let textWidth = min(ControlText.width(segment.words.text), max(0, lit.width - 2 * size.padding))
            segment.words.frame = CGRect(
                x: lit.minX + ((lit.width - textWidth) / 2).rounded(), y: lit.minY + ((lit.height - textHeight) / 2).rounded(), width: textWidth,
                height: textHeight)
            x += width
        }
    }

    private func paint() {
        for (index, segment) in segments.enumerated() {
            let selected = index == selection
            let lit = enabled && hovered == index
            segment.light.fill = selected ? Surface.background.platform(.controlLit) : lit ? Surface.background.platform(.control) : .clear
            let ink = !enabled ? Theme.mutedStrongerForeground : selected || lit ? Theme.foreground : Theme.mutedForeground
            segment.words.text = ControlText.line(options[index], font: ControlView.font(size), color: ink)
            segment.area.describe(options[index], role: .radio, checked: selected)
        }
    }

    private func hover(_ index: Int, _ on: Bool) {
        hovered = on ? index : hovered == index ? nil : hovered
        paint()
    }

    private func choose(_ index: Int) {
        guard enabled, index != selection else { return }
        selection = index
        onSelect(index)
    }

    private func key(_ key: ControlKey) -> Bool {
        switch key {
        case .left, .up: choose(max(0, selection - 1))
        case .right, .down: choose(min(options.count - 1, selection + 1))
        case .home: choose(0)
        case .end: choose(options.count - 1)
        case .press: return false
        }
        return true
    }
}

/// `PickMark` for the views the transcript draws itself: a radio's ring, or a box where any of a
/// list are picked, filled in the primary colour once it is.
final class PickMarkView: LayerView {
    static let side = scaled(16)

    let multiple: Bool
    var picked = false { didSet { repaint() } }
    private let dot = CALayer()
    private let check = SymbolView(.check, size: .xs, tint: Theme.primaryForeground)

    init(multiple: Bool) {
        self.multiple = multiple
        super.init(frame: .zero)
        addSubview(check)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override func layoutNow() {
        check.frame = bounds
        let side = Self.side * 0.375
        dot.frame = CGRect(x: (bounds.width - side) / 2, y: (bounds.height - side) / 2, width: side, height: side)
        dot.cornerRadius = side / 2
    }

    override func paint(_ layer: CALayer) {
        if dot.superlayer !== layer { layer.addSublayer(dot) }
        layer.cornerRadius = multiple ? Radius.xs : Self.side / 2
        layer.cornerCurve = .continuous
        layer.borderWidth = 1.5
        layer.borderColor = resolved(picked ? Theme.primary : Theme.mutedStrongestForeground)
        layer.backgroundColor = picked ? resolved(Theme.primary) : nil
        dot.backgroundColor = resolved(Theme.primaryForeground)
        dot.isHidden = multiple || !picked
        check.isHidden = !multiple || !picked
    }
}

/// `PickRow` for the views the transcript draws itself: a checkbox, or a radio of a list, as a
/// row of its mark and its title, with what it means under it. The whole row is pressed.
final class PickRowView: FlippedView {
    let multiple: Bool
    let size: ControlSize
    var picked = false {
        didSet {
            mark.picked = picked
            describeSelf()
        }
    }
    var title = NSAttributedString() {
        didSet {
            titleView.text = title
            describeSelf()
            relayout()
        }
    }
    var detail: NSAttributedString? {
        didSet {
            detailView.text = detail ?? NSAttributedString()
            relayout()
        }
    }
    var enabled = true { didSet { opacity = enabled ? 1 : Opacity.disabled.fixed } }

    private let mark: PickMarkView
    private let titleView = TextBlockView()
    private let detailView = TextBlockView()

    init(multiple: Bool, size: ControlSize = .regular, onPress: @escaping () -> Void) {
        self.multiple = multiple
        self.size = size
        mark = PickMarkView(multiple: multiple)
        super.init(frame: .zero)
        for view in [mark, titleView, detailView] as [PlatformView] { addSubview(view) }
        focusable = true
        self.onPress = { [weak self] _ in
            guard self?.enabled == true else { return }
            onPress()
        }
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    /// A title as rows set it: crossed out and muted once `struck`.
    static func title(_ text: String, struck: Bool = false) -> NSAttributedString {
        ControlText.wrapped(
            text, font: PickText.title.font, lineHeight: PickText.title.lineHeight, color: struck ? Theme.mutedForeground : Theme.foreground,
            struck: struck)
    }

    static func detail(_ text: String) -> NSAttributedString {
        ControlText.wrapped(text, font: PickText.detail.font, lineHeight: PickText.detail.lineHeight, color: Theme.mutedForeground)
    }

    private static func textWidth(_ width: CGFloat) -> CGFloat { width - PickMarkView.side - PickText.gap }

    private static func column(title: NSAttributedString, detail: NSAttributedString?, width: CGFloat) -> CGFloat {
        let words = textWidth(width)
        let under = detail.map { PickText.detailGap + ControlText.height($0, width: words) } ?? 0
        return ControlText.height(title, width: words) + under
    }

    static func height(title: NSAttributedString, detail: NSAttributedString?, width: CGFloat, size: ControlSize = .regular) -> CGFloat {
        max(size.height, column(title: title, detail: detail, width: width))
    }

    override var focusShape: (rect: CGRect, radius: CGFloat) {
        (mark.frame.insetBy(dx: -2, dy: -2), (multiple ? Radius.xs : PickMarkView.side / 2) + 2)
    }

    override func layoutNow() {
        let side = PickMarkView.side
        mark.frame = CGRect(x: 0, y: ((bounds.height - side) / 2).rounded(), width: side, height: side)
        let words = Self.textWidth(bounds.width)
        let column = Self.column(title: title, detail: detail, width: bounds.width)
        var y = ((bounds.height - column) / 2).rounded()
        let x = side + PickText.gap
        let titleHeight = ControlText.height(title, width: words)
        titleView.frame = CGRect(x: x, y: y, width: words, height: titleHeight)
        y += titleHeight + PickText.detailGap
        detailView.frame = CGRect(x: x, y: y, width: words, height: detail.map { ControlText.height($0, width: words) } ?? 0)
    }

    private func describeSelf() {
        describe(title.string, role: multiple ? .checkbox : .radio, checked: picked)
    }
}

/// `OptionCard` for the views the transcript draws itself: a choice as a bordered card of its
/// title and what it means, lit under the pointer, in the primary colour once it is chosen.
final class OptionCardView: FlippedView {
    var selected = false { didSet { paint(); describeSelf() } }
    var title = "" {
        didSet {
            titleView.text = Self.title(title)
            describeSelf()
            relayout()
        }
    }
    var detail: String? {
        didSet {
            detailView.text = detail.map(PickRowView.detail) ?? NSAttributedString()
            relayout()
        }
    }
    var surface = Surface.background { didSet { paint() } }
    var enabled = true { didSet { opacity = enabled ? 1 : Opacity.disabled.fixed; paint() } }
    var radius = Radius.lg { didSet { box.radius = radius } }

    private let box = SurfaceView()
    private let titleView = TextBlockView()
    private let detailView = TextBlockView()
    private var lit = false

    init(onPress: @escaping () -> Void) {
        super.init(frame: .zero)
        box.radius = radius
        for view in [box, titleView, detailView] as [PlatformView] { addSubview(view) }
        focusable = true
        self.onPress = { [weak self] _ in
            guard self?.enabled == true else { return }
            onPress()
        }
        onHover = { [weak self] point in
            self?.lit = point != nil
            self?.paint()
        }
        paint()
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    private static func title(_ text: String) -> NSAttributedString {
        ControlText.wrapped(text, font: PickText.cardTitle.font, lineHeight: PickText.cardTitle.lineHeight, color: Theme.foreground)
    }

    static func height(title: String, detail: String?, width: CGFloat) -> CGFloat {
        let padding = PickText.cardPadding
        let words = width - 2 * padding.sides
        let under = detail.map { PickText.detailGap + ControlText.height(PickRowView.detail($0), width: words) } ?? 0
        return 2 * padding.ends + ControlText.height(Self.title(title), width: words) + under
    }

    override var focusShape: (rect: CGRect, radius: CGFloat) { (bounds, radius) }

    override func layoutNow() {
        box.frame = bounds
        let padding = PickText.cardPadding
        let words = bounds.width - 2 * padding.sides
        let titleHeight = ControlText.height(titleView.text, width: words)
        titleView.frame = CGRect(x: padding.sides, y: padding.ends, width: words, height: titleHeight)
        detailView.frame = CGRect(
            x: padding.sides, y: padding.ends + titleHeight + PickText.detailGap, width: words,
            height: ControlText.height(detailView.text, width: words))
    }

    private func paint() {
        box.stroke = selected ? Theme.primary : surface.platformBorder
        box.fill = selected ? Theme.primaryTint : enabled && lit ? surface.platform(.row) : .clear
    }

    private func describeSelf() {
        describe(title, role: .radio, checked: selected)
    }
}

/// `Slider` for the views the transcript draws itself: its label and the value it reads above,
/// the round track with its thumb, the marks under it. It keeps its own value while it is
/// dragged and says each one it takes; the arrow keys and assistive technologies step it.
final class SliderView: FlippedView {
    typealias Mark = Slider.Mark

    var value: Double { didSet { if dragged == nil { relayout() } } }
    var range: ClosedRange<Double> { didSet { relayout() } }
    var step: Double?
    var label: String? { didSet { labelView.text = label.map(Self.label) ?? NSAttributedString(); describeSelf(); relayout() } }
    var readout: String? {
        didSet {
            readoutView.text = readout.map(Self.readout) ?? NSAttributedString()
            describeSelf()
            relayout()
        }
    }
    var marks: [Mark] = [] { didSet { buildMarks() } }
    var surface = Surface.background { didSet { paint() } }
    var enabled = true { didSet { opacity = enabled ? 1 : Opacity.disabled.fixed } }
    var onChange: (Double) -> Void
    /// Called with true when a drag starts and false when it ends.
    var onEditingChanged: (Bool) -> Void = { _ in }

    private let labelView = TextBlockView()
    private let readoutView = TextBlockView()
    private let track = SurfaceView()
    private let thumb = SurfaceView()
    private var markViews: [TextBlockView] = []
    private var dragged: Double?
    private var hovering = false

    init(value: Double, in range: ClosedRange<Double>, step: Double? = nil, onChange: @escaping (Double) -> Void) {
        self.value = value
        self.range = range
        self.step = step
        self.onChange = onChange
        super.init(frame: .zero)
        readoutView.oneLine = true
        thumb.radius = SliderTrack.thumb / 2
        thumb.fill = Theme.foreground
        for view in [labelView, readoutView, track, thumb] as [PlatformView] { addSubview(view) }
        focusable = true
        onDrag = { [weak self] phase, point in self?.drag(phase, at: point) }
        onPress = { [weak self] point in
            self?.drag(.began, at: point)
            self?.drag(.ended, at: point)
        }
        onHover = { [weak self] point in
            self?.hovering = point != nil
            self?.paint()
        }
        onKey = { [weak self] key in self?.key(key) ?? false }
        onAdjust = { [weak self] steps in self?.nudge(steps) }
        paint()
        describeSelf()
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    private static let labelFont = PlatformFont.ui(13)
    private static let labelLine = scaled(20)
    private static let readoutFont = PlatformFont.uiDigits(12, weight: .medium)
    private static let readoutLine = scaled(18)
    private static let tickFont = PlatformFont.uiDigits(11)
    private static let tickLine = scaled(14)

    static func label(_ text: String) -> NSAttributedString {
        ControlText.wrapped(text, font: labelFont, lineHeight: labelLine, color: Theme.foreground)
    }

    static func readout(_ text: String) -> NSAttributedString {
        ControlText.wrapped(text, font: readoutFont, lineHeight: readoutLine, color: Theme.foreground)
    }

    private static func tick(_ text: String) -> NSAttributedString {
        ControlText.wrapped(text, font: tickFont, lineHeight: tickLine, color: Theme.mutedForeground)
    }

    private static func labelWidth(_ width: CGFloat, readout: NSAttributedString?) -> CGFloat {
        guard let readout else { return width }
        return max(0, width - ControlText.width(readout) - SliderTrack.headGap)
    }

    private static func head(label: NSAttributedString?, readout: NSAttributedString?, width: CGFloat) -> CGFloat {
        if let label { return max(labelLine, ControlText.height(label, width: labelWidth(width, readout: readout))) }
        return readout == nil ? 0 : readoutLine
    }

    /// How tall it is at the width, with the label and the readout set as `label(_:)` and `readout(_:)` do.
    static func height(label: NSAttributedString?, readout: NSAttributedString?, marks: Bool, width: CGFloat) -> CGFloat {
        head(label: label, readout: readout, width: width) + SliderTrack.controlHeight
            + (marks ? SliderTrack.marksHeight - SliderTrack.marksOverlap : 0)
    }

    private var shown: Double { dragged ?? value }

    private var head: CGFloat {
        Self.head(label: label == nil ? nil : labelView.text, readout: readout == nil ? nil : readoutView.text, width: bounds.width)
    }

    private var band: CGRect { CGRect(x: 0, y: head, width: bounds.width, height: SliderTrack.controlHeight) }

    override func takesPress(at point: CGPoint) -> Bool {
        enabled && band.contains(point)
    }

    override var focusShape: (rect: CGRect, radius: CGFloat) { (track.frame, track.radius) }

    override func layoutNow() {
        let width = bounds.width
        let readoutWidth = readout == nil ? 0 : ControlText.width(readoutView.text)
        let labelWidth = Self.labelWidth(width, readout: readout == nil ? nil : readoutView.text)
        labelView.frame = CGRect(x: 0, y: 0, width: labelWidth, height: label == nil ? 0 : ControlText.height(labelView.text, width: labelWidth))
        let readoutTop = label == nil
            ? 0 : ControlText.baseline(Self.labelFont, lineHeight: Self.labelLine) - ControlText.baseline(Self.readoutFont, lineHeight: Self.readoutLine)
        readoutView.frame = CGRect(x: width - readoutWidth, y: readoutTop, width: readoutWidth, height: Self.readoutLine)
        let band = self.band
        let trackHeight = SliderTrack.trackHeight
        track.frame = CGRect(x: 0, y: band.minY + ((band.height - trackHeight) / 2).rounded(), width: width, height: trackHeight)
        track.radius = trackHeight / 2
        let center = SliderTrack.center(SliderTrack.fraction(shown, in: range), width: width)
        let side = SliderTrack.thumb
        thumb.frame = CGRect(x: center - side / 2, y: track.frame.midY - side / 2, width: side, height: side)
        let row = band.maxY - SliderTrack.marksOverlap
        for (index, view) in markViews.enumerated() {
            let textWidth = ControlText.width(view.text)
            let fraction = SliderTrack.fraction(marks[index].value, in: range)
            let (x, anchor) = SliderTrack.place(fraction, first: index == 0, last: index == marks.count - 1, width: width)
            view.frame = CGRect(
                x: (x - anchor * textWidth).rounded(), y: row + ((SliderTrack.marksHeight - Self.tickLine) / 2).rounded(), width: textWidth,
                height: Self.tickLine)
        }
    }

    private func buildMarks() {
        for view in markViews { view.removeFromSuperview() }
        markViews = marks.map { mark in
            let view = TextBlockView()
            view.oneLine = true
            view.text = Self.tick(mark.title)
            addSubview(view)
            return view
        }
        relayout()
    }

    private func paint() {
        track.fill = surface.platform(hovering || dragged != nil ? .controlLit : .control)
    }

    private func drag(_ phase: DragPhase, at point: CGPoint) {
        guard enabled else { return }
        if phase == .began {
            dragged = shown
            onEditingChanged(true)
        }
        take(SliderTrack.value(at: point.x, width: bounds.width, in: range, step: step))
        guard phase == .ended else { return paint() }
        value = shown
        dragged = nil
        onEditingChanged(false)
        paint()
    }

    private func nudge(_ steps: Int) {
        take(SliderTrack.stepped(shown, by: steps, in: range, step: step))
    }

    private func take(_ new: Double) {
        guard enabled, new != shown else { return }
        if dragged != nil { dragged = new } else { value = new }
        onChange(new)
        relayout()
    }

    private func key(_ key: ControlKey) -> Bool {
        switch key {
        case .left, .down: nudge(-1)
        case .right, .up: nudge(1)
        case .home: take(range.lowerBound)
        case .end: take(range.upperBound)
        case .press: return false
        }
        return true
    }

    private func describeSelf() {
        describe(label ?? "", role: .slider, value: readout ?? String(shown))
    }
}

/// `DateField` for the views the transcript draws itself: a field that shows a day, which a press
/// opens the system's calendar for, held to `earliest` and `latest`.
final class DateFieldView: FlippedView {
    let size: ControlSize
    var text: String? { didSet { paint(); describeSelf() } }
    var placeholder = "Date" { didSet { paint(); describeSelf() } }
    /// What assistive technologies call it.
    var label: String? { didSet { describeSelf() } }
    var day: Day?
    var earliest: Day?
    var latest: Day?
    var surface = Surface.background { didSet { paint() } }
    var enabled = true { didSet { opacity = enabled ? 1 : Opacity.disabled.fixed } }
    var onPick: (Day) -> Void

    private let frameView = SurfaceView()
    private let words = TextBlockView()
    private let calendar: SymbolView

    init(size: ControlSize = .regular, onPick: @escaping (Day) -> Void) {
        self.size = size
        self.onPick = onPick
        calendar = SymbolView(.calendar, size: size.smallSymbol, tint: Theme.mutedStrongerForeground)
        super.init(frame: .zero)
        words.oneLine = true
        frameView.radius = size.radius
        for view in [frameView, words, calendar] as [PlatformView] { addSubview(view) }
        focusable = true
        onPress = { [weak self] _ in self?.open() }
        paint()
        describeSelf()
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override var focusShape: (rect: CGRect, radius: CGFloat) { (frameView.frame, size.radius) }

    override func layoutNow() {
        let height = size.height
        let top = ((bounds.height - height) / 2).rounded()
        frameView.frame = CGRect(x: 0, y: top, width: bounds.width, height: height)
        let inset = size.padding - 2
        let side = PlatformImage.symbolSide(size.smallSymbol)
        calendar.frame = CGRect(x: bounds.width - inset - side, y: top + ((height - side) / 2).rounded(), width: side, height: side)
        let textHeight = ControlText.height(words.text, width: .greatestFiniteMagnitude)
        words.frame = CGRect(
            x: inset, y: top + ((height - textHeight) / 2).rounded(), width: max(0, calendar.frame.minX - size.gap - inset), height: textHeight)
    }

    private func paint() {
        frameView.fill = fieldFill(surface)
        let font = PlatformFont.ui(size.textSize + 0.5)
        words.text = ControlText.line(text ?? placeholder, font: font, color: text == nil ? Theme.mutedStrongerForeground : Theme.foreground)
    }

    private func open() {
        guard enabled else { return }
        DayPicker.present(from: self, day: day ?? .today, earliest: earliest, latest: latest) { [weak self] day in
            self?.day = day
            self?.onPick(day)
        }
    }

    private func describeSelf() {
        describe(label ?? placeholder, role: .button, value: text)
    }
}

/// `GlassButton` for the views the transcript draws itself: words, a symbol or both on a capsule of
/// glass that floats over a map or a picture, lit under the pointer. A symbol alone is round. One
/// that `opens` has a chevron in its trailing padding and offers choices.
final class GlassButtonView: FlippedView {
    static let size = ControlSize.regular
    private static let fadeDuration: TimeInterval = 0.12

    let height: CGFloat
    var title: String? { didSet { paint(); describeSelf(); relayout() } }
    var symbol: Symbol? { didSet { showSymbol(); relayout() } }
    /// The side of the square its symbol is drawn in, on the Mac.
    var symbolSide: CGFloat { didSet { showSymbol(); relayout() } }
    /// Its symbol stands after its words.
    var symbolTrails = false { didSet { relayout() } }
    var opens = false { didSet { chevron.isHidden = !opens; describeSelf(); relayout() } }
    var padding: (leading: CGFloat, trailing: CGFloat) { didSet { relayout() } }
    var help: String? { didSet { describeSelf() } }
    /// Whether what it shows is open, for assistive technologies.
    var expanded: Bool? { didSet { describeSelf() } }
    var enabled = true { didSet { opacity = enabled ? 1 : Opacity.disabled.fixed } }
    var action: () -> Void

    private let glass = GlassView()
    private let light = SurfaceView()
    private let words = TextBlockView()
    private let mark = SymbolView()
    private let chevron = SymbolView()

    init(
        _ title: String? = nil, symbol: Symbol? = nil, symbolSide: CGFloat = GlassButtonView.size.symbol.rawValue, help: String? = nil,
        height: CGFloat = GlassButtonView.size.height, action: @escaping () -> Void
    ) {
        self.title = title
        self.symbol = symbol
        self.symbolSide = symbolSide
        self.help = help
        self.height = height
        self.action = action
        padding = (Self.size.padding, Self.size.padding)
        super.init(frame: .zero)
        words.oneLine = true
        light.fill = Surface.background.platform(.row)
        light.opacity = 0
        mark.tint = Theme.foreground
        chevron.image = .symbol(.chevronDown, size: Self.size.chevron, trimmed: true)
        chevron.tint = Theme.foreground
        chevron.isHidden = true
        for view in [glass, light, mark, words, chevron] as [PlatformView] { addSubview(view) }
        focusable = true
        pointer = .hand
        onPress = { [weak self] _ in self?.press() }
        onHover = { [weak self] point in self?.lightUp(point != nil) }
        showSymbol()
        paint()
        describeSelf()
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    private var wordless: Bool { (title ?? "").isEmpty }
    private var side: CGFloat { (symbolSide * Platform.scale).rounded() }

    /// How wide it is: as tall as it is when it shows a symbol alone.
    var naturalWidth: CGFloat {
        guard !wordless else { return height }
        let text = ControlText.width(words.text)
        let mark = symbol == nil ? 0 : side + Self.size.gap
        return ceil(padding.leading + text + mark + padding.trailing)
    }

    override var focusShape: (rect: CGRect, radius: CGFloat) { (bounds, bounds.height / 2) }

    override func layoutNow() {
        glass.frame = bounds
        light.frame = bounds
        light.radius = bounds.height / 2
        let side = self.side
        let middle = ((bounds.height - side) / 2).rounded()
        guard !wordless else {
            mark.frame = CGRect(x: ((bounds.width - side) / 2).rounded(), y: middle, width: side, height: side)
            return
        }
        let textWidth = ControlText.width(words.text)
        let textHeight = ControlText.height(words.text, width: .greatestFiniteMagnitude)
        var x = padding.leading
        if symbol != nil, !symbolTrails {
            mark.frame = CGRect(x: x, y: middle, width: side, height: side)
            x += side + Self.size.gap
        }
        let end = bounds.width - padding.trailing - (symbol != nil && symbolTrails ? side + Self.size.gap : 0)
        words.frame = CGRect(x: x, y: ((bounds.height - textHeight) / 2).rounded(), width: max(0, min(textWidth, end - x)), height: textHeight)
        if symbol != nil, symbolTrails {
            mark.frame = CGRect(x: words.frame.maxX + Self.size.gap, y: middle, width: side, height: side)
        }
        let chevronSide = chevron.image?.size ?? .zero
        chevron.frame = CGRect(
            x: bounds.width - padding.trailing + Self.size.gap, y: ((bounds.height - chevronSide.height) / 2).rounded(), width: chevronSide.width,
            height: chevronSide.height)
    }

    private func showSymbol() {
        mark.isHidden = symbol == nil
        guard let symbol else { return }
        mark.image = .symbol(symbol, side: side)
    }

    private func paint() {
        words.text = ControlText.line(title ?? "", font: ControlView.font(Self.size), color: Theme.foreground)
    }

    private func lightUp(_ lit: Bool) {
        let opacity: CGFloat = lit && enabled ? 1 : 0
        guard !Platform.reducesMotion else { return light.opacity = opacity }
        light.fade(to: opacity, duration: Self.fadeDuration)
    }

    func press() {
        guard enabled else { return }
        action()
    }

    private func describeSelf() {
        let value = expanded.map { $0 ? "Expanded" : "Collapsed" }
        describe(title.flatMap { $0.isEmpty ? nil : $0 } ?? help ?? "", role: opens ? .popUp : .button, value: value)
        tip = help
    }
}
