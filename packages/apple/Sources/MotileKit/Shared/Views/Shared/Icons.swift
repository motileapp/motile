import SwiftUI
import UniformTypeIdentifiers

/// The agents' logos and GitHub's, from the marks their makers publish (as collected by Simple
/// Icons).
enum AgentLogo {
    static let claude = logo(fill: "#D97757", template: false, path: "m4.7144 15.9555 4.7174-2.6471.079-.2307-.079-.1275h-.2307l-.7893-.0486-2.6956-.0729-2.3375-.0971-2.2646-.1214-.5707-.1215-.5343-.7042.0546-.3522.4797-.3218.686.0608 1.5179.1032 2.2767.1578 1.6514.0972 2.4468.255h.3886l.0546-.1579-.1336-.0971-.1032-.0972L6.973 9.8356l-2.55-1.6879-1.3356-.9714-.7225-.4918-.3643-.4614-.1578-1.0078.6557-.7225.8803.0607.2246.0607.8925.686 1.9064 1.4754 2.4893 1.8336.3643.3035.1457-.1032.0182-.0728-.164-.2733-1.3539-2.4467-1.445-2.4893-.6435-1.032-.17-.6194c-.0607-.255-.1032-.4674-.1032-.7285L6.287.1335 6.6997 0l.9957.1336.419.3642.6192 1.4147 1.0018 2.2282 1.5543 3.0296.4553.8985.2429.8318.091.255h.1579v-.1457l.1275-1.706.2368-2.0947.2307-2.6957.0789-.7589.3764-.9107.7468-.4918.5828.2793.4797.686-.0668.4433-.2853 1.8517-.5586 2.9021-.3643 1.9429h.2125l.2429-.2429.9835-1.3053 1.6514-2.0643.7286-.8196.85-.9046.5464-.4311h1.0321l.759 1.1293-.34 1.1657-1.0625 1.3478-.8804 1.1414-1.2628 1.7-.7893 1.36.0729.1093.1882-.0183 2.8535-.607 1.5421-.2794 1.8396-.3157.8318.3886.091.3946-.3278.8075-1.967.4857-2.3072.4614-3.4364.8136-.0425.0304.0486.0607 1.5482.1457.6618.0364h1.621l3.0175.2247.7892.522.4736.6376-.079.4857-1.2142.6193-1.6393-.3886-3.825-.9107-1.3113-.3279h-.1822v.1093l1.0929 1.0686 2.0035 1.8092 2.5075 2.3314.1275.5768-.3218.4554-.34-.0486-2.2039-1.6575-.85-.7468-1.9246-1.621h-.1275v.17l.4432.6496 2.3436 3.5214.1214 1.0807-.17.3521-.6071.2125-.6679-.1214-1.3721-1.9246L14.38 17.959l-1.1414-1.9428-.1397.079-.674 7.2552-.3156.3703-.7286.2793-.6071-.4614-.3218-.7468.3218-1.4753.3886-1.9246.3157-1.53.2853-1.9004.17-.6314-.0121-.0425-.1397.0182-1.4328 1.9672-2.1796 2.9446-1.7243 1.8456-.4128.164-.7164-.3704.0667-.6618.4008-.5889 2.386-3.0357 1.4389-1.882.929-1.0868-.0062-.1579h-.0546l-6.3385 4.1164-1.1293.1457-.4857-.4554.0608-.7467.2307-.2429 1.9064-1.3114Z")
    static let openAI = logo(fill: "#000000", template: true, path: "M22.2819 9.8211a5.9847 5.9847 0 0 0-.5157-4.9108 6.0462 6.0462 0 0 0-6.5098-2.9A6.0651 6.0651 0 0 0 4.9807 4.1818a5.9847 5.9847 0 0 0-3.9977 2.9 6.0462 6.0462 0 0 0 .7427 7.0966 5.98 5.98 0 0 0 .511 4.9107 6.051 6.051 0 0 0 6.5146 2.9001A5.9847 5.9847 0 0 0 13.2599 24a6.0557 6.0557 0 0 0 5.7718-4.2058 5.9894 5.9894 0 0 0 3.9977-2.9001 6.0557 6.0557 0 0 0-.7475-7.0729zm-9.022 12.6081a4.4755 4.4755 0 0 1-2.8764-1.0408l.1419-.0804 4.7783-2.7582a.7948.7948 0 0 0 .3927-.6813v-6.7369l2.02 1.1686a.071.071 0 0 1 .038.052v5.5826a4.504 4.504 0 0 1-4.4945 4.4944zm-9.6607-4.1254a4.4708 4.4708 0 0 1-.5346-3.0137l.142.0852 4.783 2.7582a.7712.7712 0 0 0 .7806 0l5.8428-3.3685v2.3324a.0804.0804 0 0 1-.0332.0615L9.74 19.9502a4.4992 4.4992 0 0 1-6.1408-1.6464zM2.3408 7.8956a4.485 4.485 0 0 1 2.3655-1.9728V11.6a.7664.7664 0 0 0 .3879.6765l5.8144 3.3543-2.0201 1.1685a.0757.0757 0 0 1-.071 0l-4.8303-2.7865A4.504 4.504 0 0 1 2.3408 7.872zm16.5963 3.8558L13.1038 8.364 15.1192 7.2a.0757.0757 0 0 1 .071 0l4.8303 2.7913a4.4944 4.4944 0 0 1-.6765 8.1042v-5.6772a.79.79 0 0 0-.407-.667zm2.0107-3.0231l-.142-.0852-4.7735-2.7818a.7759.7759 0 0 0-.7854 0L9.409 9.2297V6.8974a.0662.0662 0 0 1 .0284-.0615l4.8303-2.7866a4.4992 4.4992 0 0 1 6.6802 4.66zM8.3065 12.863l-2.02-1.1638a.0804.0804 0 0 1-.038-.0567V6.0742a4.4992 4.4992 0 0 1 7.3757-3.4537l-.142.0805L8.704 5.459a.7948.7948 0 0 0-.3927.6813zm1.0976-2.3654l2.602-1.4998 2.6069 1.4998v2.9994l-2.5974 1.4997-2.6067-1.4997Z")
    static let github = logo(fill: "#000000", template: true, path: "M12 .297c-6.63 0-12 5.373-12 12 0 5.303 3.438 9.8 8.205 11.385.6.113.82-.258.82-.577 0-.285-.01-1.04-.015-2.04-3.338.724-4.042-1.61-4.042-1.61C4.422 18.07 3.633 17.7 3.633 17.7c-1.087-.744.084-.729.084-.729 1.205.084 1.838 1.236 1.838 1.236 1.07 1.835 2.809 1.305 3.495.998.108-.776.417-1.305.76-1.605-2.665-.3-5.466-1.332-5.466-5.93 0-1.31.465-2.38 1.235-3.22-.135-.303-.54-1.523.105-3.176 0 0 1.005-.322 3.3 1.23.96-.267 1.98-.399 3-.405 1.02.006 2.04.138 3 .405 2.28-1.552 3.285-1.23 3.285-1.23.645 1.653.24 2.873.12 3.176.765.84 1.23 1.91 1.23 3.22 0 4.61-2.805 5.625-5.475 5.92.42.36.81 1.096.81 2.22 0 1.606-.015 2.896-.015 3.286 0 .315.21.69.825.57C20.565 22.092 24 17.592 24 12.297c0-6.627-5.373-12-12-12")

