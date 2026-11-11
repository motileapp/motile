#if os(iOS)
import SwiftUI

/// A field that shows a time of day, written "9:30" in `time`, and opens the system's wheels in a
/// popover to pick another. It looks as `DateField` does, with a clock at its end.
struct TimeField: View {
    @Binding private var time: String
    private let size: ControlSize
    @State private var picking = false
    @Environment(\.isEnabled) private var enabled

    init(time: Binding<String>, size: ControlSize = .regular) {
        _time = time
        self.size = size
    }

    var body: some View {
        Button { picking = true } label: {
            HStack(spacing: size.gap) {
                Text(clock.formatted(date: .omitted, time: .shortened))
                    .font(.ui(size.textSize))
                    .foregroundStyle(Color.themeForeground)
                    .lineLimit(1)
                    .frame(maxWidth: .infinity, alignment: .leading)
                Image(.clock, size: size.smallSymbol)
                    .foregroundStyle(Color.themeMutedStrongerForeground)
            }
            .padding(.horizontal, size.padding - 2)
            .frame(height: size.height)
            .contentShape(Rectangle())
            .fieldFrame(size)
        }
        .buttonStyle(DimButtonStyle())
        .opacity(.disabled, when: !enabled)
        .accessibilityLabel("Time")
        .accessibilityValue(clock.formatted(date: .omitted, time: .shortened))
        .popover(isPresented: $picking) {
            DatePicker("", selection: selection, displayedComponents: .hourAndMinute)
                .datePickerStyle(.wheel)
                .labelsHidden()
                .frame(width: SystemPickerSize.wheels.width, height: SystemPickerSize.wheels.height)
                .padding(12)
                .presentationCompactAdaptation(.popover)
        }
    }

    /// Today at `time`, as the wheels show it.
    private var clock: Date {
        let parts = time.split(separator: ":").compactMap { Int($0) }
        let (hour, minute) = parts.count == 2 ? (parts[0], parts[1]) : (9, 0)
        return Calendar.current.date(bySettingHour: hour, minute: minute, second: 0, of: Date()) ?? Date()
    }

    private var selection: Binding<Date> {
        Binding {
            clock
        } set: { date in
            let parts = Calendar.current.dateComponents([.hour, .minute], from: date)
            time = String(format: "%d:%02d", parts.hour ?? 0, parts.minute ?? 0)
        }
    }
}
#endif
