import SwiftUI

/// An account's windows as thin pills side by side, each filled as far as it was used.
struct LimitPills: View {
    let windows: [LimitsReport.Window]

    var body: some View {
        HStack(spacing: 12) {
            ForEach(windows) { window in
                LimitPill(usedPercent: window.usedPercent, color: window.color)
                    .accessibilityElement()
                    .accessibilityLabel(Self.description(of: window))
                    .tooltip(Self.description(of: window))
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

/// How much of something is used, as one of the account limits' bars.
struct LimitPill: View {
    private static let height: CGFloat = 4

    @Environment(\.surface) private var surface
    let usedPercent: Double
    let color: Color

    var body: some View {
        GeometryReader { pill in
            Capsule()
                .fill(surface.color(.control))
                .overlay(alignment: .leading) {
                    Capsule()
                        .fill(color)
                        .opacity(.colorTintChartStrongest)
                        .frame(width: pill.size.width * usedPercent / 100)
                }
                .clipShape(Capsule())
        }
        .frame(height: scaled(Self.height))
    }
}

extension LimitsReport.Window {
    var color: Color { tone.color }
}

extension LimitsReport.Window.Tone {
    var color: Color {
        switch self {
        case .success: .themeSuccess
        case .pending: .themePending
        case .warning: .themeWarning
        }
    }
}
