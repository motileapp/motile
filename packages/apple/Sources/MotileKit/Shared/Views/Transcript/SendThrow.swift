import Foundation
import QuartzCore
import SwiftUI

#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// The composer's text, which a message is thrown from into the transcript when it is sent.
protocol ThrowSource: PlatformView {
    /// What is written, as it is sent.
    var written: String { get }
    /// Where the written text is in the view.
    var writtenRect: CGRect { get }
    /// Brings the placeholder back once a message thrown from here has left, so that it doesn't
    /// show through the message's bubble while that is still taking its colour.
    func fadeInPlaceholder()
}

extension ThrowSource {
    static var placeholderDelay: TimeInterval { 0.15 }
    static var placeholderFade: TimeInterval { 0.2 }
}

/// Where a message was in the composer when it was sent, in the transcript: its text, and the
/// tile of each file it attaches, when the composer shows one.
struct ThrowOrigin {
    let text: CGRect?
    let tiles: [CGRect?]
    let source: ThrowSource
}

enum ThrowSources {
    static let composers = NSHashTable<PlatformView>.weakObjects()
    static let tiles = NSHashTable<ThrowAnchorView>.weakObjects()

    /// Where the composer of the window shows the message of `row` in `view`, while it still shows it.
    static func origin(of row: RowModel, in view: PlatformView) -> ThrowOrigin? {
        let composer = composers.allObjects.lazy.compactMap { $0 as? ThrowSource }.first { $0.window != nil && $0.window === view.window }
        guard let composer, let text = row.sentText else { return nil }
        let written = composer.written.trimmingCharacters(in: .whitespacesAndNewlines)
        guard written == text.trimmingCharacters(in: .whitespacesAndNewlines) else { return nil }
        var anchors = tiles.allObjects.filter { $0.window === view.window }
        let rects = row.sentFiles.map { file -> CGRect? in
            guard let index = anchors.firstIndex(where: { $0.file == file }) else { return nil }
            let anchor = anchors.remove(at: index)
            return view.convert(anchor.bounds, from: anchor)
        }
        guard !written.isEmpty || rects.contains(where: { $0 != nil }) else { return nil }
        return ThrowOrigin(text: written.isEmpty ? nil : view.convert(composer.writtenRect, from: composer), tiles: rects, source: composer)
    }
}

/// Marks where the composer shows a file the message attaches, for the file to be thrown from there.
struct ThrowAnchor {
    let file: AttachedFile
}

final class ThrowAnchorView: PlatformView {
    var file: AttachedFile?

    init() {
        super.init(frame: .zero)
        ThrowSources.tiles.add(self)
        #if os(iOS)
        isUserInteractionEnabled = false
        #endif
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    #if os(macOS)
    override func hitTest(_ point: NSPoint) -> NSView? { nil }
    #endif
}

#if os(macOS)
extension ThrowAnchor: NSViewRepresentable {
    func makeNSView(context: Context) -> ThrowAnchorView { ThrowAnchorView() }
    func updateNSView(_ view: ThrowAnchorView, context: Context) { view.file = file }
}
#else
extension ThrowAnchor: UIViewRepresentable {
    func makeUIView(context: Context) -> ThrowAnchorView { ThrowAnchorView() }
    func updateUIView(_ view: ThrowAnchorView, context: Context) { view.file = file }
}
#endif

/// A row whose message lies in a bubble, which can be thrown in from the composer.
protocol ThrowableRow: RowView {
    /// Where the bubble is in the row, as the row laid it out.
    var bubbleFrame: CGRect { get }
    /// Where the text is in the bubble.
    var textFrame: CGRect { get }
    /// What the message attaches, in its order, and where each is in the bubble.
    var attachedPieces: [(view: PlatformView, frame: CGRect)] { get }
    /// Whether the bubble can take another size while it flies, its text staying at its top left.
    var reshapes: Bool { get }
    /// Gives the bubble that size, keeping its right edge and top, and its fill that much of its colour.
    func shapeBubble(_ size: CGSize, fill: CGFloat)
}

/// A message on its way from the composer to its row, as Messages throws it: a bubble that starts
/// around the text where it was written and takes its own size, travels to the row on a spring
/// that leaves a moment before the one that lifts it, so that it curves, and shrinks as it leaves
/// and grows back as it lands. Its row stays hidden until it is there.
final class ThrownMessage {
    /// The springs and curves Messages throws with.
    private static let travel = Spring(mass: 1, stiffness: 141.759, damping: 17.3503)
    private static let lift: CFTimeInterval = 0.055
    private static let shrink = Spring(mass: 2, stiffness: 310, damping: 38)
    private static let regrow = Spring(mass: 2, stiffness: 320, damping: 38)
    private static let regrowAfter: CFTimeInterval = 0.185
    private static let reshape = CubicCurve(0.542, 0, 0.58, 1)
    private static let reshapeTime: CFTimeInterval = 0.2
    static let duration: CFTimeInterval = 1

