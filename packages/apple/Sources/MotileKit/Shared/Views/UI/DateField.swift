import SwiftUI

/// A day of the calendar, as a date field picks one: no time of day and no time zone.
struct Day: Hashable, Comparable {
    let year: Int
    let month: Int
    let day: Int

    init(year: Int, month: Int, day: Int) {
        self.year = year
        self.month = month
        self.day = day
    }

    /// The day `date` falls on here.
    init(_ date: Date) {
        let parts = Calendar.current.dateComponents([.year, .month, .day], from: date)
        self.init(year: parts.year ?? 1970, month: parts.month ?? 1, day: parts.day ?? 1)
    }

    /// The day written as "2026-10-19".
    init?(iso: String) {
        let parts = iso.prefix(10).split(separator: "-").compactMap { Int($0) }
        guard parts.count == 3 else { return nil }
        self.init(year: parts[0], month: parts[1], day: parts[2])
    }

    static var today: Day { Day(Date()) }

    /// Midday of the day here, which every picker shows as that day.
    var date: Date {
        Calendar.current.date(from: DateComponents(year: year, month: month, day: day, hour: 12)) ?? Date()
    }

    var iso: String { String(format: "%04d-%02d-%02d", year, month, day) }

    static func < (lhs: Day, rhs: Day) -> Bool {
        (lhs.year, lhs.month, lhs.day) < (rhs.year, rhs.month, rhs.day)
    }
}

/// A field that shows a day and opens the system's calendar to pick another, held to `earliest`
/// and `latest`: in a popover on the Mac and on iOS alike. It looks as `InputField` does, with the
/// calendar's symbol at its end; `text` is the day as the caller writes it.
struct DateField: View {
    private let text: String?
    private let placeholder: String
    private let day: Day?
    private let earliest: Day?
    private let latest: Day?
    private let size: ControlSize
    private let onPick: (Day) -> Void
    @State private var picking = false
    @Environment(\.isEnabled) private var enabled

    init(
        _ text: String?, placeholder: String = "Date", day: Day?, earliest: Day? = nil, latest: Day? = nil, size: ControlSize = .regular,
        onPick: @escaping (Day) -> Void
    ) {
        self.text = text
        self.placeholder = placeholder
        self.day = day
        self.earliest = earliest
        self.latest = latest
        self.size = size
        self.onPick = onPick
    }

    var body: some View {
        Button { picking = true } label: {
            HStack(spacing: size.gap) {
                Text(text ?? placeholder)
                    .font(.ui(size: size.textSize + 0.5))
                    .foregroundStyle(text == nil ? Color.themeMutedStrongerForeground : Color.themeForeground)
                    .lineLimit(1)
                    .frame(maxWidth: .infinity, alignment: .leading)
                Image(.calendar, size: size.smallSymbol)
                    .foregroundStyle(Color.themeMutedStrongerForeground)
            }
            .padding(.horizontal, size.padding - 2)
            .frame(height: size.height)
            .contentShape(Rectangle())
            .fieldFrame(size)
        }
        .buttonStyle(DimButtonStyle())
        .opacity(.disabled, when: !enabled)
        .accessibilityLabel(placeholder)
        .accessibilityValue(text ?? "")
        .popover(isPresented: $picking) {
            DatePicker("", selection: selection, in: bounds, displayedComponents: .date)
                .datePickerStyle(.graphical)
                .labelsHidden()
                .padding(12)
                .presentationCompactAdaptation(.popover)
        }
    }

    private var selection: Binding<Date> {
        Binding {
            (day ?? .today).date
        } set: { date in
            picking = false
            onPick(Day(date))
        }
    }

    private var bounds: ClosedRange<Date> {
        let low = earliest?.date ?? .distantPast
        return low...max(low, latest?.date ?? .distantFuture)
    }
}
