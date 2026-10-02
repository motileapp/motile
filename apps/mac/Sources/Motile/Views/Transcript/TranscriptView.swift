import AppKit

/// The transcript: a scrolling column of rows of which only the ones on screen exist as views.
///
/// Every row has a height: measured once it has been shown, estimated until then. A row's views
/// are made when it scrolls in and reused when it scrolls out, so a thread of any length costs
/// what is on screen. When heights above the viewport turn out different from their estimates,
/// the scroll position is corrected in the same pass, so nothing on screen moves.
final class TranscriptView: FlippedView, RowHost {
    var onAllow: (([Denial]) -> Void)?
    /// Code rows that came into view without highlighting.
    var onNeedHighlight: (([String]) -> Void)?

    /// Room left under the last row for what floats over the transcript's end.
    var bottomInset: CGFloat = 0 {
        didSet {
            guard bottomInset != oldValue else { return }
            updateVisible()
            jumpButton.frame.origin.y = bounds.height - bottomInset - 44
        }
    }

    private static let topPadding: CGFloat = 20
    private static let overscan: CGFloat = 400
    private static let pooledPerKind = 10
    private static let workingHeight: CGFloat = 30

    private let scrollView = NSScrollView()
    private let document = FlippedView()
    private let working = WorkingView()
    private let jumpButton = SurfaceView()

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

    /// Whether the view follows the end of the transcript as it grows.
    private var pinned = true
    private var updating = false
    /// Where the viewport was after the last scroll, to tell which way the next one goes.
    private var lastScrollY: CGFloat = 0
    /// Moves the viewport to the end in steps while a reply streams.
    private var glide: Timer?
    /// Rows that just arrived in a streaming reply; they fade in.
    private var fresh: Set<String> = []
    private var layoutWidth: CGFloat = 0

    private struct Anchor {
        let id: String
        /// How far the viewport's top is below the top of the row.
        let delta: CGFloat
    }

