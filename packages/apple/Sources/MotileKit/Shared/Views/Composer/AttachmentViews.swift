import SwiftUI

/// An image or a video in the composer: its picture, how far it is on its way to the server, and
/// the button that removes it. A click opens it in the viewer.
struct AttachmentTile: View {
    static let side: CGFloat = 96

    @Environment(AppStore.self) private var store
    let attachment: Attachment
    let open: () -> Void
    @State private var picture: CGImage?

    var body: some View {
        ZStack {
            Color.themeBackgroundSecondary
            if let picture {
                Image(decorative: picture, scale: 1)
                    .resizable()
                    .scaledToFit()
                    .frame(width: Self.side, height: Self.side)
            }
            if attachment.state == .preparing {
                Spinner(size: .lg)
                    .foregroundStyle(Color.themeMutedForeground)
            } else if attachment.video {
                Image(.play, size: .lg)
                    .foregroundStyle(Color.themeForegroundOverPicture)
                    .shadow(.sm, .shadowStrongest)
            }
        }
        .modifier(TileLook(attachment: attachment, open: open))
        .task(id: attachment.file) { loadPicture() }
    }

    private func loadPicture() {
        let key = "tile:\(attachment.id)"
        picture = Pictures.cached(key)
        guard picture == nil else { return }
        let maxPixels = Self.side * 2 * Platform.pixelsPerPoint
        let decode = { (file: URL) in
            guard attachment.video, attachment.file != nil else {
                return Pictures.decode(file, id: key, maxPixels: maxPixels) { picture = $0 }
            }
            Pictures.firstFrame(file, id: key, maxPixels: maxPixels) { picture = $0 }
        }
        if let file = attachment.file { return decode(file) }
        guard let id = attachment.attached.picture else { return }
        store.media(id) { file in
            guard let file else { return }
            decode(file)
        }
    }
}

/// A file in the composer that isn't shown as a picture, as a tile beside the pictures: its kind,
/// its name, and how far it is on its way to the server.
struct AttachmentFileTile: View {
    let attachment: Attachment

    var body: some View {
        Text(attachment.name)
            .font(.ui(size: 12, weight: .medium))
            .foregroundStyle(Color.themeForeground)
            .lineLimit(3)
            .padding(nameInset)
            .frame(width: AttachmentTile.side, height: AttachmentTile.side, alignment: .bottomLeading)
            .overlay(alignment: .topLeading) {
                if let kind = attachment.kind {
                    Chip(kind, muted: true)
                        .padding(kindInset)
                }
            }
            .background(Color.themeBackgroundSecondary)
            .modifier(TileLook(attachment: attachment, open: nil))
    }
}

private let nameInset: CGFloat = 8
private let kindInset: CGFloat = 6
/// The remove button sits level with a file's kind.
private let removeInset = kindInset - (ControlSize.small.height - Chip.height) / 2

/// The square of an attachment: its edge, how far its upload is along its bottom, and the button
/// that removes it.
private struct TileLook: ViewModifier {
    @Environment(AppStore.self) private var store
    let attachment: Attachment
    let open: (() -> Void)?

    func body(content: Content) -> some View {
        let shape = RoundedRectangle(cornerRadius: Radius.xl, style: .continuous)
        let tile = content
            .frame(width: AttachmentTile.side, height: AttachmentTile.side)
            .overlay(alignment: .bottom) { AttachmentProgress(attachment: attachment) }
            .clipShape(shape)
            .overlay(shape.stroke(Color.themeBorder, lineWidth: 1))
        Group {
            if let open {
                tile.button(DimButtonStyle(), action: open)
            } else {
                tile
            }
        }
        .overlay(alignment: .topTrailing) {
            ActionButton(icon: .x, help: "Remove", variant: .overlay, size: .small, round: true) {
                store.removeAttachment(attachment.id)
            }
            .padding(removeInset)
        }
        .help(attachment.summary)
        .accessibilityLabel(attachment.summary)
    }
}

/// The thread of a removed server that a new thread goes on from, and the button that starts it
/// without it.
struct ContinuedThreadChip: View {
    @Environment(AppStore.self) private var store
    let from: ContinuedThread

    var body: some View {
        HStack(spacing: 5) {
            Image(.messageSquareText, size: .sm)
            Text("Continues “\(from.title)”")
                .lineLimit(1)
            ActionButton(icon: .x, help: "Start without it", size: .small, round: true) { store.dropContinuation() }
                .environment(\.row, .chip)
        }
        .modifier(ComposerChipLook())
        .help("The new thread starts with everything that happened in “\(from.title)”")
    }
}

/// The capsule of the thread a message continues in the composer.
private struct ComposerChipLook: ViewModifier {
    @Environment(\.surface) private var surface

    func body(content: Content) -> some View {
        content
            .font(.ui(size: 12))
            .padding(.leading, 9)
            .padding([.vertical, .trailing], 2)
            .background(surface.color(.chip), in: Capsule())
            .overlay(Capsule().strokeBorder(Color.themeBorderAccent, lineWidth: 1))
    }
}

/// How far an attachment is on its way to the server, along its tile's bottom edge, or the button
/// that tries again when it didn't get there. Nothing once it is there.
private struct AttachmentProgress: View {
    @Environment(AppStore.self) private var store
    let attachment: Attachment

    var body: some View {
        switch attachment.state {
        case .ready, .preparing:
            EmptyView()
        case .uploading(let fraction):
            Text("\(Int(fraction * 100))%")
                .monospacedDigit()
                .font(.ui(size: 10, weight: .medium))
                .foregroundStyle(Color.themeForegroundOverPicture)
                .frame(maxWidth: .infinity)
                .padding(.vertical, 2)
                .background(Color.themeScrim)
        case .failed(let reason):
            Label("Retry", symbol: .rotateCw, size: .xs)
                .font(.ui(size: 10, weight: .medium))
                .foregroundStyle(Color.themeDestructiveForeground)
                .frame(maxWidth: .infinity)
                .padding(.vertical, 2)
                .background(Color.themeDestructive)
                .button(DimButtonStyle()) { store.retryAttachment(attachment.id) }
                .help("\(reason) Click to try again.")
        }
    }
}
