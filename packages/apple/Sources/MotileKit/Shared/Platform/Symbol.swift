import CoreText
import SwiftUI

/// The icons the apps draw, from Lucide's font: each is the character it has there.
enum Symbol: String {
    case arrowDown = "\u{e042}"
    case arrowLeft = "\u{e048}"
    case arrowUp = "\u{e04a}"
    case bookMarked = "\u{e3f1}"
    case braces = "\u{e36a}"
    case brain = "\u{e3c6}"
    case camera = "\u{e064}"
    case check = "\u{e06c}"
    case chevronDown = "\u{e06d}"
    case chevronLeft = "\u{e06e}"
    case chevronRight = "\u{e06f}"
    case chevronUp = "\u{e070}"
    case circle = "\u{e076}"
    case circleAlert = "\u{e077}"
    case circleArrowDown = "\u{e078}"
    case circleCheck = "\u{e226}"
    case circleDashed = "\u{e4b0}"
    case circleDot = "\u{e345}"
    case circlePlay = "\u{e080}"
    case circleQuestionMark = "\u{e082}"
    case circleStop = "\u{e083}"
    case circleUser = "\u{e461}"
    case circleX = "\u{e084}"
    case clipboardList = "\u{e086}"
    case clock = "\u{e087}"
    case cloudDownload = "\u{e089}"
    case cloudUpload = "\u{e091}"
    case code = "\u{e093}"
    case command = "\u{e09a}"
    case copy = "\u{e09e}"
    case cornerLeftUp = "\u{e0a4}"
    case diff = "\u{e30c}"
    case download = "\u{e0b2}"
    case eye = "\u{e0ba}"
    case file = "\u{e0c0}"
    case fileText = "\u{e0cc}"
    case foldVertical = "\u{e43c}"
    case folder = "\u{e0d7}"
    case folderGit2 = "\u{e40a}"
    case folderPlus = "\u{e0d9}"
    case gitBranch = "\u{e0e2}"
    case gitCommitHorizontal = "\u{e0e3}"
    case gitMerge = "\u{e0e4}"
    case gitPullRequest = "\u{e0e5}"
    case gitPullRequestClosed = "\u{e35a}"
    case gitPullRequestCreate = "\u{e556}"
    case gitPullRequestDraft = "\u{e35b}"
    case globe = "\u{e0e8}"
    case image = "\u{e0f6}"
    case images = "\u{e5c4}"
    case listChecks = "\u{e1d0}"
    case lock = "\u{e10b}"
    case lockOpen = "\u{e10c}"
    case logOut = "\u{e10e}"
    case maximize2 = "\u{e113}"
    case menu = "\u{e115}"
    case messageCircleQuestionMark = "\u{e568}"
    case messageSquareText = "\u{e575}"
    case minimize2 = "\u{e11b}"
    case panelLeft = "\u{e12a}"
    case panelRight = "\u{e431}"
    case paperclip = "\u{e12d}"
    case pencil = "\u{e1f9}"
    case pencilLine = "\u{e4f0}"
    case play = "\u{e13c}"
    case plus = "\u{e13d}"
    case refreshCw = "\u{e145}"
    case rotateCw = "\u{e149}"
    case search = "\u{e151}"
    case server = "\u{e153}"
    case settings = "\u{e154}"
    case share = "\u{e155}"
    case sparkle = "\u{e47e}"
    case sparkles = "\u{e412}"
    case square = "\u{e167}"
    case squareArrowOutUpRight = "\u{e5a4}"
    case squareCheck = "\u{e559}"
    case squarePen = "\u{e172}"
    case squarePlus = "\u{e173}"
    case terminal = "\u{e181}"
    case triangleAlert = "\u{e193}"
    case undo2 = "\u{e2a1}"
    case unfoldVertical = "\u{e43e}"
    case users = "\u{e1a4}"
    case wrench = "\u{e1b1}"
    case x = "\u{e1b2}"
}

extension PlatformImage {
    /// How much larger than the text beside it an icon's square is.
    private static let symbolScale: CGFloat = 1.2
    private static var symbols: [String: PlatformImage] = [:]
    private static let symbolLock = NSLock()

    private static let symbolFont: CTFontDescriptor? = {
        let folder = "Fonts"
        let url = Bundle.main.url(forResource: "lucide", withExtension: "ttf", subdirectory: folder)
            ?? Bundle.module.url(forResource: "lucide", withExtension: "ttf", subdirectory: folder)
        guard let url, let fonts = CTFontManagerCreateFontDescriptorsFromURL(url as CFURL) as? [CTFontDescriptor] else { return nil }
        return fonts.first
    }()

    /// The symbol in one colour that a view tints, sized for text of `size`. Each is made once.
    static func symbol(_ symbol: Symbol, size: CGFloat) -> PlatformImage {
        let side = (size * Platform.scale * symbolScale).rounded()
        let key = "\(symbol.rawValue)/\(side)"
        symbolLock.lock()
        defer { symbolLock.unlock() }
        if let made = symbols[key] { return made }
        let made = drawn(symbol, side: side)
        symbols[key] = made
        return made
    }

    private static func drawn(_ symbol: Symbol, side: CGFloat) -> PlatformImage {
        guard let symbolFont else { return PlatformImage() }
        let font = CTFontCreateWithFontDescriptor(symbolFont, side, nil)
        var character = Array(symbol.rawValue.utf16)
        var glyph = CGGlyph()
        guard CTFontGetGlyphsForCharacters(font, &character, &glyph, 1), let path = CTFontCreatePathForGlyph(font, glyph, nil) else {
            return PlatformImage()
        }
        let size = CGSize(width: side, height: side)
        #if os(macOS)
        let image = NSImage(size: size, flipped: false) { _ in
            guard let context = NSGraphicsContext.current?.cgContext else { return false }
            context.setFillColor(.black)
            context.addPath(path)
            context.fillPath()
            return true
        }
        image.isTemplate = true
        return image
        #else
        let image = UIGraphicsImageRenderer(size: size).image { renderer in
            let context = renderer.cgContext
            context.translateBy(x: 0, y: side)
            context.scaleBy(x: 1, y: -1)
            context.setFillColor(UIColor.black.cgColor)
            context.addPath(path)
            context.fillPath()
        }
        return image.withRenderingMode(.alwaysTemplate)
        #endif
    }
}

extension Image {
    /// The symbol in the colour of the text around it, sized for text of `size`.
    init(_ symbol: Symbol, size: CGFloat) {
        self = Image(platform: .symbol(symbol, size: size)).renderingMode(.template)
    }
}

extension Label where Title == Text, Icon == Image {
    init(_ title: String, symbol: Symbol, size: CGFloat = 13) {
        self.init {
            Text(title)
        } icon: {
            Image(symbol, size: size)
        }
    }
}
