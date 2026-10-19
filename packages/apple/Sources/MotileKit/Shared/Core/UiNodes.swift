// Written by scripts/ui-types.sh from crates/core/src/render/openui/tree.rs: do not edit.

import Foundation

/// A colour: the text's own ("current"), a token by name, a token's wash, a layer of the surface
/// it lies on, or `[r, g, b, a]`.
enum UiColor {
    case current
    case token(String)
    case wash(String)
    case surface(String)
    case rgba([Int])

    init?(json value: Any?) {
        switch value {
        case let name as String:
            self = name == "current" ? .current : .token(name)
        case let object as JSON:
            if let token = object["wash"] as? String {
                self = .wash(token)
            } else if let layer = object["surface"] as? String {
                self = .surface(layer)
            } else {
                return nil
            }
        case let channels as [NSNumber]:
            self = .rgba(channels.map(\.intValue))
        default:
            return nil
        }
    }
}

struct UiNode {
    /// The path from the program's root, its props and indexes joined by `/`: `children/2`.
    let id: String
    let a11y: UiA11y?
    let kind: UiKind

    init?(json: JSON) {
        id = json["id"] as? String ?? ""
        a11y = (json["a11y"] as? JSON).map(UiA11y.init(json:))
        guard let kind = UiKind(json: json) else { return nil }
        self.kind = kind
    }
}

struct UiA11y {
    let role: UiRole
    let label: String?
    let value: String?
    /// A heading's level.
    let level: Int?
    /// Pressed, expanded or checked, by the role.
    let on: Bool?

    init(json: JSON) {
        role = UiRole(rawValue: json["role"] as? String ?? "") ?? .group
        label = json["label"] as? String
        value = json["value"] as? String
        level = (json["level"] as? NSNumber)?.intValue
        on = json["on"] as? Bool
    }
}

enum UiRole: String {
    case group
    case button
    case dialog
    case heading
    case text
    case link
    case list
    case listItem = "list_item"
    case blockquote
    case code
    case image
    case textField = "text_field"
    case popUpButton = "pop_up_button"
    case radioGroup = "radio_group"
    case adjustable
    case checkbox
    case timer
    case form
    case table
    case region
    case hidden
}

