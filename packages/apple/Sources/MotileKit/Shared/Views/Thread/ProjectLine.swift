import SwiftUI

/// The line over a thread's title: its project's icon, then its parts with muted dots between.
struct ProjectLine: View {
    let project: Project?
    let parts: [String]
    let size: CGFloat
    let iconSize: CGFloat

    var body: some View {
        HStack(spacing: 4) {
            ProjectIcon(project: project, size: iconSize)
            parts.dropFirst().reduce(Text(parts.first ?? "")) { line, part in
                Text("\(line)\(Text(" · ").foregroundStyle(Color.themeMutedMoreForeground))\(part)")
            }
            .font(.system(size: size))
            .foregroundStyle(Color.themeMutedForeground)
        }
    }
}
