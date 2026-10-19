import Foundation

#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// Sends what a control says while it changes, at most every `interval`, and the last of it once
/// it rests.
final class ProgramThrottle {
    private static let interval: TimeInterval = 0.12
    private var last: Date = .distantPast
    private var waiting: (() -> Void)?

    func send(_ action: @escaping () -> Void) {
        let wait = Self.interval - Date().timeIntervalSince(last)
        guard wait > 0 else {
            last = Date()
            waiting = nil
            return action()
        }
        let scheduled = waiting == nil
        waiting = action
        guard scheduled else { return }
        DispatchQueue.main.asyncAfter(deadline: .now() + wait) { [weak self] in
            guard let self, let action = waiting else { return }
            waiting = nil
            last = Date()
            action()
        }
    }

    /// Sends now, whatever waits.
    func flush(_ action: () -> Void) {
        waiting = nil
        last = Date()
        action()
    }
}

/// A node with a control the user types in or picks with: a press on it is the control's.
class ProgramFieldHolder: ProgramNodeView {}

/// Button and IconButton: the kit's button.
final class ProgramButtonView: ProgramNodeView {
    private var control: ControlView?

    override func configure(_ box: ProgramBox) {
        switch box.node.kind {
        case .button(let button):
            let control = control(size: ProgramStyle.controlSize(button.size))
            control.title = button.label
            control.glyph = button.icon?.glyph
            control.variant = ProgramStyle.variant(button.variant)
            control.tint = button.tint.flatMap(Theme.named)
            control.fills = button.block
            control.enabled = button.disabled == nil
            control.surface = box.surface
        case .iconButton(let button):
            let control = control(size: .regular)
            control.title = nil
            control.glyph = button.icon?.glyph
            control.help = button.tooltip ?? button.label
            control.variant = ProgramStyle.variant(button.variant)
            control.enabled = button.disabled == nil
            control.surface = box.surface
        default:
            break
        }
    }

    private func control(size: ControlSize) -> ControlView {
        if let control, control.size == size { return control }
        self.control?.removeFromSuperview()
        let control = ControlView(nil, size: size) { [weak self] in
            guard let self else { return }
            host?.press(nodeID)
        }
        addSubview(control)
        self.control = control
        return control
    }

    override func layoutNow() {
        control?.frame = bounds
    }
}

/// Input: the kit's field under its label. A number's field keeps what is typed until it reads as
/// one; the core has the number.
final class ProgramInputView: ProgramFieldHolder {
    private var field: FieldView?
    private let throttle = ProgramThrottle()

    override func configure(_ box: ProgramBox) {
        guard case .input(let input) = box.node.kind else { return }
        let field = self.field?.numeric == input.number ? self.field! : make(numeric: input.number)
        field.placeholder = input.placeholder ?? ""
        field.surface = box.surface
        field.enabled = input.disabled == nil
        if !field.editing { field.text = input.value }
        field.frame = box.rects["control"] ?? bounds
    }

    private func make(numeric: Bool) -> FieldView {
        self.field?.removeFromSuperview()
        let field = FieldView(placeholder: "", numeric: numeric) { [weak self] text in self?.changed(text) }
        addSubview(field)
        self.field = field
        return field
    }

    private func changed(_ text: String) {
        guard case .input(let input) = box?.node.kind else { return }
        let node = nodeID
        guard input.number else { return throttle.send { [weak self] in self?.host?.value(node, text) } }
        let trimmed = text.replacingOccurrences(of: ",", with: "").trimmingCharacters(in: .whitespaces)
        guard !trimmed.isEmpty else { return throttle.send { [weak self] in self?.host?.value(node, NSNull()) } }
        guard let number = Double(trimmed) else { return }
        throttle.send { [weak self] in self?.host?.value(node, number) }
    }
}

final class ProgramTextAreaView: ProgramFieldHolder {
    private var area: TextAreaView?
    private let throttle = ProgramThrottle()

    override func configure(_ box: ProgramBox) {
        guard case .textArea(let node) = box.node.kind else { return }
        let rows = max(2, min(20, node.rows))
        if area?.rows != rows {
            area?.removeFromSuperview()
            let area = TextAreaView(placeholder: "", rows: rows) { [weak self] text in
                guard let self else { return }
                let id = nodeID
                throttle.send { [weak self] in self?.host?.value(id, text) }
            }
            addSubview(area)
            self.area = area
        }
        guard let area else { return }
        area.placeholder = node.placeholder ?? ""
        area.surface = box.surface
        area.enabled = node.disabled == nil
        if !area.editing { area.text = node.value }
        area.frame = box.rects["control"] ?? bounds
    }
}