indirect enum UiKind {
    /// Its children stacked, each `spacing` below the one before.
    case col(Col)
    case row(Row)
    case box(Box)
    case grid(Grid)
    case card(Card)
    case carousel(Carousel)
    case popover(Popover)
    case divider
    case spacer
    case title(Title)
    case text(Text)
    case caption(Caption)
    case label(Label)
    case badge(Badge)
    case link(Link)
    case list(List)
    case listItem(ListItem)
    case markdown(Markdown)
    case quote(Quote)
    case code(Code)
    case math(Math)
    case button(Button)
    case iconButton(IconButton)
    case pressable(Pressable)
    case input(Input)
    case textArea(TextArea)
    case select(Select)
    case segmentedControl(SegmentedControl)
    case slider(Slider)
    case checkbox(Checkbox)
    case radioGroup(RadioGroup)
    case datePicker(DatePicker)
    case timer(Timer)
    case form(Form)
    case searchImage(SearchImage)
    case image(Image)
    case icon(Icon)
    case table(Table)
    case chart(Chart)
    case map(Map)
    case mapMarker(MapMarker)
    case youTube(YouTube)
    case flashcard(Flashcard)
    case celebration(Celebration)
    case svg(Svg)
    case g(G)
    case circle(Circle)
    case ellipse(Ellipse)
    case rect(Rect)
    case line(Line)
    case path(Path)
    case polyline(Polyline)
    case polygon(Polygon)
    case svgText(SvgText)
    case htmlView(HtmlView)

    init?(json: JSON) {
        switch json["component"] as? String {
        case "Col":
            self = .col(Col(json: json))
        case "Row":
            self = .row(Row(json: json))
        case "Box":
            self = .box(Box(json: json))
        case "Grid":
            self = .grid(Grid(json: json))
        case "Card":
            self = .card(Card(json: json))
        case "Carousel":
            self = .carousel(Carousel(json: json))
        case "Popover":
            self = .popover(Popover(json: json))
        case "Divider":
            self = .divider
        case "Spacer":
            self = .spacer
        case "Title":
            guard let value = Title(json: json) else { return nil }
            self = .title(value)
        case "Text":
            guard let value = Text(json: json) else { return nil }
            self = .text(value)
        case "Caption":
            guard let value = Caption(json: json) else { return nil }
            self = .caption(value)
        case "Label":
            guard let value = Label(json: json) else { return nil }
            self = .label(value)
        case "Badge":
            self = .badge(Badge(json: json))
        case "Link":
            self = .link(Link(json: json))
        case "List":
            self = .list(List(json: json))
        case "ListItem":
            guard let value = ListItem(json: json) else { return nil }
            self = .listItem(value)
        case "Markdown":
            self = .markdown(Markdown(json: json))
        case "Quote":
            self = .quote(Quote(json: json))
        case "Code":
            self = .code(Code(json: json))
        case "Math":
            self = .math(Math(json: json))
        case "Button":
            self = .button(Button(json: json))
        case "IconButton":
            self = .iconButton(IconButton(json: json))
        case "Pressable":
            self = .pressable(Pressable(json: json))
        case "Input":
            self = .input(Input(json: json))
        case "TextArea":
            self = .textArea(TextArea(json: json))
        case "Select":
            self = .select(Select(json: json))
        case "SegmentedControl":
            self = .segmentedControl(SegmentedControl(json: json))
        case "Slider":
            self = .slider(Slider(json: json))
        case "Checkbox":
            self = .checkbox(Checkbox(json: json))
        case "RadioGroup":
            self = .radioGroup(RadioGroup(json: json))
        case "DatePicker":
            self = .datePicker(DatePicker(json: json))
        case "Timer":
            self = .timer(Timer(json: json))
        case "Form":
            self = .form(Form(json: json))
        case "SearchImage":
            self = .searchImage(SearchImage(json: json))
        case "Image":
            self = .image(Image(json: json))
        case "Icon":
            guard let value = Icon(json: json) else { return nil }
            self = .icon(value)
        case "Table":
            self = .table(Table(json: json))
        case "Chart":
            self = .chart(Chart(json: json))
        case "Map":
            self = .map(Map(json: json))
        case "MapMarker":
            self = .mapMarker(MapMarker(json: json))
        case "YouTube":
            self = .youTube(YouTube(json: json))
        case "Flashcard":
            self = .flashcard(Flashcard(json: json))
        case "Celebration":
            self = .celebration(Celebration(json: json))
        case "Svg":
            self = .svg(Svg(json: json))
        case "G":
            self = .g(G(json: json))
        case "Circle":
            self = .circle(Circle(json: json))
        case "Ellipse":
            self = .ellipse(Ellipse(json: json))
        case "Rect":
            self = .rect(Rect(json: json))
        case "Line":
            self = .line(Line(json: json))
        case "Path":
            self = .path(Path(json: json))
        case "Polyline":
            self = .polyline(Polyline(json: json))
        case "Polygon":
            self = .polygon(Polygon(json: json))
        case "SvgText":
            self = .svgText(SvgText(json: json))
        case "HtmlView":
            self = .htmlView(HtmlView(json: json))
        default:
            return nil
        }
    }

    struct Col {
        let children: [UiNode]
        let align: UiAlign
        let spacing: [Double]

        init(json: JSON) {
            children = (json["children"] as? [Any] ?? []).compactMap { UiNode(json: $0 as? JSON ?? [:]) }
            align = UiAlign(rawValue: json["align"] as? String ?? "") ?? .start
            spacing = (json["spacing"] as? [Any] ?? []).map { ($0 as? NSNumber)?.doubleValue ?? 0 }
        }
    }

    struct Row {
        let children: [UiNode]
        let gap: Double
        let align: UiAlign
        let justify: UiJustify
        let wrap: Bool
        /// How wide each child starts and whether it grows or shrinks.
        let sizing: [UiSizing]

        init(json: JSON) {
            children = (json["children"] as? [Any] ?? []).compactMap { UiNode(json: $0 as? JSON ?? [:]) }
            gap = (json["gap"] as? NSNumber)?.doubleValue ?? 0
            align = UiAlign(rawValue: json["align"] as? String ?? "") ?? .start
            justify = UiJustify(rawValue: json["justify"] as? String ?? "") ?? .start
            wrap = json["wrap"] as? Bool ?? false
            sizing = (json["sizing"] as? [Any] ?? []).map { UiSizing(json: $0 as? JSON ?? [:]) }
        }
    }

    struct Box {
        let children: [UiNode]
        let direction: UiDirection
        /// Between children in a row; a column has `spacing`.
        let gap: Double
        let spacing: [Double]
        let sizing: [UiSizing]
        let padding: Double
        let background: UiColor?
        /// The content lies on the box's inner surface.
        let innerSurface: Bool
        /// Rounds and clips; nothing is clipped without one.
        let radius: UiRadius?
        let border: Bool
        let width: UiLength?
        let height: UiLength?
        let flex: Double?
        let align: UiAlign
        let justify: UiJustify
        let wrap: Bool

        init(json: JSON) {
            children = (json["children"] as? [Any] ?? []).compactMap { UiNode(json: $0 as? JSON ?? [:]) }
            direction = UiDirection(rawValue: json["direction"] as? String ?? "") ?? .row
            gap = (json["gap"] as? NSNumber)?.doubleValue ?? 0
            spacing = (json["spacing"] as? [Any] ?? []).map { ($0 as? NSNumber)?.doubleValue ?? 0 }
            sizing = (json["sizing"] as? [Any] ?? []).map { UiSizing(json: $0 as? JSON ?? [:]) }
            padding = (json["padding"] as? NSNumber)?.doubleValue ?? 0
            background = UiColor(json: json["background"])
            innerSurface = json["inner_surface"] as? Bool ?? false
            radius = (json["radius"] as? JSON).map(UiRadius.init(json:))
            border = json["border"] as? Bool ?? false
            width = (json["width"] as? JSON).map(UiLength.init(json:))
            height = (json["height"] as? JSON).map(UiLength.init(json:))
            flex = (json["flex"] as? NSNumber)?.doubleValue
            align = UiAlign(rawValue: json["align"] as? String ?? "") ?? .start
            justify = UiJustify(rawValue: json["justify"] as? String ?? "") ?? .start
            wrap = json["wrap"] as? Bool ?? false
        }
    }

    struct Grid {
        let children: [UiNode]
        /// The most columns; `grid_columns` gives how many fit a width.
        let columns: Int
        let minChildWidth: Double?
        let gap: Double

        init(json: JSON) {
            children = (json["children"] as? [Any] ?? []).compactMap { UiNode(json: $0 as? JSON ?? [:]) }
            columns = (json["columns"] as? NSNumber)?.intValue ?? 0
            minChildWidth = (json["min_child_width"] as? NSNumber)?.doubleValue
            gap = (json["gap"] as? NSNumber)?.doubleValue ?? 0
        }
    }

    struct Card {
        let children: [UiNode]
        let soft: Bool
        /// A card in a card.
        let nested: Bool
        /// The program's padding, else the client's for the column's width.
        let padding: Double?
        let spacing: [Double]

        init(json: JSON) {
            children = (json["children"] as? [Any] ?? []).compactMap { UiNode(json: $0 as? JSON ?? [:]) }
            soft = json["soft"] as? Bool ?? false
            nested = json["nested"] as? Bool ?? false
            padding = (json["padding"] as? NSNumber)?.doubleValue
            spacing = (json["spacing"] as? [Any] ?? []).map { ($0 as? NSNumber)?.doubleValue ?? 0 }
        }
    }

    struct Carousel {
        let children: [UiNode]
        let itemWidth: Double
        let gap: Double
        /// The items as tall as the tallest: a Card or a Pressable.
        let fill: [Bool]

        init(json: JSON) {
            children = (json["children"] as? [Any] ?? []).compactMap { UiNode(json: $0 as? JSON ?? [:]) }
            itemWidth = (json["item_width"] as? NSNumber)?.doubleValue ?? 0
            gap = (json["gap"] as? NSNumber)?.doubleValue ?? 0
            fill = (json["fill"] as? [Any] ?? []).map { $0 as? Bool ?? false }
        }
    }

    struct Popover {
        let trigger: UiSlot
        let content: UiSlot
        let open: Bool

        init(json: JSON) {
            trigger = UiSlot(json: json["trigger"] as? JSON ?? [:])
            content = UiSlot(json: json["content"] as? JSON ?? [:])
            open = json["open"] as? Bool ?? false
        }
    }

    struct Title {
        let text: UiRich
        let style: UiTextStyle
        let color: UiColor

        init?(json: JSON) {
            text = UiRich(json: json["text"] as? JSON ?? [:])
            style = UiTextStyle(rawValue: json["style"] as? String ?? "") ?? .body
            guard let color = UiColor(json: json["color"]) else { return nil }
            self.color = color
        }
    }

    struct Text {
        /// A U+FFFC stands for each of `inlines`, in order.
        let text: UiRich
        let inlines: [UiNode]
        let style: UiTextStyle
        let color: UiColor
        let weight: UiWeight
        let italic: Bool
        let strike: Bool
        let digits: Bool
        let maxLines: Int?

        init?(json: JSON) {
            text = UiRich(json: json["text"] as? JSON ?? [:])
            inlines = (json["inlines"] as? [Any] ?? []).compactMap { UiNode(json: $0 as? JSON ?? [:]) }
            style = UiTextStyle(rawValue: json["style"] as? String ?? "") ?? .body
            guard let color = UiColor(json: json["color"]) else { return nil }
            self.color = color
            weight = UiWeight(rawValue: json["weight"] as? String ?? "") ?? .regular
            italic = json["italic"] as? Bool ?? false
            strike = json["strike"] as? Bool ?? false
            digits = json["digits"] as? Bool ?? false
            maxLines = (json["max_lines"] as? NSNumber)?.intValue
        }
    }

    struct Caption {
        let text: UiRich
        let style: UiTextStyle
        let color: UiColor

        init?(json: JSON) {
            text = UiRich(json: json["text"] as? JSON ?? [:])
            style = UiTextStyle(rawValue: json["style"] as? String ?? "") ?? .body
            guard let color = UiColor(json: json["color"]) else { return nil }
            self.color = color
        }
    }

    struct Label {
        let text: String
        let color: UiColor

        init?(json: JSON) {
            text = json["text"] as? String ?? ""
            guard let color = UiColor(json: json["color"]) else { return nil }
            self.color = color
        }
    }

    struct Badge {
        let text: String
        /// The chip's tone, a token; none for neutral and info.
        let tone: String?
        let icon: UiGlyph?

        init(json: JSON) {
            text = json["text"] as? String ?? ""
            tone = json["tone"] as? String
            icon = (json["icon"] as? JSON).map(UiGlyph.init(json:))
        }
    }

    struct Link {
        let text: String
        /// None when the program's URL isn't http(s): the text is plain.
        let url: String?

        init(json: JSON) {
            text = json["text"] as? String ?? ""
            url = json["url"] as? String
        }
    }

    struct List {
        /// ListItems.
        let items: [UiNode]
        let marker: UiListMarker
        let timeline: Bool
        /// A marker longer than two characters: the markers have a lane of text.
        let wide: Bool

        init(json: JSON) {
            items = (json["items"] as? [Any] ?? []).compactMap { UiNode(json: $0 as? JSON ?? [:]) }
            marker = UiListMarker(rawValue: json["marker"] as? String ?? "") ?? .bullet
            timeline = json["timeline"] as? Bool ?? false
            wide = json["wide"] as? Bool ?? false
        }
    }

    struct ListItem {
        let title: UiRich
        let style: UiTextStyle
        /// Medium, as it has a description or children.
        let medium: Bool
        let description: UiRich?
        let marker: UiItemMarker
        let children: [UiNode]

        init?(json: JSON) {
            title = UiRich(json: json["title"] as? JSON ?? [:])
            style = UiTextStyle(rawValue: json["style"] as? String ?? "") ?? .body
            medium = json["medium"] as? Bool ?? false
            description = (json["description"] as? JSON).map(UiRich.init(json:))
            guard let marker = UiItemMarker(json: json["marker"] as? JSON ?? [:]) else { return nil }
            self.marker = marker
            children = (json["children"] as? [Any] ?? []).compactMap { UiNode(json: $0 as? JSON ?? [:]) }
        }
    }

    struct Markdown {
        let blocks: [UiMarkdownBlock]

        init(json: JSON) {
            blocks = (json["blocks"] as? [Any] ?? []).compactMap { UiMarkdownBlock(json: $0 as? JSON ?? [:]) }
        }
    }

    struct Quote {
        let text: UiRich
        let cite: UiRich?

        init(json: JSON) {
            text = UiRich(json: json["text"] as? JSON ?? [:])
            cite = (json["cite"] as? JSON).map(UiRich.init(json:))
        }
    }

    struct Code {
        /// "code" when the program names none.
        let language: String
        let code: String
        /// `None` until the code has been highlighted, like a code row's.
        let spans: [Int]?

        init(json: JSON) {
            language = json["language"] as? String ?? ""
            code = json["code"] as? String ?? ""
            spans = json["spans"] == nil ? nil : (json["spans"] as? [Any] ?? []).map { ($0 as? NSNumber)?.intValue ?? 0 }
        }
    }

    struct Math {
        let canvas: UiCanvas

        init(json: JSON) {
            canvas = UiCanvas(json: json["canvas"] as? JSON ?? [:])
        }
    }

    struct Button {
        let label: String
        let variant: UiButtonVariant
        /// The colour an outline, soft or ghost button takes.
        let tint: String?
        let size: UiControlSize
        let icon: UiGlyph?
        let block: Bool
        let disabled: UiDisabled?

        init(json: JSON) {
            label = json["label"] as? String ?? ""
            variant = UiButtonVariant(rawValue: json["variant"] as? String ?? "") ?? .primary
            tint = json["tint"] as? String
            size = UiControlSize(rawValue: json["size"] as? String ?? "") ?? .small
            icon = (json["icon"] as? JSON).map(UiGlyph.init(json:))
            block = json["block"] as? Bool ?? false
            disabled = (json["disabled"] as? String).flatMap(UiDisabled.init(rawValue:))
        }
    }

    struct IconButton {
        let icon: UiGlyph?
        let label: String
        let tooltip: String?
        let variant: UiButtonVariant
        let disabled: UiDisabled?

        init(json: JSON) {
            icon = (json["icon"] as? JSON).map(UiGlyph.init(json:))
            label = json["label"] as? String ?? ""
            tooltip = json["tooltip"] as? String
            variant = UiButtonVariant(rawValue: json["variant"] as? String ?? "") ?? .primary
            disabled = (json["disabled"] as? String).flatMap(UiDisabled.init(rawValue:))
        }
    }

    struct Pressable {
        let children: [UiNode]
        let padding: Double
        let background: UiColor?
        let innerSurface: Bool
        let radius: UiRadius
        let selected: Bool?
        /// Selected, and its only child isn't a Row: the ring and the picked mark.
        let ring: Bool
        let disabled: UiDisabled?

        init(json: JSON) {
            children = (json["children"] as? [Any] ?? []).compactMap { UiNode(json: $0 as? JSON ?? [:]) }
            padding = (json["padding"] as? NSNumber)?.doubleValue ?? 0
            background = UiColor(json: json["background"])
            innerSurface = json["inner_surface"] as? Bool ?? false
            radius = UiRadius(json: json["radius"] as? JSON ?? [:])
            selected = json["selected"] as? Bool
            ring = json["ring"] as? Bool ?? false
            disabled = (json["disabled"] as? String).flatMap(UiDisabled.init(rawValue:))
        }
    }

    struct Input {
        let value: String
        let number: Bool
        let label: String?
        let placeholder: String?
        let error: String?
        let disabled: UiDisabled?

        init(json: JSON) {
            value = json["value"] as? String ?? ""
            number = json["number"] as? Bool ?? false
            label = json["label"] as? String
            placeholder = json["placeholder"] as? String
            error = json["error"] as? String
            disabled = (json["disabled"] as? String).flatMap(UiDisabled.init(rawValue:))
        }
    }

    struct TextArea {
        let value: String
        let label: String?
        let placeholder: String?
        let rows: Int
        let error: String?
        let disabled: UiDisabled?

        init(json: JSON) {
            value = json["value"] as? String ?? ""
            label = json["label"] as? String
            placeholder = json["placeholder"] as? String
            rows = (json["rows"] as? NSNumber)?.intValue ?? 0
            error = json["error"] as? String
            disabled = (json["disabled"] as? String).flatMap(UiDisabled.init(rawValue:))
        }
    }

    struct Select {
        let value: String
        let options: [UiChoice]
        let selected: Int?
        let label: String?
        /// Shown when no option is chosen.
        let placeholder: String
        let error: String?
        let disabled: UiDisabled?

        init(json: JSON) {
            value = json["value"] as? String ?? ""
            options = (json["options"] as? [Any] ?? []).map { UiChoice(json: $0 as? JSON ?? [:]) }
            selected = (json["selected"] as? NSNumber)?.intValue
            label = json["label"] as? String
            placeholder = json["placeholder"] as? String ?? ""
            error = json["error"] as? String
            disabled = (json["disabled"] as? String).flatMap(UiDisabled.init(rawValue:))
        }
    }

    struct SegmentedControl {
        let value: String
        let options: [UiChoice]
        let selected: Int?
        let block: Bool
        let disabled: UiDisabled?

        init(json: JSON) {
            value = json["value"] as? String ?? ""
            options = (json["options"] as? [Any] ?? []).map { UiChoice(json: $0 as? JSON ?? [:]) }
            selected = (json["selected"] as? NSNumber)?.intValue
            block = json["block"] as? Bool ?? false
            disabled = (json["disabled"] as? String).flatMap(UiDisabled.init(rawValue:))
        }
    }

    struct Slider {
        let value: Double
        let min: Double
        let max: Double
        let step: Double
        /// Where the thumb is, from 0 to 1.
        let at: Double
        let label: String?
        let readout: String
        /// The label and the readout show above the track.
        let head: Bool
        let marks: [UiSliderMark]
        let disabled: UiDisabled?

        init(json: JSON) {
            value = (json["value"] as? NSNumber)?.doubleValue ?? 0
            min = (json["min"] as? NSNumber)?.doubleValue ?? 0
            max = (json["max"] as? NSNumber)?.doubleValue ?? 0
            step = (json["step"] as? NSNumber)?.doubleValue ?? 0
            at = (json["at"] as? NSNumber)?.doubleValue ?? 0
            label = json["label"] as? String
            readout = json["readout"] as? String ?? ""
            head = json["head"] as? Bool ?? false
            marks = (json["marks"] as? [Any] ?? []).map { UiSliderMark(json: $0 as? JSON ?? [:]) }
            disabled = (json["disabled"] as? String).flatMap(UiDisabled.init(rawValue:))
        }
    }

    struct Checkbox {
        let checked: Bool
        let label: String
        let strike: Bool
        let error: String?
        let disabled: UiDisabled?

        init(json: JSON) {
            checked = json["checked"] as? Bool ?? false
            label = json["label"] as? String ?? ""
            strike = json["strike"] as? Bool ?? false
            error = json["error"] as? String
            disabled = (json["disabled"] as? String).flatMap(UiDisabled.init(rawValue:))
        }
    }

    struct RadioGroup {
        let value: String
        let options: [UiChoice]
        let selected: Int?
        let label: String?
        let variant: UiRadioVariant
        let error: String?
        let disabled: UiDisabled?

        init(json: JSON) {
            value = json["value"] as? String ?? ""
            options = (json["options"] as? [Any] ?? []).map { UiChoice(json: $0 as? JSON ?? [:]) }
            selected = (json["selected"] as? NSNumber)?.intValue
            label = json["label"] as? String
            variant = UiRadioVariant(rawValue: json["variant"] as? String ?? "") ?? .list
            error = json["error"] as? String
            disabled = (json["disabled"] as? String).flatMap(UiDisabled.init(rawValue:))
        }
    }

    struct DatePicker {
        /// `YYYY-MM-DD`.
        let value: String?
        /// The date as the field shows it, or "Date".
        let text: String
        let min: String?
        let max: String?
        let label: String?
        let error: String?
        let disabled: UiDisabled?

        init(json: JSON) {
            value = json["value"] as? String
            text = json["text"] as? String ?? ""
            min = json["min"] as? String
            max = json["max"] as? String
            label = json["label"] as? String
            error = json["error"] as? String
            disabled = (json["disabled"] as? String).flatMap(UiDisabled.init(rawValue:))
        }
    }

    struct Timer {
        let label: String?
        let time: String
        let seconds: Double
        let running: Bool
        let done: Bool
        let toggle: UiTimerButton
        let reset: UiTimerButton

        init(json: JSON) {
            label = json["label"] as? String
            time = json["time"] as? String ?? ""
            seconds = (json["seconds"] as? NSNumber)?.doubleValue ?? 0
            running = json["running"] as? Bool ?? false
            done = json["done"] as? Bool ?? false
            toggle = UiTimerButton(json: json["toggle"] as? JSON ?? [:])
            reset = UiTimerButton(json: json["reset"] as? JSON ?? [:])
        }
    }

    struct Form {
        let children: [UiNode]
        let spacing: [Double]

        init(json: JSON) {
            children = (json["children"] as? [Any] ?? []).compactMap { UiNode(json: $0 as? JSON ?? [:]) }
            spacing = (json["spacing"] as? [Any] ?? []).map { ($0 as? NSNumber)?.doubleValue ?? 0 }
        }
    }

    struct SearchImage {
        let query: String
        let aspectRatio: Double
        let radius: UiRadius
        let fit: UiFit
        let photos: UiPhotos

        init(json: JSON) {
            query = json["query"] as? String ?? ""
            aspectRatio = (json["aspect_ratio"] as? NSNumber)?.doubleValue ?? 0
            radius = UiRadius(json: json["radius"] as? JSON ?? [:])
            fit = UiFit(rawValue: json["fit"] as? String ?? "") ?? .cover
            photos = UiPhotos(json: json["photos"] as? JSON ?? [:])
        }
    }

    struct Image {
        let src: String?
        let alt: String
        let aspectRatio: Double
        let radius: UiRadius
        let fit: UiFit
        let photos: UiPhotos

        init(json: JSON) {
            src = json["src"] as? String
            alt = json["alt"] as? String ?? ""
            aspectRatio = (json["aspect_ratio"] as? NSNumber)?.doubleValue ?? 0
            radius = UiRadius(json: json["radius"] as? JSON ?? [:])
            fit = UiFit(rawValue: json["fit"] as? String ?? "") ?? .cover
            photos = UiPhotos(json: json["photos"] as? JSON ?? [:])
        }
    }

    struct Icon {
        /// None for a name Lucide doesn't have: nothing is drawn.
        let icon: UiGlyph?
        let size: Double
        let color: UiColor

        init?(json: JSON) {
            icon = (json["icon"] as? JSON).map(UiGlyph.init(json:))
            size = (json["size"] as? NSNumber)?.doubleValue ?? 0
            guard let color = UiColor(json: json["color"]) else { return nil }
            self.color = color
        }
    }

    struct Table {
        let columns: [UiTableColumn]
        let rows: [[UiTableCell]]

        init(json: JSON) {
            columns = (json["columns"] as? [Any] ?? []).map { UiTableColumn(json: $0 as? JSON ?? [:]) }
            rows = (json["rows"] as? [Any] ?? []).map { ($0 as? [Any] ?? []).compactMap { UiTableCell(json: $0 as? JSON ?? [:]) } }
        }
    }

    struct Chart {
        let canvas: UiCanvas

        init(json: JSON) {
            canvas = UiCanvas(json: json["canvas"] as? JSON ?? [:])
        }
    }

    struct Map {
        let pins: [UiMapPin]
        let route: UiRouteKind
        /// `[lat, lng]` along the streets, or straight between the pins shown.
        let line: [[Double]]
        let selected: String?
        let height: Double
        let categories: [String]
        /// The category shown, none for every place.
        let filter: String?
        let note: String

        init(json: JSON) {
            pins = (json["pins"] as? [Any] ?? []).map { UiMapPin(json: $0 as? JSON ?? [:]) }
            route = UiRouteKind(rawValue: json["route"] as? String ?? "") ?? .off
            line = (json["line"] as? [Any] ?? []).map { ($0 as? [Any] ?? []).map { ($0 as? NSNumber)?.doubleValue ?? 0 } }
            selected = json["selected"] as? String
            height = (json["height"] as? NSNumber)?.doubleValue ?? 0
            categories = (json["categories"] as? [Any] ?? []).map { $0 as? String ?? "" }
            filter = json["filter"] as? String
            note = json["note"] as? String ?? ""
        }
    }

    struct MapMarker {
        let label: String
        let lat: Double
        let lng: Double
        let category: String?
        let icon: String?

        init(json: JSON) {
            label = json["label"] as? String ?? ""
            lat = (json["lat"] as? NSNumber)?.doubleValue ?? 0
            lng = (json["lng"] as? NSNumber)?.doubleValue ?? 0
            category = json["category"] as? String
            icon = json["icon"] as? String
        }
    }

    struct YouTube {
        /// The embed's address; none for a video it can't name, which shows nothing.
        let url: String?
        let title: String

        init(json: JSON) {
            url = json["url"] as? String
            title = json["title"] as? String ?? ""
        }
    }

    struct Flashcard {
        let front: UiSlot
        let back: UiSlot
        let hint: String
        let flipped: Bool
        /// The flip.
        let canvas: UiCanvas

        init(json: JSON) {
            front = UiSlot(json: json["front"] as? JSON ?? [:])
            back = UiSlot(json: json["back"] as? JSON ?? [:])
            hint = json["hint"] as? String ?? ""
            flipped = json["flipped"] as? Bool ?? false
            canvas = UiCanvas(json: json["canvas"] as? JSON ?? [:])
        }
    }

    struct Celebration {
        /// Counts the bursts: a new one fires the confetti.
        let burst: Int
        let canvas: UiCanvas

        init(json: JSON) {
            burst = (json["burst"] as? NSNumber)?.intValue ?? 0
            canvas = UiCanvas(json: json["canvas"] as? JSON ?? [:])
        }
    }

    struct Svg {
        let canvas: UiCanvas

        init(json: JSON) {
            canvas = UiCanvas(json: json["canvas"] as? JSON ?? [:])
        }
    }

    struct G {
        let canvas: UiCanvas

        init(json: JSON) {
            canvas = UiCanvas(json: json["canvas"] as? JSON ?? [:])
        }
    }

    struct Circle {
        let canvas: UiCanvas

        init(json: JSON) {
            canvas = UiCanvas(json: json["canvas"] as? JSON ?? [:])
        }
    }

    struct Ellipse {
        let canvas: UiCanvas

        init(json: JSON) {
            canvas = UiCanvas(json: json["canvas"] as? JSON ?? [:])
        }
    }

    struct Rect {
        let canvas: UiCanvas

        init(json: JSON) {
            canvas = UiCanvas(json: json["canvas"] as? JSON ?? [:])
        }
    }

    struct Line {
        let canvas: UiCanvas

        init(json: JSON) {
            canvas = UiCanvas(json: json["canvas"] as? JSON ?? [:])
        }
    }

    struct Path {
        let canvas: UiCanvas

        init(json: JSON) {
            canvas = UiCanvas(json: json["canvas"] as? JSON ?? [:])
        }
    }

    struct Polyline {
        let canvas: UiCanvas

        init(json: JSON) {
            canvas = UiCanvas(json: json["canvas"] as? JSON ?? [:])
        }
    }

    struct Polygon {
        let canvas: UiCanvas

        init(json: JSON) {
            canvas = UiCanvas(json: json["canvas"] as? JSON ?? [:])
        }
    }

    struct SvgText {
        let canvas: UiCanvas

        init(json: JSON) {
            canvas = UiCanvas(json: json["canvas"] as? JSON ?? [:])
        }
    }

    struct HtmlView {
        let html: String
        let height: Double?
        let title: String
        /// The spinner shows in its place.
        let streaming: Bool

        init(json: JSON) {
            html = json["html"] as? String ?? ""
            height = (json["height"] as? NSNumber)?.doubleValue
            title = json["title"] as? String ?? ""
            streaming = json["streaming"] as? Bool ?? false
        }
    }
}