    override init(frame: NSRect) {
        super.init(frame: frame)
        scrollView.drawsBackground = false
        scrollView.hasVerticalScroller = true
        scrollView.hasHorizontalScroller = false
        scrollView.autohidesScrollers = true
        scrollView.scrollerStyle = .overlay
        scrollView.automaticallyAdjustsContentInsets = false
        scrollView.documentView = document
        scrollView.contentView.postsBoundsChangedNotifications = true
        addSubview(scrollView)
        document.addSubview(working)
        working.isHidden = true

        jumpButton.fill = Theme.raised
        jumpButton.stroke = Theme.strongBorder
        jumpButton.radius = 16
        jumpButton.frame = NSRect(x: 0, y: 0, width: 32, height: 32)
        jumpButton.isHidden = true
        jumpButton.toolTip = "Scroll to end"
        let arrow = NSImageView(frame: NSRect(x: 8, y: 8, width: 16, height: 16))
        arrow.image = NSImage(systemSymbolName: "arrow.down", accessibilityDescription: "Scroll to end")?
            .withSymbolConfiguration(.init(pointSize: 12, weight: .semibold))
        arrow.contentTintColor = Theme.secondary
        jumpButton.addSubview(arrow)
        jumpButton.onClick = { [weak self] in self?.scrollToEnd() }
        jumpButton.shadow = {
            let shadow = NSShadow()
            shadow.shadowColor = NSColor.black.withAlphaComponent(0.18)
            shadow.shadowBlurRadius = 8
            shadow.shadowOffset = NSSize(width: 0, height: -2)
            return shadow
        }()
        addSubview(jumpButton)

        NotificationCenter.default.addObserver(
            self,
            selector: #selector(scrolled),
            name: NSView.boundsDidChangeNotification,
            object: scrollView.contentView
        )
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    deinit {
        NotificationCenter.default.removeObserver(self)
        glide?.invalidate()
    }

    // MARK: Geometry

    private var columnWidth: CGFloat {
        max(200, min(bounds.width - Theme.contentPadding * 2, Theme.contentWidth))
    }

    private var columnX: CGFloat {
        ((bounds.width - columnWidth) / 2).rounded()
    }

    private var viewportHeight: CGFloat { scrollView.contentView.bounds.height }

    private var contentHeight: CGFloat {
        let workingHeight = activity.running ? Self.workingHeight : 0
        return Self.topPadding + (offsets.last ?? 0) + workingHeight + bottomInset + 16
    }

    override func layout() {
        super.layout()
        scrollView.frame = bounds
        jumpButton.frame.origin = NSPoint(x: ((bounds.width - 32) / 2).rounded(), y: bounds.height - bottomInset - 44)
        guard bounds.width != layoutWidth else {
            updateVisible()
            return
        }
        layoutWidth = bounds.width
        // Wrapping changes with the width, so every height is an estimate again.
        let anchor = currentAnchor()
        for index in measured.indices { measured[index] = false }
        stale.formUnion(views.keys)
        updateVisible(anchor: anchor)
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
        let top = scrollView.contentView.bounds.minY - Self.topPadding
        let index = self.index(at: top)
        return Anchor(id: rows[index].id, delta: top - offsets[index])
    }

    private var animates: Bool {
        activity.running && !NSWorkspace.shared.accessibilityDisplayShouldReduceMotion
    }

    // MARK: Content

    func reset(rows new: [RowModel]) {
        for id in Array(views.keys) { recycle(id) }
        rows = new
        hasPending = false
        heights = new.map { RowView.estimatedHeight($0, width: columnWidth) }
        measured = [Bool](repeating: false, count: new.count)
        offsets = []
        recomputeOffsets(from: 0)
        expanded.removeAll()
        requestedHighlight.removeAll()
        stale.removeAll()
        stopGlide()
        pinned = true
        updateVisible()
    }

    func splice(start: Int, remove: Int, rows new: [RowModel]) {
        let coreCount = rows.count - (hasPending ? 1 : 0)
        guard start >= 0, remove >= 0, start + remove <= coreCount else { return }
        let anchor = currentAnchor()
        let range = start..<(start + remove)

        // A row that is replaced by one with the same id keeps its view and, as a first guess,
        // its height: it is usually the same row with more text.
        var known: [String: CGFloat] = [:]
        for index in range { known[rows[index].id] = heights[index] }
        let kept = Set(new.map(\.id))
        for index in range where !kept.contains(rows[index].id) { recycle(rows[index].id) }

        if animates {
            fresh.formUnion(new.filter { known[$0.id] == nil && !$0.isUser }.map(\.id))
        }
        rows.replaceSubrange(range, with: new)
        heights.replaceSubrange(range, with: new.map { known[$0.id] ?? RowView.estimatedHeight($0, width: columnWidth) })
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
        updateVisible(anchor: anchor)
        // Only what came into view fades in; the rest is simply there when it is scrolled to.
        fresh.removeAll()
    }

    /// Shows a message at the end before the host has confirmed it, or takes it away again.
    func setPending(_ row: RowModel?) {
        if hasPending {
            let last = rows.count - 1
            recycle(rows[last].id)
            rows.removeLast()
            heights.removeLast()
            measured.removeLast()
            hasPending = false
        }
        if let row {
            rows.append(row)
            heights.append(RowView.estimatedHeight(row, width: columnWidth))
            measured.append(false)
            hasPending = true
            pinned = true
        }
        offsets = []
        recomputeOffsets(from: 0)
        updateVisible()
    }

    func setActivity(_ activity: Activity) {
        self.activity = activity
        working.update(activity)
        updateVisible()
    }

    /// The row's code has been highlighted; shows the colours if the row is on screen.
    func recolor(rowID: String, content: CodeContent) {
        (views[rowID] as? CodeRowView)?.recolor(content)
    }

    func scrollToEnd() {
        pinned = true
        updateVisible()
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
            view.host = self
            document.addSubview(view)
        }
        view.configure(row)
        view.alphaValue = 1
        if fresh.remove(row.id) != nil {
            view.alphaValue = 0
            NSAnimationContext.runAnimationGroup { context in
                context.duration = 0.35
                view.animator().alphaValue = 1
            }
        }
        views[row.id] = view
        stale.insert(row.id)
        if case .code(let content) = row.kind, !content.highlighted, !requestedHighlight.contains(row.id) {
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
        case is ErrorRowView: "error"
        default: "turnEnd"
        }
    }

