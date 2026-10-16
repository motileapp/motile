import CoreText
import SwiftUI

/// The icons the apps draw, from Lucide's font: each is the character it has there.
enum Symbol: String {
    case arrowDown = "\u{e042}"
    case arrowLeft = "\u{e048}"
    case arrowRight = "\u{e049}"
    case arrowUp = "\u{e04a}"
    case blocks = "\u{e4fa}"
    case bookMarked = "\u{e3f1}"
    case bot = "\u{e1bb}"
    case botMessageSquare = "\u{e5ce}"
    case box = "\u{e061}"
    case braces = "\u{e36a}"
    case brain = "\u{e3c6}"
    case camera = "\u{e064}"
    case chartColumn = "\u{e2a3}"
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
    case circleDotDashed = "\u{e4b1}"
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
    /// The reasoning efforts up to high: a ring round a pie a quarter, half or three quarters
    /// full, which Lucide doesn't have.
    case effortLow = "effortLow"
    case effortMedium = "effortMedium"
    case effortHigh = "effortHigh"
    case ellipsis = "\u{e0b6}"
    case eye = "\u{e0ba}"
    case eyeOff = "\u{e0bb}"
    case file = "\u{e0c0}"
    case fileText = "\u{e0cc}"
    case foldVertical = "\u{e43c}"
    case flag = "\u{e0d1}"
    case folder = "\u{e0d7}"
    case folderGit2 = "\u{e40a}"
    case folderPlus = "\u{e0d9}"
    case gauge = "\u{e1bf}"
    case gitBranch = "\u{e0e2}"
    case gitCommitHorizontal = "\u{e0e3}"
    case gitMerge = "\u{e0e4}"
    case gitPullRequest = "\u{e0e5}"
    case gitPullRequestClosed = "\u{e35a}"
    case gitPullRequestCreate = "\u{e556}"
    case gitPullRequestDraft = "\u{e35b}"
    case globe = "\u{e0e8}"
    case hourglass = "\u{e296}"
    case fileBraces = "\u{e36b}"
    case image = "\u{e0f6}"
    case images = "\u{e5c4}"
    case keyboard = "\u{e284}"
    case layers = "\u{e529}"
    /// Linear's logo, which Lucide doesn't have.
    case linear = "linear"
    case link = "\u{e102}"
    case list = "\u{e106}"
    case listChecks = "\u{e1d0}"
    case listFilter = "\u{e460}"
    case loader = "\u{e109}"
    case lockOpen = "\u{e10c}"
    case logOut = "\u{e10e}"
    case maximize2 = "\u{e113}"
    case menu = "\u{e115}"
    case messageCircleQuestionMark = "\u{e568}"
    case messageSquareDashed = "\u{e40b}"
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
    /// Ricrac's logo, which Lucide doesn't have.
    case ricrac = "ricrac"
    case rocket = "\u{e286}"
    case rotateCw = "\u{e149}"
    case search = "\u{e151}"
    case server = "\u{e153}"
    case settings = "\u{e154}"
    case share = "\u{e155}"
    case shield = "\u{e158}"
    case signalHigh = "\u{e260}"
    case signalLow = "\u{e261}"
    case signalMedium = "\u{e262}"
    case slidersHorizontal = "\u{e29a}"
    case smilePlus = "\u{e301}"
    case sparkle = "\u{e47e}"
    case sparkles = "\u{e412}"
    case square = "\u{e167}"
    case squareArrowOutUpRight = "\u{e5a4}"
    case squareCheck = "\u{e559}"
    case squarePen = "\u{e172}"
    case squarePlus = "\u{e173}"
    case terminal = "\u{e181}"
    case ticket = "\u{e20f}"
    case trash2 = "\u{e18e}"
    case trendingDown = "\u{e190}"
    case trendingUp = "\u{e191}"
    case triangleAlert = "\u{e193}"
    case undo2 = "\u{e2a1}"
    case unlink = "\u{e19c}"
    case unfoldVertical = "\u{e43e}"
    case userRound = "\u{e468}"
    case users = "\u{e1a4}"
    case wrench = "\u{e1b1}"
    case x = "\u{e1b2}"
    case zap = "\u{e1b4}"

    /// An arrow that goes round, which turns in place of the spinner while it waits.
    var turns: Bool { self == .refreshCw || self == .rotateCw }
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

    /// The side of the square a symbol for text of `size` is drawn in.
    static func symbolSide(_ size: CGFloat) -> CGFloat {
        (size * Platform.scale * symbolScale).rounded()
    }

    /// The symbol in one colour that a view tints, sized for text of `size`. Each is made once.
    /// `trimmed` leaves out the empty room Lucide draws around it, for one like a chevron that
    /// fills little of its square.
    static func symbol(_ symbol: Symbol, size: CGFloat, trimmed: Bool = false) -> PlatformImage {
        let side = symbolSide(size)
        let key = "\(symbol.rawValue)/\(side)/\(trimmed)"
        symbolLock.lock()
        defer { symbolLock.unlock() }
        if let made = symbols[key] { return made }
        let made = drawn(symbol, side: side, trimmed: trimmed)
        symbols[key] = made
        return made
    }

    /// The symbol's outline for text of `size`, in its square with the origin at the bottom
    /// left, for a layer that draws it itself.
    static func symbolPath(_ symbol: Symbol, size: CGFloat) -> CGPath? {
        glyph(symbol, side: symbolSide(size))
    }

