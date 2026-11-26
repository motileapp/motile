#if os(iOS)
import SwiftUI
import UIKit

/// The tokens the widgets draw with, from the generated SharedTokens.
public enum WidgetTheme {
    public static let background = tone(SharedTokens.background)
    public static let foreground = tone(SharedTokens.foreground)
    public static let mutedForeground = tone(SharedTokens.mutedForeground)
    public static let mutedStrongerForeground = tone(SharedTokens.mutedStrongerForeground)
    public static let process = tone(SharedTokens.process)
    public static let warning = tone(SharedTokens.warning)
    public static let success = tone(SharedTokens.success)
    public static let destructive = tone(SharedTokens.destructive)
    /// The `disabled` opacity, for what may no longer be so.
    public static let disabled = Double(SharedTokens.disabledOpacity.dark)
    /// How much larger than on the Mac things are on iOS, as `Platform.scale` says.
    static let scale: CGFloat = 1.14

    public typealias TextSize = SharedTextSize

    public static func color(of state: WorkState) -> Color {
        switch state {
        case .waiting: warning
        case .working: process
        case .finished: success
        case .failed: destructive
        case .other: mutedForeground
        }
    }

    private static func tone(_ token: (light: UInt32, dark: UInt32)) -> Color {
        Color(UIColor { $0.userInterfaceStyle == .dark ? rgb(token.dark) : rgb(token.light) })
    }

    private static func rgb(_ hex: UInt32) -> UIColor {
        UIColor(
            red: CGFloat((hex >> 16) & 0xff) / 255, green: CGFloat((hex >> 8) & 0xff) / 255, blue: CGFloat(hex & 0xff) / 255, alpha: 1
        )
    }
}

extension SharedTextSize {
    public var points: CGFloat { rawValue * WidgetTheme.scale }
}

extension Font {
    public static func widget(_ size: WidgetTheme.TextSize, weight: Font.Weight = .regular) -> Font {
        .system(size: size.points, weight: weight)
    }
}
#endif
