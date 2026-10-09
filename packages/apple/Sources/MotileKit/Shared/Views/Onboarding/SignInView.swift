import SwiftUI

struct SignInView: View {
    @Environment(AppStore.self) private var store

    var body: some View {
        VStack(spacing: 0) {
            Spacer()
            LogoView(size: 72)
                .shadow(.lg, .shadowStronger)
            Text("Motile")
                .font(.ui(size: 30, weight: .semibold))
                .padding(.top, 22)
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
                .padding(.bottom, 24)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

/// Google's "G": a ring with its bar, in the Google colour.
private struct GoogleMark: View {
    var body: some View {
        Canvas { context, size in
            let center = CGPoint(x: size.width / 2, y: size.height / 2)
            let radius = min(size.width, size.height) / 2 - size.width * 0.11
            let width = size.width * 0.22
            var ring = Path()
            ring.addArc(center: center, radius: radius, startAngle: .degrees(-45), endAngle: .degrees(318), clockwise: false)
            context.stroke(ring, with: .color(.themeGoogle), lineWidth: width)
            var bar = Path()
            bar.move(to: center)
            bar.addLine(to: CGPoint(x: center.x + radius + width / 2, y: center.y))
            context.stroke(bar, with: .color(.themeGoogle), lineWidth: width)
        }
    }
}
