import Foundation
import QuartzCore
import os

#if os(macOS)
import AppKit
#else
import UIKit
#endif

/// The transcript: a scrolling column of rows of which only the ones on screen exist as views.
///
/// Every row has a height, measured off the main thread before the row is scrolled to. A row's
/// views are made when it scrolls in and reused when it scrolls out, so a thread of any length
/// costs what is on screen. When heights above the viewport change, as they do with the width,
/// the scroll position is corrected in the same pass, so nothing on screen moves.
final class TranscriptView: FlippedView, RowOwner {
    /// Rows that came into view with code that isn't highlighted.
    var onNeedHighlight: (([String]) -> Void)?
    var onToggleRow: ((String) -> Void)?
    /// A row needs the file of an image or a video; it is called back with it.
    var onNeedMedia: ((String, @escaping (URL?) -> Void) -> Void)?
    var onViewMedia: (([ViewedMedia], Int) -> Void)?
    /// A queued message is to be given to the agent now, or taken back.
    var onSendQueued: ((String) -> Void)?
    var onCancelQueued: ((String) -> Void)?
    /// Asks for the turns before the first row, and is called back once they are among the rows.
    var onNeedEarlier: ((@escaping () -> Void) -> Void)?
    /// Lets go of the turns before the last rows, that many, and is called back once they are gone.
    var onTrimEarlier: ((Int, @escaping () -> Void) -> Void)?
    /// The diff of the turn that ended with the item is to be shown, with a file in view.
    var onOpenDiff: ((String, String?) -> Void)?
    /// What the agent did that the item's tool call started is to be shown.
    var onOpenAgent: ((String) -> Void)?
    /// Whether rows are measured for this view's width as they are decoded. Only one transcript
    /// can have that; another measures its rows when they arrive.
    var measuresAhead = true

    /// Room left under the last row for what floats over the transcript's end.
    var bottomInset: CGFloat = 0 {
        didSet {
            guard bottomInset != oldValue else { return }
            updateVisible()
            jumpButton.frame.origin.y = jumpButtonY
            layoutFade()
        }
    }

    /// Room under what floats over the transcript's end. The rows fade out before it.
    var bottomGap: CGFloat = 0 {
        didSet {
            guard bottomGap != oldValue else { return }
            layoutFade()
        }
    }

    private static let bottomFade: CGFloat = 48

    private static let jumpButtonGap: CGFloat = 12

    /// The room between the last row and the composer.
    static let composerGap: CGFloat = 24

    /// Above the composer, which starts `composerGap` into the inset.
    private var jumpButtonY: CGFloat {
        bounds.height - bottomInset + Self.composerGap - Self.jumpButtonGap - Self.jumpButtonSide
    }

    private static let jumpButtonSide: CGFloat = Platform.scale > 1 ? 38 : 32

    /// What covers the top of the transcript, like a bar the rows scroll under.
    var topInset: CGFloat = 0 {
        didSet {
            guard topInset != oldValue else { return }
            updateVisible()
            layoutFade()
        }
    }

    /// Room above the first row, which the transcript fades out in.
    private var topPadding: CGFloat { 20 + topInset }
    private static let overscan: CGFloat = 400
    private static let pooledPerKind = 10
    /// As tall as the row that ends a turn, which takes the working line's place.
    private static let workingHeight = TurnEndRowView.height
    /// How close to the end a scroll has to come for the view to follow the end again.
    private static let pinDistance: CGFloat = 1

    /// How far the end has to be below the viewport for the jump button to show.
    private var jumpDistance: CGFloat { max(200, viewportHeight / 2) }

    private let scroller = TranscriptScroller()
    private var document: FlippedView { scroller.document }
    private let working = WorkingView()
    private let jumpButton = SurfaceView()
    /// Fades the rows out under the top bar and towards the gap under the composer. The jump
    /// button is not under it.
    private let fade = CAGradientLayer()

