import SwiftUI

/// Where a slider's thumb stands and which value a point of its track means, the same for
/// `Slider` and the transcript's `SliderView`.
enum SliderTrack {
    static let thumb: CGFloat = 18
    /// How far the thumb's centre is from the track's ends at the least and the most.
    static let reach: CGFloat = 12
    static let marksHeight: CGFloat = 18
    /// How far the marks reach up into the control.
    static let marksOverlap: CGFloat = 2
    /// The least room between the label and the value it reads.
    static let headGap: CGFloat = 12
    static var controlHeight: CGFloat { ControlSize.large.height }
    static var trackHeight: CGFloat { ControlSize.small.height }

    static func fraction(_ value: Double, in range: ClosedRange<Double>) -> Double {
        let span = range.upperBound - range.lowerBound
        guard span > 0 else { return 0 }
        return min(max((value - range.lowerBound) / span, 0), 1)
    }

    static func center(_ fraction: Double, width: CGFloat) -> CGFloat {
        reach + (width - 2 * reach) * fraction
    }

    /// Where a mark's title stands, and which of its points stands there: the first one starts at
    /// the track's start and the last one ends at its end when they are that near them.
    static func place(_ fraction: Double, first: Bool, last: Bool, width: CGFloat) -> (x: CGFloat, anchor: CGFloat) {
        if first, fraction <= 0.02 { return (0, 0) }
        if last, fraction >= 0.98 { return (width, 1) }
        return (center(fraction, width: width), 0.5)
    }

    static func value(at x: CGFloat, width: CGFloat, in range: ClosedRange<Double>, step: Double?) -> Double {
        let fraction = width > 2 * reach ? min(max(Double((x - reach) / (width - 2 * reach)), 0), 1) : 0
        return snapped(range.lowerBound + fraction * (range.upperBound - range.lowerBound), in: range, step: step)
    }

    static func stepped(_ value: Double, by steps: Int, in range: ClosedRange<Double>, step: Double?) -> Double {
        snapped(value + Double(steps) * size(of: step, in: range), in: range, step: step)
    }

    static func snapped(_ value: Double, in range: ClosedRange<Double>, step: Double?) -> Double {
        let size = size(of: step, in: range)
        let steps = ((value - range.lowerBound) / size).rounded()
        return min(max(range.lowerBound + steps * size, range.lowerBound), range.upperBound)
    }

    /// A step, or a hundredth of the range without one.
    private static func size(of step: Double?, in range: ClosedRange<Double>) -> Double {
        if let step, step > 0 { return step }
        let span = range.upperBound - range.lowerBound
        return span > 0 ? span / 100 : 1
    }
}

/// A value between two ends, dragged along a round track: its label and the value it reads above
/// it, its marks under it. It keeps its own value while it is dragged and says each one it takes;
/// the arrow keys step it.
struct Slider: View {
    struct Mark {
        let value: Double
        let title: String
    }

    private let value: Double
    private let range: ClosedRange<Double>
    private let step: Double?
    private let label: String?
    private let readout: String?
    private let marks: [Mark]
    private let onEditingChanged: (Bool) -> Void
    private let onChange: (Double) -> Void
    @State private var dragged: Double?
    @State private var hovering = false
    @Environment(\.isEnabled) private var enabled
    @Environment(\.surface) private var surface

    init(
        value: Double, in range: ClosedRange<Double>, step: Double? = nil, label: String? = nil, readout: String? = nil, marks: [Mark] = [],
        onEditingChanged: @escaping (Bool) -> Void = { _ in }, onChange: @escaping (Double) -> Void
    ) {
        self.value = value
        self.range = range
        self.step = step
        self.label = label
        self.readout = readout
        self.marks = marks
        self.onEditingChanged = onEditingChanged
        self.onChange = onChange
    }

