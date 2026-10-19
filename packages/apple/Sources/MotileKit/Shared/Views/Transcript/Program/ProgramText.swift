import Foundation

#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// Sets a program's text as the core resolved it: its style, its runs of bold, italic, code,
/// struck and linked text, on lines of a fixed height. Safe to call from any thread.
enum ProgramText {
    struct Options {
        var weight: UiWeight?
        var italic = false
        var strike = false
        var digits = false
        var alignment = NSTextAlignment.natural
        /// Cut short at the end of the line rather than wrapped.
        var oneLine = false
    }

    private static let bold = 1
    private static let italicBit = 2
    private static let code = 4
    private static let struck = 8

    static func set(_ rich: UiRich, look: ProgramStyle.Look, color: PlatformColor, options: Options = Options()) -> NSAttributedString {
        let base = font(look.font, options: options)
        let result = NSMutableAttributedString(
            string: rich.text,
            attributes: [.font: base, .foregroundColor: color, .paragraphStyle: paragraph(look.lineHeight, options: options)])
        if options.strike {
            result.addAttribute(.strikethroughStyle, value: NSUnderlineStyle.single.rawValue, range: NSRange(location: 0, length: result.length))
        }
        let length = result.length
        for run in rich.runs where run.count == 3 {
            let range = NSRange(location: run[0], length: run[1])
            guard range.location >= 0, range.length > 0, NSMaxRange(range) <= length else { continue }
            let style = run[2]
            if style & code != 0 {
                let mono = PlatformFont.uiMono(12.5, weight: style & bold != 0 ? .semibold : .regular)
                result.addAttributes([.font: mono, .foregroundColor: Theme.foreground, .backgroundColor: Typesetter.inlineCodeBackground], range: range)
            } else if style & (bold | italicBit) != 0 {
                var font = style & bold != 0 ? PlatformFont.systemFont(ofSize: base.pointSize, weight: .semibold) : base
                if options.digits || style & bold != 0 && base.isDigits { font = font.digits }
                if style & italicBit != 0 { font = font.italicised }
                result.addAttribute(.font, value: font, range: range)
                if style & bold != 0 { result.addAttribute(.foregroundColor, value: Theme.foreground, range: range) }
            }
            if style & struck != 0 {
                result.addAttribute(.strikethroughStyle, value: NSUnderlineStyle.single.rawValue, range: range)
            }
        }
        for link in rich.links {
            let range = NSRange(location: link.start, length: link.len)
            guard range.location >= 0, range.length > 0, NSMaxRange(range) <= length, let url = URL(string: link.url) else { continue }
            result.addAttributes([.link: url, .foregroundColor: Theme.primary], range: range)
        }
        return result
    }

    static func plain(_ text: String, look: ProgramStyle.Look, color: PlatformColor, options: Options = Options()) -> NSAttributedString {
        set(UiRich(text: text), look: look, color: color, options: options)
    }

    static func paragraph(_ lineHeight: CGFloat, options: Options) -> NSParagraphStyle {
        let style = NSMutableParagraphStyle()
        style.minimumLineHeight = lineHeight
        style.maximumLineHeight = lineHeight
        style.alignment = options.alignment
        style.lineBreakMode = options.oneLine ? .byTruncatingTail : .byWordWrapping
        return style
    }

    private static func font(_ font: PlatformFont, options: Options) -> PlatformFont {
        var result = font
        if let weight = options.weight {
            result = .systemFont(ofSize: font.pointSize, weight: weight == .semibold ? .semibold : weight == .medium ? .medium : .regular)
        }
        if options.digits || font.isDigits { result = result.digits }
        if options.italic { result = result.italicised }
        return result
    }

    /// How tall the text is wrapped at `width`, cut at `maxLines` lines.
    static func height(_ text: NSAttributedString, width: CGFloat, lineHeight: CGFloat, maxLines: Int? = nil) -> CGFloat {
        guard text.length > 0 else { return 0 }
        let height = ControlText.height(text, width: width)
        guard let maxLines, maxLines > 0 else { return height }
        return min(height, CGFloat(maxLines) * lineHeight)
    }

    /// How wide the text is with every paragraph on one line.
    static func width(_ text: NSAttributedString) -> CGFloat {
        guard text.length > 0 else { return 0 }
        return ControlText.width(text)
    }
}

extension UiRich {
    init(text: String) {
        self.init(json: ["text": text])
    }
}

extension PlatformFont {
    /// The font with figures of one width.
    var digits: PlatformFont {
        #if os(macOS)
        let feature: [NSFontDescriptor.FeatureKey: Int] = [.typeIdentifier: kNumberSpacingType, .selectorIdentifier: kMonospacedNumbersSelector]
        let descriptor = fontDescriptor.addingAttributes([.featureSettings: [feature]])
        return NSFont(descriptor: descriptor, size: pointSize) ?? self
        #else
        let feature: [UIFontDescriptor.FeatureKey: Int] = [.type: kNumberSpacingType, .selector: kMonospacedNumbersSelector]
        return UIFont(descriptor: fontDescriptor.addingAttributes([.featureSettings: [feature]]), size: pointSize)
        #endif
    }

    var isDigits: Bool {
        let settings = fontDescriptor.object(forKey: .featureSettings) as? [[AnyHashable: Any]] ?? []
        return !settings.isEmpty
    }
}
