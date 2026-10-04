import SwiftUI

/// A horizontal line in the colour of the client's other borders.
struct ThemeDivider: View {
    var body: some View {
        Rectangle()
            .fill(Color.themeBorder)
            .frame(height: 1)
    }
}
