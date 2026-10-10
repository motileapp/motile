import SwiftUI

struct SignInView: View {
    @Environment(AppStore.self) private var store

    var body: some View {
        VStack(spacing: 0) {
            Spacer()
            Mark()
                .fill(Color.themeEmphasizedForeground)
                .frame(width: 48, height: 48)
            Text("Motile")
                .font(.ui(size: 30, weight: .semibold))
                .padding(.top, 14)
            Text("The command center for coding agents.")
                .font(.ui(size: 15))
                .foregroundStyle(Color.themeMutedForeground)
                .padding(.top, 6)

            ActionButton("Continue with Google", picture: AnyView(GoogleMark()), size: .large, pending: store.signingIn, fills: true) {
                store.signIn()
            }
            .frame(width: 250)
            .padding(.top, 34)
            if store.signingIn {
                Text("Waiting for the browser")
                    .font(.ui(size: 12))
                    .foregroundStyle(Color.themeMutedStrongerForeground)
                    .padding(.top, 10)
            }

            if let error = store.signInError {
                Text(error)
                    .font(.ui(size: 13))
                    .foregroundStyle(Color.themeDestructive)
                    .multilineTextAlignment(.center)
                    .frame(maxWidth: 360)
                    .padding(.top, 14)
            }
            Spacer()
            Text("Signing in links this \(Platform.device) to your account. Your threads stay on your own machines.")
                .multilineTextAlignment(.center)
                .font(.ui(size: 12))
                .foregroundStyle(Color.themeMutedStrongerForeground)
                .padding(.horizontal, 20)
                .padding(.bottom, 24)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

/// Motile's mark, as the app icon draws it (apps/macos/Resources/AppIcon.icon).
private struct Mark: Shape {
    private static let outline = SVGPath.path(
        "M18.477 1.929c1.107.2 1.988 1.241 1.701 2.468-.281 1.205.603 2.372 1.883 2.488 1.863.167 2.616 2.39 1.212 3.577a1.994 1.994 0 0 0 0 3.075c1.404 1.186.651 3.41-1.212 3.577-1.28.116-2.164 1.284-1.883 2.488.41 1.752-1.56 3.126-3.17 2.212-1.107-.63-2.538-.184-3.048.95-.508 1.128-1.81 1.484-2.818 1.068-.934-.385-.922-1.601-.598-2.559L16.52 3.63c.315-.929.993-1.874 1.958-1.7zm-8.437-.693C10.547.108 11.849-.248 12.857.168c.934.385.922 1.6.598 2.558L7.48 20.37c-.314.929-.992 1.874-1.957 1.7-1.107-.199-1.988-1.241-1.701-2.469.281-1.204-.603-2.372-1.884-2.488-1.863-.167-2.615-2.39-1.21-3.577a1.995 1.995 0 0 0 0-3.075c-1.405-1.187-.653-3.41 1.21-3.577 1.28-.116 2.165-1.283 1.884-2.488-.41-1.751 1.56-3.125 3.17-2.21 1.106.629 2.538.182 3.048-.95z")

    func path(in rect: CGRect) -> Path {
        let scale = CGAffineTransform(translationX: rect.minX, y: rect.minY).scaledBy(x: rect.width / 24, y: rect.height / 24)
        return Path(Self.outline).applying(scale)
    }
}

/// Google's "G" in its four colours, from Google's own mark on its 48-point grid.
private struct GoogleMark: View {
    private static let pieces: [(Color, CGPath)] = [
        (Color.themeGoogleRed, SVGPath.path("M24 9.5c3.54 0 6.71 1.22 9.21 3.6l6.85-6.85C35.9 2.38 30.47 0 24 0 14.62 0 6.51 5.38 2.56 13.22l7.98 6.19C12.43 13.72 17.74 9.5 24 9.5z")),
        (Color.themeGoogleBlue, SVGPath.path("M46.98 24.55c0-1.57-.15-3.09-.38-4.55H24v9.02h12.94c-.58 2.96-2.26 5.48-4.78 7.18l7.73 6c4.51-4.18 7.09-10.36 7.09-17.65z")),
        (Color.themeGoogleYellow, SVGPath.path("M10.53 28.59c-.48-1.45-.76-2.99-.76-4.59s.27-3.14.76-4.59l-7.98-6.19C.92 16.46 0 20.12 0 24c0 3.88.92 7.54 2.56 10.78l7.97-6.19z")),
        (Color.themeGoogleGreen, SVGPath.path("M24 48c6.48 0 11.93-2.13 15.89-5.81l-7.73-6c-2.15 1.45-4.92 2.3-8.16 2.3-6.26 0-11.57-4.22-13.47-9.91l-7.98 6.19C6.51 42.62 14.62 48 24 48z")),
    ]

    var body: some View {
        Canvas { context, size in
            let side = min(size.width, size.height)
            context.translateBy(x: (size.width - side) / 2, y: (size.height - side) / 2)
            context.scaleBy(x: side / 48, y: side / 48)
            for (color, path) in Self.pieces {
                context.fill(Path(path), with: .color(color))
            }
        }
    }
}
