import SwiftUI

#if os(macOS)
import AppKit

typealias PlatformColor = NSColor
typealias PlatformFont = NSFont
typealias PlatformImage = NSImage
typealias PlatformView = NSView
typealias PlatformEdgeInsets = NSEdgeInsets
#else
import UIKit

typealias PlatformColor = UIColor
typealias PlatformFont = UIFont
typealias PlatformImage = UIImage
typealias PlatformView = UIView
typealias PlatformEdgeInsets = UIEdgeInsets
#endif

/// What the Mac and iOS do differently, behind one name each.
enum Platform {
    #if os(macOS)
    static let name = "macos"
    /// How much larger than on the Mac text and the controls around it are.
    static let scale: CGFloat = 1
    /// What only matters under the pointer is hidden until the pointer is over it.
    static let hoverReveals = true
    /// The least a control is tall and wide for a finger to press it.
    static let minimumPress: CGFloat = 0
    #else
    static let name = "ios"
    static let scale: CGFloat = 1.14
    static let hoverReveals = false
    static let minimumPress: CGFloat = 44
    #endif

    /// What the device is called in what the user reads.
    static var device: String {
        #if os(macOS)
        "Mac"
        #else
        UIDevice.current.userInterfaceIdiom == .pad ? "iPad" : "iPhone"
        #endif
    }

    static func open(_ url: URL) {
        #if os(macOS)
        NSWorkspace.shared.open(url)
        #else
        UIApplication.shared.open(url)
        #endif
    }

    static func copy(_ text: String) {
        #if os(macOS)
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
        #else
        UIPasteboard.general.string = text
        #endif
    }

    static var reducesMotion: Bool {
        #if os(macOS)
        NSWorkspace.shared.accessibilityDisplayShouldReduceMotion
        #else
        UIAccessibility.isReduceMotionEnabled
        #endif
    }

    /// How many pixels a point of the main screen has.
    static var pixelsPerPoint: CGFloat {
        #if os(macOS)
        NSScreen.main?.backingScaleFactor ?? 2
        #else
        UITraitCollection.current.displayScale
        #endif
    }

    /// Whether the app is in front, which is when a reply counts as seen.
    static var isActive: Bool {
        #if os(macOS)
        NSApp.isActive
        #else
        UIApplication.shared.applicationState == .active
        #endif
    }

    static var becameActive: Notification.Name {
        #if os(macOS)
        NSApplication.didBecomeActiveNotification
        #else
        UIApplication.didBecomeActiveNotification
        #endif
    }

    static var deviceName: String {
        #if os(macOS)
        Foundation.Host.current().localizedName ?? "Mac"
        #else
        UIDevice.current.name
        #endif
    }

    /// Takes the keyboard away from whatever has it.
    static func endEditing() {
        #if os(macOS)
        NSApp.keyWindow?.makeFirstResponder(nil)
        #else
        UIApplication.shared.sendAction(#selector(UIResponder.resignFirstResponder), to: nil, from: nil, for: nil)
        #endif
    }

    /// The context that is being drawn into.
    static var drawing: CGContext? {
        #if os(macOS)
        NSGraphicsContext.current?.cgContext
        #else
        UIGraphicsGetCurrentContext()
        #endif
    }
}

/// A size as it is on the Mac, larger on iOS, where text is read from further and hit by fingers.
func scaled(_ value: CGFloat) -> CGFloat {
    (value * Platform.scale).rounded()
}

extension Font {
    /// The system font at a size given as it is on the Mac.
    static func ui(size: CGFloat, weight: Font.Weight = .regular, design: Font.Design = .default) -> Font {
        .system(size: size * Platform.scale, weight: weight, design: design)
    }
}

extension PlatformFont {
    static func ui(_ size: CGFloat, weight: PlatformFont.Weight = .regular) -> PlatformFont {
        .systemFont(ofSize: size * Platform.scale, weight: weight)
    }

    static func uiMono(_ size: CGFloat, weight: PlatformFont.Weight = .regular) -> PlatformFont {
        .monospacedSystemFont(ofSize: size * Platform.scale, weight: weight)
    }