enum UiAlign: String {
    case start
    case center
    case end
    case stretch
}

enum UiJustify: String {
    case start
    case center
    case end
    case between
}

/// A row's child: its width before the row's space is shared.
struct UiSizing {
    let basis: UiBasis
    let length: UiLength?
    let grow: Double
    let shrink: Bool

    init(json: JSON) {
        basis = UiBasis(rawValue: json["basis"] as? String ?? "") ?? .content
        length = (json["length"] as? JSON).map(UiLength.init(json:))
        grow = (json["grow"] as? NSNumber)?.doubleValue ?? 0
        shrink = json["shrink"] as? Bool ?? false
    }
}

enum UiBasis: String {
    /// Its widest single line.
    case content
    case zero
    /// `length`.
    case length
    /// The row's width.
    case row
    /// A media row's image: clamp(120, a third of the row, 200).
    case media
}

struct UiLength {
    let unit: UiLengthUnit
    let value: Double

    init(json: JSON) {
        unit = UiLengthUnit(rawValue: json["unit"] as? String ?? "") ?? .points
        value = (json["value"] as? NSNumber)?.doubleValue ?? 0
    }
}

enum UiLengthUnit: String {
    case points
    /// Of the parent's content box.
    case fraction
    /// Of the transcript column's width.
    case column
    /// Of the window's height.
    case window
}

