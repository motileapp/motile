import Charts
import SwiftUI

extension View {
    /// The time a usage chart points at: where the pointer is on the Mac, and on iOS the bar last
    /// tapped, which a tap on it again lets go, so that a drag across the chart scrolls the page.
    func chartPointing(_ pointed: Binding<Date?>, period: UsageModel.Period) -> some View {
        #if os(iOS)
        chartXSelection(value: pointed)
            .chartGesture { proxy in
                SpatialTapGesture().onEnded { tap in
                    guard let tapped: Date = proxy.value(atX: tap.location.x) else { return }
                    let same = pointed.wrappedValue.map { abs($0.timeIntervalSince(tapped)) < Double(period.bucketSeconds) / 2 } ?? false
                    pointed.wrappedValue = same ? nil : tapped
                }
            }
        #else
        chartXSelection(value: pointed)
        #endif
    }
}