    /// A file the message attaches, which flies from its tile in the composer to its place in the
    /// bubble while the bubble takes its shape. A file's card looks unlike its tile and comes in.
    private struct Piece {
        let view: PlatformView
        let from: CGRect
        let to: CGRect
        let fades: Bool
    }

    /// The row it lands on: the message on its way, then the one the server has.
    var rowID: String
    let view: PlatformView = CopyView()
    private let row: ThrowableRow
    /// The bubble where it starts, in the transcript.
    private let start: CGRect
    private let smallest: CGFloat
    private let pieces: [Piece]
    /// How far it is on its way, which only starts on the first frame after the send and only
    /// moves a frame at a time, so that a moment the main thread is held doesn't skip any of it.
    private var elapsed: CFTimeInterval = 0
    private var lastTick: CFTimeInterval?

    init?(_ model: RowModel, from origin: ThrowOrigin, width: CGFloat, owner: RowOwner) {
        guard let row = RowView.make(for: model) as? ThrowableRow else { return nil }
        row.owner = owner
        row.configure(model)
        row.showsMeta = false
        row.frame = CGRect(x: 0, y: 0, width: width, height: row.layout(width: width))
        let final = CGRect(origin: .zero, size: row.bubbleFrame.size)
        let attached = row.attachedPieces
        // What was written lies where the bubble's text will, or the tiles where its files will.
        let known = zip(attached, origin.tiles).compactMap { piece, tile in tile.map { (piece.frame, $0) } }
        let (inBubble, inComposer) = origin.text.map { (row.textFrame, $0) }
            ?? (known.map(\.0).reduce(CGRect.null) { $0.union($1) }, known.map(\.1).reduce(CGRect.null) { $0.union($1) })
        guard !inBubble.isNull, !inComposer.isNull else { return nil }
        let size = row.reshapes ? CGSize(
            width: inComposer.width + inBubble.minX + final.width - inBubble.maxX,
            height: inComposer.height + inBubble.minY + final.height - inBubble.maxY
        ) : final.size
        let start = CGRect(origin: CGPoint(x: inComposer.minX - inBubble.minX, y: inComposer.minY - inBubble.minY), size: size)
        self.start = start
        pieces = zip(zip(attached, origin.tiles), model.sentFiles.map(\.pictured)).compactMap { pair, pictured in
            guard let tile = pair.1 else { return nil }
            return Piece(view: pair.0.view, from: tile.offsetBy(dx: -start.minX, dy: -start.minY), to: pair.0.frame, fades: !pictured)
        }
        // A taller composer throws a message that shrinks less, as in Messages.
        smallest = min(0.9, max(0.7, 0.7 + (start.height - 40.25) * 0.000827))
        rowID = model.id
        self.row = row
        view.addSubview(row)
    }

    var done: Bool { elapsed >= Self.duration }

    func tick() {
        let now = CACurrentMediaTime()
        defer { lastTick = now }
        guard let lastTick else { return }
        elapsed += min(now - lastTick, 1.0 / 30)
    }

    /// How far along the transcript is on its way to its end, which it travels with the message.
    var scrolled: CGFloat { Self.travel.progress(elapsed - Self.lift) }

    /// Puts the bubble where it is by now on its way to `target`, the row's bubble, both in the
    /// transcript, whose coordinates `host` takes from `transcript`.
    func place(toward target: CGRect, in host: PlatformView, from transcript: PlatformView) {
        let time = elapsed
        let reshaped = Self.reshape.value(time / Self.reshapeTime)
        let size = CGSize(
            width: start.width + (target.width - start.width) * reshaped,
            height: start.height + (target.height - start.height) * reshaped
        )
        let right = start.maxX + (target.maxX - start.maxX) * Self.travel.progress(time)
        let middle = start.midY + (target.midY - start.midY) * Self.travel.progress(time - Self.lift)
        let scale = (1 + (smallest - 1) * Self.shrink.progress(time))
            * (1 + (1 / smallest - 1) * Self.regrow.progress(time - Self.regrowAfter))
        row.shapeBubble(size, fill: reshaped)
        for piece in pieces { place(piece, at: reshaped) }
        let bubble = CGRect(x: right - size.width, y: middle - size.height / 2, width: size.width, height: size.height)
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        #if os(iOS)
        view.transform = .identity
        #endif
        view.frame = host.convert(bubble, from: transcript)
        let frame = row.bubbleFrame
        row.frame.origin = CGPoint(x: -frame.minX, y: -frame.minY)
        // Around the bubble's right edge, where it is held.
        #if os(macOS)
        view.layer?.transform = CATransform3DConcat(
            CATransform3DConcat(CATransform3DMakeTranslation(-size.width, -size.height / 2, 0), CATransform3DMakeScale(scale, scale, 1)),
            CATransform3DMakeTranslation(size.width, size.height / 2, 0)
        )
        #else
        view.transform = CGAffineTransform(translationX: size.width / 2, y: 0).scaledBy(x: scale, y: scale).translatedBy(x: -size.width / 2, y: 0)
        #endif
        CATransaction.commit()
    }