enum UiDirection: String {
    case row
    case col
}

struct UiRadius {
    let kind: UiRadiusKind
    /// 0 for `full`, half the shorter side.
    let points: Double

    init(json: JSON) {
        kind = UiRadiusKind(rawValue: json["kind"] as? String ?? "") ?? .sm
        points = (json["points"] as? NSNumber)?.doubleValue ?? 0
    }
}

enum UiRadiusKind: String {
    case sm
    case md
    case lg
    case xl
    case full
    case points
}

/// Text or components, where a slot takes either.
struct UiSlot {
    let text: UiRich?
    let style: UiTextStyle
    let children: [UiNode]

    init(json: JSON) {
        text = (json["text"] as? JSON).map(UiRich.init(json:))
        style = UiTextStyle(rawValue: json["style"] as? String ?? "") ?? .body
        children = (json["children"] as? [Any] ?? []).compactMap { UiNode(json: $0 as? JSON ?? [:]) }
    }
}

/// Text with its inline formatting, as a prose row's: `[start, length, style bits]` runs.
struct UiRich {
    let text: String
    let runs: [[Int]]
    let links: [UiLink]

    init(json: JSON) {
        text = json["text"] as? String ?? ""
        runs = (json["runs"] as? [Any] ?? []).map { ($0 as? [Any] ?? []).map { ($0 as? NSNumber)?.intValue ?? 0 } }
        links = (json["links"] as? [Any] ?? []).map { UiLink(json: $0 as? JSON ?? [:]) }
    }
}

