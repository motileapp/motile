import SwiftUI

/// In place of the composer of a thread whose server was unlinked: the composer's box, saying why
/// the thread can only be read, with the ways on from it under that. On the Mac the strip below
/// still says where the thread worked.
struct UnlinkedThreadBar: View {
    @Environment(AppStore.self) private var store
    let server: Server

    var body: some View {
        VStack(spacing: 0) {
            box
            #if os(macOS)
            if let project = store.composerProject {
                ContextStrip(project: project, server: server)
            }
            #endif
        }
        .composerOutlineShadow()
        .frame(maxWidth: Theme.composerWidth)
    }

    private var box: some View {
        VStack(alignment: .leading, spacing: 6) {
            title
            Text("This thread is read-only. You can continue it on another server, or delete the server in settings.")
                .font(.ui(size: 13))
                .foregroundStyle(Color.themeMutedForeground)
                .fixedSize(horizontal: false, vertical: true)
            HStack(spacing: 0) {
                Spacer(minLength: 0)
                buttons
            }
            .padding(.horizontal, -Self.textInset)
        }
        .padding(.horizontal, Self.textInset)
        .padding(.top, 14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .composerSurface(in: RoundedRectangle(cornerRadius: ComposerView.radius, style: .continuous), shadow: .md)
        .anchorPreference(key: ComposerPlace.self, value: .bounds) { ComposerPlace.Value(box: $0) }
    }

    private var title: some View {
        HStack(spacing: 6) {
            Image(.unlink, size: .sm)
            nameChip
            Text("is unlinked")
                .font(.ui(size: 13, weight: .medium))
                .lineLimit(1)
        }
        .foregroundStyle(Color.themeWarning)
    }

    /// The server's name, set apart as a skill is in a message: on a wash of the title's colour,
    /// inside an edge.
    private var nameChip: some View {
        let shape = RoundedRectangle(cornerRadius: SkillChip.radius, style: .continuous)
        return Text(server.name)
            .font(.ui(size: 13, weight: .medium))
            .lineLimit(1)
            .padding(.horizontal, scaled(SkillChip.padding))
            .padding(.vertical, 1)
            .background(Color.themeWarning.wash(), in: shape)
            .overlay { shape.strokeBorder(Color.themeWarning.at(.colorTintChart), lineWidth: 1) }
    }

    private var buttons: some View {
        HStack(spacing: 0) {
            ActionButton(
                "Settings", help: "Delete “\(server.name)” and its threads in Settings", round: true, glass: true,
                margin: ComposerView.margin(trailing: 4)
            ) { store.openSettings(.servers) }
            ActionButton(
                "Continue", help: "Start a thread on another server with everything that happened here", variant: .primary,
                round: true, glass: true, margin: ComposerView.margin(leading: 4, trailing: ComposerView.edge)
            ) { store.continueOnAnotherServer() }
            .disabled(store.servers.isEmpty)
        }
    }

    /// How far the words stand from the box's side, as the composer's text does.
    private static let textInset: CGFloat = 14
}
