import SwiftUI

/// Whether a row of a list is picked: a radio's ring where one of the list is picked, a box where
/// any are, filled in the primary colour once it is.
struct PickMark: View {
    let picked: Bool
    let multiple: Bool

    private static let size = scaled(16)

    var body: some View {
        ZStack {
            if multiple {
                mark(RoundedRectangle(cornerRadius: Radius.xs, style: .continuous))
            } else {
                mark(Circle())
            }
            if picked, multiple {
                Image(.check, size: 10)
                    .foregroundStyle(Color.themePrimaryForeground)
            } else if picked {
                Circle()
                    .fill(Color.themePrimaryForeground)
                    .frame(width: Self.size * 0.375, height: Self.size * 0.375)
            }
        }
        .frame(width: Self.size, height: Self.size)
        .animation(.easeOut(duration: 0.12), value: picked)
    }

    private func mark(_ shape: some InsettableShape) -> some View {
        shape
            .fill(picked ? Color.themePrimary : Color.clear)
            .strokeBorder(picked ? Color.themePrimary : Color.themeMutedStrongestForeground, lineWidth: 1.5)
    }
}