struct UiLink {
    let start: Int
    let len: Int
    let url: String

    init(json: JSON) {
        start = (json["start"] as? NSNumber)?.intValue ?? 0
        len = (json["len"] as? NSNumber)?.intValue ?? 0
        url = json["url"] as? String ?? ""
    }
}

enum UiTextStyle: String {
    case body
    case bodyCard = "body_card"
    case large
    case small
    case caption
    case label
    case titleSm = "title_sm"
    case titleMd = "title_md"
    case titleLg = "title_lg"
    case titleXl = "title_xl"
    case title2xl
    case title3xl
    case cardTitle = "card_title"
    case cardSubtitle = "card_subtitle"
    case cardHeading = "card_heading"
    case readout
    case tick
    case timer
    case code
    /// A flashcard's word: `title-lg` on the front.
    case word
    /// The back's word, `body` medium.
    case wordBack = "word_back"
}

enum UiWeight: String {
    case regular
    case medium
    case semibold
}

/// A Lucide icon: its name, and its character in `lucide.ttf`.
struct UiGlyph {
    let name: String
    let glyph: String

    init(json: JSON) {
        name = json["name"] as? String ?? ""
        glyph = json["glyph"] as? String ?? ""
    }
}

enum UiListMarker: String {
    case bullet
    case number
    case plain
}

