#if os(macOS)
import CMotileCore
import SwiftUI

/// The model with its account, the effort with the context window, and the access: one button
/// that opens the picker which chooses all of them.
struct ComposerSettings: View {
    @Environment(AppStore.self) private var store
    let labeled: Bool
    let modelLabeled: Bool

    var body: some View {
        let reach = ControlReach(size: .regular, wordless: false, margin: ComposerView.margin(trailing: 4))
        let open = store.composerPicker
        Button { toggle(.model) } label: {
            parts
                .anchorPreference(key: OpenDropdown.self, value: .bounds) { anchor in
                    guard let open else { return nil }
                    let picker = ComposerPicker(part: open, agent: store.composerModel?.agent).environment(store)
                    return OpenDropdown.Shown(anchor: anchor, content: AnyView(picker), up: true, trailing: true) {
                        guard !store.composerAccessOpen else {
                            store.composerAccessOpen = false
                            return
                        }
                        store.composerPicker = nil
                    } closeAll: {
                        store.composerAccessOpen = false
                        store.composerPicker = nil
                    }
                }
                .padding(reach.around)
        }
        .buttonStyle(ControlButtonStyle(look: ControlLook(variant: .ghost, size: .regular, selected: open != nil), margin: reach.around))
        .padding(reach.outset)
        .help(store.shortcuts.help("Model, effort and access", "modelPicker.toggle"))
        .disabled(store.composerModels.isEmpty)
        .shortcutTarget("modelPicker.toggle")
        .fixedSize()
    }

    /// The room on either side of a divider: after a part, and before the next part's symbol,
    /// which stands closer to its edge than words do. Both look 9 points wide.
    private static let roomAfter: CGFloat = 8
    private static let roomBeforeSymbol: CGFloat = 13

    private var parts: some View {
        let model = store.composerModel
        let effort = store.composerEffort
        let context = store.composerContext?.label
        let access = store.composerAccess
        let modelTitle = modelLabeled || model == nil ? store.composerModelLabel : nil
        // "Medium · 1M": the effort, and the context window where the model offers more than one.
        let effortTitle = labeled ? [effort.map(ComposerView.effortLabel), context].compactMap { $0 }.joined(separator: " · ") : context
        return HStack(spacing: 0) {
            part(modelTitle, icon: model.map { .picture(AnyView(AgentIcon(agent: $0.agent, size: ControlSize.regular.symbol))) },
                 proxied: store.composerProxied, pinned: store.composerPinned)
            if effort != nil || context != nil {
                divider
                part(effortTitle, icon: effort.map { .symbol(ComposerView.effortSymbol($0)) })
            }
            divider
            part(labeled ? access.label : nil, icon: .symbol(access.symbol))
        }
        .padding(.horizontal, ControlSize.regular.padding)
    }

    /// A part: its symbol and its words, or its symbol alone, which stands as close to the edge.
    @ViewBuilder private func part(_ title: String?, icon: ControlIcon?, proxied: Bool = false, pinned: Bool = false) -> some View {
        if let title {
            ControlLabel(title: title, icon: icon, size: .regular, gap: 5, padding: 0, proxied: proxied, pinned: pinned)
        } else {
            Group {
                switch icon {
                case .symbol(let symbol): Image(symbol, size: ControlSize.regular.symbol)
                case .picture(let picture), .pictures(let picture): picture
                case nil: EmptyView()
                }
            }
            .padding(.leading, -ControlSize.regular.symbolOutset)
            .frame(height: ControlSize.regular.height)
        }
    }

    private var divider: some View {
        ComposerDivider()
            .padding(.leading, Self.roomAfter)
            .padding(.trailing, Self.roomBeforeSymbol)
    }

    private func toggle(_ part: ComposerPickerPart) {
        store.composerPicker = store.composerPicker == nil ? part : nil
        store.composerAccessOpen = store.composerPicker == .access
    }
}

