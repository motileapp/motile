import QuartzCore
import SwiftUI

/// A line of the Linear list: a status, or an issue under it with the statuses its team has.
enum LinearListEntry: Equatable {
    case header(state: LinearState, count: Int, folded: Bool)
    case issue(row: LinearRow, state: LinearState, states: [LinearState])
}

/// The issues under their statuses, in a list that only builds the rows on screen and reuses
/// them as they scroll by, as the transcript does. A click on an issue opens it, on its priority
/// or status offers the others, and on a status folds its issues up and out.
struct LinearList {
    let entries: [LinearListEntry]
    /// The issues whose status, priority or assignee is being changed.
    let pending: Set<String>
    /// The panel is too narrow for the labels beside the titles.
    let narrow: Bool
    let users: [LinearUser]
    let offered: Binding<[Choice]>
    let open: (LinearRow) -> Void
    let fold: (String) -> Void
    let change: (LinearRow, JSON) -> Void

    private func make() -> LinearListView {
        let view = LinearListView()
        update(view)
        return view
    }

    private func update(_ view: LinearListView) {
        let (users, offered, change) = (users, offered, change)
        view.onOpen = open
        view.onFold = fold
        view.onOffer = { Choice.offer($0, in: offered) }
        view.onChange = change
        view.menuFor = { row in
            var actions = [
                MenuAction(title: "Assign To", symbol: .circleUser) {
                    Choice.offer(Self.assignees(users, of: row, change: change), in: offered)
                }
            ]
            if let url = row.url {
                actions.append(MenuAction(title: "Open in Linear", symbol: .squareArrowOutUpRight) { Platform.open(url) })
                actions.append(MenuAction(title: "Copy Link", symbol: .link) { Platform.copy(url.absoluteString) })
            }
            actions.append(MenuAction(title: "Copy Identifier", symbol: .copy) { Platform.copy(row.identifier) })
            return actions
        }
        view.set(entries: entries, pending: pending, narrow: narrow)
    }

    private static func assignees(_ users: [LinearUser], of row: LinearRow, change: @escaping (LinearRow, JSON) -> Void) -> [Choice] {
        let nobody = Choice("Unassigned", chosen: row.assigneeID == nil) { change(row, ["assignee": ""]) }
        return [nobody] + users.map { user in
            Choice(user.me ? "\(user.name) (you)" : user.name, chosen: user.id == row.assigneeID) { change(row, ["assignee": user.id]) }
        }
    }
}

#if os(macOS)
extension LinearList: NSViewRepresentable {
    func makeNSView(context: Context) -> LinearListView { make() }

    func updateNSView(_ view: LinearListView, context: Context) {
        view.putAway(context.environment.putAway)
        update(view)
    }
}
#else
extension LinearList: UIViewRepresentable {
    func makeUIView(context: Context) -> LinearListView { make() }

    func updateUIView(_ view: LinearListView, context: Context) { update(view) }
}
#endif

final class LinearListView: FlippedView {
    var onOpen: ((LinearRow) -> Void)?
    var onFold: ((String) -> Void)?
    var onOffer: (([Choice]) -> Void)?
    var onChange: ((LinearRow, JSON) -> Void)?
    var menuFor: ((LinearRow) -> [MenuAction])?

    /// The room around the list.
    private static let padding: CGFloat = 8

    private let scroller = TranscriptScroller()
    private var entries: [LinearListEntry] = []
    private var pending: Set<String> = []
    private var narrow = false
    /// Where each entry starts, and where the last one ends.
    private var tops: [CGFloat] = [0]
    /// The cells on screen, by the entry they show.
    private var shown: [Int: LinearCell] = [:]
    private var spareHeaders: [LinearHeaderCell] = []
    private var spareIssues: [LinearIssueCell] = []

