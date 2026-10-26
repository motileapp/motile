import SwiftUI
import UniformTypeIdentifiers

extension Agent {
    var symbol: Symbol { self == .claude ? .claude : .openAI }

    /// The logo in its colour, for what draws images itself.
    func logo(size: IconSize) -> PlatformImage { ImageFiles.tinted(.symbol(symbol, size: size), platformColor) }

    /// The logo at the size a menu shows images, in its colour.
    var menuLogo: PlatformImage { logo(size: .md) }

    var platformColor: PlatformColor { self == .claude ? Theme.claude : Theme.openai }

    var color: Color { Color(platform: platformColor) }
}

struct AgentIcon: View {
    let agent: Agent
    var size: IconSize = ControlSize.regular.symbol

    var body: some View {
        Image(agent.symbol, size: size)
            .foregroundStyle(agent.color)
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

    /// The image as a square with its picture in the middle, trimmed of the empty margin some
    /// icons come with and as wide as Lucide's icons draw, 22 of 24.
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
            let opaque = opaqueBounds(of: canvas, side: drawn),
            let cropped = picture.cropping(to: opaque),
            let result = bitmapContext(side: side)
        else { return image }
        let box = CGFloat(side) * 22 / 24
        let scale = min(box / opaque.width, box / opaque.height)
        let target = CGSize(width: opaque.width * scale, height: opaque.height * scale)
        let placed = CGRect(x: (CGFloat(side) - target.width) / 2, y: (CGFloat(side) - target.height) / 2, width: target.width, height: target.height)
        result.interpolationQuality = .high
        result.addPath(CGPath(roundedRect: placed, cornerWidth: min(target.width, target.height) * 0.22, cornerHeight: min(target.width, target.height) * 0.22, transform: nil))
        result.clip()
        result.draw(cropped, in: placed)
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

    /// A template image drawn in one colour, which follows the appearance.
    static func tinted(_ image: PlatformImage, _ color: PlatformColor) -> PlatformImage {
        #if os(macOS)
        NSImage(size: image.size, flipped: false) { rect in
            image.draw(in: rect)
            color.set()
            rect.fill(using: .sourceAtop)
            return true
        }
        #else
        image.withTintColor(color, renderingMode: .alwaysOriginal)
        #endif
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
    var size: IconSize = ControlSize.regular.symbol
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
            } else {
                Image(project?.menuSymbol ?? .folder, size: size)
                    .foregroundStyle(Color.themeMutedForeground)
            }
        }
        .frame(width: PlatformImage.symbolSide(size), height: PlatformImage.symbolSide(size))
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
    var weight: Font.Weight = .regular
    var showsState = false

    var body: some View {
        HStack(spacing: 3) {
            Image(.server, size: IconSize(text: size))
                .foregroundStyle(showsState ? server.stateTint ?? .themeMutedStrongerForeground : .themeMutedStrongerForeground)
            Text(server.shortName)
                .font(.ui(size: size, weight: weight))
                .lineLimit(1)
        }
        .foregroundStyle(Color.themeMutedStrongerForeground)
        .help("On \(server.name)")
    }
}

/// The server's symbol, in its state's colour while it can't be reached and in what is around it
/// otherwise, also while it connects.
struct ServerSymbol: View {
    let server: Server
    var size: IconSize = ControlSize.regular.symbol

    var body: some View {
        let image = Image(.server, size: size)
        if let tint = server.stateTint, server.state != .connecting {
            image.foregroundStyle(tint)
        } else {
            image
        }
    }
}

/// The menu that moves the open draft to another server that has its project.
struct ServerMenuItems: View {
    @Environment(AppStore.self) private var store
    let copies: [Project]

    var body: some View {
        Section("Server") {
            ForEach(copies) { copy in
                Toggle(isOn: Binding(get: { store.selectedDraft?.projectID == copy.id }, set: { _ in store.setNewThreadProject(copy.id) })) {
                    Text(store.server(copy.serverID)?.name ?? "")
                }
            }
        }
    }
}

extension Server {
    /// The colour of the server's state while it isn't connected.
    var stateTint: Color? {
        switch state {
        case .connected: nil
        case .connecting: .themePending
        case .disconnected, .refused: .themeDestructive
        }
    }
}

extension PullRequest.State {
    var platformColor: PlatformColor {
        switch self {
        case .open: Theme.success
        case .draft: Theme.mutedForeground
        case .merged: Theme.merged
        case .closed: Theme.destructive
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
        HStack(spacing: 3) {
            Image(pullRequest.state.symbol, size: .sm)
            Text(verbatim: "\(pullRequest.number)")
                .font(.ui(size: 11, weight: .medium))
                .monospacedDigit()
                .underline(hovering)
        }
        .foregroundStyle(colored ? pullRequest.state.color : Color.themeMutedStrongerForeground)
        .help(pullRequest.title)
    }
}

extension Project {
    /// The icon at the size a menu shows images, if it has been read already.
    var menuIcon: PlatformImage? {
        ImageFiles.shared.cached(iconPath).map { ImageFiles.sized($0, 16) }
    }

    /// The symbol a menu shows for it when it has no icon.
    var menuSymbol: Symbol { noProject ? .messageSquareDashed : .folder }
}
