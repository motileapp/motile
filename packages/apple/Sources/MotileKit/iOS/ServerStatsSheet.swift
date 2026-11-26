#if os(iOS)
import SwiftUI

/// What a server's machine does, as the Mac's dropdown shows it, as the system's grouped list in a
/// sheet named after the server and as tall as the list: its uptime and update, then a row for
/// each of its parts.
struct ServerStatsSheet: View {
    private static let rowPadding: CGFloat = 6
    private static let sectionGap: CGFloat = 24
    /// The update's button stands as far from the group's top, bottom and side; the uptime keeps
    /// the list's own room before it.
    private static let headerInsets = EdgeInsets(top: 10, leading: 20, bottom: 10, trailing: 10)

    @Environment(AppStore.self) private var store
    let server: Server
    /// What the list takes with its bars, so that the sheet is only as tall.
    @State private var height: CGFloat?

    var body: some View {
        NavigationStack {
            SettingsList {
                Section {
                    ServerStatsHeader(server: server, named: false)
                        .listRowInsets(Self.headerInsets)
                }
                Section {
                    ForEach(store.stats(of: server.id)?.rows ?? []) { row in
                        ServerStatsRow(row: row)
                            .padding(.vertical, Self.rowPadding)
                            .alignmentGuide(.listRowSeparatorLeading) { $0[.leading] }
                    }
                }
            }
            .listSectionSpacing(Self.sectionGap)
            .onScrollGeometryChange(for: CGFloat.self) { geometry in
                geometry.contentSize.height + geometry.contentInsets.top + geometry.contentInsets.bottom
            } action: { _, taken in
                height = taken
            }
            .navigationTitle(server.name)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { GlassToolbarItem(placement: .topBarLeading) { SheetCloseButton() } }
        }
        .presentationDetents(height.map { [.height($0)] } ?? [.medium])
        .presentationDragIndicator(.visible)
        .onAppear { store.watchStats(of: server.id, true) }
        .onDisappear { store.watchStats(of: server.id, false) }
    }
}
#endif
