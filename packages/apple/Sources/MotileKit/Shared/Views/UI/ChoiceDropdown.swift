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
    let title: String
    let choices: [DropdownChoice]
    let close: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            DropdownTitle(title)
            ForEach(choices) { choice in
                row(choice)
            }
        }
        .padding(DropdownList.padding)
        .frame(minWidth: 200, alignment: .leading)
        .fixedSize()
        .dropdownCard()
    }

    private func row(_ choice: DropdownChoice) -> some View {
        HStack(spacing: DropdownList.gap) {
            switch choice.icon {
            case .symbol(let symbol):
                Image(symbol, size: DropdownList.symbol)
                    .foregroundStyle(Color.themeMutedForeground)
            case .picture(let picture):
                picture
            default:
                EmptyView()
            }
            VStack(alignment: .leading, spacing: 1) {
                Text(choice.title)
                if let detail = choice.detail {
                    Text(detail)
                        .font(.ui(.xs))
                        .foregroundStyle(Color.themeMutedForeground)
                }
            }
            .lineLimit(1)
            Spacer(minLength: 12)
            Image(.check, size: DropdownList.check)
                .opacity(choice.chosen ? 1 : 0)
        }
        .padding(.vertical, choice.detail == nil ? 0 : 6)
        .dropdownRow()
        .button(.dropdownLight()) {
            choice.choose()
            close()
        }
    }
}
