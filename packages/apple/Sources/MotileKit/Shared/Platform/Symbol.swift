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
    case calendar = "\u{e063}"
    case camera = "\u{e064}"
    case chartColumn = "\u{e2a3}"
    case check = "\u{e06c}"
    case chevronDown = "\u{e06d}"
    case chevronLeft = "\u{e06e}"
    case chevronRight = "\u{e06f}"
    case chevronUp = "\u{e070}"
    /// Claude's, OpenAI's and GitHub's logos, which Lucide doesn't have.
    case claude = "claude"
    case openAI = "openAI"
    case github = "github"
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
    case info = "\u{e0f9}"
    case keyboard = "\u{e284}"
    case layers = "\u{e529}"
    /// Linear's logo, which Lucide doesn't have.
    case linear = "linear"
    case link = "\u{e102}"
    case list = "\u{e106}"
    case listChecks = "\u{e1d0}"
    case listFilter = "\u{e460}"
    case loader = "\u{e109}"
    case locateFixed = "\u{e1db}"
    case lockOpen = "\u{e10c}"
    case logOut = "\u{e10e}"
    case maximize2 = "\u{e113}"
    case menu = "\u{e115}"
    case messageCircleQuestionMark = "\u{e568}"
    case messageSquareDashed = "\u{e40b}"
    case messageSquareText = "\u{e575}"
    case minimize2 = "\u{e11b}"
    case minus = "\u{e11c}"
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
    private static var symbols: [String: PlatformImage] = [:]
    private static let symbolLock = NSLock()

    private static let symbolFont: CTFontDescriptor? = {
        let folder = "Fonts"
        let url = Bundle.main.url(forResource: "lucide", withExtension: "ttf", subdirectory: folder)
            ?? Bundle.module.url(forResource: "lucide", withExtension: "ttf", subdirectory: folder)
        guard let url, let fonts = CTFontManagerCreateFontDescriptorsFromURL(url as CFURL) as? [CTFontDescriptor] else { return nil }
        return fonts.first
    }()

    /// The side of a symbol's square, given as on the Mac and enlarged on iOS.
    static func symbolSide(_ size: IconSize) -> CGFloat {
        (size.rawValue * Platform.scale).rounded()
    }

    /// The symbol in one colour that a view tints, in the square of `size`. Each is made once.
    /// `trimmed` leaves out the empty room Lucide draws around it, for one like a chevron that
    /// fills little of its square.
    static func symbol(_ symbol: Symbol, size: IconSize, trimmed: Bool = false) -> PlatformImage {
        Self.symbol(symbol, side: symbolSide(size), trimmed: trimmed)
    }

    /// The same in a square exactly `side` points wide, for one sized from what is around it.
    static func symbol(_ symbol: Symbol, side: CGFloat, trimmed: Bool = false) -> PlatformImage {
        let side = side.rounded()
        let key = "\(symbol.rawValue)/\(side)/\(trimmed)"
        symbolLock.lock()
        defer { symbolLock.unlock() }
        if let made = symbols[key] { return made }
        let made = drawn(symbol, side: side, trimmed: trimmed)
        symbols[key] = made
        return made
    }

    /// The symbol's outline in a square exactly `side` points wide, with the origin at the bottom
    /// left, for a layer that draws it itself.
    static func symbolPath(_ symbol: Symbol, side: CGFloat) -> CGPath? {
        glyph(symbol, side: side)
    }

    private static func glyph(_ symbol: Symbol, side: CGFloat) -> CGPath? {
        glyph(character: symbol.rawValue, side: side)
    }

    private static func glyph(character: String, side: CGFloat) -> CGPath? {
        guard let symbolFont else { return nil }
        let font = CTFontCreateWithFontDescriptor(symbolFont, side, nil)
        var character = Array(character.utf16)
        guard character.count == 1 else { return nil }
        var glyph = CGGlyph()
        guard CTFontGetGlyphsForCharacters(font, &character, &glyph, 1) else { return nil }
        return CTFontCreatePathForGlyph(font, glyph, nil)
    }

    /// Any of Lucide's icons by its character in the font, drawn in a square `side` wide, which a
    /// view tints. A program names icons the apps' own `Symbol` doesn't have.
    static func lucide(_ character: String, side: CGFloat) -> PlatformImage {
        let side = side.rounded()
        let key = "lucide/\(character)/\(side)"
        symbolLock.lock()
        defer { symbolLock.unlock() }
        if let made = symbols[key] { return made }
        let made = glyph(character: character, side: side).map { drawn($0, box: CGRect(x: 0, y: 0, width: side, height: side)) } ?? PlatformImage()
        symbols[key] = made
        return made
    }

    private static let claudeLogo = "m4.7144 15.9555 4.7174-2.6471.079-.2307-.079-.1275h-.2307l-.7893-.0486-2.6956-.0729-2.3375-.0971-2.2646-.1214-.5707-.1215-.5343-.7042.0546-.3522.4797-.3218.686.0608 1.5179.1032 2.2767.1578 1.6514.0972 2.4468.255h.3886l.0546-.1579-.1336-.0971-.1032-.0972L6.973 9.8356l-2.55-1.6879-1.3356-.9714-.7225-.4918-.3643-.4614-.1578-1.0078.6557-.7225.8803.0607.2246.0607.8925.686 1.9064 1.4754 2.4893 1.8336.3643.3035.1457-.1032.0182-.0728-.164-.2733-1.3539-2.4467-1.445-2.4893-.6435-1.032-.17-.6194c-.0607-.255-.1032-.4674-.1032-.7285L6.287.1335 6.6997 0l.9957.1336.419.3642.6192 1.4147 1.0018 2.2282 1.5543 3.0296.4553.8985.2429.8318.091.255h.1579v-.1457l.1275-1.706.2368-2.0947.2307-2.6957.0789-.7589.3764-.9107.7468-.4918.5828.2793.4797.686-.0668.4433-.2853 1.8517-.5586 2.9021-.3643 1.9429h.2125l.2429-.2429.9835-1.3053 1.6514-2.0643.7286-.8196.85-.9046.5464-.4311h1.0321l.759 1.1293-.34 1.1657-1.0625 1.3478-.8804 1.1414-1.2628 1.7-.7893 1.36.0729.1093.1882-.0183 2.8535-.607 1.5421-.2794 1.8396-.3157.8318.3886.091.3946-.3278.8075-1.967.4857-2.3072.4614-3.4364.8136-.0425.0304.0486.0607 1.5482.1457.6618.0364h1.621l3.0175.2247.7892.522.4736.6376-.079.4857-1.2142.6193-1.6393-.3886-3.825-.9107-1.3113-.3279h-.1822v.1093l1.0929 1.0686 2.0035 1.8092 2.5075 2.3314.1275.5768-.3218.4554-.34-.0486-2.2039-1.6575-.85-.7468-1.9246-1.621h-.1275v.17l.4432.6496 2.3436 3.5214.1214 1.0807-.17.3521-.6071.2125-.6679-.1214-1.3721-1.9246L14.38 17.959l-1.1414-1.9428-.1397.079-.674 7.2552-.3156.3703-.7286.2793-.6071-.4614-.3218-.7468.3218-1.4753.3886-1.9246.3157-1.53.2853-1.9004.17-.6314-.0121-.0425-.1397.0182-1.4328 1.9672-2.1796 2.9446-1.7243 1.8456-.4128.164-.7164-.3704.0667-.6618.4008-.5889 2.386-3.0357 1.4389-1.882.929-1.0868-.0062-.1579h-.0546l-6.3385 4.1164-1.1293.1457-.4857-.4554.0608-.7467.2307-.2429 1.9064-1.3114Z"
    private static let openAILogo = "M22.2819 9.8211a5.9847 5.9847 0 0 0-.5157-4.9108 6.0462 6.0462 0 0 0-6.5098-2.9A6.0651 6.0651 0 0 0 4.9807 4.1818a5.9847 5.9847 0 0 0-3.9977 2.9 6.0462 6.0462 0 0 0 .7427 7.0966 5.98 5.98 0 0 0 .511 4.9107 6.051 6.051 0 0 0 6.5146 2.9001A5.9847 5.9847 0 0 0 13.2599 24a6.0557 6.0557 0 0 0 5.7718-4.2058 5.9894 5.9894 0 0 0 3.9977-2.9001 6.0557 6.0557 0 0 0-.7475-7.0729zm-9.022 12.6081a4.4755 4.4755 0 0 1-2.8764-1.0408l.1419-.0804 4.7783-2.7582a.7948.7948 0 0 0 .3927-.6813v-6.7369l2.02 1.1686a.071.071 0 0 1 .038.052v5.5826a4.504 4.504 0 0 1-4.4945 4.4944zm-9.6607-4.1254a4.4708 4.4708 0 0 1-.5346-3.0137l.142.0852 4.783 2.7582a.7712.7712 0 0 0 .7806 0l5.8428-3.3685v2.3324a.0804.0804 0 0 1-.0332.0615L9.74 19.9502a4.4992 4.4992 0 0 1-6.1408-1.6464zM2.3408 7.8956a4.485 4.485 0 0 1 2.3655-1.9728V11.6a.7664.7664 0 0 0 .3879.6765l5.8144 3.3543-2.0201 1.1685a.0757.0757 0 0 1-.071 0l-4.8303-2.7865A4.504 4.504 0 0 1 2.3408 7.872zm16.5963 3.8558L13.1038 8.364 15.1192 7.2a.0757.0757 0 0 1 .071 0l4.8303 2.7913a4.4944 4.4944 0 0 1-.6765 8.1042v-5.6772a.79.79 0 0 0-.407-.667zm2.0107-3.0231l-.142-.0852-4.7735-2.7818a.7759.7759 0 0 0-.7854 0L9.409 9.2297V6.8974a.0662.0662 0 0 1 .0284-.0615l4.8303-2.7866a4.4992 4.4992 0 0 1 6.6802 4.66zM8.3065 12.863l-2.02-1.1638a.0804.0804 0 0 1-.038-.0567V6.0742a4.4992 4.4992 0 0 1 7.3757-3.4537l-.142.0805L8.704 5.459a.7948.7948 0 0 0-.3927.6813zm1.0976-2.3654l2.602-1.4998 2.6069 1.4998v2.9994l-2.5974 1.4997-2.6067-1.4997Z"
    private static let githubLogo = "M12 .297c-6.63 0-12 5.373-12 12 0 5.303 3.438 9.8 8.205 11.385.6.113.82-.258.82-.577 0-.285-.01-1.04-.015-2.04-3.338.724-4.042-1.61-4.042-1.61C4.422 18.07 3.633 17.7 3.633 17.7c-1.087-.744.084-.729.084-.729 1.205.084 1.838 1.236 1.838 1.236 1.07 1.835 2.809 1.305 3.495.998.108-.776.417-1.305.76-1.605-2.665-.3-5.466-1.332-5.466-5.93 0-1.31.465-2.38 1.235-3.22-.135-.303-.54-1.523.105-3.176 0 0 1.005-.322 3.3 1.23.96-.267 1.98-.399 3-.405 1.02.006 2.04.138 3 .405 2.28-1.552 3.285-1.23 3.285-1.23.645 1.653.24 2.873.12 3.176.765.84 1.23 1.91 1.23 3.22 0 4.61-2.805 5.625-5.475 5.92.42.36.81 1.096.81 2.22 0 1.606-.015 2.896-.015 3.286 0 .315.21.69.825.57C20.565 22.092 24 17.592 24 12.297c0-6.627-5.373-12-12-12"
    private static let linearLogo = "M2.3 14.4 9.6 21.7a10 10 0 0 1-7.3-7.3ZM2 11.4 12.6 22a10 10 0 0 0 2.6-.6L2.6 8.8a10 10 0 0 0-.6 2.6ZM3.3 7 17 20.7a10 10 0 0 0 1.9-1.4L4.7 5.1A10 10 0 0 0 3.3 7ZM5.8 3.8a10 10 0 0 1 14.4 14.4Z"
    private static let ricracLogo = "M10 0a1 1 0 0 1 1 1v22a1 1 0 1 1-2 0V1a1 1 0 0 1 1-1zm4 0a1 1 0 0 1 1 1v22a1 1 0 1 1-2 0V1a1 1 0 0 1 1-1zM5.454 1.941a1 1 0 0 1 1.092 1.676A9.991 9.991 0 0 0 2 12a9.992 9.992 0 0 0 4.546 8.384 1 1 0 0 1-1.092 1.676A11.993 11.993 0 0 1 0 12C0 7.787 2.173 4.08 5.454 1.941zm11.708.292a1 1 0 0 1 1.384-.292A11.992 11.992 0 0 1 24 12c0 4.213-2.173 7.92-5.454 10.06a1.001 1.001 0 0 1-1.092-1.676A9.992 9.992 0 0 0 22 12a9.99 9.99 0 0 0-4.546-8.383 1 1 0 0 1-.292-1.384zM5.435 7.428a1 1 0 0 1 1.64 1.144A5.968 5.968 0 0 0 6 12c0 1.276.398 2.456 1.075 3.428a1 1 0 0 1-1.64 1.144A7.968 7.968 0 0 1 4 12c0-1.698.53-3.276 1.435-4.572zm11.738-.248a1 1 0 0 1 1.392.248A7.968 7.968 0 0 1 20 12c0 1.698-.53 3.276-1.435 4.572a1 1 0 0 1-1.64-1.144A5.967 5.967 0 0 0 18 12a5.968 5.968 0 0 0-1.075-3.428 1 1 0 0 1 .248-1.392z"

    /// An SVG's path in a square of 24, drawn `share` of its square across, centred and turned
    /// for drawing with y running up. A logo is drawn as large as Lucide's icons look: one that
    /// fills its square, or is solid, is drawn smaller.
    private static func logo(_ data: String, side: CGFloat, share: CGFloat) -> CGPath {
        let path = SVGPath.path(data)
        let scale = side / 24 * share
        var placed = CGAffineTransform(translationX: side / 2, y: side / 2).scaledBy(x: scale, y: -scale).translatedBy(x: -12, y: -12)
        return path.copy(using: &placed) ?? path
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
        case .claude: logo(claudeLogo, side: side, share: 22 / 24)
        case .openAI: logo(openAILogo, side: side, share: 22 / 24)
        case .github: logo(githubLogo, side: side, share: 20 / 24)
        case .linear: logo(linearLogo, side: side, share: 0.88)
        case .ricrac: logo(ricracLogo, side: side, share: 20 / 24)
        case .effortLow: gauge(0.25, side: side)
        case .effortMedium: gauge(0.5, side: side)
        case .effortHigh: gauge(0.75, side: side)
        default: glyph(symbol, side: side)
        }
    }

    private static func drawn(_ symbol: Symbol, side: CGFloat, trimmed: Bool) -> PlatformImage {
        guard let path = outline(symbol, side: side) else { return PlatformImage() }
        return drawn(path, box: trimmed ? path.boundingBoxOfPath.integral : CGRect(x: 0, y: 0, width: side, height: side))
    }

    private static func drawn(_ path: CGPath, box: CGRect) -> PlatformImage {
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
    /// The symbol in the colour of the text around it.
    init(_ symbol: Symbol, size: IconSize, trimmed: Bool = false) {
        self = Image(platform: .symbol(symbol, size: size, trimmed: trimmed)).renderingMode(.template)
    }

    init(_ symbol: Symbol, side: CGFloat) {
        self = Image(platform: .symbol(symbol, side: side)).renderingMode(.template)
    }
}

extension Label where Title == Text, Icon == Image {
    init(_ title: String, symbol: Symbol, size: IconSize = .md) {
        self.init {
            Text(title)
        } icon: {
            Image(symbol, size: size)
        }
    }
}