    private static func logo(fill: String, template: Bool, path: String) -> PlatformImage? {
        #if os(macOS)
        let svg = """
            <svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24">\
            <path fill="\(fill)" d="\(path)"/></svg>
            """
        guard let image = NSImage(data: Data(svg.utf8)) else { return nil }
        image.isTemplate = template
        return image
        #else
        let value = UInt32(fill.dropFirst(), radix: 16) ?? 0
        return SVGPath.image(path, side: 24, fill: Theme.hex(value), template: template)
        #endif
    }
}

extension Agent {
    var logo: PlatformImage? {
        switch self {
        case .claude: AgentLogo.claude
        case .codex: AgentLogo.openAI
        }
    }

    /// The logo at the size a menu shows images.
    var menuLogo: PlatformImage? { logo.map { ImageFiles.sized($0, 16) } }
}

struct AgentIcon: View {
    let agent: Agent
    var size: CGFloat = 14

    var body: some View {
        if let logo = agent.logo {
            Image(platform: logo)
                .resizable()
                .interpolation(.high)
                .frame(width: size, height: size)
        } else {
            Image(.sparkle, size: size * 0.85)
                .frame(width: size, height: size)
        }
    }
}

/// Images read from files, each read once and off the main thread.
final class ImageFiles {
    static let shared = ImageFiles()
    private let cache = NSCache<NSString, PlatformImage>()