indirect enum UiItemMarker {
    case blank
    case dot
    case number(Number)
    /// One or two letters or digits, in a pill.
    case chip(Chip)
    /// Any other marker, as text.
    case label(Label)

    init?(json: JSON) {
        switch json["kind"] as? String {
        case "blank":
            self = .blank
        case "dot":
            self = .dot
        case "number":
            self = .number(Number(json: json))
        case "chip":
            self = .chip(Chip(json: json))
        case "label":
            self = .label(Label(json: json))
        default:
            return nil
        }
    }

    struct Number {
        let number: Int

        init(json: JSON) {
            number = (json["number"] as? NSNumber)?.intValue ?? 0
        }
    }

    struct Chip {
        let text: String

        init(json: JSON) {
            text = json["text"] as? String ?? ""
        }
    }

    struct Label {
        let text: String

        init(json: JSON) {
            text = json["text"] as? String ?? ""
        }
    }
}

indirect enum UiMarkdownBlock {
    case prose(Prose)
    case code(Code)

    init?(json: JSON) {
        switch json["kind"] as? String {
        case "prose":
            self = .prose(Prose(json: json))
        case "code":
            self = .code(Code(json: json))
        default:
            return nil
        }
    }

    struct Prose {
        let prose: JSON

        init(json: JSON) {
            prose = json["prose"] as? JSON ?? [:]
        }
    }

    struct Code {
        let language: String
        let code: String
        let spans: [Int]?

        init(json: JSON) {
            language = json["language"] as? String ?? ""
            code = json["code"] as? String ?? ""
            spans = json["spans"] == nil ? nil : (json["spans"] as? [Any] ?? []).map { ($0 as? NSNumber)?.intValue ?? 0 }
        }
    }
}

