import SwiftUI

/// A horizontal line in the colour of the client's other borders.
struct ThemeDivider: View {
    var color = Color.themeBorder

    var body: some View {
        Rectangle()
            .fill(color)
            .frame(height: 1)
    }
}
