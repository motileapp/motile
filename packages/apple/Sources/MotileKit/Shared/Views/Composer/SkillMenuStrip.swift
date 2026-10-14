import SwiftUI

/// The skills that fit the word being typed, on top of the composer as one of its strips: the
/// best first, each with what it does and where it comes from. The arrows move the light, Return
/// or Tab puts the lit one in, Esc puts the menu away, and a click puts in the one clicked.
struct SkillMenuStrip: View {
    @Environment(AppStore.self) private var store
    let menu: SkillMenu

    /// On iOS the description is under the name, where it has room.
    static let rowHeight: CGFloat = Platform.name == "ios" ? scaled(46) : scaled(32)
    static let inset: CGFloat = 4
    /// As many rows as show before the rest scroll, the last cut in half to say there are more.
    private static let shownRows: CGFloat = 7.5

    var body: some View {
        ScrollViewReader { scroller in
            ScrollView {
                VStack(spacing: 0) {
                    ForEach(Array(menu.rows.enumerated()), id: \.element.id) { index, row in
                        SkillMenuRow(row: row)
                            .button(.highlight(radius: Radius.md, lit: index == store.skillHighlight, inset: Self.rowInset, fill: .themeBorderComposer)) {
                                store.pickSkill(row)
                            }
                            .onHover { if $0 { store.skillHighlight = index } }
                            .id(row.id)
                    }
                }
                .padding(.vertical, Self.inset)
            }
            .scrollIndicators(.never)
            .frame(height: min(CGFloat(menu.rows.count), Self.shownRows) * Self.rowHeight + Self.inset * 2)
            .onChange(of: store.skillHighlight) {
                guard menu.rows.indices.contains(store.skillHighlight) else { return }
                scroller.scrollTo(menu.rows[store.skillHighlight].id)
            }
        }
        .modifier(ComposerStrip(edge: .top, height: nil))
        .accessibilityLabel("Skills")
    }

    private static let rowInset = EdgeInsets(top: 0, leading: inset, bottom: 0, trailing: inset)
}

private struct SkillMenuRow: View {
    let row: SkillRow

    var body: some View {
        HStack(spacing: 8) {
            Image(SkillChip.symbol, size: 13)
                .foregroundStyle(Color.themeSkill)
            #if os(iOS)
            VStack(alignment: .leading, spacing: 2) {
                label
                description(size: 12)
            }
            #else
            label
                .layoutPriority(1)
            description(size: 13)
            #endif
            Spacer(minLength: 8)
            Chip(row.source, icon: row.symbol)
                .fixedSize()
        }
        .padding(.horizontal, 10)
        .frame(height: SkillMenuStrip.rowHeight)
        .padding(.horizontal, SkillMenuStrip.inset)
        .contentShape(Rectangle())
    }

    private var label: some View {
        Text(row.label)
            .font(.ui(size: 13, weight: .medium))
            .foregroundStyle(Color.themeForeground)
            .lineLimit(1)
    }

    private func description(size: CGFloat) -> some View {
        Text(row.description)
            .font(.ui(size: size))
            .foregroundStyle(Color.themeMutedForeground)
            .lineLimit(1)
    }
}
