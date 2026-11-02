import SwiftUI

/// An account's windows as thin pills side by side, each filled as far as it was used.
struct LimitPills: View {
    private static let height: CGFloat = 4

    @Environment(\.surface) private var surface
    let windows: [LimitsReport.Window]

    var body: some View {
        HStack(spacing: 12) {
            ForEach(windows) { window in
                GeometryReader { pill in
                    Capsule()
                        .fill(surface.color(.control))
                        .overlay(alignment: .leading) {
                            Capsule()
                                .fill(window.color)
                                .opacity(.colorTintChartStrongest)
                                .frame(width: pill.size.width * window.usedPercent / 100)
                        }
                        .clipShape(Capsule())
                }
                .frame(height: scaled(Self.height))
                .accessibilityElement()
                .accessibilityLabel(Self.description(of: window))
                .help(Self.description(of: window))
            }
        }
    }

    /// "Session · 34% used · resets in 3h 30m".
    private static func description(of window: LimitsReport.Window) -> String {
        [window.label, "\(window.used) used", window.resetsIn.map { "resets in \($0)" }]
            .compactMap { $0 }
            .joined(separator: " · ")
    }
}

extension LimitsReport.Window {
    var color: Color {
        switch tone {
        case .success: .themeSuccess
        case .pending: .themePending
        case .warning: .themeWarning
        }
    }
}
