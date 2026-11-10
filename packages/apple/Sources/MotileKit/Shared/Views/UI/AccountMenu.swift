import SwiftUI

/// The account as its picture, which opens a menu with who is signed in, the settings, the usage
/// and the way to sign out. On iOS it floats on glass beside the search, as the sidebar's buttons do.
struct AccountMenu: View {
    /// The picture on the Mac, which fills its button but for a margin.
    static let picture = IconSize.xl
    #if os(iOS)
    /// The picture on iOS, 6 points inside its glass button.
    static let glassPicture = GlassButtonStyle.side - 12
    #endif

    var margin = EdgeInsets()
    @Environment(AppStore.self) private var store

    var body: some View {
        #if os(macOS)
        ActionMenu(
            picture: AnyView(AccountPicture(account: store.account, initialSize: .xxs)), help: store.account.email,
            symbolSize: Self.picture, margin: margin
        ) { items }
        #else
        Menu { items } label: {
            AccountPicture(account: store.account, initialSize: .lg)
                .frame(width: Self.glassPicture, height: Self.glassPicture)
                .frame(width: GlassButtonStyle.side, height: GlassButtonStyle.side)
                .contentShape(Circle())
        }
        .menuStyle(.button)
        .menuOrder(.fixed)
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
    /// Half as tall as the disc.
    let initialSize: TextSize

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
        Text((account.name ?? account.email).prefix(1).uppercased())
            .font(.ui(initialSize, weight: .semibold))
            .foregroundStyle(Color.themeForeground)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(Color.themeBorder)
    }
}
