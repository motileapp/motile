import SwiftUI

/// A row of a `RecycledList`. It is drawn in a view of its own, outside the list's hierarchy, so
/// it is handed what the rows read from there.
struct RecycledRow<Row: View>: View {
    let row: Row
    let store: AppStore
    let surface: Surface

    var body: some View {
        row
            .environment(store)
            .environment(\.surface, surface)
    }
}
