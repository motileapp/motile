import SwiftUI

struct SignInView: View {
    @Environment(AppStore.self) private var store

    var body: some View {
        VStack(spacing: 0) {
            Spacer()
            LogoView(size: 72)
                .shadow(color: .black.opacity(0.18), radius: 14, y: 6)
            Text("Motile")
                .font(.ui(size: 30, weight: .semibold))
                .padding(.top, 22)
            Text("The command center for coding agents.")
                .font(.ui(size: 15))
                .foregroundStyle(Color.themeSecondary)
                .padding(.top, 6)

            Button {
                store.signIn()
            } label: {
                HStack(spacing: 10) {
                    if store.signingIn {
                        ProgressView().controlSize(.small)
                    } else {
                        GoogleMark().frame(width: 16, height: 16)
                    }
                    Text(store.signingIn ? "Waiting for the browser…" : "Continue with Google")
                        .font(.ui(size: 14, weight: .medium))
                }
                .frame(width: 250, height: 40)
                .background(Color.themeRaised, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
                .overlay(RoundedRectangle(cornerRadius: 10, style: .continuous).stroke(Color.themeStrongBorder))
                .contentShape(RoundedRectangle(cornerRadius: 10))
            }
            .buttonStyle(.plain)
            .disabled(store.signingIn)
            .padding(.top, 34)

            if let error = store.signInError {
                Text(error)
                    .font(.ui(size: 13))
                    .foregroundStyle(Color.themeDanger)
                    .multilineTextAlignment(.center)
                    .frame(maxWidth: 360)
                    .padding(.top, 14)
            }
            Spacer()
            Text("Signing in links this \(Platform.device) to your account. Your threads stay on your own machines.")
                .multilineTextAlignment(.center)
                .font(.ui(size: 12))
                .foregroundStyle(Color.themeTertiary)
                .padding(.bottom, 24)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

/// Google's "G", drawn as four arcs in its colours.
private struct GoogleMark: View {
    var body: some View {
        Canvas { context, size in
            let center = CGPoint(x: size.width / 2, y: size.height / 2)
            let radius = min(size.width, size.height) / 2 - size.width * 0.11
            let width = size.width * 0.22
            let arcs: [(Double, Double, Color)] = [
                (-45, 45, Color(red: 0.26, green: 0.52, blue: 0.96)),
                (45, 150, Color(red: 0.2, green: 0.66, blue: 0.33)),
                (150, 210, Color(red: 0.98, green: 0.74, blue: 0.02)),
                (210, 318, Color(red: 0.92, green: 0.26, blue: 0.21)),
            ]
            for (start, end, color) in arcs {
                var path = Path()
                path.addArc(center: center, radius: radius, startAngle: .degrees(start), endAngle: .degrees(end), clockwise: false)
                context.stroke(path, with: .color(color), lineWidth: width)
            }
            var bar = Path()
            bar.move(to: center)
            bar.addLine(to: CGPoint(x: center.x + radius + width / 2, y: center.y))
            context.stroke(bar, with: .color(Color(red: 0.26, green: 0.52, blue: 0.96)), lineWidth: width)
        }
    }
}