    static func uiDigits(_ size: CGFloat, weight: PlatformFont.Weight = .regular) -> PlatformFont {
        .monospacedDigitSystemFont(ofSize: size * Platform.scale, weight: weight)
    }

    var italicised: PlatformFont {
        #if os(macOS)
        let descriptor = fontDescriptor.withSymbolicTraits(fontDescriptor.symbolicTraits.union(.italic))
        return NSFont(descriptor: descriptor, size: pointSize) ?? self
        #else
        guard let descriptor = fontDescriptor.withSymbolicTraits(fontDescriptor.symbolicTraits.union(.traitItalic)) else { return self }
        return UIFont(descriptor: descriptor, size: pointSize)
        #endif
    }

    /// How wide a letter of a monospaced font is.
    var letterWidth: CGFloat {
        #if os(macOS)
        maximumAdvancement.width
        #else
        ("M" as NSString).size(withAttributes: [.font: self]).width
        #endif
    }
}

extension PlatformColor {
    /// A colour that is one thing in the light and another in the dark.
    static func dynamic(_ light: PlatformColor, _ dark: PlatformColor) -> PlatformColor {
        #if os(macOS)
        NSColor(name: nil) { appearance in
            appearance.bestMatch(from: [.aqua, .darkAqua]) == .darkAqua ? dark : light
        }
        #else
        UIColor { traits in traits.userInterfaceStyle == .dark ? dark : light }
        #endif
    }
}

extension Color {
    init(platform color: PlatformColor) {
        #if os(macOS)
        self.init(nsColor: color)
        #else
        self.init(uiColor: color)
        #endif
    }
}

extension Image {
    init(platform image: PlatformImage) {
        #if os(macOS)
        self.init(nsImage: image)
        #else
        self.init(uiImage: image)
        #endif
    }
}

extension PlatformImage {
    static func symbol(_ name: String, size: CGFloat, weight: PlatformFont.Weight = .regular) -> PlatformImage? {
        #if os(macOS)
        let configuration = NSImage.SymbolConfiguration(pointSize: size, weight: weight)
        return NSImage(systemSymbolName: name, accessibilityDescription: nil)?.withSymbolConfiguration(configuration)
        #else
        let symbolWeight = UIImage.SymbolWeight(weight)
        return UIImage(systemName: name, withConfiguration: UIImage.SymbolConfiguration(pointSize: size, weight: symbolWeight))
        #endif
    }

    /// The image of a file's bytes, or nothing when they aren't an image.
    static func decoded(_ data: Data) -> PlatformImage? {
        #if os(macOS)
        NSImage(data: data)
        #else
        UIImage(data: data)
        #endif
    }
}

#if os(iOS)
extension UIImage.SymbolWeight {
    init(_ weight: UIFont.Weight) {
        switch weight {
        case .ultraLight: self = .ultraLight
        case .thin: self = .thin
        case .light: self = .light
        case .medium: self = .medium
        case .semibold: self = .semibold
        case .bold: self = .bold
        case .heavy: self = .heavy
        case .black: self = .black
        default: self = .regular
        }
    }
}
#endif

extension CGRect {
    /// Fills the rectangle with the fill colour that is set.
    func fillCurrent() {
        Platform.drawing?.fill(self)
    }
}

/// A rounded rectangle, filled or outlined in the colours that are set.
enum RoundedBox {
    static func path(_ rect: CGRect, radius: CGFloat) -> CGPath {
        CGPath(roundedRect: rect, cornerWidth: min(radius, rect.width / 2), cornerHeight: min(radius, rect.height / 2), transform: nil)
    }

    static func fill(_ rect: CGRect, radius: CGFloat) {
        guard let context = Platform.drawing, rect.width > 0, rect.height > 0 else { return }
        context.addPath(path(rect, radius: radius))
        context.fillPath()
    }

    static func stroke(_ rect: CGRect, radius: CGFloat) {
        guard let context = Platform.drawing, rect.width > 0, rect.height > 0 else { return }
        context.setLineWidth(1)
        context.addPath(path(rect, radius: radius))
        context.strokePath()
    }
}