/// The agents down its side, the models of each account of the one picked under the account's
/// name and the limits it has used, the chosen model's efforts and context windows beside them,
/// and under all of them the access, which opens a menu of its own. A search goes through every agent's models, and finds each
/// of its words in a model's name or its account's, in any order.
private struct ComposerPicker: View {
    private static let railWidth: CGFloat = 44
    private static let listWidth: CGFloat = 232
    private static let settingsWidth: CGFloat = 200
    private static let checkGap: CGFloat = 12
    private static let height: CGFloat = 348
    private static let shortest: CGFloat = 180
    private static let padding = DropdownList.padding
    private static let headerGap: CGFloat = 6
    /// The search's symbol stands this much closer to the edge than the rows' symbols, which it looks aligned with.
    private static let searchNudge: CGFloat = 2
    /// The room below the title's baseline, which makes the bars look further from the title than `headerGap`.
    private static let titleDescent = -PlatformFont.ui(ControlSize.regular.textSize, weight: .medium).descender
    private static let railLight = EdgeInsets(top: 2, leading: 0, bottom: 2, trailing: 0)

    @Environment(AppStore.self) private var store
    let part: ComposerPickerPart
    @State private var query = ""
    @State private var agent: Agent?

    init(part: ComposerPickerPart, agent: Agent?) {
        self.part = part
        _agent = State(initialValue: agent)
    }
    @State private var column = ComposerPickerPart.model
    @State private var lit: Int?
    @State private var shownAccess: Access?
    /// The model row the list shows next. Only the keys and opening move the list, never the pointer.
    @State private var scrollTarget: Int?
    @FocusState private var searching: Bool

    private struct Group: Identifiable {
        let title: String
        let account: AgentAccount
        let windows: [LimitsReport.Window]
        /// The gateway's account whose limits `windows` are, for the Gateway account.
        let next: String?
        let rows: [Row]
        var id: String { account.id }
    }

    private struct Row: Identifiable {
        let account: AgentAccount
        let model: ModelInfo
        let index: Int
        var id: String { "\(account.id)/\(model.id)" }
    }

    /// The effort and context window rows of the column beside the models, in order.
    private enum Setting: Equatable {
        case effort(String)
        case context(ModelContext)
    }

    var body: some View {
        let groups = self.groups
        let rows = groups.flatMap(\.rows)
        let settings = self.settings
        VStack(spacing: 0) {
            HStack(spacing: 0) {
                rail
                    .frame(width: Self.railWidth)
                Rectangle()
                    .fill(Color.themeBorderPopover)
                    .frame(width: 1)
                VStack(spacing: 0) {
                    search(rows: rows, settings: settings)
                    ThemeDivider()
                    list(groups)
                }
                .frame(width: Self.listWidth)
                Rectangle()
                    .fill(Color.themeBorderPopover)
                    .frame(width: 1)
                settingsColumn(settings)
                    .frame(width: Self.settingsWidth)
            }
            .frame(minHeight: Self.shortest, maxHeight: Self.height)
            ThemeDivider()
            footer
        }
        .overlay {
            // A press anywhere else in the picker closes the access menu only, as a menu open
            // inside a menu does.
            if store.composerAccessOpen {
                PressCatcherView { openAccess(false) }
            }
        }
        .fixedSize(horizontal: true, vertical: false)
        .dropdownCard()
        // Over the card rather than inside it, which would clip the menu's shadow.
        .overlay(alignment: .bottomTrailing) {
            if store.composerAccessOpen {
                accessMenu
                    .padding(.trailing, Self.padding)
                    .padding(.bottom, Self.accessMenuLift)
            }
        }
        .onAppear {
            column = part
            lit = chosenIndex(in: part, rows: rows, settings: settings)
            scrollTarget = lit
            searching = true
        }
        .onChange(of: part) { _, part in
            column = part
            lit = chosenIndex(in: part, rows: rows, settings: settings)
            scrollTarget = lit
        }
        .onChange(of: store.composerAccessOpen) { _, open in
            shownAccess = nil
            guard open else { return }
            column = .access
            lit = chosenIndex(in: .access, rows: rows, settings: settings)
        }
        .onChange(of: store.composerPickerJump) { _, jump in
            guard let jump else { return }
            store.composerPickerJump = nil
            guard rows.indices.contains(jump) else { return }
            choose(rows[jump])
        }
        .onChange(of: store.composerPickerProviderStep) { _, step in
            guard let step else { return }
            store.composerPickerProviderStep = nil
            let agents = self.agents
            guard !agents.isEmpty else { return }
            let index = agent.flatMap { agents.firstIndex(of: $0) }
            let next = index.map { ($0 + step + agents.count) % agents.count } ?? (step > 0 ? 0 : agents.count - 1)
            agent = agents[next]
            query = ""
            column = .model
            lit = 0
            scrollTarget = 0
            store.composerAccessOpen = false
            searching = true
        }
    }