/// Select: the kit's select, filling the width, offering the options with a check by the chosen.
final class ProgramSelectView: ProgramFieldHolder {
    private lazy var select = SelectView(options: [], chosen: nil) { [weak self] index in self?.choose(index) }

    override init(frame: CGRect) {
        super.init(frame: frame)
        addSubview(select)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override func configure(_ box: ProgramBox) {
        guard case .select(let node) = box.node.kind else { return }
        select.options = node.options.map(\.label)
        select.chosen = node.selected
        select.placeholderText = node.placeholder
        select.surface = box.surface
        select.enabled = node.disabled == nil
        select.frame = box.rects["control"] ?? bounds
    }

    private func choose(_ index: Int) {
        guard case .select(let node) = box?.node.kind, node.options.indices.contains(index) else { return }
        host?.value(nodeID, node.options[index].value)
    }
}

/// SegmentedControl: the kit's, at its own width, moving sideways in a column narrower than it.
final class ProgramSegmentedView: ProgramFieldHolder {
    private let clip = SidewaysClipView()
    private var segments: SegmentedView?

    override init(frame: CGRect) {
        super.init(frame: frame)
        addSubview(clip)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override func configure(_ box: ProgramBox) {
        guard case .segmentedControl(let node) = box.node.kind else { return }
        let labels = node.options.map(\.label)
        let segments = self.segments ?? {
            let view = SegmentedView(labels, selection: node.selected ?? -1) { [weak self] index in self?.choose(index) }
            clip.addSubview(view)
            self.segments = view
            return view
        }()
        if segments.options != labels { segments.options = labels }
        segments.selection = node.selected ?? -1
        segments.fills = node.block
        segments.enabled = node.disabled == nil
        let own = box.extra as? CGFloat ?? box.size.width
        clip.frame = bounds
        clip.setContent(segments, size: CGSize(width: node.block ? box.size.width : own, height: box.size.height))
    }

    private func choose(_ index: Int) {
        guard case .segmentedControl(let node) = box?.node.kind, node.options.indices.contains(index) else { return }
        host?.value(nodeID, node.options[index].value)
    }
}

/// Slider: the kit's, with core's readout and marks. It keeps its own value while it is dragged.
final class ProgramSliderView: ProgramFieldHolder {
    private let slider = SliderView(value: 0, in: 0...1) { _ in }
    private let throttle = ProgramThrottle()

    override init(frame: CGRect) {
        super.init(frame: frame)
        addSubview(slider)
        slider.onChange = { [weak self] value in
            guard let self else { return }
            let id = nodeID
            throttle.send { [weak self] in self?.host?.value(id, value) }
        }
        slider.onEditingChanged = { [weak self] dragging in
            guard let self, !dragging else { return }
            let (id, value) = (nodeID, slider.value)
            throttle.flush { self.host?.value(id, value) }
        }
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override func configure(_ box: ProgramBox) {
        guard case .slider(let node) = box.node.kind else { return }
        let range = node.min...max(node.min, node.max)
        if slider.range != range { slider.range = range }
        slider.step = node.step > 0 ? node.step : nil
        slider.value = node.value
        slider.label = node.head ? node.label : nil
        slider.readout = node.head ? node.readout : nil
        let marks = node.marks.map { SliderView.Mark(value: node.min + $0.at * (node.max - node.min), title: $0.label) }
        if slider.marks.map(\.title) != marks.map(\.title) || slider.marks.map(\.value) != marks.map(\.value) { slider.marks = marks }
        slider.surface = box.surface
        slider.enabled = node.disabled == nil
        slider.frame = bounds
    }
}

final class ProgramCheckboxView: ProgramFieldHolder {
    private lazy var row = PickRowView(multiple: true) { [weak self] in self?.toggle() }

    override init(frame: CGRect) {
        super.init(frame: frame)
        addSubview(row)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override func configure(_ box: ProgramBox) {
        guard case .checkbox(let node) = box.node.kind else { return }
        row.title = PickRowView.title(node.label, struck: node.strike && node.checked)
        row.picked = node.checked
        row.enabled = node.disabled == nil
        row.frame = box.rects["control"] ?? bounds
    }

    private func toggle() {
        guard case .checkbox(let node) = box?.node.kind else { return }
        host?.value(nodeID, !node.checked)
    }
}

/// RadioGroup: a list of rows with rings, pills, or cards of choices.
final class ProgramRadioView: ProgramFieldHolder {
    private var options: [PlatformView] = []
    private var variant: UiRadioVariant?

