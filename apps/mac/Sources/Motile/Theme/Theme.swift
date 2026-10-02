import AppKit
import SwiftUI

/// Colours and type for the whole app. Colours are dynamic, so text built once follows the
/// appearance without being built again.
enum Theme {
    static func dynamic(_ light: NSColor, _ dark: NSColor) -> NSColor {
        NSColor(name: nil) { appearance in
            appearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua ? dark : light
        }
    }

    static func hex(_ value: UInt32, alpha: CGFloat = 1) -> NSColor {
        NSColor(
            srgbRed: CGFloat((value >> 16) & 0xff) / 255,
            green: CGFloat((value >> 8) & 0xff) / 255,
            blue: CGFloat(value & 0xff) / 255,
            alpha: alpha
        )
    }

    private static func white(_ alpha: CGFloat) -> NSColor { NSColor(srgbRed: 1, green: 1, blue: 1, alpha: alpha) }

    // Surfaces
    static let background = dynamic(hex(0xfcfcfc), hex(0x0f0f10))
    static let raised = dynamic(hex(0xffffff), hex(0x1a1a1c))
    static let bubble = dynamic(hex(0xf1f1f3), hex(0x232326))
    static let codeBackground = dynamic(hex(0xf6f6f7), hex(0x18181a))
    /// Over the blurred backdrop of the window: nearly opaque, so only a hint of it comes through.
    static let glassTint = dynamic(hex(0xfcfcfc, alpha: 0.86), hex(0x0d0d0e, alpha: 0.86))
    static let hover = dynamic(hex(0x000000, alpha: 0.045), white(0.06))
    static let selected = dynamic(hex(0x000000, alpha: 0.08), white(0.1))
    static let border = dynamic(hex(0x000000, alpha: 0.09), white(0.09))
    static let strongBorder = dynamic(hex(0x000000, alpha: 0.14), white(0.14))

    // Text
    static let text = dynamic(hex(0x27272a), hex(0xececee))
    static let prose = dynamic(hex(0x3a3a40), hex(0xd6d6da))
    static let secondary = dynamic(hex(0x71717a), hex(0x9c9ca6))
    static let tertiary = dynamic(hex(0xa1a1aa), hex(0x6c6c75))

    // Meaning
    static let primary = dynamic(hex(0x2a5bd7), hex(0x4f7cff))
    static let link = dynamic(hex(0x1d4ed8), hex(0x7aa2ff))
    static let danger = dynamic(hex(0xc62828), hex(0xff7b72))
    static let dangerBackground = dynamic(hex(0xdc2626, alpha: 0.07), hex(0xff5c5c, alpha: 0.1))
    static let warning = dynamic(hex(0xb45309), hex(0xf5b454))
    static let warningBackground = dynamic(hex(0xf59e0b, alpha: 0.1), hex(0xf59e0b, alpha: 0.12))
    static let success = dynamic(hex(0x047857), hex(0x4ade80))
    static let working = dynamic(hex(0x0284c7), hex(0x38bdf8))

    /// The colours code is highlighted with, by the core's palette index.
    static let syntax: [NSColor] = [
        text,
        dynamic(hex(0x6e7781), hex(0x8b949e)),  // comment
        dynamic(hex(0xcf222e), hex(0xff7b72)),  // keyword
        dynamic(hex(0x0a3069), hex(0xa5d6ff)),  // string
        dynamic(hex(0x0550ae), hex(0x79c0ff)),  // constant
        dynamic(hex(0x8250df), hex(0xd2a8ff)),  // function
        dynamic(hex(0x953800), hex(0xffa657)),  // type
        dynamic(hex(0x953800), hex(0xffa657)),  // variable
        dynamic(hex(0x116329), hex(0x7ee787)),  // tag
        dynamic(hex(0x0550ae), hex(0x79c0ff)),  // attribute
        dynamic(hex(0x116329), hex(0x7ee787)),  // inserted
        dynamic(hex(0xb3261e), hex(0xffa198)),  // deleted
        dynamic(hex(0x0550ae), hex(0x79c0ff)),  // heading
        dynamic(hex(0x0550ae), hex(0x79c0ff)),  // escape
        dynamic(hex(0x0550ae), hex(0x79c0ff)),  // property
        dynamic(hex(0x6e7781), hex(0x8b949e)),  // meta
    ]

    // Type
    static let proseSize: CGFloat = 14
    static let codeSize: CGFloat = 12.5
    static let proseFont = NSFont.systemFont(ofSize: proseSize)
    static let proseBold = NSFont.systemFont(ofSize: proseSize, weight: .semibold)
    static let codeFont = NSFont.monospacedSystemFont(ofSize: codeSize, weight: .regular)
    static let inlineCodeFont = NSFont.monospacedSystemFont(ofSize: 12.5, weight: .regular)
    static let smallFont = NSFont.systemFont(ofSize: 12)
    static let smallMono = NSFont.monospacedSystemFont(ofSize: 11.5, weight: .regular)
    static let proseLineHeight: CGFloat = 21
    static let codeLineHeight: CGFloat = 18

    static func heading(_ level: Int) -> NSFont {
        switch level {
        case 1: NSFont.systemFont(ofSize: 20, weight: .semibold)
        case 2: NSFont.systemFont(ofSize: 17, weight: .semibold)
        case 3: NSFont.systemFont(ofSize: 15, weight: .semibold)
        default: NSFont.systemFont(ofSize: 14, weight: .semibold)
        }
    }

    // Layout
    /// The widest the transcript and the composer get.
    static let contentWidth: CGFloat = 768
    static let contentPadding: CGFloat = 24
}

extension Color {
    static let themeBackground = Color(nsColor: Theme.background)
    static let themeRaised = Color(nsColor: Theme.raised)
    static let themeBubble = Color(nsColor: Theme.bubble)
    static let themeGlassTint = Color(nsColor: Theme.glassTint)
    static let themeHover = Color(nsColor: Theme.hover)
    static let themeSelected = Color(nsColor: Theme.selected)
    static let themeBorder = Color(nsColor: Theme.border)
    static let themeStrongBorder = Color(nsColor: Theme.strongBorder)
    static let themeText = Color(nsColor: Theme.text)
    static let themeSecondary = Color(nsColor: Theme.secondary)
    static let themeTertiary = Color(nsColor: Theme.tertiary)
    static let themePrimary = Color(nsColor: Theme.primary)
    static let themeDanger = Color(nsColor: Theme.danger)
    static let themeWarning = Color(nsColor: Theme.warning)
    static let themeSuccess = Color(nsColor: Theme.success)
    static let themeWorking = Color(nsColor: Theme.working)
}
