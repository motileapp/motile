#if os(iOS)
import SwiftUI

/// The client's icon: its mark on its tile, `size` wide.
struct LogoView: View {
    let size: CGFloat

    var body: some View {
        RoundedRectangle(cornerRadius: size * 0.225, style: .continuous)
            .fill(Color(platform: Theme.hex(0x08090d)))
            .overlay {
                Mark()
                    .fill(.white)
                    .frame(width: size * 0.7, height: size * 0.7)
            }
            .frame(width: size, height: size)
    }
}

/// Motile's mark, as the app icon draws it (apps/macos/Resources/AppIcon.icon).
struct Mark: Shape {
    private static let outline = SVGPath.path(
        "M17.72 1.928c1.417-.251 2.809.97 2.458 2.47-.281 1.204.603 2.371 1.883 2.487 1.863.167 2.616 2.39 1.212 3.577a1.995 1.995 0 0 0 0 3.075c1.404 1.186.651 3.41-1.212 3.577-1.28.116-2.164 1.284-1.883 2.488.41 1.752-1.561 3.126-3.17 2.212-1.107-.63-2.538-.184-3.048.95-.629 1.396-2.472 1.609-3.472.64-.363-.351-.358-.907-.195-1.385l6.47-19.112c.157-.464.474-.894.956-.98zm-7.68-.692c.628-1.396 2.47-1.61 3.471-.641.363.351.358.908.196 1.386l-6.471 19.11c-.157.465-.473.896-.956.981-1.418.251-2.809-.97-2.458-2.47.281-1.204-.603-2.372-1.884-2.488-1.863-.167-2.615-2.39-1.21-3.577a1.995 1.995 0 0 0 0-3.075c-1.405-1.187-.653-3.41 1.21-3.577 1.28-.116 2.165-1.283 1.884-2.488-.41-1.751 1.56-3.125 3.17-2.21 1.106.629 2.537.182 3.048-.95z")

    func path(in rect: CGRect) -> Path {
        let scale = CGAffineTransform(translationX: rect.minX, y: rect.minY).scaledBy(x: rect.width / 24, y: rect.height / 24)
        return Path(Self.outline).applying(scale)
    }
}
#endif
