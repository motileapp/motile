import SwiftUI

/// One of what a `ChoiceDropdown` offers: its symbol or picture, its name, a line about it, and
/// whether it is the one chosen.
struct DropdownChoice: Identifiable {
    let id: String
    let title: String
    var detail: String?
    var icon: ControlIcon?
    var chosen = false
    let choose: () -> Void
}

/// A short list to pick one from, under a title, as the composer's strip offers its server and
/// its workspace. Picking one closes it.
struct ChoiceDropdown: View {
    private static let padding: CGFloat = 4
    private static let rowLight = EdgeInsets(top: 1, leading: 0, bottom: 1, trailing: 0)

    let title: String
    let choices: [DropdownChoice]
    let close: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text(title)
                .font(.ui(size: 11.5, weight: .medium))
                .foregroundStyle(Color.themeMutedForeground)
                .padding(.horizontal, 8)
                .padding(.top, 6)
                .padding(.bottom, 4)
            ForEach(choices) { choice in
                row(choice)
            }
        }
        .padding(Self.padding)
        .frame(minWidth: 200, alignment: .leading)
        .fixedSize()
        .dropdownCard()
    }

    private func row(_ choice: DropdownChoice) -> some View {
        HStack(spacing: 8) {
            switch choice.icon {
            case .symbol(let symbol):
                Image(symbol, size: .md)
                    .foregroundStyle(Color.themeMutedForeground)
            case .picture(let picture):
                picture
            default:
                EmptyView()
            }
            VStack(alignment: .leading, spacing: 1) {
                Text(choice.title)
                    .font(.ui(size: 13))
                    .foregroundStyle(Color.themeForeground)
                if let detail = choice.detail {
                    Text(detail)
                        .font(.ui(size: 12))
                        .foregroundStyle(Color.themeMutedForeground)
                }
            }
            .lineLimit(1)
            Spacer(minLength: 12)
            Image(.check, size: .sm)
                .foregroundStyle(Color.themeForeground)
                .opacity(choice.chosen ? 1 : 0)
        }
        .padding(.horizontal, 8)
        .padding(.vertical, choice.detail == nil ? 0 : 6)
        .frame(minHeight: 28)
        .button(.highlight(radius: Radius.sm, inset: Self.rowLight)) {
            choice.choose()
            close()
        }
    }
}