    private var rows: [RowModel] = []
    private var hasPending = false
    private var heights: [CGFloat] = []
    private var measured: [Bool] = []
    /// `offsets[i]` is where row `i` starts; the last entry is where the rows end.
    private var offsets: [CGFloat] = [0]
    private var views: [String: RowView] = [:]
    /// Views whose content or width changed since they were last laid out.
    private var stale: Set<String> = []
    private var pool: [String: [RowView]] = [:]
    private var expanded: Set<String> = []
    private var requestedHighlight: Set<String> = []
    private var activity = Activity()
    /// The row that ended the turn the server still reports as running.
    private var endOfRunningTurn: String?

    /// Whether the view follows the end of the transcript as it grows. It then rests on the end
    /// whenever the user isn't scrolling.
    private var pinned = true
    /// Whether the user's fingers, or the momentum they gave it, are moving the viewport.
    private var userScrolling = false
    /// Where the end was when the user pulled the viewport past it; the scroll view bounces
    /// back to there by itself.
    private var bounceEnd: CGFloat?
    private var updating = false
    /// Where the viewport was after the last scroll, to tell which way the next one goes.
    private var lastScrollY: CGFloat = 0
    /// Moves the viewport to the end with every frame while a reply streams.
    private var glide: CADisplayLink?
    /// Whether the thread has turns before the first row. They are asked for when the viewport
    /// comes within `earlierDistance` of the first row, and let go when it rests on the end
    /// with more than `trimDistance` above it.
    private var hasEarlier = false
    private var loadingEarlier = false
    private var trimming = false
    /// Where the viewport was when rows came in front of it while it bounced at the top.
    private var heldAnchor: Anchor?
    /// Rows that just arrived in a streaming reply; they fade in.
    private var fresh: Set<String> = []
    /// The group or fold that was just clicked. It stays where it is while its rows come and go.
    private var toggled: Anchor?
    private var layoutWidth: CGFloat = 0
    /// Where the pointer is over the transcript, and the row whose time and copy button it shows:
    /// the message under it, or the end of the reply under it.
    private var pointerAt: CGPoint?
    private var metaRowID: String?
    private static let measuring = DispatchQueue(label: "app.motile.heights", qos: .userInitiated)
    /// Stops the measuring that is under way, when rows or the width it measures for are gone.
    private var measuringStopped: OSAllocatedUnfairLock<Bool>?

    private struct Anchor {
        let id: String
        /// How far the viewport's top is below the top of the row.
        let delta: CGFloat
    }

