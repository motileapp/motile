import SwiftUI

/// The bucket the settings set up or change, as it was when the sheet opened.
struct EditedBackup: Identifiable {
    let bucket: BackupBucket?

    var id: String { "backup" }
}

/// The account's S3 bucket to set up or change. One server checks it before every server uses it.
struct BackupSheet: View {
    @Environment(AppStore.self) private var store
    @Environment(\.closeModal) private var close
    @State private var bucket: BackupBucket
    @State private var saving = false
    @State private var error: String?

    init(bucket: BackupBucket?) {
        _bucket = State(initialValue: bucket ?? BackupBucket())
    }

    private let title = "S3 Backup"
    private var caption: String {
        "One of your servers checks the bucket first. Each server then backs up to it by itself and syncs through it. The keys stay on your servers and this \(Platform.device)."
    }

    var body: some View {
        #if os(macOS)
        VStack(alignment: .leading, spacing: 14) {
            Text(title)
                .font(.ui(size: 15, weight: .semibold))
            form
            HStack(spacing: 0) {
                Spacer()
                ActionButton("Cancel", margin: .beside(trailing: 4)) { close() }
                    .keyboardShortcut(.cancelAction)
                ActionButton("Save", variant: .primary, pending: saving, margin: .beside(leading: 4)) { save() }
                    .keyboardShortcut(.defaultAction)
                    .disabled(!bucket.isComplete)
            }
        }
        .padding(20)
        .frame(width: 460)
        #else
        NavigationStack {
            SettingsList { listForm }
                .scrollDismissesKeyboard(.interactively)
                .navigationTitle(title)
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    GlassToolbarItem(placement: .cancellationAction) { SheetCloseButton() }
                    ToolbarItem(placement: .confirmationAction) {
                        if saving {
                            Spinner(size: .xl)
                        } else {
                            Button("Save") { save() }
                                .disabled(!bucket.isComplete)
                        }
                    }
                }
        }
        .presentationDragIndicator(.visible)
        .interactiveDismissDisabled(saving)
        #endif
    }

    private func save() {
        saving = true
        error = nil
        store.setBackup(bucket) { failure in
            saving = false
            error = failure
            if failure == nil { close() }
        }
    }

    #if os(macOS)
    private var form: some View {
        VStack(alignment: .leading, spacing: 14) {
            field("Endpoint", caption: "Empty for AWS.") {
                InputField("https://<account>.r2.cloudflarestorage.com", text: $bucket.endpoint, monospaced: true)
            }
            HStack(alignment: .top, spacing: 12) {
                field("Region", caption: "auto for Cloudflare R2.") {
                    InputField("us-east-1", text: $bucket.region, monospaced: true)
                }
                field("Bucket") {
                    InputField("motile-backup", text: $bucket.bucket, monospaced: true)
                }
            }
            field("Access key") {
                InputField("", text: $bucket.accessKey, monospaced: true)
            }
            field("Secret key", caption: caption, failure: error) {
                InputField("", text: $bucket.secretKey, monospaced: true, secure: true)
            }
        }
    }

    private func field<Content: View>(
        _ title: String, caption: String? = nil, failure: String? = nil, @ViewBuilder content: () -> Content
    ) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(title)
                .font(.ui(size: 12, weight: .medium))
                .padding(.horizontal, 4)
            content()
            if let caption = failure ?? caption {
                Text(caption)
                    .font(.ui(size: 11.5))
                    .foregroundStyle(failure == nil ? Color.themeMutedStrongerForeground : Color.themeDestructive)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.horizontal, 4)
            }
        }
    }
    #else
    @ViewBuilder private var listForm: some View {
        Section {
            plainField("https://<account>.r2.cloudflarestorage.com", text: $bucket.endpoint)
        } header: {
            Text("Endpoint")
        } footer: {
            Text("Empty for AWS.")
                .foregroundStyle(Color.themeMutedForeground)
        }
        Section {
            plainField("us-east-1", text: $bucket.region)
        } header: {
            Text("Region")
        } footer: {
            Text("auto for Cloudflare R2.")
                .foregroundStyle(Color.themeMutedForeground)
        }
        Section("Bucket") {
            plainField("motile-backup", text: $bucket.bucket)
        }
        Section {
            plainField("Access key", text: $bucket.accessKey)
            SecureField("Secret key", text: $bucket.secretKey)
                .font(.system(.body, design: .monospaced))
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
        } header: {
            Text("Keys")
        } footer: {
            Text(error ?? caption)
                .foregroundStyle(error == nil ? Color.themeMutedForeground : Color.themeDestructive)
        }
    }

    private func plainField(_ placeholder: String, text: Binding<String>) -> some View {
        TextField(placeholder, text: text)
            .font(.system(.body, design: .monospaced))
            .textInputAutocapitalization(.never)
            .autocorrectionDisabled()
            .keyboardType(.asciiCapable)
    }
    #endif
}
