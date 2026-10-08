import SwiftUI

#if os(macOS)
/// A new version of the client, where the sidebar ends: the offer, the download as it goes, and
/// the restart that finishes it.
struct AppUpdateRow: View {
    let updater: AppUpdater

    var body: some View {
        switch updater.state {
        case .idle:
            EmptyView()
        case .checking:
            line("Checking for updates", symbol: .refreshCw, turning: true)
        case .upToDate:
            HStack {
                UpdateLabel.upToDate(updater.current)
                Spacer(minLength: 4)
            }
            .frame(minHeight: ControlSize.small.height)
        case .available(let version):
            line("v\(version) is available", symbol: .circleArrowDown, tint: .themeSuccess) {
                ActionButton("Update", size: .small) { updater.install() }
            }
        case .downloading(let version, let fraction):
            VStack(alignment: .leading, spacing: 5) {
                line("Downloading v\(version)", symbol: .circleArrowDown, tint: .themeText) {
                    Text("\(Int(fraction * 100))%")
                        .font(.ui(size: 11))
                        .foregroundStyle(Color.themeTertiary)
                        .monospacedDigit()
                }
                ProgressView(value: fraction)
                    .progressViewStyle(.linear)
                    .controlSize(.small)
                    .tint(Color.themeSecondary)
            }
        case .installing(let version):
            line("Installing v\(version)", symbol: .circleArrowDown, tint: .themeText) { spinner }
        case .ready(let version):
            line("v\(version) is installed", symbol: .circleCheck, tint: .themeSuccess) {
                ActionButton("Restart", size: .small) { updater.relaunch() }
            }
        case .failed(let message):
            VStack(alignment: .leading, spacing: 4) {
                line("The update didn’t work", symbol: .triangleAlert) {
                    ActionButton("Try Again", size: .small) { updater.retry() }
                }
                Text(message)
                    .font(.ui(size: 11))
                    .foregroundStyle(Color.themeSecondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }

    private var spinner: some View {
        Spinner(size: ControlSize.small.symbol)
            .foregroundStyle(Color.themeSecondary)
    }

    private func line(_ text: String, symbol: Symbol, turning: Bool = false) -> some View {
        line(text, symbol: symbol, turning: turning) { EmptyView() }
    }

    private func line<Trailing: View>(
        _ text: String, symbol: Symbol, tint: Color = .themeSecondary, turning: Bool = false, @ViewBuilder trailing: () -> Trailing
    ) -> some View {
        HStack(spacing: 7) {
            UpdateLabel(text: text, symbol: symbol, tint: tint, turning: turning)
            Spacer(minLength: 4)
            trailing()
        }
        .frame(minHeight: ControlSize.small.height)
    }
}

/// What an update is at, after its symbol, with the version muted.
struct UpdateLabel: View {
    let text: String
    var detail: String?
    let symbol: Symbol
    var tint = Color.themeSecondary
    var turning = false

    static func upToDate(_ version: String) -> UpdateLabel {
        UpdateLabel(text: "Up to date", detail: "(\(version))", symbol: .circleCheck, tint: .themeSuccess)
    }

    var body: some View {
        HStack(spacing: 5) {
            TurningSymbol(symbol: symbol, size: 12, turning: turning)
                .foregroundStyle(tint)
            label
                .font(.ui(size: 12, weight: .medium))
                .lineLimit(1)
        }
    }

    private var label: Text {
        guard let detail else { return Text(text) }
        return Text("\(text) \(Text(detail).foregroundStyle(Color.themeSecondary))")
    }
}

#endif

/// What stands at the end of a server's line: the offer to update it, the update as it goes, or
/// `otherwise`. While agents work there, the server restarts once they have finished, or at once
/// with their threads going on after.
struct ServerUpdateStatus<Otherwise: View>: View {
    @Environment(AppStore.self) private var store
    let server: Server
    @ViewBuilder let otherwise: () -> Otherwise

    var body: some View {
        if let update = store.serverUpdate(of: server), update.waiting, !update.restarting {
            HStack(spacing: 6) {
                Text("Waiting")
                    .font(.ui(size: 11))
                    .foregroundStyle(Color.themeSecondary)
                ActionButton(
                    "Restart Now", help: "Stop the agents on \(server.name) and restart it. Their threads continue once it is back.",
                    size: .small
                ) {
                    store.update(server, when: .now)
                }
            }
            .help("\(server.name) restarts once its agents have finished")
        } else if let update = store.serverUpdate(of: server) {
            HStack(spacing: 6) {
                Text(progress(of: update))
                    .font(.ui(size: 11))
                    .foregroundStyle(Color.themeSecondary)
                    .monospacedDigit()
                Spinner(size: ControlSize.small.symbol)
                    .foregroundStyle(Color.themeSecondary)
            }
        } else if store.isOutdated(server), store.isBusy(server), store.canChooseRestart(server) {
            ActionMenu(
                "Update", help: "Agents are working on \(server.name). Update it once they finish, or now: they stop and continue once it is back.",
                variant: .secondary, size: .small
            ) {
                Button("Update when agents finish") { store.update(server, when: .idle) }
                Button("Update now and auto-resume agents") { store.update(server, when: .now) }
            }
        } else if store.isOutdated(server) {
            ActionButton("Update", help: updateHelp, size: .small) {
                store.update(server)
            }
        } else {
            otherwise()
        }
    }

    private var updateHelp: String {
        let version = store.updater.latest ?? ""
        guard store.canChooseRestart(server) else { return "Install version \(version) on \(server.name). It restarts, and no agent may be working." }
        return "Install version \(version) on \(server.name). It restarts once no agent works there."
    }

    private func progress(of update: ServerUpdate) -> String {
        if update.restarting { return "Restarting" }
        guard let fraction = update.fraction else { return "Updating" }
        return "Updating \(Int(fraction * 100))%"
    }
}