    override init(frame: CGRect) {
        super.init(frame: frame)
        addSubview(scroller)
        scroller.onScroll = { [weak self] in self?.updateVisible() }
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    func set(entries: [LinearListEntry], pending: Set<String>, narrow: Bool) {
        guard entries != self.entries || pending != self.pending || narrow != self.narrow else { return }
        self.entries = entries
        self.pending = pending
        self.narrow = narrow
        var tops = [CGFloat(0)]
        for entry in entries { tops.append(tops[tops.count - 1] + Self.height(of: entry)) }
        self.tops = tops
        // A cell that still shows the same kind of entry keeps its place and its light.
        for (index, cell) in shown {
            guard index < entries.count, cell.shows(entries[index]) else {
                recycle(index)
                continue
            }
            cell.show(entries[index], pending: pending, narrow: narrow)
        }
        layoutNow()
    }

    private static func height(of entry: LinearListEntry) -> CGFloat {
        switch entry {
        case .header: LinearHeaderCell.height
        case .issue: LinearIssueCell.height
        }
    }

    override func layoutNow() {
        scroller.frame = bounds
        scroller.setDocument(width: bounds.width, height: (tops.last ?? 0) + Self.padding * 2)
        for (index, cell) in shown { place(cell, at: index) }
        updateVisible()
    }

    private func place(_ cell: LinearCell, at index: Int) {
        let width = max(0, bounds.width - Self.padding * 2)
        cell.frame = CGRect(x: Self.padding, y: Self.padding + tops[index], width: width, height: tops[index + 1] - tops[index])
        cell.layoutNow()
    }

    /// The entry that lies at `y`, or the first one after it.
    private func index(at y: CGFloat) -> Int {
        var low = 0
        var high = entries.count
        while low < high {
            let middle = (low + high) / 2
            if tops[middle + 1] <= y { low = middle + 1 } else { high = middle }
        }
        return low
    }

    private func updateVisible() {
        let top = scroller.offsetY - Self.padding
        let first = index(at: top)
        let last = min(entries.count - 1, index(at: top + scroller.viewportHeight))
        for index in shown.keys where index < first || index > last { recycle(index) }
        guard scroller.viewportHeight > 0, first <= last else { return }
        for index in first...last where shown[index] == nil {
            let cell = take(for: entries[index])
            cell.show(entries[index], pending: pending, narrow: narrow)
            place(cell, at: index)
            shown[index] = cell
        }
    }

    private func take(for entry: LinearListEntry) -> LinearCell {
        let cell: LinearCell
        switch entry {
        case .header: cell = spareHeaders.popLast() ?? LinearHeaderCell(list: self)
        case .issue: cell = spareIssues.popLast() ?? LinearIssueCell(list: self)
        }
        if cell.superview == nil { scroller.document.addSubview(cell) }
        cell.isHidden = false
        cell.dim()
        return cell
    }

    private func recycle(_ index: Int) {
        guard let cell = shown.removeValue(forKey: index) else { return }
        cell.isHidden = true
        switch cell {
        case let header as LinearHeaderCell: spareHeaders.append(header)
        case let issue as LinearIssueCell: spareIssues.append(issue)
        default: break
        }
    }
}

/// A line of the list. One is made for each kind of entry and shown again for the next of its kind.
class LinearCell: FlippedView {
    weak var list: LinearListView?

    init(list: LinearListView) {
        self.list = list
        super.init(frame: .zero)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    func show(_ entry: LinearListEntry, pending: Set<String>, narrow: Bool) {}

    /// Whether the cell is for the entry's kind.
    func shows(_ entry: LinearListEntry) -> Bool { false }

    /// A cell shown again starts unlit, wherever the pointer left it.
    func dim() {}

    #if os(macOS)
    /// No text lies under the pointer here, so the arrow needs no rect: one would have the window
    /// look over every view on every scroll.
    override func resetCursorRects() {}
    #endif
}

/// The space between the rows' lights, and around each one: it looks empty but is the row's.
private let rowGap: CGFloat = 2

/// A status over its issues, on the secondary background, which a click folds up and out.
private final class LinearHeaderCell: LinearCell {
    static let height = pressable(30) + 6 + rowGap

    private let light = SurfaceView()
    private let symbol = SymbolView(size: 12)
    private let name = TextLabel(font: .ui(12, weight: .semibold), color: Theme.foreground)
    private let count = TextLabel(font: .ui(12), color: Theme.mutedStrongerForeground)
    private let chevron = SymbolView(.chevronDown, size: 10, tint: Theme.mutedStrongerForeground)
    private var stateID = ""

