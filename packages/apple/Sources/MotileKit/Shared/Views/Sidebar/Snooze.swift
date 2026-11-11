import CMotileCore
import SwiftUI

/// A time a snooze menu offers, as the core picks them.
struct SnoozePreset: Decodable, Hashable {
    let label: String
    let until: Double

    /// "In 1 Hour (3:15 PM)", "Next Week (Mon 9:00 AM)".
    var title: String {
        let date = Date(timeIntervalSince1970: until)
        let time = date.formatted(date: .omitted, time: .shortened)
        let soon = Calendar.current.isDateInToday(date) || Calendar.current.isDateInTomorrow(date)
        return "\(label) (\(soon ? time : "\(date.formatted(.dateTime.weekday(.abbreviated))) \(time)"))"
    }
}

enum Snooze {
    static var presets: [SnoozePreset] {
        struct Answer: Decodable { let presets: [SnoozePreset] }
        return ask(["type": "presets"], Answer.self)?.presets ?? []
    }

    /// When a snooze written as a day and a time ends, or why it can't.
    static func until(day: Day, time: String) -> Result<Double, SnoozeError> {
        struct Answer: Decodable {
            let until: Double?
            let error: String?
        }
        let answer = ask(["type": "custom", "day": day.iso, "time": time], Answer.self)
        guard let until = answer?.until else { return .failure(SnoozeError(message: answer?.error ?? "Pick a time.")) }
        return .success(until)
    }

    private static func ask<Answer: Decodable>(_ question: [String: String], _: Answer.Type) -> Answer? {
        guard let data = try? JSONSerialization.data(withJSONObject: question), let json = String(data: data, encoding: .utf8),
              let answer = json.withCString({ motile_snooze($0) }) else { return nil }
        defer { motile_free_string(answer) }
        return try? JSONDecoder().decode(Answer.self, from: Data(String(cString: answer).utf8))
    }
}

struct SnoozeError: Error {
    let message: String
}

/// The threads a custom snooze is being set for.
struct CustomSnooze: Identifiable {
    let threadIDs: [String]
    let fromSidebar: Bool

    var id: String { threadIDs.joined(separator: ",") }
}

/// The times to snooze threads until, read when the menu opens, and the way to pick another.
struct SnoozeItems: View {
    /// Given, not taken from the environment, since the app's menus have none.
    let store: AppStore
    let threadIDs: [String]
    var fromSidebar = false

    var body: some View {
        ForEach(Snooze.presets, id: \.self) { preset in
            Button(preset.title) { store.snooze(threadIDs, until: preset.until, fromSidebar: fromSidebar) }
        }
        Divider()
        Button("Custom…") { store.customSnooze = CustomSnooze(threadIDs: threadIDs, fromSidebar: fromSidebar) }
    }
}

/// Snoozes threads until a day and a time the user picks.
struct CustomSnoozeSheet: View {
    @Environment(AppStore.self) private var store
    @Environment(\.closeModal) private var close
    let snooze: CustomSnooze
    @State private var day = Day(Date().addingTimeInterval(86400))
    @State private var time = "9:00"
    @State private var error: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("Snooze Until")
                .font(.ui(.lg, weight: .semibold))
            HStack(spacing: 8) {
                DateField(day.date.formatted(date: .abbreviated, time: .omitted), day: day, earliest: .today) {
                    day = $0
                    error = nil
                }
                timeField
                    .frame(width: 130)
                    .onChange(of: time) { error = nil }
            }
            if let error {
                Text(error)
                    .font(.ui(.xs))
                    .foregroundStyle(Color.themeDestructive)
            }
            HStack(spacing: 0) {
                Spacer()
                ActionButton("Cancel", margin: .beside(trailing: 4)) { close() }
                ActionButton("Snooze", variant: .primary, margin: .beside(leading: 4)) { submit() }
                    .keyboardShortcut(.defaultAction)
            }
        }
        .padding(20)
        .frame(maxWidth: 400)
        #if os(iOS)
        .presentationDetents([.height(200)])
        #endif
    }

    @ViewBuilder private var timeField: some View {
        #if os(iOS)
        TimeField(time: $time)
        #else
        InputField("Time", text: $time)
        #endif
    }

    private func submit() {
        switch Snooze.until(day: day, time: time) {
        case .success(let until):
            store.snooze(snooze.threadIDs, until: until, fromSidebar: snooze.fromSidebar)
            close()
        case .failure(let failure):
            error = failure.message
        }
    }
}