    override func configure(_ box: ProgramBox) {
        guard case .radioGroup(let node) = box.node.kind else { return }
        let frames = box.extra as? [CGRect] ?? []
        if variant != node.variant || options.count != node.options.count {
            options.forEach { $0.removeFromSuperview() }
            options = node.options.indices.map { index in make(node.variant, index: index) }
            options.forEach(addSubview)
            variant = node.variant
        }
        let enabled = node.disabled == nil
        for (index, option) in node.options.enumerated() {
            let chosen = node.selected == index
            switch options[index] {
            case let row as PickRowView:
                row.title = PickRowView.title(option.label)
                row.detail = option.description.map(PickRowView.detail)
                row.picked = chosen
                row.enabled = enabled
            case let pill as ControlView:
                pill.title = option.label
                pill.variant = chosen ? .accent : .secondary
                pill.surface = box.surface
                pill.enabled = enabled
            case let card as OptionCardView:
                card.title = option.label
                card.detail = option.description
                card.selected = chosen
                card.surface = box.surface
                card.enabled = enabled
            default:
                break
            }
            options[index].frame = index < frames.count ? frames[index] : .zero
        }
    }

    private func make(_ variant: UiRadioVariant, index: Int) -> PlatformView {
        let choose = { [weak self] in self?.choose(index) ?? () }
        switch variant {
        case .list: return PickRowView(multiple: false, onPress: choose)
        case .pills: return ControlView(nil, variant: .secondary, size: .regular, action: choose)
        case .cards: return OptionCardView(onPress: choose)
        }
    }

    private func choose(_ index: Int) {
        guard case .radioGroup(let node) = box?.node.kind, node.options.indices.contains(index) else { return }
        host?.value(nodeID, node.options[index].value)
    }
}

/// DatePicker: the kit's date field, held to the program's first and last days.
final class ProgramDateView: ProgramFieldHolder {
    private lazy var field = DateFieldView { [weak self] day in
        guard let self else { return }
        host?.value(nodeID, day.iso)
    }

    override init(frame: CGRect) {
        super.init(frame: frame)
        addSubview(field)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override func configure(_ box: ProgramBox) {
        guard case .datePicker(let node) = box.node.kind else { return }
        field.text = node.value == nil ? nil : node.text
        field.placeholder = node.value == nil ? node.text : "Date"
        field.label = node.label
        field.day = node.value.flatMap(Day.init(iso:))
        field.earliest = node.min.flatMap(Day.init(iso:))
        field.latest = node.max.flatMap(Day.init(iso:))
        field.surface = box.surface
        field.enabled = node.disabled == nil
        field.frame = box.rects["control"] ?? bounds
    }
}

/// Timer: its label and time, and the buttons that start, pause and reset it. The core counts.
final class ProgramTimerView: ProgramFieldHolder {
    private let face = SurfaceView()
    private lazy var toggle = ControlView(nil, variant: .primary, size: .small) { [weak self] in self?.pressToggle() }
    private lazy var reset = ControlView(nil, variant: .ghost, size: .small) { [weak self] in
        guard let self else { return }
        host?.send(nodeID, ["type": "reset"])
    }

    override init(frame: CGRect) {
        super.init(frame: frame)
        face.radius = Radius.lg
        addSubview(face)
        addSubview(toggle)
        addSubview(reset)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override func configure(_ box: ProgramBox) {
        guard case .timer(let node) = box.node.kind else { return }
        face.stroke = box.surface.platformBorder
        for (control, button) in [(toggle, node.toggle), (reset, node.reset)] {
            control.title = button.label
            control.glyph = button.icon?.glyph
            control.enabled = button.disabled == nil
            control.surface = box.surface
        }
        toggle.frame = box.rects["toggle"] ?? .zero
        reset.frame = box.rects["reset"] ?? .zero
    }

    private func pressToggle() {
        guard case .timer(let node) = box?.node.kind else { return }
        host?.send(nodeID, ["type": node.running ? "pause" : "start"])
    }

    override func layoutNow() {
        face.frame = bounds
    }
}
