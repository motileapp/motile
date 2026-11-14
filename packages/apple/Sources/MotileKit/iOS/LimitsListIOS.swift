#if os(iOS)
import SwiftUI

/// The limits tab as the system's grouped list, as the gateway's accounts are in settings: a group
/// for each account with what it has used and the limit resets it holds.
struct LimitsListIOS: View {
    @Environment(AppStore.self) private var store
    let model: UsageModel
    @State private var width: CGFloat = 0

    var body: some View {
        SettingsList {
            switch model.limits {
            case .loading:
                Section {
                    Spinner(size: .xl)
                        .frame(maxWidth: .infinity)
                }
            case .failed(let message):
                Section { note(message) }
            case .ready(let report):
                if report.sections.isEmpty && report.notes.isEmpty {
                    Section { note("No agent is installed on these servers.") }
                }
                ForEach(report.sections) { section in
                    Section {
                        LimitsSectionLabel(section: section)
                        LimitRowsIOS(windows: section.windows, resets: section.resets, note: section.note, width: width) {
                            model.refresh()
                        }
                    }
                }
                if !report.notes.isEmpty {
                    Section {} footer: {
                        Text(report.notes.joined(separator: "\n"))
                    }
                }
            }
        }
        .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { width = $0 }
        .refreshable { await model.refreshed() }
        // The gateway's accounts are among the limits while it is followed.
        .onAppear { store.watchGateway(true) }
        .onDisappear { store.watchGateway(false) }
    }

    private func note(_ text: String) -> some View {
        Text(text)
            .font(.footnote)
            .foregroundStyle(Color.themeMutedForeground)
    }
}

/// An account's agent, plan and whether it is the gateway's, and who is signed in under them.
private struct LimitsSectionLabel: View {
    let section: LimitsReport.Section

    var body: some View {
        HStack(spacing: 10) {
            AgentIcon(agent: section.agent, size: .md)
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 6) {
                    Text(section.agent.name)
                        .foregroundStyle(Color.themeForeground)
                    if let plan = section.plan {
                        Chip(plan)
                    }
                    if section.gateway {
                        Chip("Gateway", icon: .waypoints)
                    }
                }
                if let account = [section.name, section.account].compactMap({ $0 }).joined(separator: " · ").nonEmpty {
                    Text(account)
                        .font(.footnote)
                        .foregroundStyle(Color.themeMutedForeground)
                        .truncationMode(.middle)
                }
            }
            .lineLimit(1)
        }
        .padding(.vertical, 2)
        .alignmentGuide(.listRowSeparatorLeading) { $0[.leading] }
    }
}

/// An account's windows, the limit resets it holds and why it shows no windows, as rows of a group
/// of the system's list: the limits tab's and the gateway's accounts' in settings.
struct LimitRowsIOS: View {
    /// How far an inset group's rows stand from the list's sides.
    static let listInset: CGFloat = 20

    let windows: [LimitsReport.Window]
    let resets: LimitResets?
    let note: String?
    /// An account that is off.
    var dimmed = false
    /// The list's width.
    let width: CGFloat
    /// After a reset was used.
    var used: (() -> Void)?

    var body: some View {
        if !windows.isEmpty {
            VStack(spacing: 0) {
                ForEach(windows) { window in
                    if window.id != windows.first?.id {
                        ThemeDivider().padding(.horizontal, LimitRow.inset)
                    }
                    LimitRow(window: window, beside: width - 2 * Self.listInset >= LimitsView.besideWidth)
                }
            }
            .opacity(.disabled, when: dimmed)
            .listRowInsets(EdgeInsets())
            .alignmentGuide(.listRowSeparatorLeading) { _ in LimitRow.inset }
        }
        if let resets {
            ResetsRow(resets: resets, used: used)
        }
        if let note {
            Text(note)
                .font(.footnote)
                .foregroundStyle(Color.themeMutedForeground)
        }
    }
}

private extension String {
    var nonEmpty: String? { isEmpty ? nil : self }
}
#endif