    override init(list: LinearListView) {
        super.init(list: list)
        light.radius = Radius.md
        addSubview(light)
        for view in [symbol, name, count, chevron] as [PlatformView] { addSubview(view) }
        onPress = { [weak self] _ in
            guard let self else { return }
            self.list?.onFold?(stateID)
        }
        light.fill = Theme.backgroundSecondary
        onHover = { [weak self] point in self?.light.fill = point == nil ? Theme.backgroundSecondary : Theme.backgroundSecondaryAccent }
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override func show(_ entry: LinearListEntry, pending: Set<String>, narrow: Bool) {
        guard case .header(let state, let count, let folded) = entry else { return }
        stateID = state.id
        symbol.show(state.symbol, size: 12)
        symbol.tint = PlatformColor(state.color)
        name.string = state.name
        self.count.string = "\(count)"
        chevron.show(folded ? .chevronRight : .chevronDown, size: 10)
        describe("\(state.name), \(count)", button: true)
    }

    override func shows(_ entry: LinearListEntry) -> Bool {
        if case .header = entry { return true }
        return false
    }

    override func dim() {
        light.fill = Theme.backgroundSecondary
    }

    override func layoutNow() {
        let lit = CGRect(x: 0, y: 6 + rowGap / 2, width: bounds.width, height: max(0, bounds.height - 6 - rowGap))
        light.frame = lit
        symbol.frame = CGRect(x: 10, y: lit.minY, width: 16, height: lit.height)
        var x: CGFloat = 10 + 16 + 8
        for label in [name, count] {
            let size = label.natural
            label.frame = CGRect(x: x, y: lit.minY + ((lit.height - size.height) / 2).rounded(), width: size.width, height: size.height)
            x += size.width + 8
        }
        chevron.frame = CGRect(x: bounds.width - 10 - 16, y: lit.minY, width: 16, height: lit.height)
    }
}

/// An issue on one line as Linear lists it: its priority and its status, which a click changes,
/// its identifier and title, then its labels, who has it and when it last changed.
private final class LinearIssueCell: LinearCell {
    static let height = pressable(34) + rowGap
    private static let inset: CGFloat = 6

    private let light = SurfaceView()
    private let priority = LinearRowButton()
    private let status = LinearRowButton()
    private let key = TextLabel(font: .ui(11.5), color: Theme.mutedStrongerForeground)
    private let title = TextLabel(font: .ui(13), color: Theme.foreground)
    private let chips = [LinearChipView(), LinearChipView()]
    private let initials = LinearInitialsView()
    private let time = TextLabel(font: .ui(11.5), color: Theme.mutedStrongerForeground)
    private var row: LinearRow?
    private var state: LinearState?
    private var states: [LinearState] = []