    // MARK: Models

    private var groups: [Group] {
        let searching = !query.trimmingCharacters(in: .whitespaces).isEmpty
        let choices = store.composerChoices.filter { searching || agent == nil || $0.account.agent == agent }
        let candidates = choices.flatMap { choice in choice.models.map { (choice, $0) } }
        let found = searching
            ? Self.found(query, in: candidates.map { choice, model in [model.name, model.shortName, choice.account.name, choice.account.agent.name] })
            : Set(candidates.indices)
        var index = 0
        var offset = 0
        return choices.compactMap { choice in
            let rows = choice.models.compactMap { model -> Row? in
                defer { offset += 1 }
                guard found.contains(offset) else { return nil }
                defer { index += 1 }
                return Row(account: choice.account, model: model, index: index)
            }
            guard !rows.isEmpty else { return nil }
            let windows = store.composerServer?.accountLimits[choice.account.id] ?? []
            let next = store.composerServer?.nextAccounts[choice.account.id]
            return Group(title: choice.title, account: choice.account, windows: windows, next: next, rows: rows)
        }
    }

    /// Which rows the core's search finds: each word in a row's texts, in any order.
    private static func found(_ query: String, in rows: [[String]]) -> Set<Int> {
        let request: [String: Any] = ["query": query, "rows": rows]
        guard let data = try? JSONSerialization.data(withJSONObject: request), let json = String(data: data, encoding: .utf8),
              let answer = json.withCString({ motile_model_search($0) }) else { return [] }
        defer { motile_free_string(answer) }
        let indices = (try? JSONDecoder().decode([Int].self, from: Data(String(cString: answer).utf8))) ?? []
        return Set(indices)
    }

    /// The agents with models here, Claude Code's first.
    private var agents: [Agent] {
        let offered = Set(store.composerChoices.map(\.account.agent))
        return Agent.allCases.filter(offered.contains)
    }

