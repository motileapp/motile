import CoreGraphics
#if os(iOS)
import UIKit
#endif

/// The path of an SVG `d` attribute: the marks drawn as shapes, and on iOS, which has nothing
/// that reads an SVG, the logos.
enum SVGPath {
    #if os(iOS)
    static func image(_ data: String, side: CGFloat, fill: UIColor, template: Bool) -> UIImage {
        let path = path(data)
        let size = CGSize(width: side, height: side)
        let image = UIGraphicsImageRenderer(size: size).image { context in
            fill.setFill()
            context.cgContext.addPath(path)
            context.cgContext.fillPath()
        }
        return template ? image.withRenderingMode(.alwaysTemplate) : image
    }
    #endif

    static func path(_ data: String) -> CGPath {
        let path = CGMutablePath()
        var numbers = Scanner(data)
        var command: Character = "M"
        var current = CGPoint.zero
        var start = CGPoint.zero
        var lastControl: CGPoint?
        while let next = numbers.command(after: command) {
            command = next
            let relative = command.isLowercase
            let from = relative ? current : .zero
            var control: CGPoint?
            switch Character(command.lowercased()) {
            case "m":
                guard let point = numbers.point(from) else { return path }
                path.move(to: point)
                (current, start) = (point, point)
                // The pairs after a move are lines.
                command = relative ? "l" : "L"
            case "l":
                guard let point = numbers.point(from) else { return path }
                path.addLine(to: point)
                current = point
            case "h":
                guard let x = numbers.number() else { return path }
                current.x = x + from.x
                path.addLine(to: current)
            case "v":
                guard let y = numbers.number() else { return path }
                current.y = y + from.y
                path.addLine(to: current)
            case "c":
                guard let first = numbers.point(from), let second = numbers.point(from), let end = numbers.point(from) else { return path }
                path.addCurve(to: end, control1: first, control2: second)
                (current, control) = (end, second)
            case "s":
                guard let second = numbers.point(from), let end = numbers.point(from) else { return path }
                let first = lastControl.map { CGPoint(x: 2 * current.x - $0.x, y: 2 * current.y - $0.y) } ?? current
                path.addCurve(to: end, control1: first, control2: second)
                (current, control) = (end, second)
            case "a":
                guard let rx = numbers.number(), let ry = numbers.number(), let rotation = numbers.number(),
                    let large = numbers.flag(), let sweep = numbers.flag(), let end = numbers.point(from)
                else { return path }
                arc(path, from: current, to: end, radii: CGSize(width: abs(rx), height: abs(ry)), rotation: rotation * .pi / 180, large: large, sweep: sweep)
                current = end
            case "z":
                path.closeSubpath()
                current = start
            default:
                return path
            }
            lastControl = control
        }
        return path
    }

    /// An elliptical arc as SVG gives it, by its ends, as the curve between them.
    private static func arc(_ path: CGMutablePath, from: CGPoint, to: CGPoint, radii: CGSize, rotation: CGFloat, large: Bool, sweep: Bool) {
        guard radii.width > 0, radii.height > 0, from != to else { return path.addLine(to: to) }
        let (cosine, sine) = (cos(rotation), sin(rotation))
        let half = CGPoint(x: (from.x - to.x) / 2, y: (from.y - to.y) / 2)
        let x = cosine * half.x + sine * half.y
        let y = -sine * half.x + cosine * half.y
        var (rx, ry) = (radii.width, radii.height)
        let stretch = x * x / (rx * rx) + y * y / (ry * ry)
        if stretch > 1 {
            rx *= sqrt(stretch)
            ry *= sqrt(stretch)
        }
        let numerator = rx * rx * ry * ry - rx * rx * y * y - ry * ry * x * x
        let factor = (large == sweep ? -1 : 1) * sqrt(max(0, numerator / (rx * rx * y * y + ry * ry * x * x)))
        let centerX = factor * rx * y / ry
        let centerY = -factor * ry * x / rx
        let center = CGPoint(
            x: cosine * centerX - sine * centerY + (from.x + to.x) / 2,
            y: sine * centerX + cosine * centerY + (from.y + to.y) / 2)
        let startAngle = atan2((y - centerY) / ry, (x - centerX) / rx)
        let endAngle = atan2((-y - centerY) / ry, (-x - centerX) / rx)
        // The arc of a circle, stretched and turned into the ellipse's.
        let ellipse = CGAffineTransform(translationX: center.x, y: center.y).rotated(by: rotation).scaledBy(x: rx, y: ry)
        path.addArc(center: .zero, radius: 1, startAngle: startAngle, endAngle: endAngle, clockwise: !sweep, transform: ellipse)
    }

    private struct Scanner {
        private let text: [Character]
        private var index = 0

        init(_ data: String) {
            text = Array(data)
        }

        private mutating func skipSeparators() {
            while index < text.count, text[index] == " " || text[index] == "," || text[index] == "\n" { index += 1 }
        }

        /// The command the next numbers belong to: a new one, or the one before again.
        mutating func command(after last: Character) -> Character? {
            skipSeparators()
            guard index < text.count else { return nil }
            guard text[index].isLetter else { return last == "z" || last == "Z" ? nil : last }
            index += 1
            return text[index - 1]
        }

        mutating func number() -> CGFloat? {
            skipSeparators()
            let start = index
            if index < text.count, text[index] == "-" || text[index] == "+" { index += 1 }
            var seenPoint = false
            while index < text.count, text[index].isNumber || (text[index] == "." && !seenPoint) {
                if text[index] == "." { seenPoint = true }
                index += 1
            }
            guard index > start, let value = Double(String(text[start..<index])) else { return nil }
            return CGFloat(value)
        }

        /// An arc's flag is one digit, which may have no space after it.
        mutating func flag() -> Bool? {
            skipSeparators()
            guard index < text.count, text[index] == "0" || text[index] == "1" else { return nil }
            index += 1
            return text[index - 1] == "1"
        }

        mutating func point(_ from: CGPoint) -> CGPoint? {
            guard let x = number(), let y = number() else { return nil }
            return CGPoint(x: x + from.x, y: y + from.y)
        }
    }
}