    override init(list: LinearListView) {
        super.init(list: list)
        light.radius = Radius.md
        addSubview(light)
        for view in [priority, status, key, title, initials, time] as [PlatformView] { addSubview(view) }
        for chip in chips { addSubview(chip) }
        onPress = { [weak self] point in self?.press(at: point) }
        onHover = { [weak self] point in self?.light(at: point) }
        menuActions = { [weak self] in
            guard let self, let row else { return [] }
            return self.list?.menuFor?(row) ?? []
        }
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    override func show(_ entry: LinearListEntry, pending: Set<String>, narrow: Bool) {
        guard case .issue(let row, let state, let states) = entry else { return }
        self.row = row
        self.state = state
        self.states = states
        priority.show(row.prioritySymbol, tip: row.priorityLabel, tint: row.priority == 1 ? Theme.warning : nil)
        status.show(state.symbol, tip: "Status: \(state.name)", tint: PlatformColor(state.color))
        status.spinning = pending.contains(row.id)
        key.string = row.identifier
        title.string = row.title
        let labels = narrow ? [] : Array(row.labels.prefix(2))
        for (chip, label) in zip(chips, labels) { chip.show(label) }
        for chip in chips.dropFirst(labels.count) { chip.isHidden = true }
        initials.isHidden = row.initials == nil
        initials.show(row.initials ?? "", name: row.assignee ?? "")
        time.string = Time.ago(row.updatedAt)
        describe("\(row.identifier) \(row.title)", button: true)
    }

    override func shows(_ entry: LinearListEntry) -> Bool {
        if case .issue = entry { return true }
        return false
    }

    override func dim() {
        light(at: nil)
    }

    override func layoutNow() {
        let lit = CGRect(x: 0, y: rowGap / 2, width: bounds.width, height: max(0, bounds.height - rowGap))
        light.frame = lit
        let side = LinearRowButton.side
        let middle = lit.midY
        priority.frame = CGRect(x: Self.inset, y: (middle - side / 2).rounded(), width: side, height: side)
        status.frame = CGRect(x: priority.frame.maxX + 4, y: priority.frame.minY, width: side, height: side)
        priority.layoutNow()
        status.layoutNow()
        var x = status.frame.maxX + 8
        x += place(key, at: x, middle: middle) + 8
        var right = bounds.width - 10
        right -= place(time, endingAt: right, middle: middle)
        if !initials.isHidden {
            right -= 8 + LinearInitialsView.side
            initials.frame = CGRect(x: right, y: (middle - LinearInitialsView.side / 2).rounded(), width: LinearInitialsView.side, height: LinearInitialsView.side)
            initials.layoutNow()
        }
        for chip in chips.reversed() where !chip.isHidden {
            right -= 8 + chip.width
            chip.frame = CGRect(x: right, y: (middle - LinearChipView.height / 2).rounded(), width: chip.width, height: LinearChipView.height)
            chip.layoutNow()
        }
        let height = title.natural.height
        title.frame = CGRect(x: x, y: (middle - height / 2).rounded(), width: max(0, right - 4 - x), height: height)
    }

    /// Places a label at its natural width and says how wide it is.
    private func place(_ label: TextLabel, at x: CGFloat, middle: CGFloat) -> CGFloat {
        let size = label.natural
        label.frame = CGRect(x: x, y: (middle - size.height / 2).rounded(), width: size.width, height: size.height)
        return size.width
    }

    private func place(_ label: TextLabel, endingAt right: CGFloat, middle: CGFloat) -> CGFloat {
        place(label, at: right - label.natural.width, middle: middle)
    }

    private func press(at point: CGPoint) {
        guard let row, let state else { return }
        if priority.frame.contains(point) {
            let choices = LinearRow.priorities.map { priority in
                Choice(priority.name, symbol: LinearRow.symbol(priority: priority.value), chosen: priority.value == row.priority) { [weak self] in
                    self?.list?.onChange?(row, ["priority": priority.value])
                }
            }
            list?.onOffer?(choices)
        } else if status.frame.contains(point) {
            guard !status.spinning else { return }
            let choices = states.map { status in
                Choice(status.name, symbol: status.symbol, chosen: status.id == state.id) { [weak self] in
                    self?.list?.onChange?(row, ["state": status.id])
                }
            }
            list?.onOffer?(choices)
        } else {
            list?.onOpen?(row)
        }
    }

    /// Lights the row under the pointer, and the control under it a layer further.
    private func light(at point: CGPoint?) {
        let over = point != nil
        light.fill = over ? Theme.backgroundAccentLarger : .clear
        priority.lit = point.map { priority.frame.contains($0) } ?? false
        status.lit = point.map { status.frame.contains($0) } ?? false
        for chip in chips { chip.lit = over }
    }
}

/// A small symbol that is a button: lit under the pointer, in its own colour when it says
/// something, and turning while what it changes is being changed.
private final class LinearRowButton: LayerView {
    static let side = ControlSize.small.height
    private static let symbolSize = ControlSize.small.symbol

    private let highlight = SurfaceView()
    private let symbol = SymbolView(size: LinearRowButton.symbolSize)
    private let loader = CAShapeLayer()
    private var tint: PlatformColor?

    var lit = false {
        didSet {
            guard lit != oldValue else { return }
            highlight.fill = lit ? Theme.backgroundAccentLargerStronger : .clear
            colorSymbol()
        }
    }