    private var shown: Double { dragged ?? value }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if label != nil || readout != nil { head }
            control
            if !marks.isEmpty {
                marksRow.padding(.top, -SliderTrack.marksOverlap)
            }
        }
        .opacity(.disabled, when: !enabled)
        .onChange(of: value) { dragged = nil }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(label ?? "")
        .accessibilityValue(readout ?? String(shown))
        .accessibilityAdjustableAction { direction in
            switch direction {
            case .increment: nudge(1)
            case .decrement: nudge(-1)
            @unknown default: break
            }
        }
    }

    private var head: some View {
        HStack(alignment: .firstTextBaseline, spacing: SliderTrack.headGap) {
            if let label {
                Text(label)
                    .lines(scaled(20), font: .ui(13))
                    .foregroundStyle(Color.themeForeground)
            }
            Spacer(minLength: 0)
            if let readout {
                Text(readout)
                    .lines(scaled(18), font: .uiDigits(12, weight: .medium))
                    .lineLimit(1)
                    .foregroundStyle(Color.themeForeground)
                    .fixedSize()
            }
        }
    }

    private var control: some View {
        GeometryReader { proxy in
            let width = proxy.size.width
            let lit = hovering || dragged != nil
            ZStack(alignment: .leading) {
                Capsule()
                    .fill(surface.color(lit ? .controlLit : .control))
                    .frame(height: SliderTrack.trackHeight)
                Circle()
                    .fill(Color.themeForeground)
                    .frame(width: SliderTrack.thumb, height: SliderTrack.thumb)
                    .offset(x: SliderTrack.center(SliderTrack.fraction(shown, in: range), width: width) - SliderTrack.thumb / 2)
            }
            .frame(maxHeight: .infinity)
            .contentShape(Rectangle())
            .gesture(drag(width: width), including: enabled ? .all : .none)
        }
        .frame(height: SliderTrack.controlHeight)
        .onHover { hovering = $0 && enabled }
        .focusable(enabled)
        .onKeyPress(keys: [.leftArrow, .downArrow, .rightArrow, .upArrow, .home, .end]) { press in
            switch press.key {
            case .leftArrow, .downArrow: nudge(-1)
            case .rightArrow, .upArrow: nudge(1)
            case .home: take(range.lowerBound)
            default: take(range.upperBound)
            }
            return .handled
        }
    }

    private func drag(width: CGFloat) -> some Gesture {
        DragGesture(minimumDistance: 0)
            .onChanged { drag in
                if dragged == nil { onEditingChanged(true) }
                take(SliderTrack.value(at: drag.location.x, width: width, in: range, step: step))
            }
            .onEnded { _ in onEditingChanged(false) }
    }

    private var marksRow: some View {
        ZStack(alignment: .leading) {
            ForEach(marks.indices, id: \.self) { index in
                mark(at: index)
            }
        }
        .frame(maxWidth: .infinity, minHeight: SliderTrack.marksHeight, maxHeight: SliderTrack.marksHeight, alignment: .leading)
    }

    private func mark(at index: Int) -> some View {
        GeometryReader { proxy in
            let fraction = SliderTrack.fraction(marks[index].value, in: range)
            let (x, anchor) = SliderTrack.place(fraction, first: index == 0, last: index == marks.count - 1, width: proxy.size.width)
            Text(marks[index].title)
                .lines(scaled(14), font: .uiDigits(11))
                .lineLimit(1)
                .foregroundStyle(Color.themeMutedForeground)
                .fixedSize()
                .alignmentGuide(.leading) { size in anchor * size.width - x }
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .leading)
        }
    }

    private func nudge(_ steps: Int) {
        take(SliderTrack.stepped(shown, by: steps, in: range, step: step))
    }

    private func take(_ new: Double) {
        guard enabled, new != shown else { return }
        dragged = new
        onChange(new)
    }
}

extension Text {
    /// The text in the font with its lines `height` apart, as the clients all set them.
    func lines(_ height: CGFloat, font: PlatformFont) -> some View {
        let lead = max(0, height - font.textLineHeight)
        return self.font(Font(font as CTFont)).lineSpacing(lead).padding(.vertical, lead / 2)
    }
}
