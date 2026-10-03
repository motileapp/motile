import AVKit
import SwiftUI

/// The images and videos of a message or of the composer, one at a time over the whole window.
/// ← and → go through them, and Esc or a click beside the picture closes it.
struct MediaViewer: View {
    private static let playedHere: Set<String> = ["mp4", "mov", "m4v"]

    @Environment(AppStore.self) private var store
    let viewing: Viewing
    @State private var loaded: Loaded?
    @State private var failed = false
    @State private var fraction: Double?
    @State private var keys: Any?

    private enum Loaded {
        case image(CGImage, scale: CGFloat)
        case video(AVPlayer)
    }

    var body: some View {
        let item = viewing.item
        ZStack {
            Color.black.opacity(0.86)
                .ignoresSafeArea()
                .onTapGesture { store.closeViewer() }
            content(item)
                .padding(.horizontal, 64)
                .padding(.vertical, 52)
            if viewing.items.count > 1 {
                HStack {
                    arrow("chevron.left", help: "Previous (←)") { store.viewNext(-1) }
                    Spacer()
                    arrow("chevron.right", help: "Next (→)") { store.viewNext(1) }
                }
                .padding(.horizontal, 14)
            }
        }
        .overlay(alignment: .top) {
            HStack(spacing: 8) {
                Text(item.name)
                    .lineLimit(1)
                    .truncationMode(.middle)
                if viewing.items.count > 1 {
                    Text("\(viewing.index + 1) of \(viewing.items.count)")
                        .foregroundStyle(.white.opacity(0.6))
                }
            }
            .font(.system(size: 13, weight: .medium))
            .foregroundStyle(.white)
            .padding(.top, 16)
            .padding(.horizontal, 80)
        }
        .overlay(alignment: .topTrailing) {
            arrow("xmark", help: "Close (Esc)") { store.closeViewer() }
                .padding(10)
        }
        .task(id: item) { load(item) }
        .onReceive(NotificationCenter.default.publisher(for: .mediaProgress)) { note in
            guard case .media(let id) = item.source, note.userInfo?["id"] as? String == id else { return }
            fraction = note.userInfo?["fraction"] as? Double
        }
        .onAppear {
            keys = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { event in
                handle(event) ? nil : event
            }
        }
        .onDisappear {
            if let keys { NSEvent.removeMonitor(keys) }
            keys = nil
            pause()
        }
    }

    @ViewBuilder private func content(_ item: ViewedMedia) -> some View {
        switch loaded {
        case .image(let image, let scale):
            Image(decorative: image, scale: scale)
                .resizable()
                .scaledToFit()
                .frame(maxWidth: CGFloat(image.width) / scale, maxHeight: CGFloat(image.height) / scale)
                .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
                .shadow(color: .black.opacity(0.5), radius: 30, y: 10)
        case .video(let player):
            VideoPlayer(player: player)
                .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
        case nil:
            Group {
                if failed {
                    Text(item.video ? "This video couldn't be loaded." : "This image couldn't be loaded.")
                } else if let fraction {
                    Text("Downloading \(item.name) · \(Int(fraction * 100))%").monospacedDigit()
                } else {
                    ProgressView().controlSize(.small).colorScheme(.dark)
                }
            }
            .font(.system(size: 13))
            .foregroundStyle(.white.opacity(0.7))
        }
    }

    private func arrow(_ symbol: String, help: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: 13, weight: .semibold))
                .foregroundStyle(.white)
                .frame(width: 32, height: 32)
                .background(.white.opacity(0.14), in: Circle())
                .contentShape(Circle())
        }
        .buttonStyle(.plain)
        .help(help)
    }

    private func handle(_ event: NSEvent) -> Bool {
        switch event.keyCode {
        case 53: store.closeViewer()
        case 123: store.viewNext(-1)
        case 124: store.viewNext(1)
        case 49:
            guard case .video(let player) = loaded else { return true }
            player.timeControlStatus == .paused ? player.play() : player.pause()
        default: return !event.modifierFlags.contains(.command)
        }
        return true
    }

    private func pause() {
        guard case .video(let player) = loaded else { return }
        player.pause()
    }

    private func load(_ item: ViewedMedia) {
        pause()
        (loaded, failed, fraction) = (nil, false, nil)
        let show = { (file: URL?) in
            guard viewing.item == item else { return }
            guard let file else { return failed = true }
            guard item.video else {
                let scale = NSScreen.main?.backingScaleFactor ?? 2
                let side = max(NSScreen.main?.frame.width ?? 1600, NSScreen.main?.frame.height ?? 1000) * scale
                return Pictures.decode(file, id: "view:\(file.path)", maxPixels: side) { image in
                    guard viewing.item == item else { return }
                    loaded = image.map { .image($0, scale: scale) }
                    failed = image == nil
                }
            }
            // A video the Mac's player can't play opens in the app the Mac has for it.
            guard Self.playedHere.contains(file.pathExtension.lowercased()) else {
                NSWorkspace.shared.open(file)
                return store.closeViewer()
            }
            let player = AVPlayer(url: file)
            loaded = .video(player)
            player.play()
        }
        switch item.source {
        case .file(let file): show(file)
        case .media(let id): store.media(id, done: show)
        }
    }
}