    var spinning = false {
        didSet {
            guard spinning != oldValue else { return }
            symbol.isHidden = spinning
            loader.isHidden = !spinning
            guard spinning else { return loader.removeAnimation(forKey: "turn") }
            let turn = CABasicAnimation(keyPath: "transform.rotation.z")
            turn.fromValue = 0
            turn.toValue = CGFloat.pi * 2
            turn.duration = 1.2
            turn.repeatCount = .infinity
            loader.add(turn, forKey: "turn")
        }
    }

    override init(frame: CGRect) {
        super.init(frame: frame)
        highlight.radius = Radius.sm
        addSubview(highlight)
        addSubview(symbol)
        loader.path = PlatformImage.symbolPath(.loader, size: Self.symbolSize * 0.85)
        loader.isHidden = true
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    func show(_ symbol: Symbol, tip: String, tint: PlatformColor?) {
        self.symbol.show(symbol, size: Self.symbolSize)
        self.tip = tip
        self.tint = tint
        colorSymbol()
        repaint()
    }

    private func colorSymbol() {
        symbol.tint = tint ?? (lit ? Theme.foreground : Theme.mutedForeground)
    }

    override func layoutNow() {
        highlight.frame = bounds
        symbol.frame = bounds
        let side = PlatformImage.symbolSide(Self.symbolSize * 0.85)
        loader.bounds = CGRect(x: 0, y: 0, width: side, height: side)
        loader.position = CGPoint(x: bounds.midX, y: bounds.midY)
    }

    override func paint(_ layer: CALayer) {
        if loader.superlayer !== layer { layer.addSublayer(loader) }
        loader.fillColor = resolved(tint ?? Theme.mutedForeground)
    }
}

/// A label's name beside its colour, on a capsule.
private final class LinearChipView: FlippedView {
    static let height = Chip.height
    private static let padding: CGFloat = 7
    private static let dot: CGFloat = 7

    private let fill = SurfaceView()
    private let dot = SurfaceView()
    private let words = TextLabel(font: .ui(11.5, weight: .medium), color: Theme.foreground)
    private(set) var width: CGFloat = 0

    var lit = false {
        didSet {
            guard lit != oldValue else { return }
            fill.fill = lit ? Theme.backgroundSecondaryAccent : Theme.backgroundSecondary
        }
    }

    override init(frame: CGRect) {
        super.init(frame: frame)
        fill.radius = Self.height / 2
        fill.fill = Theme.backgroundSecondary
        dot.radius = Self.dot / 2
        addSubview(fill)
        addSubview(dot)
        addSubview(words)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    func show(_ label: LinearRow.Label) {
        isHidden = false
        dot.fill = PlatformColor(label.color)
        words.string = label.name
        width = Self.padding + Self.dot + 5 + words.natural.width + Self.padding
    }

    override func layoutNow() {
        fill.frame = bounds
        dot.frame = CGRect(x: Self.padding, y: ((bounds.height - Self.dot) / 2).rounded(), width: Self.dot, height: Self.dot)
        let height = words.natural.height
        let x = Self.padding + Self.dot + 5
        words.frame = CGRect(x: x, y: ((bounds.height - height) / 2).rounded(), width: max(0, bounds.width - x - Self.padding), height: height)
    }
}

/// Whose the issue is, by initials in a disc that says the name under the pointer.
private final class LinearInitialsView: FlippedView {
    static let side = scaled(18)

    private let disc = SurfaceView()
    private let letters = TextLabel(font: .ui(9, weight: .semibold), color: Theme.mutedForeground)

    override init(frame: CGRect) {
        super.init(frame: frame)
        disc.radius = Self.side / 2
        disc.fill = Theme.backgroundAccent
        letters.centered = true
        addSubview(disc)
        addSubview(letters)
    }

    required init?(coder: NSCoder) { fatalError("not used") }

    func show(_ initials: String, name: String) {
        letters.string = initials
        tip = name
    }

    override func layoutNow() {
        disc.frame = bounds
        let height = letters.natural.height
        letters.frame = CGRect(x: 0, y: ((bounds.height - height) / 2).rounded(), width: bounds.width, height: height)
    }
}
