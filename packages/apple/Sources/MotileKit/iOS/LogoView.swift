#if os(iOS)
import SwiftUI

/// The client's icon: its mark on its tile, `size` wide.
struct LogoView: View {
    let size: CGFloat

    var body: some View {
        RoundedRectangle(cornerRadius: size * 0.225, style: .continuous)
            .fill(Color(platform: Theme.hex(0x0a0b0f)))
            .overlay {
                Mark()
                    .fill(.white)
                    .frame(width: size * 720 / 1024, height: size * 720 / 1024)
            }
            .frame(width: size, height: size)
    }
}

/// Motile's mark, as the app icon draws it (apps/macos/Resources/AppIcon.icon).
struct Mark: Shape {
    private static let outline = SVGPath.path(
        "M18.477 1.929c1.107.2 1.988 1.241 1.701 2.468-.281 1.205.603 2.372 1.883 2.488 1.863.167 2.616 2.39 1.212 3.577a1.994 1.994 0 0 0 0 3.075c1.404 1.186.651 3.41-1.212 3.577-1.28.116-2.164 1.284-1.883 2.488.41 1.752-1.56 3.126-3.17 2.212-1.107-.63-2.538-.184-3.048.95-.508 1.128-1.81 1.484-2.818 1.068-.934-.385-.922-1.601-.598-2.559L16.52 3.63c.315-.929.993-1.874 1.958-1.7zm-8.437-.693C10.547.108 11.849-.248 12.857.168c.934.385.922 1.6.598 2.558L7.48 20.37c-.314.929-.992 1.874-1.957 1.7-1.107-.199-1.988-1.241-1.701-2.469.281-1.204-.603-2.372-1.884-2.488-1.863-.167-2.615-2.39-1.21-3.577a1.995 1.995 0 0 0 0-3.075c-1.405-1.187-.653-3.41 1.21-3.577 1.28-.116 2.165-1.283 1.884-2.488-.41-1.751 1.56-3.125 3.17-2.21 1.106.629 2.538.182 3.048-.95z")

    func path(in rect: CGRect) -> Path {
        let scale = CGAffineTransform(translationX: rect.minX, y: rect.minY).scaledBy(x: rect.width / 24, y: rect.height / 24)
        return Path(Self.outline).applying(scale)
    }
}
#endif