/// What `motile_ui_layout` lays out, which the client gives its width and text size: the
/// component and its props by name, in order, an Svg's shapes as `{kind, props}` too.
struct UiCanvas {
    let kind: String
    let props: Any?

    init(json: JSON) {
        kind = json["kind"] as? String ?? ""
        props = json["props"]
    }
}

enum UiButtonVariant: String {
    case primary
    case danger
    case success
    case outline
    case secondary
    case ghost
}

enum UiControlSize: String {
    case small
    case regular
    case large
}

/// Why a control takes no presses.
enum UiDisabled: String {
    /// The program says so.
    case program
    /// The program is still streaming.
    case streaming
    /// It runs a tool, which the thread's server can't.
    case noTools = "no_tools"
}

struct UiChoice {
    let value: String
    let label: String
    let description: String?

    init(json: JSON) {
        value = json["value"] as? String ?? ""
        label = json["label"] as? String ?? ""
        description = json["description"] as? String
    }
}

struct UiSliderMark {
    /// From 0 to 1.
    let at: Double
    let label: String
    let edge: UiMarkEdge

    init(json: JSON) {
        at = (json["at"] as? NSNumber)?.doubleValue ?? 0
        label = json["label"] as? String ?? ""
        edge = UiMarkEdge(rawValue: json["edge"] as? String ?? "") ?? .start
    }
}