    func cached(_ path: String?) -> PlatformImage? {
        guard let path else { return nil }
        return cache.object(forKey: path as NSString)
    }

    func load(_ path: String) async -> PlatformImage? {
        if let image = cached(path) { return image }
        let image = await Task.detached(priority: .userInitiated) {
            await Self.read(path).map(Self.filling)
        }.value
        guard let image else { return nil }
        cache.setObject(image, forKey: path as NSString)
        return image
    }

    private static func read(_ path: String) async -> PlatformImage? {
        guard let data = try? Data(contentsOf: URL(fileURLWithPath: path)) else { return nil }
        #if os(iOS)
        if SVGImage.isSVG(data) { return await SVGImage.image(of: data) }
        #endif
        return PlatformImage.decoded(data)
    }

    /// The image as a square that its picture fills: drawn at one size and without the empty
    /// margin some icons come with, so that every icon is as large as the next.
    private static func filling(_ image: PlatformImage) -> PlatformImage {
        let drawn = 256
        let side = 128
        guard image.size.width > 0, image.size.height > 0,
            let canvas = bitmapContext(side: drawn)
        else { return image }
        let fit = min(CGFloat(drawn) / image.size.width, CGFloat(drawn) / image.size.height)
        let size = CGSize(width: image.size.width * fit, height: image.size.height * fit)
        let origin = CGPoint(x: (CGFloat(drawn) - size.width) / 2, y: (CGFloat(drawn) - size.height) / 2)
        #if os(macOS)
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = NSGraphicsContext(cgContext: canvas, flipped: false)
        NSGraphicsContext.current?.imageInterpolation = .high
        image.draw(in: NSRect(origin: origin, size: size), from: .zero, operation: .sourceOver, fraction: 1)
        NSGraphicsContext.restoreGraphicsState()
        #else
        guard let pixels = image.cgImage else { return image }
        canvas.interpolationQuality = .high
        canvas.draw(pixels, in: CGRect(origin: origin, size: size))
        #endif

        guard let picture = canvas.makeImage(),
            let bounds = opaqueBounds(of: canvas, side: drawn),
            let cropped = picture.cropping(to: bounds),
            let result = bitmapContext(side: side)
        else { return image }
        let scale = min(CGFloat(side) / bounds.width, CGFloat(side) / bounds.height)
        let target = CGSize(width: bounds.width * scale, height: bounds.height * scale)
        result.interpolationQuality = .high
        result.draw(
            cropped,
            in: CGRect(x: (CGFloat(side) - target.width) / 2, y: (CGFloat(side) - target.height) / 2, width: target.width, height: target.height)
        )
        guard let filled = result.makeImage() else { return image }
        #if os(macOS)
        return NSImage(cgImage: filled, size: NSSize(width: side, height: side))
        #else
        return UIImage(cgImage: filled)
        #endif
    }

    private static func bitmapContext(side: Int) -> CGContext? {
        CGContext(
            data: nil,
            width: side,
            height: side,
            bitsPerComponent: 8,
            bytesPerRow: side * 4,
            space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
        )
    }

    /// The part of the bitmap that isn't transparent, in the coordinates `CGImage.cropping` takes.
    private static func opaqueBounds(of context: CGContext, side: Int) -> CGRect? {
        guard let data = context.data else { return nil }
        let pixels = data.bindMemory(to: UInt8.self, capacity: side * side * 4)
        var (left, right, top, bottom) = (side, -1, side, -1)
        for row in 0..<side {
            for column in 0..<side where pixels[(row * side + column) * 4 + 3] > 24 {
                left = min(left, column)
                right = max(right, column)
                top = min(top, row)
                bottom = max(bottom, row)
            }
        }
        guard right >= left, bottom >= top else { return nil }
        return CGRect(x: left, y: top, width: right - left + 1, height: bottom - top + 1)
    }

    /// Reads the images now, so that menus, which can't wait, find them.
    func warm(_ paths: [String]) {
        for path in paths where cached(path) == nil {
            Task { _ = await load(path) }
        }
    }

