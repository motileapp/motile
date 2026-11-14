import SwiftUI

/// The relay the settings set or change, as it was when the sheet opened.
struct EditedRelay: Identifiable {
    let url: String?

    var id: String { "relay" }
}

/// The iroh relay every device of the account uses in place of iroh's public ones.
struct RelaySheet: View {
    @Environment(AppStore.self) private var store
    @Environment(\.closeModal) private var close
    @Environment(\.dismiss) private var dismiss
    @State private var url: String
    @State private var saving = false
    @State private var error: String?

    init(url: String?) {
        _url = State(initialValue: url ?? "")
    }

    private let title = "Relay"
    private let placeholder = "https://relay.example.com"
    private let caption = "Every device of your account uses this iroh relay in place of iroh's public ones. Leave it empty to go back to them."

    var body: some View {
        #if os(macOS)
        VStack(alignment: .leading, spacing: 14) {
            Text(title)
                .font(.ui(.lg, weight: .semibold))
            VStack(alignment: .leading, spacing: 6) {
                InputField(placeholder, text: $url, monospaced: true)
                Text(error ?? caption)
                    .font(.ui(.xs))
                    .foregroundStyle(error == nil ? Color.themeMutedStrongerForeground : Color.themeDestructive)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.horizontal, 4)
            }
            HStack(spacing: 0) {
                Spacer()
                ActionButton("Cancel", margin: .beside(trailing: 4)) { close() }
                    .keyboardShortcut(.cancelAction)
                ActionButton("Save", variant: .primary, pending: saving, margin: .beside(leading: 4)) { save() }
                    .keyboardShortcut(.defaultAction)
            }
        }
        .padding(20)
        .frame(width: 460)
        #else
        SettingsList {
            Section {
                TextField(placeholder, text: $url)
                    .font(.system(.body, design: .monospaced))
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .keyboardType(.URL)
            } footer: {
                Text(error ?? caption)
                    .foregroundStyle(error == nil ? Color.themeMutedForeground : Color.themeDestructive)
            }
        }
        .scrollDismissesKeyboard(.interactively)
        .navigationTitle(title)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .confirmationAction) {
                if saving {
                    Spinner(size: .xl)
                } else {
                    Button("Save") { save() }
                }
            }
        }
        #endif
    }

    /// Closes the Mac's modal, or goes back from the iOS page.
    private func finish() {
        #if os(macOS)
        close()
        #else
        dismiss()
        #endif
    }

    private func save() {
        saving = true
        error = nil
        let trimmed = url.trimmingCharacters(in: .whitespacesAndNewlines)
        store.setRelay(trimmed.isEmpty ? nil : trimmed) { failure in
            saving = false
            error = failure
            if failure == nil { finish() }
        }
    }
}