enum UiMarkEdge: String {
    /// Leading at the track's start.
    case start
    case middle
    /// Trailing at the track's end.
    case end
}

enum UiRadioVariant: String {
    case list
    case pills
    case cards
}

struct UiTimerButton {
    let label: String
    let icon: UiGlyph?
    let disabled: UiDisabled?

    init(json: JSON) {
        label = json["label"] as? String ?? ""
        icon = (json["icon"] as? JSON).map(UiGlyph.init(json:))
        disabled = (json["disabled"] as? String).flatMap(UiDisabled.init(rawValue:))
    }
}

enum UiFit: String {
    case cover
    case contain
}

/// The photos found for an image's query.
struct UiPhotos {
    let state: UiPhotoState
    /// Media refs, best first.
    let media: [String]

    init(json: JSON) {
        state = UiPhotoState(rawValue: json["state"] as? String ?? "") ?? .idle
        media = (json["media"] as? [Any] ?? []).map { $0 as? String ?? "" }
    }
}

enum UiPhotoState: String {
    /// Not looked for: the image has its own address.
    case idle
    case waiting
    case found
    /// Nothing found, or the lookup failed.
    case missing
}

struct UiTableColumn {
    let title: String
    /// Numbers: trailing, digits, one line.
    let numeric: Bool

    init(json: JSON) {
        title = json["title"] as? String ?? ""
        numeric = json["numeric"] as? Bool ?? false
    }
}

indirect enum UiTableCell {
    case text(Text)
    case node(Node)

    init?(json: JSON) {
        switch json["kind"] as? String {
        case "text":
            self = .text(Text(json: json))
        case "node":
            guard let value = Node(json: json) else { return nil }
            self = .node(value)
        default:
            return nil
        }
    }

    struct Text {
        let text: UiRich

        init(json: JSON) {
            text = UiRich(json: json["text"] as? JSON ?? [:])
        }
    }

    struct Node {
        let node: UiNode

        init?(json: JSON) {
            guard let node = UiNode(json: json["node"] as? JSON ?? [:]) else { return nil }
            self.node = node
        }
    }
}

struct UiMapPin {
    /// The label, numbered when another pin has it too: "Café (2)".
    let key: String
    let label: String
    let lat: Double
    let lng: Double
    let category: String?
    /// One emoji, else the pin shows `number`.
    let icon: String?
    let number: Int
    let selected: Bool
    /// In the category shown.
    let visible: Bool

    init(json: JSON) {
        key = json["key"] as? String ?? ""
        label = json["label"] as? String ?? ""
        lat = (json["lat"] as? NSNumber)?.doubleValue ?? 0
        lng = (json["lng"] as? NSNumber)?.doubleValue ?? 0
        category = json["category"] as? String
        icon = json["icon"] as? String
        number = (json["number"] as? NSNumber)?.intValue ?? 0
        selected = json["selected"] as? Bool ?? false
        visible = json["visible"] as? Bool ?? false
    }
}

enum UiRouteKind: String {
    /// No route is drawn.
    case off
    /// Straight while the street route is looked for.
    case loading
    case street
    /// Straight, as no street route was found: "Route overview".
    case overview
}