    /// Moves the file from its tile toward its place, scaled as a whole between their sizes.
    private func place(_ piece: Piece, at shaped: CGFloat) {
        let from = CGPoint(x: piece.from.midX, y: piece.from.midY)
        let to = CGPoint(x: piece.to.midX, y: piece.to.midY)
        let offset = CGPoint(x: (from.x - to.x) * (1 - shaped), y: (from.y - to.y) * (1 - shaped))
        let first = min(piece.from.width / max(1, piece.to.width), piece.from.height / max(1, piece.to.height))
        let scale = first + (1 - first) * shaped
        piece.view.opacity = piece.fades ? shaped : 1
        #if os(macOS)
        guard let layer = piece.view.layer else { return }
        let (width, height) = (piece.view.bounds.width, piece.view.bounds.height)
        // AppKit flips a layer only where flipped and unflipped views meet, so down is down once
        // an odd number of layers above it are flipped.
        var flipped = false
        var above = layer.superlayer
        while let next = above {
            if next.isGeometryFlipped { flipped.toggle() }
            above = next.superlayer
        }
        let down: CGFloat = flipped ? 1 : -1
        layer.transform = CATransform3DConcat(
            CATransform3DConcat(
                CATransform3DConcat(CATransform3DMakeTranslation(-width / 2, -height / 2, 0), CATransform3DMakeScale(scale, scale, 1)),
                CATransform3DMakeTranslation(width / 2, height / 2, 0)
            ),
            CATransform3DMakeTranslation(offset.x, offset.y * down, 0)
        )
        #else
        piece.view.transform = CGAffineTransform(translationX: offset.x, y: offset.y).scaledBy(x: scale, y: scale)
        #endif
    }
}

/// What a thrown message flies as, a copy of its row: it takes no clicks and isn't read out, as
/// its row is.
private final class CopyView: FlippedView {
    override init(frame: CGRect) {
        super.init(frame: frame)
        #if os(macOS)
        wantsLayer = true
        #else
        isUserInteractionEnabled = false
        accessibilityElementsHidden = true
        #endif
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    #if os(macOS)
    override func hitTest(_ point: NSPoint) -> NSView? { nil }
    override func accessibilityChildren() -> [Any]? { [] }
    #endif
}

/// A spring as Core Animation runs one from rest: how far along it is `time` seconds in.
struct Spring {
    let mass: Double
    let stiffness: Double
    let damping: Double

    func progress(_ time: Double) -> CGFloat {
        guard time > 0 else { return 0 }
        let natural = (stiffness / mass).squareRoot()
        let ratio = damping / (2 * (stiffness * mass).squareRoot())
        guard ratio < 1 else { return CGFloat(1 - exp(-natural * time) * (1 + natural * time)) }
        let damped = natural * (1 - ratio * ratio).squareRoot()
        let decay = exp(-ratio * natural * time)
        return CGFloat(1 - decay * (cos(damped * time) + ratio * natural / damped * sin(damped * time)))
    }
}

/// A timing curve as `CAMediaTimingFunction(controlPoints:)` draws it.
struct CubicCurve {
    let x1: Double, y1: Double, x2: Double, y2: Double

    init(_ x1: Double, _ y1: Double, _ x2: Double, _ y2: Double) {
        (self.x1, self.y1, self.x2, self.y2) = (x1, y1, x2, y2)
    }

    func value(_ x: Double) -> CGFloat {
        guard x > 0 else { return 0 }
        guard x < 1 else { return 1 }
        var (low, high) = (0.0, 1.0)
        for _ in 0..<24 {
            let middle = (low + high) / 2
            if Self.bezier(middle, x1, x2) < x { low = middle } else { high = middle }
        }
        return CGFloat(Self.bezier((low + high) / 2, y1, y2))
    }

    private static func bezier(_ t: Double, _ a: Double, _ b: Double) -> Double {
        let u = 1 - t
        return 3 * u * u * t * a + 3 * u * t * t * b + t * t * t
    }
}
