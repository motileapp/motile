import SwiftUI

/// The line over a thread's title: its project's icon, then its parts with muted dots between.
struct ProjectLine: View {
    let project: Project?
    let parts: [String]
    let size: CGFloat

    var body: some View {
        HStack(spacing: 6) {
            ProjectIcon(project: project, size: (size * 1.25).rounded())
            parts.dropFirst().reduce(Text(parts.first ?? "")) { line, part in
                Text("\(line)\(Text(" · ").foregroundStyle(Color.themeTertiary))\(part)")
            }
            .font(.system(size: size))
            .foregroundStyle(Color.themeSecondary)
        }
    }
}