    override init(frame: CGRect) {
        super.init(frame: frame)
        addSubview(scroller)
        fade.colors = [PlatformColor.clear.cgColor, PlatformColor.black.cgColor, PlatformColor.black.cgColor, PlatformColor.clear.cgColor]
        scroller.fadeMask = fade
        document.addSubview(working)
        working.isHidden = true

        let side = Self.jumpButtonSide
        jumpButton.fill = Theme.raised
        jumpButton.stroke = Theme.strongBorder
        jumpButton.radius = side / 2
        jumpButton.frame = CGRect(x: 0, y: 0, width: side, height: side)
        jumpButton.isHidden = true
        jumpButton.tip = "Scroll to end"
        jumpButton.describe("Scroll to end", button: true)
        let arrow = SymbolView("arrow.down", size: 12, weight: .semibold)
        arrow.frame = CGRect(x: (side - 16) / 2, y: (side - 16) / 2, width: 16, height: 16)
        jumpButton.addSubview(arrow)
        jumpButton.onClick = { [weak self] in self?.scrollToEnd() }
        jumpButton.dropShadow(opacity: 0.18, radius: 8, down: 2)
        addSubview(jumpButton)

        if Platform.hoverReveals {
            onHover = { [weak self] point in
                self?.pointerAt = point
                self?.updateMetaRow()
            }
        }
        scroller.onScroll = { [weak self] in self?.scrolled() }
        scroller.onUserScroll = { [weak self] began in
            if began { self?.userScrollBegan() } else { self?.userScrollEnded() }
        }
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    deinit {
        measuringStopped?.withLock { $0 = true }
        glide?.invalidate()
    }

    // MARK: Geometry

    private var columnWidth: CGFloat {
        max(200, min(bounds.width - Theme.contentPadding * 2, Theme.contentWidth))
    }

    private var columnX: CGFloat {
        ((bounds.width - columnWidth) / 2).rounded()
    }

    private var viewportHeight: CGFloat { scroller.viewportHeight }

    private var earlierDistance: CGFloat { viewportHeight * 3 }
    private var trimDistance: CGFloat { viewportHeight * 12 }

    /// Where the viewport's top is when it shows the end.
    private var endY: CGFloat { max(0, scroller.documentSize.height - viewportHeight) }

    /// Whether the line that says the agent is at work shows under the rows. The row that ends
    /// a turn replaces it right away, a moment before the server says that the agent stopped,
    /// unless agents it started work on. An agent that only monitors is shown by the composer
    /// instead.
    private var showsWorking: Bool {
        guard activity.running else { return false }
        return activity.agents > 0 || endOfRunningTurn == nil || rows.last?.id != endOfRunningTurn
    }

    /// The row the working line goes above: the first of the messages that wait for the agent,
    /// which are the last rows the core sends. Without them it goes under every row.
    private var workingIndex: Int {
        let end = rows.count - (hasPending ? 1 : 0)
        var index = end
        while index > 0, rows[index - 1].isQueued { index -= 1 }
        return index == end ? rows.count : index
    }

    private var contentHeight: CGFloat {
        let workingHeight = showsWorking ? Self.workingHeight : 0
        return topPadding + (offsets.last ?? 0) + workingHeight + bottomInset
    }

    private func layoutFade() {
        guard bounds.height > 0 else { return }
        scroller.setIndicatorInsets(top: topInset, bottom: bottomInset)
        let end = bounds.height - bottomGap
        let stops = [topInset, topPadding, end - min(Self.bottomFade, max(0, bottomInset - bottomGap)), end]
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        fade.frame = bounds
        fade.locations = stops.map { NSNumber(value: Double(min(1, max(0, $0 / bounds.height)))) }
        CATransaction.commit()
    }

    override func layoutNow() {
        scroller.frame = bounds
        layoutFade()
        jumpButton.frame.origin = CGPoint(x: ((bounds.width - Self.jumpButtonSide) / 2).rounded(), y: jumpButtonY)
        guard bounds.width != layoutWidth else {
            updateVisible()
            return
        }
        layoutWidth = bounds.width
        if measuresAhead { TranscriptColumn.width = columnWidth }
        // Wrapping changes with the width, so every height is an estimate again.
        let anchor = currentAnchor()
        for index in measured.indices { measured[index] = false }
        stale.formUnion(views.keys)
        updateVisible(anchor: anchor)
        measureRows()
    }

    private func recomputeOffsets(from start: Int) {
        if offsets.count != rows.count + 1 {
            offsets = [CGFloat](repeating: 0, count: rows.count + 1)
            var y: CGFloat = 0
            for index in rows.indices {
                offsets[index] = y
                y += heights[index]
            }
            offsets[rows.count] = y
            return
        }
        var y = offsets[start]
        for index in start..<rows.count {
            offsets[index] = y
            y += heights[index]
        }
        offsets[rows.count] = y
    }

    /// The row that contains the distance `y` from the top of the first row.
    private func index(at y: CGFloat) -> Int {
        guard !rows.isEmpty else { return 0 }
        var low = 0
        var high = rows.count - 1
        while low < high {
            let middle = (low + high + 1) / 2
            if offsets[middle] <= y { low = middle } else { high = middle - 1 }
        }
        return low
    }

    private func currentAnchor() -> Anchor? {
        pinned ? nil : viewportAnchor()
    }

    private func viewportAnchor() -> Anchor? {
        guard !rows.isEmpty else { return nil }
        let top = scroller.offsetY - topPadding
        let index = self.index(at: top)
        return Anchor(id: rows[index].id, delta: top - offsets[index])
    }

    private func height(of row: RowModel) -> CGFloat {
        measuredHeight(of: row) ?? RowView.estimatedHeight(row, width: columnWidth)
    }

    /// The row's height when it was measured for this width. An open row and a queued message
    /// have the heights their views give them.
    private func measuredHeight(of row: RowModel) -> CGFloat? {
        guard let measured = row.measured, measured.width == columnWidth, !expanded.contains(row.id), !row.isQueued else { return nil }
        return measured.height
    }

    /// Measures the rows that weren't measured for this width, off the main thread and the last
    /// ones first, and takes their heights as they come.
    private func measureRows() {
        measuringStopped?.withLock { $0 = true }
        measuringStopped = nil
        let width = columnWidth
        let waiting = rows.filter { $0.measured?.width != width }
        guard !waiting.isEmpty, bounds.width > 0 else { return }
        let stopped = OSAllocatedUnfairLock(initialState: false)
        measuringStopped = stopped
        Self.measuring.async { [weak self] in
            var batch: [(RowModel, CGFloat)] = []
            for (count, row) in waiting.reversed().enumerated() {
                guard !stopped.withLock({ $0 }) else { return }
                batch.append((row, RowView.height(row, width: width)))
                guard batch.count == 50 || count == waiting.count - 1 else { continue }
                let measured = batch
                batch = []
                DispatchQueue.main.async { self?.take(measured, width: width) }
            }
        }
    }

    private func take(_ measured: [(RowModel, CGFloat)], width: CGFloat) {
        guard width == columnWidth else { return }
        for (row, height) in measured { row.measured = (width, height) }
        let anchor = currentAnchor()
        var firstChanged: Int?
        // A row on screen has the height its view gave it.
        for index in rows.indices where views[rows[index].id] == nil {
            guard let height = measuredHeight(of: rows[index]), abs(height - heights[index]) > 0.5 else { continue }
            heights[index] = height
            firstChanged = firstChanged ?? index
        }
        guard let firstChanged else { return }
        recomputeOffsets(from: firstChanged)
        updateVisible(anchor: anchor)
    }

    private var reducesMotion: Bool { Platform.reducesMotion }

    private var animates: Bool { activity.running && !reducesMotion }

    // MARK: Content

    func reset(rows new: [RowModel]) {
        for id in Array(views.keys) { recycle(id) }
        rows = new
        hasPending = false
        expanded.removeAll()
        heights = new.map(height(of:))
        measured = [Bool](repeating: false, count: new.count)
        offsets = []
        recomputeOffsets(from: 0)
        requestedHighlight.removeAll()
        stale.removeAll()
        endOfRunningTurn = nil
        loadingEarlier = false
        trimming = false
        heldAnchor = nil
        stopGlide()
        pin()
        updateVisible()
        measureRows()
    }

    func splice(start: Int, remove: Int, rows new: [RowModel]) {
        let coreCount = rows.count - (hasPending ? 1 : 0)
        guard start >= 0, remove >= 0, start + remove <= coreCount else { return }
        let opening = toggled.flatMap { $0.id == new.first?.id ? $0 : nil }
        let anchor = opening ?? currentAnchor()
        let range = start..<(start + remove)
        // The server has the message now: its row takes the place of the copy shown while it
        // travelled, and as a first guess its height.
        let sentHeight = hasPending && new.contains(where: \.isSentMessage) ? heights.last : nil
        if sentHeight != nil { removePending() }

        // A row that is replaced by one with the same id keeps its view and, as a first guess,
        // its height: it is usually the same row with more text.
        var known: [String: CGFloat] = [:]
        for index in range { known[rows[index].id] = heights[index] }
        let kept = Set(new.map(\.id))
        for index in range where !kept.contains(rows[index].id) { recycle(rows[index].id) }

        if animates {
            fresh.formUnion(new.filter { known[$0.id] == nil && !$0.isSentMessage }.map(\.id))
        }
        rows.replaceSubrange(range, with: new)
        if activity.running, let last = rows.last, case .turnEnd = last.kind, kept.contains(last.id) {
            endOfRunningTurn = last.id
        }
        heights.replaceSubrange(range, with: new.map { row in
            measuredHeight(of: row) ?? known[row.id] ?? (row.isSentMessage ? sentHeight : nil) ?? height(of: row)
        })
        measured.replaceSubrange(range, with: [Bool](repeating: false, count: new.count))
        offsets = []
        recomputeOffsets(from: 0)
        for row in new {
            guard let view = views[row.id] else { continue }
            // The same id can become another kind of row, as when prose turns out to be code.
            guard reuseKey(of: view) == RowView.reuseKey(for: row) else {
                recycle(row.id)
                continue
            }
            view.configure(row)
            view.fadesGrowth = animates
            stale.insert(row.id)
        }
        if opening != nil {
            // Opening a row shouldn't drag the view to the end.
            toggled = nil
            let wasPinned = pinned
            pinned = false
            updateVisible(anchor: anchor)
            pinned = wasPinned && contentHeight - (scroller.offsetY + viewportHeight) < Self.pinDistance
            updateJumpButton()
        } else {
            updateVisible(anchor: anchor)
        }
        // Only what came into view fades in; the rest is simply there when it is scrolled to.
        fresh.removeAll()
        if start == 0, anchor != nil, scroller.offsetY < -0.5 { heldAnchor = anchor }
        trimEarlierIfFar()
        guard new.contains(where: { $0.measured?.width != columnWidth }) else { return }
        measureRows()
    }

    func setEarlier(_ earlier: Bool) {
        hasEarlier = earlier
        loadEarlierIfNear()
    }

    private func loadEarlierIfNear() {
        guard hasEarlier, !loadingEarlier, !rows.isEmpty, bounds.width > 0 else { return }
        let y = scroller.offsetY
        // Rows that come in front while the view bounces at the top can't be scrolled past.
        guard y > -0.5, y < earlierDistance else { return }
        loadingEarlier = true
        onNeedEarlier? { [weak self] in
            self?.loadingEarlier = false
            self?.loadEarlierIfNear()
        }
    }

    private func trimEarlierIfFar() {
        guard pinned, !userScrolling, !trimming, endY > trimDistance else { return }
        let keptFrom = index(at: endY - trimDistance / 2 - topPadding)
        trimming = true
        onTrimEarlier?(rows.count - keptFrom) { [weak self] in self?.trimming = false }
    }

    /// Shows a message at the end before the server has confirmed it, or takes it away again.
    func setPending(_ row: RowModel?) {
        removePending()
        if let row {
            rows.append(row)
            heights.append(height(of: row))
            measured.append(false)
            hasPending = true
            pin()
        }
        offsets = []
        recomputeOffsets(from: 0)
        updateVisible()
    }

    private func removePending() {
        guard hasPending else { return }
        recycle(rows[rows.count - 1].id)
        rows.removeLast()
        heights.removeLast()
        measured.removeLast()
        hasPending = false
    }

    func setActivity(_ activity: Activity) {
        if !activity.running || activity.startedAt != self.activity.startedAt { endOfRunningTurn = nil }
        self.activity = activity
        working.update(activity)
        updateVisible(follows: false)
        land()
    }

    /// The row's code has been highlighted; shows the colours if the row is on screen.
    func recolor(rowID: String, content: CodeContent) {
        (views[rowID] as? CodeRowView)?.recolor(content)
    }

    func scrollToEnd() {
        pin()
        updateVisible()
        trimEarlierIfFar()
    }

    /// Follows the end from now on, whatever the user was doing with the viewport.
    private func pin() {
        pinned = true
        userScrolling = false
        bounceEnd = nil
    }

    // MARK: Rows on screen

    private func view(for row: RowModel) -> RowView {
        if let view = views[row.id] { return view }
        let key = RowView.reuseKey(for: row)
        let view: RowView
        if let reused = pool[key]?.popLast() {
            view = reused
            view.isHidden = false
        } else {
            view = RowView.make(for: row)
            view.owner = self
            document.addSubview(view)
        }
        view.configure(row)
        view.showsMeta = !Platform.hoverReveals || row.id == metaRowID
        view.opacity = 1
        if fresh.remove(row.id) != nil {
            view.opacity = 0
            view.fade(to: 1, duration: 0.35)
        }
        views[row.id] = view
        stale.insert(row.id)
        if row.needsHighlight, !requestedHighlight.contains(row.id) {
            requestedHighlight.insert(row.id)
            pendingHighlight.append(row.id)
        }
        return view
    }

    private var pendingHighlight: [String] = []

    private func recycle(_ id: String) {
        guard let view = views.removeValue(forKey: id) else { return }
        stale.remove(id)
        view.clearSelection()
        let key = reuseKey(of: view)
        if (pool[key]?.count ?? 0) < Self.pooledPerKind {
            view.isHidden = true
            pool[key, default: []].append(view)
        } else {
            view.removeFromSuperview()
        }
    }

    private func reuseKey(of view: RowView) -> String {
        switch view {
        case is UserRowView: "user"
        case is ProseRowView: "prose"
        case is CodeRowView: "code"
        case is ToolRowView: "tool"
        case is MediaRowView: "media"
        case is ErrorRowView: "error"
        case is ChangesRowView: "changes"
        case is QueuedRowView: "queued"
        default: "turnEnd"
        }
    }

    /// The user scrolled. Scrolling up, by however little, stops following the end; coming down
    /// near the end follows it again. The way back from a bounce past an edge is neither.
    private func scrolled() {
        guard !updating else { return }
        let y = scroller.offsetY
        let end = endY
        let rising = y < lastScrollY - 0.5
        let falling = y > lastScrollY + 0.5
        // A window made taller also leaves the viewport past the end, without moving it.
        if falling, y > end + 0.5 { bounceEnd = end }
        let bouncing = y < 0 || (bounceEnd.map { y > $0 - 0.5 } ?? false)
        if rising, !bouncing, y < end - 1 {
            pinned = false
        } else if !rising, end - y < Self.pinDistance {
            pinned = true
        }
        if let settled = bounceEnd, y < settled + 0.5 { bounceEnd = nil }
        lastScrollY = y
        var anchor = viewportAnchor()
        if let held = heldAnchor, y > -0.5 {
            anchor = held
            heldAnchor = nil
        }
        updateVisible(anchor: anchor, follows: false)
        land()
        loadEarlierIfNear()
        trimEarlierIfFar()
    }

    private func userScrollBegan() {
        userScrolling = true
        stopGlide()
    }

    private func userScrollEnded() {
        userScrolling = false
        land()
        trimEarlierIfFar()
    }

    /// Brings a view that follows the end to rest on it, once the user has let go of it.
    private func land() {
        guard pinned, !userScrolling else { return }
        let distance = endY - scroller.offsetY
        guard distance > 0.5 else { return }
        guard distance < viewportHeight, !reducesMotion else {
            updateVisible()
            return
        }
        startGlide()
    }

    /// Makes the views for the rows in and near the viewport, measures the ones that need it,
    /// and keeps the viewport where it was: at the end if it follows the end, otherwise with
    /// `anchor` in the same place.
    private func updateVisible(anchor: Anchor? = nil, follows: Bool = true) {
        guard !updating, bounds.width > 0 else { return }
        updating = true
        defer { updating = false }
        let anchor = anchor ?? currentAnchor()
        let width = columnWidth
        let x = columnX
        // The rows under the working line stand lower by its height.
        let firstLowered = workingIndex
        let workingRoom = showsWorking ? Self.workingHeight : 0

        // Measuring changes heights, which moves the viewport, which changes what is visible.
        // It settles in a pass or two.
        for _ in 0..<4 {
            position(anchor: anchor, follows: follows)
            let top = scroller.offsetY - topPadding - Self.overscan
            let bottom = scroller.offsetY + viewportHeight - topPadding + Self.overscan

            var index = self.index(at: top)
            var y = rows.isEmpty ? 0 : offsets[index]
            var firstChanged: Int?
            var onScreen = Set<String>()
            while index < rows.count, y < bottom {
                let row = rows[index]
                let view = self.view(for: row)
                if !measured[index] || stale.contains(row.id) {
                    let height = view.layout(width: width)
                    stale.remove(row.id)
                    measured[index] = true
                    if abs(height - heights[index]) > 0.5 {
                        heights[index] = height
                        firstChanged = firstChanged ?? index
                    }
                }
                let lowered = index >= firstLowered ? workingRoom : 0
                view.frame = CGRect(x: x, y: topPadding + y + lowered, width: width, height: heights[index])
                onScreen.insert(row.id)
                y += heights[index]
                index += 1
            }
            for id in Array(views.keys) where !onScreen.contains(id) { recycle(id) }
            guard let firstChanged else { break }
            recomputeOffsets(from: firstChanged)
        }
        position(anchor: anchor, follows: follows)

        let workingY = firstLowered < offsets.count ? offsets[firstLowered] : 0
        working.frame = CGRect(x: x, y: topPadding + workingY + 2, width: width, height: Self.workingHeight)
        working.isHidden = !showsWorking
        updateJumpButton()
        updateMetaRow()
        if !pendingHighlight.isEmpty {
            let ids = pendingHighlight
            pendingHighlight.removeAll()
            onNeedHighlight?(ids)
        }
    }

    /// Sizes the document and puts the viewport where it belongs.
    private func position(anchor: Anchor?, follows: Bool) {
        let height = max(contentHeight, viewportHeight)
        scroller.setDocument(width: bounds.width, height: height)
        let end = height - viewportHeight
        let current = scroller.offsetY
        // Past an edge the scroll view is bouncing back by itself; moving it then makes it shake.
        guard current > -0.5, bounceEnd == nil || current < end + 0.5 else { return }
        // While the user scrolls, the viewport is theirs.
        let toEnd = pinned && follows && !userScrolling
        var target = current
        if toEnd {
            target = end
        } else if let anchor, let index = rows.firstIndex(where: { $0.id == anchor.id }) {
            target = offsets[index] + anchor.delta + topPadding
        }
        target = max(0, min(target, end))
        let distance = target - current
        guard abs(distance) > 0.5 else { return }
        // A streaming reply pushes the end down a block at a time; the viewport glides after it.
        if toEnd, animates || glide != nil, distance > 0, distance < viewportHeight {
            startGlide()
            return
        }
        scroll(to: target)
    }

    /// Rows are stacked without room between them and are as wide as the transcript for this,
    /// so the pointer never leaves a message on its way to the message's copy button.
    private func updateMetaRow() {
        guard Platform.hoverReveals else { return }
        let id = pointerAt.flatMap { metaRow(at: $0.y + scroller.offsetY - topPadding) }
        guard id != metaRowID else { return }
        if let metaRowID { views[metaRowID]?.showsMeta = false }
        metaRowID = id
        if let id { views[id]?.showsMeta = true }
    }

    private func metaRow(at y: CGFloat) -> String? {
        guard !rows.isEmpty, y >= 0, y < offsets[rows.count] else { return nil }
        let under = index(at: y)
        guard !rows[under].isUser else { return rows[under].id }
        for row in rows[under...] {
            guard !row.isUser else { return nil }
            if case .turnEnd = row.kind { return row.id }
        }
        return nil
    }

    private func updateJumpButton() {
        let distance = endY - scroller.offsetY
        jumpButton.isHidden = pinned || rows.isEmpty || distance < jumpDistance
    }

    private func scroll(to y: CGFloat) {
        let wasUpdating = updating
        updating = true
        scroller.scroll(to: y)
        lastScrollY = y
        bounceEnd = nil
        updating = wasUpdating
    }

    private func startGlide() {
        guard glide == nil else { return }
        let link = ticker(target: self, selector: #selector(glideStep))
        link.add(to: .main, forMode: .common)
        glide = link
    }

    private func stopGlide() {
        glide?.invalidate()
        glide = nil
    }

    /// Goes a little over a fifth of the way in every sixtieth of a second, whatever the display's
    /// rate, and never slower than two points in that time.
    @objc private func glideStep(_ link: CADisplayLink) {
        let y = scroller.offsetY
        let remaining = endY - y
        guard pinned, !userScrolling, remaining > 0.5 else {
            stopGlide()
            return
        }
        let sixtieths = max(0.25, (link.targetTimestamp - link.timestamp) * 60)
        let step = max(2 * sixtieths, remaining * (1 - pow(0.78, sixtieths)))
        scroll(to: y + min(remaining, step))
        updateVisible(follows: false)
    }

    // MARK: RowOwner

    func rowWillSelect(_ view: RowView) {
        for other in views.values where other !== view { other.clearSelection() }
    }

    func rowToggledExpansion(id: String) {
        if !expanded.insert(id).inserted { expanded.remove(id) }
        guard let index = rows.firstIndex(where: { $0.id == id }) else { return }
        measured[index] = false
        // Opening a row shouldn't drag the view to the end.
        let anchor = currentAnchor() ?? Anchor(id: id, delta: scroller.offsetY - topPadding - offsets[index])
        let wasPinned = pinned
        pinned = false
        updateVisible(anchor: anchor)
        pinned = wasPinned && contentHeight - (scroller.offsetY + viewportHeight) < Self.pinDistance
        updateJumpButton()
    }

    func toggleRow(id: String) {
        guard let index = rows.firstIndex(where: { $0.id == id }) else { return }
        toggled = Anchor(id: id, delta: scroller.offsetY - topPadding - offsets[index])
        onToggleRow?(id)
    }

    func isExpanded(id: String) -> Bool { expanded.contains(id) }

    func media(id: String, done: @escaping (URL?) -> Void) {
        guard let onNeedMedia else { return done(nil) }
        onNeedMedia(id, done)
    }

    func view(_ media: [ViewedMedia], at index: Int) { onViewMedia?(media, index) }

    func sendQueued(messageID: String) { onSendQueued?(messageID) }

    func toggleFolder(id: String, rowID: String) {
        guard let index = rows.firstIndex(where: { $0.id == rowID }) else { return }
        toggled = Anchor(id: rowID, delta: scroller.offsetY - topPadding - offsets[index])
        onToggleRow?(id)
    }

    func openDiff(turn itemID: String, path: String?) { onOpenDiff?(itemID, path) }

    func openAgent(itemID: String) { onOpenAgent?(itemID) }

    func cancelQueued(messageID: String) { onCancelQueued?(messageID) }

    /// Copies the reply that ends at the given row: every stretch of prose and code back to the
    /// user's message.
    func copyReply(endingAt rowID: String) {
        guard let end = rows.firstIndex(where: { $0.id == rowID }) else { return }
        var parts: [String] = []
        var index = end - 1
        while index >= 0, !rows[index].isUser {
            if let text = rows[index].plainText { parts.append(text) }
            index -= 1
        }
        Platform.copy(parts.reversed().joined(separator: "\n\n"))
    }
}