    /// The agents to pick the models of. While a search goes through all of them, none is picked.
    private var rail: some View {
        VStack(spacing: 0) {
            ForEach(agents, id: \.self) { shown in
                AgentIcon(agent: shown, size: .lg)
                    .frame(width: Self.railWidth - 2 * Self.padding, height: Self.railWidth - 2 * Self.padding + 4)
                    .button(.highlight(radius: Radius.sm, selected: query.isEmpty && agent == shown, inset: Self.railLight)) {
                        agent = shown
                        query = ""
                        column = .model
                        lit = 0
                        searching = true
                    }
                    .help(shown.name)
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, Self.padding)
        .padding(.vertical, Self.padding - 2)
    }

    private func search(rows: [Row], settings: [Setting]) -> some View {
        HStack(spacing: DropdownList.gap) {
            Image(.search, size: DropdownList.symbol)
                .foregroundStyle(Color.themeMutedStrongerForeground)
                .allowsHitTesting(false)
            LineField(text: $query, font: DropdownList.lineFont)
                .placeholder("Search models", shown: query.isEmpty)
                .font(DropdownList.font)
                .focused($searching)
                .onChange(of: query) {
                    column = .model
                    lit = rows.isEmpty ? nil : 0
                }
                .onReturn { pickLit(rows: rows, settings: settings) }
                .onLineKey(.downArrow) { step(1, rows: rows, settings: settings) }
                .onLineKey(.upArrow) { step(-1, rows: rows, settings: settings) }
                .onLineKey(.rightArrow) { move(1, rows: rows, settings: settings) }
                .onLineKey(.leftArrow) { move(-1, rows: rows, settings: settings) }
        }
        .padding(.leading, DropdownList.searchPadding - Self.searchNudge)
        .padding(.trailing, DropdownList.searchPadding)
        .frame(height: DropdownList.searchHeight)
        // All of the row is the field: a click anywhere on it, its icon too, types there.
        .background { TypingArea { searching = true } }
    }

    private func list(_ groups: [Group]) -> some View {
        ScrollViewReader { proxy in
            ScrollView {
                VStack(alignment: .leading, spacing: 0) {
                    ForEach(groups) { group in
                        if group.id != groups.first?.id {
                            sectionDivider
                        }
                        header(group)
                        ForEach(group.rows) { row in
                            modelRow(row)
                                .id(row.id)
                        }
                    }
                    if groups.isEmpty {
                        Text("No model matches")
                            .foregroundStyle(Color.themeMutedForeground)
                            .dropdownRow()
                    }
                }
                .padding(Self.padding)
            }
            .scrollIndicators(.never)
            .fadesHiddenEdges()
            .onChange(of: scrollTarget) { _, target in
                guard let target else { return }
                scrollTarget = nil
                guard column == .model, let row = groups.flatMap(\.rows).first(where: { $0.index == target }) else { return }
                proxy.scrollTo(row.id)
            }
        }
    }

    /// Between two sections, as far from the last row's words as from the next title's, which has
    /// room of its own above it.
    private var sectionDivider: some View {
        ThemeDivider()
            .padding(.horizontal, 8)
            .padding(.top, 6)
            .padding(.bottom, 2)
    }

    private func header(_ group: Group) -> some View {
        VStack(alignment: .leading, spacing: Self.headerGap) {
            // "Claude Code", the gateway's symbol, then the account its bars are from, a step quieter.
            HStack(spacing: 0) {
                Text(verbatim: group.title)
                    .gatewayMarked(group.account.gateway, fontSize: ControlSize.regular.textSize, size: ControlSize.regular.symbol)
                    .foregroundStyle(Color.themeMutedForeground)
                    .layoutPriority(1)
                if let next = group.next {
                    Text(verbatim: " · ")
                        .foregroundStyle(Color.themeMutedStrongestForeground)
                    Text(verbatim: next)
                        .foregroundStyle(Color.themeMutedStrongerForeground)
                        .truncationMode(.middle)
                }
            }
            .font(ControlSize.regular.font)
            .lineLimit(1)
            if !group.windows.isEmpty {
                LimitPills(windows: group.windows)
            }
        }
        .padding(.horizontal, DropdownList.rowPadding)
        .padding(.top, 8)
        // The bars stand as far from a lit row as from the words above them.
        .padding(.bottom, group.windows.isEmpty ? 4 : Self.headerGap + Self.titleDescent - DropdownList.light.top)
    }

    private func modelRow(_ row: Row) -> some View {
        let chosen = row.model.id == store.composerModel?.id && row.account.id == store.composerAccount?.id
        let key = row.index < 9 ? store.shortcuts.label("modelPicker.jump.\(row.index + 1)", in: store.shortcutContext(composerFocus: false, editableFocus: true)) : nil
        return HStack(spacing: DropdownList.gap) {
            AgentIcon(agent: row.model.agent, size: DropdownList.symbol)
            Text(row.model.shortName)
                .lineLimit(1)
            Spacer(minLength: 8)
            if let key {
                KeyCaps([key])
                    .foregroundStyle(Color.themeMutedForeground)
            }
            Image(.check, size: DropdownList.check)
                .opacity(chosen ? 1 : 0)
        }
        .dropdownRow()
        .button(.dropdownLight(lit: column == .model && lit == row.index)) { choose(row) }
        .onHover { inside in
            guard inside else { return }
            column = .model
            lit = row.index
        }
    }

    private func choose(_ row: Row) {
        store.setModel(row.model, account: row.account)
        store.composerPicker = nil
    }

    // MARK: Effort and context window

    private var settings: [Setting] {
        guard let model = store.composerModel else { return [] }
        let efforts = model.efforts.isEmpty ? [] : model.effortChoices.map(Setting.effort)
        let contexts = model.contexts.count > 1 ? model.contexts.map(Setting.context) : []
        return efforts + contexts
    }

    private func settingsColumn(_ settings: [Setting]) -> some View {
        let model = store.composerModel
        return ScrollView {
            settingsList(settings, model: model)
        }
        .scrollIndicators(.never)
        .fadesHiddenEdges()
    }

    private func settingsList(_ settings: [Setting], model: ModelInfo?) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(settings.enumerated()), id: \.offset) { index, setting in
                if index == 0, case .effort = setting {
                    DropdownTitle("Reasoning", about: model?.shortName)
                }
                if case .context = setting, isFirstContext(at: index, in: settings) {
                    if index > 0 {
                        sectionDivider
                    }
                    DropdownTitle("Context window")
                }
                settingRow(setting, index: index, model: model)
            }
            if settings.isEmpty {
                Text("\(model?.name ?? "This model") has no settings to choose")
                    .font(.ui(.xs))
                    .foregroundStyle(Color.themeMutedForeground)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(8)
            }
        }
        .padding(Self.padding)
    }

    private func isFirstContext(at index: Int, in settings: [Setting]) -> Bool {
        guard index > 0 else { return true }
        if case .context = settings[index - 1] { return false }
        return true
    }

    private func settingRow(_ setting: Setting, index: Int, model: ModelInfo?) -> some View {
        let (title, symbol, isDefault, chosen): (String, Symbol?, Bool, Bool) = switch setting {
        case .effort(let effort):
            (ComposerView.effortLabel(effort), ComposerView.effortSymbol(effort), effort == model?.defaultEffort, effort == store.composerEffort)
        case .context(let context):
            (context.label, nil, context.isDefault, context == store.composerContext)
        }
        return HStack(spacing: Self.checkGap) {
            HStack(spacing: DropdownList.gap) {
                if let symbol {
                    Image(symbol, size: DropdownList.symbol)
                        .foregroundStyle(Color.themeMutedForeground)
                }
                Text(title)
                    .lineLimit(1)
                if isDefault {
                    Chip("Default", muted: true)
                        .fixedSize()
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            Image(.check, size: DropdownList.check)
                .opacity(chosen ? 1 : 0)
        }
        .dropdownRow()
        .button(.dropdownLight(lit: column == .effort && lit == index)) { pick(setting) }
        .onHover { inside in
            guard inside else { return }
            column = .effort
            lit = index
        }
    }

    private func pick(_ setting: Setting) {
        switch setting {
        case .effort(let effort): store.setEffort(effort)
        case .context(let context): store.setContext(context)
        }
        store.composerPicker = nil
    }

    // MARK: Access

    private static let accessMenuWidth: CGFloat = 288
    private static let accessMenuPadding = EdgeInsets(top: 12, leading: 14, bottom: 12, trailing: 14)
    /// How far the tabs reach past the words' edges, into the menu's padding.
    private static let accessTabsOutset: CGFloat = 4

    private var footer: some View {
        let current = store.composerAccess
        let open = store.composerAccessOpen
        return HStack(spacing: 0) {
            Spacer(minLength: 0)
            ActionButton(current.label, icon: current.symbol, help: current.detail, variant: .ghost, size: .regular, selected: open || column == .access, opens: true, open: open, tint: .themeForeground) {
                openAccess(!open)
            }
        }
        .padding(Self.padding)
    }

    /// Over the columns, just above the footer's button, as a menu opens above what opened it.
    private static let accessMenuLift = ControlSize.regular.height + 2 * padding + 4

    /// The access modes as one row of their symbols, the one under the pointer or the arrow keys
    /// named and told about under it.
    private var accessMenu: some View {
        let current = store.composerAccess
        let shown = shownAccess ?? current
        return VStack(alignment: .leading, spacing: 8) {
            Text("Access")
                .font(ControlSize.regular.font)
                .foregroundStyle(Color.themeMutedForeground)
            Segmented(
                symbols: Access.allCases.map { ($0.label, $0.symbol, $0) },
                selection: Binding { current } set: { setAccess($0) },
                fills: true, compact: true, lit: litAccess, hovered: $shownAccess
            )
            .padding(.horizontal, -Self.accessTabsOutset)
            VStack(alignment: .leading, spacing: 2) {
                Text(shown.label)
                    .font(.ui(.sm))
                    .foregroundStyle(Color.themeForeground)
                Text(shown.detail)
                    .font(.ui(.xs))
                    .foregroundStyle(Color.themeMutedForeground)
                    .lineLimit(1)
            }
        }
        .padding(Self.accessMenuPadding)
        .frame(width: Self.accessMenuWidth)
        .dropdownCard()
    }

    /// `lit` counts the access modes only while the access column has the keys.
    private var litAccess: Access? {
        guard column == .access, let lit, Access.allCases.indices.contains(lit) else { return nil }
        return Access.allCases[lit]
    }

    private func openAccess(_ open: Bool) {
        store.composerAccessOpen = open
        column = .access
        lit = open ? Access.allCases.firstIndex(of: store.composerAccess) : 0
        shownAccess = nil
    }

    private func setAccess(_ access: Access) {
        store.setAccess(access)
        store.composerAccessOpen = false
        store.composerPicker = nil
    }

    // MARK: Keys

    private func count(of column: ComposerPickerPart, rows: [Row], settings: [Setting]) -> Int {
        switch column {
        case .model: rows.count
        case .effort: settings.count
        case .access: store.composerAccessOpen ? Access.allCases.count : 1
        }
    }

    private func chosenIndex(in column: ComposerPickerPart, rows: [Row], settings: [Setting]) -> Int? {
        switch column {
        case .model:
            rows.firstIndex { $0.model.id == store.composerModel?.id && $0.account.id == store.composerAccount?.id }
        case .effort:
            settings.firstIndex(of: .effort(store.composerEffort ?? "")) ?? (settings.isEmpty ? nil : 0)
        case .access:
            store.composerAccessOpen ? Access.allCases.firstIndex(of: store.composerAccess) : 0
        }
    }

    private func step(_ by: Int, rows: [Row], settings: [Setting]) -> KeyPress.Result {
        guard !store.composerAccessOpen else { return .handled }
        let count = count(of: column, rows: rows, settings: settings)
        guard count > 0 else { return .handled }
        lit = min(max((lit ?? -by) + by, 0), count - 1)
        scrollTarget = lit
        return .handled
    }

    /// Left and right go between the columns, while there is no search whose text they move in.
    private func move(_ by: Int, rows: [Row], settings: [Setting]) -> KeyPress.Result {
        if store.composerAccessOpen {
            lit = min(max((litAccess.flatMap(Access.allCases.firstIndex) ?? 0) + by, 0), Access.allCases.count - 1)
            column = .access
            shownAccess = lit.map { Access.allCases[$0] }
            return .handled
        }
        guard query.isEmpty else { return .ignored }
        let columns: [ComposerPickerPart] = [.model, .effort, .access].filter { count(of: $0, rows: rows, settings: settings) > 0 }
        guard let at = columns.firstIndex(of: column) else { return .handled }
        let next = columns[min(max(at + by, 0), columns.count - 1)]
        guard next != column else { return .handled }
        column = next
        lit = chosenIndex(in: next, rows: rows, settings: settings) ?? 0
        scrollTarget = lit
        return .handled
    }

    private func pickLit(rows: [Row], settings: [Setting]) {
        guard let lit else { return }
        switch column {
        case .model:
            guard rows.indices.contains(lit) else { return }
            choose(rows[lit])
        case .effort:
            guard settings.indices.contains(lit) else { return }
            pick(settings[lit])
        case .access:
            guard store.composerAccessOpen else { return openAccess(true) }
            guard let access = litAccess else { return }
            setAccess(access)
        }
    }
}
#endif
