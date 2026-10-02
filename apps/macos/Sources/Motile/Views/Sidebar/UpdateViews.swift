import SwiftUI

/// A new version of the app, where the sidebar ends: the offer, the download as it goes, and
/// the restart that finishes it.
struct AppUpdateRow: View {
    let updater: AppUpdater

    var body: some View {
        switch updater.state {
        case .idle:
            EmptyView()
        case .upToDate:
            line("Motile \(updater.current) is the newest version", symbol: "checkmark.circle")
        case .available(let version):
            line("Motile \(version) is available", symbol: "arrow.down.circle") {
                PillButton("Update") { updater.install() }
            }
        case .downloading(let version, let fraction):
            VStack(alignment: .leading, spacing: 5) {
                line("Downloading Motile \(version)", symbol: "arrow.down.circle") {
                    Text("\(Int(fraction * 100))%")
                        .font(.system(size: 11))
                        .foregroundStyle(.tertiary)
                        .monospacedDigit()
                }
                ProgressView(value: fraction)
                    .progressViewStyle(.linear)
                    .controlSize(.small)
                    .tint(Color.themeSecondary)
            }
        case .installing(let version):
            line("Installing Motile \(version)…", symbol: "arrow.down.circle") {
                ProgressView().controlSize(.small)
            }
        case .ready(let version):
            line("Motile \(version) is installed", symbol: "checkmark.circle") {
                PillButton("Restart") { updater.relaunch() }
            }
        case .failed(let message):
            VStack(alignment: .leading, spacing: 4) {
                line("The update didn’t work", symbol: "exclamationmark.triangle") {
                    PillButton("Try Again") { updater.retry() }
                }
                Text(message)
                    .font(.system(size: 11))
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }

    private func line(_ text: String, symbol: String) -> some View {
        line(text, symbol: symbol) { EmptyView() }
    }

    private func line<Trailing: View>(_ text: String, symbol: String, @ViewBuilder trailing: () -> Trailing) -> some View {
        HStack(spacing: 7) {
            Image(systemName: symbol)
                .font(.system(size: 13, weight: .medium))
                .foregroundStyle(.secondary)
            Text(text)
                .font(.system(size: 12, weight: .medium))
                .lineLimit(1)
            Spacer(minLength: 4)
            trailing()
        }
        .frame(minHeight: 22)
    }
}

/// What stands at the end of a server's line: the offer to update it, the update as it goes, or
/// `otherwise`.
struct ServerUpdateStatus<Otherwise: View>: View {
    @Environment(AppStore.self) private var store
    let server: Server
    @ViewBuilder let otherwise: () -> Otherwise

    var body: some View {
        if let update = store.serverUpdates[server.id] {
            HStack(spacing: 6) {
                Text(progress(of: update))
                    .font(.system(size: 11))
                    .foregroundStyle(.secondary)
                    .monospacedDigit()
                ProgressView().controlSize(.small)
            }
        } else if store.isOutdated(server) {
            PillButton("Update") { store.update(server) }
                .help("Install version \(store.updater.latest ?? "") on \(server.name). It restarts, and no agent may be working.")
        } else {
            otherwise()
        }
    }

    private func progress(of update: ServerUpdate) -> String {
        if update.restarting { return "Restarting…" }
        guard let fraction = update.fraction else { return "Updating…" }
        return "Updating \(Int(fraction * 100))%"
    }
}

/// A small button that says what it does.
struct PillButton: View {
    let title: String
    let action: () -> Void
    @State private var hovering = false

    init(_ title: String, action: @escaping () -> Void) {
        self.title = title
        self.action = action
    }

    var body: some View {
        Button(action: action) {
            Text(title)
                .font(.system(size: 11, weight: .medium))
                .padding(.horizontal, 9)
                .frame(height: 20)
                .background(hovering ? Color.themeSelected : Color.themeHover, in: Capsule())
                .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .onHover { hovering = $0 }
    }
}