    /// The user scrolled. Scrolling up, by however little, stops following the end; coming back
    /// near the end follows it again. The viewport itself is left where they put it.
    @objc private func scrolled() {
        guard !updating else { return }
        let clip = scrollView.contentView.bounds
        let fromEnd = contentHeight - clip.maxY
        if clip.minY < lastScrollY - 0.5, fromEnd > 1 {
            pinned = false
        } else if fromEnd < 30 {
            pinned = true
        }
        lastScrollY = clip.minY
        updateVisible(anchor: viewportAnchor(), follows: false)
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

        // Measuring changes heights, which moves the viewport, which changes what is visible.
        // It settles in a pass or two.
        for _ in 0..<4 {
            position(anchor: anchor, follows: follows)
            let clip = scrollView.contentView.bounds
            let top = clip.minY - Self.topPadding - Self.overscan
            let bottom = clip.maxY - Self.topPadding + Self.overscan

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
                view.frame = NSRect(x: x, y: Self.topPadding + y, width: width, height: heights[index])
                onScreen.insert(row.id)
                y += heights[index]
                index += 1
            }
            for id in Array(views.keys) where !onScreen.contains(id) { recycle(id) }
            guard let firstChanged else { break }
            recomputeOffsets(from: firstChanged)
        }
        position(anchor: anchor, follows: follows)

        working.frame = NSRect(x: x, y: Self.topPadding + (offsets.last ?? 0) + 2, width: width, height: Self.workingHeight)
        jumpButton.isHidden = pinned || rows.isEmpty
        if !pendingHighlight.isEmpty {
            let ids = pendingHighlight
            pendingHighlight.removeAll()
            onNeedHighlight?(ids)
        }
    }

    /// Sizes the document and puts the viewport where it belongs.
    private func position(anchor: Anchor?, follows: Bool) {
        let clip = scrollView.contentView
        let height = max(contentHeight, viewportHeight)
        if document.frame.height != height || document.frame.width != bounds.width {
            document.frame = NSRect(x: 0, y: 0, width: bounds.width, height: height)
        }
        let toEnd = pinned && follows
        var target = clip.bounds.minY
        if toEnd {
            target = height - viewportHeight
        } else if let anchor, let index = rows.firstIndex(where: { $0.id == anchor.id }) {
            target = offsets[index] + anchor.delta + Self.topPadding
        }
        target = max(0, min(target, height - viewportHeight))
        let distance = target - clip.bounds.minY
        guard abs(distance) > 0.5 else { return }
        // A streaming reply pushes the end down a block at a time; the viewport glides after it.
        if toEnd, animates || glide != nil, distance > 0, distance < viewportHeight {
            startGlide()
            return
        }
        scroll(to: target)
    }

    private func scroll(to y: CGFloat) {
        let wasUpdating = updating
        updating = true
        scrollView.contentView.scroll(to: NSPoint(x: 0, y: y))
        scrollView.reflectScrolledClipView(scrollView.contentView)
        lastScrollY = y
        updating = wasUpdating
    }

    private func startGlide() {
        guard glide == nil else { return }
        let timer = Timer(timeInterval: 1.0 / 60, repeats: true) { [weak self] _ in self?.glideStep() }
        RunLoop.main.add(timer, forMode: .common)
        glide = timer
    }

    private func stopGlide() {
        glide?.invalidate()
        glide = nil
    }

    private func glideStep() {
        let y = scrollView.contentView.bounds.minY
        let remaining = max(0, document.frame.height - viewportHeight) - y
        guard pinned, remaining > 0.5 else {
            stopGlide()
            return
        }
        scroll(to: y + min(remaining, max(2, remaining * 0.22)))
        updateVisible(follows: false)
    }

    // MARK: RowHost

    func rowWillSelect(_ view: RowView) {
        for other in views.values where other !== view { other.clearSelection() }
    }

    func rowToggledExpansion(id: String) {
        if !expanded.insert(id).inserted { expanded.remove(id) }
        guard let index = rows.firstIndex(where: { $0.id == id }) else { return }
        measured[index] = false
        // Opening a row shouldn't drag the view to the end.
        let anchor = currentAnchor() ?? Anchor(id: id, delta: scrollView.contentView.bounds.minY - Self.topPadding - offsets[index])
        let wasPinned = pinned
        pinned = false
        updateVisible(anchor: anchor)
        pinned = wasPinned && contentHeight - scrollView.contentView.bounds.maxY < 30
        jumpButton.isHidden = pinned || rows.isEmpty
    }

    func isExpanded(id: String) -> Bool { expanded.contains(id) }

    func allow(_ denials: [Denial]) { onAllow?(denials) }

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
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(parts.reversed().joined(separator: "\n\n"), forType: .string)
    }
}
