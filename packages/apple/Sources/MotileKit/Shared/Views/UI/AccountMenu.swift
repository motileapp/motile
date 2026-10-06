import SwiftUI

/// The account as its picture, which opens a menu with who is signed in, the settings, the usage
/// and the way to sign out. On iOS it floats on glass beside the search, as the sidebar's buttons do.
struct AccountMenu: View {
    /// The picture's side on the Mac, which fills its button but for a margin.
    static let side = ControlSize.regular.height - 8

    @Environment(AppStore.self) private var store

    var body: some View {
        #if os(macOS)
        ActionMenu(
            picture: AnyView(AccountPicture(account: store.account)), help: store.account.email,
            symbolSize: Self.side
        ) { items }
        #else
        Menu { items } label: {
            AccountPicture(account: store.account)
                .frame(width: 34, height: 34)
                .frame(width: 46, height: 46)
                .contentShape(Circle())
        }
        .menuStyle(.button)
        .buttonStyle(.plain)
        .glassButton(in: Circle())
        .accessibilityLabel("Account")
        #endif
    }

    @ViewBuilder private var items: some View {
        Section(store.account.email) {
            Button("Settings") { store.openSettings() }
            Button("Usage") { store.openUsage() }
        }
        Divider()
        Button("Sign Out") { store.signOut() }
    }
}

/// The account's picture, or the first letter of its name on a disc until there is one.
struct AccountPicture: View {
    let account: Account

    var body: some View {
        AsyncImage(url: account.picture) { image in
            image
                .resizable()
                .scaledToFill()
        } placeholder: {
            initial
        }
        .clipShape(Circle())
    }

    private var initial: some View {
        GeometryReader { disc in
            Text((account.name ?? account.email).prefix(1).uppercased())
                .font(.ui(size: disc.size.height * 0.5, weight: .semibold))
                .foregroundStyle(Color.themeText)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .background(Color.themeBorderSecondary)
        }
    }
}
