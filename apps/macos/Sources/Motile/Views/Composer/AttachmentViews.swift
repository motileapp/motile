import SwiftUI

/// An image or a video in the composer: its picture, how far it is on its way to the server, and
/// the button that removes it. A click opens it in the viewer.
struct AttachmentTile: View {
    static let side: CGFloat = 64

    @Environment(AppStore.self) private var store
    let attachment: Attachment
    let open: () -> Void
    @State private var picture: CGImage?

    var body: some View {
        ZStack {
            Color.themeBubble
            if let picture {
                Image(decorative: picture, scale: 1)
                    .resizable()
                    .scaledToFill()
                    .frame(width: Self.side, height: Self.side)
            }
            if attachment.video {
                Image(systemName: "play.fill")
                    .font(.system(size: 16))
                    .foregroundStyle(.white)
                    .shadow(color: .black.opacity(0.5), radius: 3)
            }
        }
        .frame(width: Self.side, height: Self.side)
        .overlay(alignment: .bottom) { AttachmentProgress(attachment: attachment, onPicture: true) }
        .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 10, style: .continuous).stroke(Color.themeBorder, lineWidth: 1))
        .contentShape(Rectangle())
        .onTapGesture(perform: open)
        .overlay(alignment: .topTrailing) {
            Button {
                store.removeAttachment(attachment.id)
            } label: {
                Image(systemName: "xmark")
                    .font(.system(size: 8, weight: .bold))
                    .foregroundStyle(.white)
                    .frame(width: 16, height: 16)
                    .background(.black.opacity(0.6), in: Circle())
                    .padding(4)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .help("Remove")
        }
        .help(attachment.name)
        .accessibilityLabel(attachment.name)
        .task(id: attachment.id) { loadPicture() }
    }

    private func loadPicture() {
        let key = "tile:\(attachment.id)"
        picture = Pictures.cached(key)
        guard picture == nil else { return }
        let maxPixels = Self.side * 2 * (NSScreen.main?.backingScaleFactor ?? 2)
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

/// A file in the composer that isn't shown as a picture: its name, its size, and how far it is on
/// its way to the server.
struct AttachmentChip: View {
    @Environment(AppStore.self) private var store
    let attachment: Attachment

    var body: some View {
        HStack(spacing: 5) {
            Image(systemName: "doc")
            Text(attachment.name)
                .lineLimit(1)
            if let bytes = attachment.bytes {
                Text(ByteCountFormatter.string(fromByteCount: bytes, countStyle: .file))
                    .foregroundStyle(Color.themeSecondary)
            }
            AttachmentProgress(attachment: attachment, onPicture: false)
            IconOnlyButton(symbol: "xmark", help: "Remove", size: 18, symbolSize: 10) {
                store.removeAttachment(attachment.id)
            }
        }
        .font(.system(size: 12))
        .padding(.leading, 9)
        .padding([.vertical, .trailing], 5)
        .background(Color.themeBubble, in: Capsule())
    }
}

/// How far an attachment is on its way to the server, or the button that tries again when it
/// didn't get there. Nothing once it is there.
private struct AttachmentProgress: View {
    @Environment(AppStore.self) private var store
    let attachment: Attachment
    /// Over a tile's picture, along its bottom edge. Otherwise in a chip's line of text.
    let onPicture: Bool

    var body: some View {
        switch attachment.state {
        case .ready:
            EmptyView()
        case .uploading(let fraction):
            let percent = Text("\(Int(fraction * 100))%").monospacedDigit()
            if onPicture {
                percent
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(.white)
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 2)
                    .background(.black.opacity(0.6))
            } else {
                percent.foregroundStyle(Color.themeSecondary)
            }
        case .failed(let reason):
            Button {
                store.retryAttachment(attachment.id)
            } label: {
                if onPicture {
                    Label("Retry", systemImage: "arrow.clockwise")
                        .font(.system(size: 10, weight: .medium))
                        .foregroundStyle(.white)
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 2)
                        .background(Color.themeDanger.opacity(0.85))
                } else {
                    Label("Retry", systemImage: "arrow.clockwise")
                        .foregroundStyle(Color.themeDanger)
                }
            }
            .buttonStyle(.plain)
            .help("\(reason) Click to try again.")
        }
    }
}