    private static func glyph(_ symbol: Symbol, side: CGFloat) -> CGPath? {
        guard let symbolFont else { return nil }
        let font = CTFontCreateWithFontDescriptor(symbolFont, side, nil)
        var character = Array(symbol.rawValue.utf16)
        var glyph = CGGlyph()
        guard CTFontGetGlyphsForCharacters(font, &character, &glyph, 1) else { return nil }
        return CTFontCreatePathForGlyph(font, glyph, nil)
    }

    /// A disc cut into stripes that fall to the right, in a square of 24 with the room Lucide
    /// leaves around its icons. A stripe is where `x + y` lies between two numbers.
    private static func linearLogo(side: CGFloat) -> CGPath {
        let stripes: [(CGFloat, CGFloat)] = [(22.706, 72), (17.674, 20.191), (12.642, 15.159), (-24, 10.127)]
        let disc = CGPath(ellipseIn: CGRect(x: 0, y: 0, width: 24, height: 24), transform: nil)
        let far: CGFloat = 48
        let logo = CGMutablePath()
        for (from, to) in stripes {
            let stripe = CGMutablePath()
            stripe.addLines(between: [
                CGPoint(x: from / 2 + far, y: from / 2 - far), CGPoint(x: to / 2 + far, y: to / 2 - far),
                CGPoint(x: to / 2 - far, y: to / 2 + far), CGPoint(x: from / 2 - far, y: from / 2 + far)
            ])
            stripe.closeSubpath()
            logo.addPath(disc.intersection(stripe))
        }
        let scale = side / 24 * 20 / 24
        var placed = CGAffineTransform(translationX: side / 2, y: side / 2).scaledBy(x: scale, y: scale).translatedBy(x: -12, y: -12)
        return logo.copy(using: &placed) ?? logo
    }

    /// Three half rings that open to the right, three bars, and three half rings that open to the
    /// left, in a glyph of 20 by 12 placed in a square of 24.
    private static func ricracLogo(side: CGFloat) -> CGPath {
        let mark = CGMutablePath()
        for radius: CGFloat in [5.5, 3.5, 1.5] {
            mark.move(to: CGPoint(x: 6, y: 6 + radius))
            mark.addArc(center: CGPoint(x: 6, y: 6), radius: radius, startAngle: .pi / 2, endAngle: .pi * 3 / 2, clockwise: false)
            mark.move(to: CGPoint(x: 14, y: 6 - radius))
            mark.addArc(center: CGPoint(x: 14, y: 6), radius: radius, startAngle: -.pi / 2, endAngle: .pi / 2, clockwise: false)
        }
        for x: CGFloat in [8, 10, 12] {
            mark.move(to: CGPoint(x: x, y: 0.5))
            mark.addLine(to: CGPoint(x: x, y: 11.5))
        }
        let logo = mark.copy(strokingWithWidth: 1, lineCap: .round, lineJoin: .round, miterLimit: 10)
        var placed = CGAffineTransform(scaleX: side / 24, y: side / 24).translatedBy(x: 2, y: 6)
        return logo.copy(using: &placed) ?? logo
    }

    /// A ring as Lucide draws a circle, round a pie that fills `share` of it clockwise from the
    /// top, in a square of 24.
    private static func gauge(_ share: CGFloat, side: CGFloat) -> CGPath {
        let center = CGPoint(x: 12, y: 12)
        let ring = CGPath(ellipseIn: CGRect(x: 2, y: 2, width: 20, height: 20), transform: nil)
        let path = CGMutablePath()
        path.addPath(ring.copy(strokingWithWidth: 2, lineCap: .butt, lineJoin: .miter, miterLimit: 10))
        path.move(to: center)
        path.addArc(center: center, radius: 6, startAngle: .pi / 2, endAngle: .pi / 2 - share * 2 * .pi, clockwise: true)
        path.closeSubpath()
        var scaled = CGAffineTransform(scaleX: side / 24, y: side / 24)
        return path.copy(using: &scaled) ?? path
    }

    private static func outline(_ symbol: Symbol, side: CGFloat) -> CGPath? {
        switch symbol {
        case .linear: linearLogo(side: side)
        case .ricrac: ricracLogo(side: side)
        case .effortLow: gauge(0.25, side: side)
        case .effortMedium: gauge(0.5, side: side)
        case .effortHigh: gauge(0.75, side: side)
        default: glyph(symbol, side: side)
        }
    }

    private static func drawn(_ symbol: Symbol, side: CGFloat, trimmed: Bool) -> PlatformImage {
        guard let path = outline(symbol, side: side) else { return PlatformImage() }
        let box = trimmed ? path.boundingBoxOfPath.integral : CGRect(x: 0, y: 0, width: side, height: side)
        #if os(macOS)
        let image = NSImage(size: box.size, flipped: false) { _ in
            guard let context = NSGraphicsContext.current?.cgContext else { return false }
            context.translateBy(x: -box.minX, y: -box.minY)
            context.setFillColor(.black)
            context.addPath(path)
            context.fillPath()
            return true
        }
        image.isTemplate = true
        return image
        #else
        let image = UIGraphicsImageRenderer(size: box.size).image { renderer in
            let context = renderer.cgContext
            context.translateBy(x: 0, y: box.height)
            context.scaleBy(x: 1, y: -1)
            context.translateBy(x: -box.minX, y: -box.minY)
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
    init(_ symbol: Symbol, size: CGFloat, trimmed: Bool = false) {
        self = Image(platform: .symbol(symbol, size: size, trimmed: trimmed)).renderingMode(.template)
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