    /// The kinds of image that can be attached without being a file first.
    static let attachable: [UTType] = [.png, .jpeg, .tiff, .gif]

    /// Writes a pasted or dropped image to a file, which is what gets attached.
    static func saveForAttaching(_ data: Data, type: UTType) -> URL? {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent("motile-attachments", isDirectory: true)
        let formatter = DateFormatter()
        formatter.dateFormat = "yyyy-MM-dd 'at' HH.mm.ss.SSS"
        let name = "Image \(formatter.string(from: Date())).\(type.preferredFilenameExtension ?? "png")"
        let file = folder.appendingPathComponent(name)
        do {
            try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
            try data.write(to: file)
        } catch {
            return nil
        }
        return file
    }

    static func sized(_ image: PlatformImage, _ side: CGFloat) -> PlatformImage {
        #if os(macOS)
        guard let copy = image.copy() as? NSImage else { return image }
        copy.size = NSSize(width: side, height: side)
        return copy
        #else
        let size = CGSize(width: side, height: side)
        let drawn = UIGraphicsImageRenderer(size: size).image { _ in image.draw(in: CGRect(origin: .zero, size: size)) }
        return drawn.withRenderingMode(image.renderingMode)
        #endif
    }
}

/// A project's icon, or a folder when it has none.
struct ProjectIcon: View {
    let project: Project?
    var size: CGFloat = 16
    /// The icon read for a path, which a row that is reused for another project doesn't show.
    @State private var loaded: (path: String, image: PlatformImage)?

    var body: some View {
        let path = project?.iconPath
        let image = loaded.flatMap { $0.path == path ? $0.image : nil } ?? ImageFiles.shared.cached(path)
        Group {
            if let image {
                Image(platform: image)
                    .resizable()
                    .interpolation(.high)
                    .aspectRatio(contentMode: .fit)
                    .clipShape(RoundedRectangle(cornerRadius: size * 0.22, style: .continuous))
            } else {
                Image(.folder, size: size * 0.78)
                    .foregroundStyle(Color.themeSecondary)
            }
        }
        .frame(width: size, height: size)
        .task(id: project?.iconPath) {
            guard let path = project?.iconPath else {
                loaded = nil
                return
            }
            loaded = await ImageFiles.shared.load(path).map { (path, $0) }
        }
    }
}

/// The server a thread or project is on, for when there is more than one.
struct ServerLabel: View {
    let server: Server
    var size: CGFloat = 11

    var body: some View {
        HStack(spacing: 3) {
            // Lucide's server fills more of its square than the icons beside it.
            Image(.server, size: size - 1)
            Text(server.name)
                .font(.ui(size: size))
                .lineLimit(1)
        }
        .foregroundStyle(Color.themeTertiary)
        .help("On \(server.name)")
    }
}

extension PullRequest.State {
    var platformColor: PlatformColor {
        switch self {
        case .open: Theme.success
        case .draft: Theme.secondary
        case .merged: Theme.merged
        case .closed: Theme.danger
        }
    }

    var color: Color { Color(platform: platformColor) }
}

/// A thread's pull request: what became of it, and its number.
struct PullRequestLabel: View {
    let pullRequest: PullRequest
    /// In the colour of what became of it, or as quiet as the row it is in.
    var colored = true
    /// What a click does, when it is a link: its number underlines under the pointer.
    var action: (() -> Void)? = nil
    @State private var hovering = false

    var body: some View {
        if let action {
            label.button(DimButtonStyle(), action: action)
                .onHover { hovering = $0 }
        } else {
            label
        }
    }

    private var label: some View {
        HStack(spacing: 2) {
            Image(pullRequest.state.symbol, size: 11)
            Text(verbatim: "\(pullRequest.number)")
                .font(.ui(size: 11, weight: .medium))
                .monospacedDigit()
                .underline(hovering)
        }
        .foregroundStyle(colored ? pullRequest.state.color : Color.themeTertiary)
        .help(pullRequest.title)
    }
}

extension Project {
    /// The icon at the size a menu shows images, if it has been read already.
    var menuIcon: PlatformImage? {
        ImageFiles.shared.cached(iconPath).map { ImageFiles.sized($0, 16) }
    }
}
