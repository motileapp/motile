import Charts
import SwiftUI

/// How a usage chart marks its buckets: bars that each span theirs, or points at their starts.
enum ChartMarks {
    case bars, points

    func index(at time: Date, in starts: [Date]) -> Int? {
        guard !starts.isEmpty else { return nil }
        switch self {
        case .bars:
            return starts.lastIndex { $0 <= time } ?? 0
        case .points:
            return starts.indices.min { abs(starts[$0].timeIntervalSince(time)) < abs(starts[$1].timeIntervalSince(time)) }
        }
    }
}

extension View {
    /// The bucket a usage chart points at: where the pointer is on the Mac. On iOS where a finger
    /// slides sideways across it, or the one tapped, which a tap on it again lets go.
    func chartPointing(_ pointed: Binding<Date?>, starts: [Date], marks: ChartMarks) -> some View {
        #if os(iOS)
        chartOverlay { proxy in
            GeometryReader { geometry in
                let plotX = proxy.plotFrame.map { geometry[$0].minX } ?? 0
                let bucket = { (x: CGFloat) -> Date? in
                    guard let time: Date = proxy.value(atX: x - plotX) else { return nil }
                    return marks.index(at: time, in: starts).map { starts[$0] }
                }
                Color.clear
                    .contentShape(Rectangle())
                    .onTapGesture(coordinateSpace: .local) { location in
                        guard let tapped = bucket(location.x) else { return }
                        pointed.wrappedValue = pointed.wrappedValue == tapped ? nil : tapped
                    }
                    .gesture(ScrubPan { location in
                        guard let slid = bucket(location.x) else { return }
                        pointed.wrappedValue = slid
                    })
            }
        }
        .sensoryFeedback(.selection, trigger: pointed.wrappedValue) { _, new in new != nil }
        #else
        chartXSelection(value: pointed)
        #endif
    }
}
