//! open-intelligent-ui's 55 components (`src/lib/components`), with the props in their positional
//! order, their types, descriptions and bindable values, and the six groups the prompt lists them
//! in. Clients draw them as `LAYOUT.md` says.

use crate::library::{ComponentDef, Group};
use crate::schema::Schema;

/// In the library's order; `Col` is the root.
pub fn components() -> Vec<ComponentDef> {
    [layout(), text(), inputs(), media_and_data(), play(), drawing()].concat()
}

pub fn groups() -> Vec<Group> {
    vec![
        Group::new("Layout", ["Col", "Row", "Box", "Grid", "Card", "Carousel", "Popover", "Divider", "Spacer"]).with_notes(["- gap and padding count 4px steps. Length is px or a CSS length like \"50%\". Radius is \"sm\", \"md\", \"lg\", \"xl\", \"full\" or px.", "- Color is \"ink\", \"muted\", \"faint\", \"surface\", \"info\", \"success\", \"warning\", \"danger\", \"purple\", \"pink\", \"chart1\" to \"chart6\", or a CSS color. As a background, status colors are soft tints."]),
        Group::new("Text", ["Title", "Text", "Caption", "Label", "Badge", "Link", "List", "ListItem", "Markdown", "Quote", "Code", "Math"]),
        Group::new("Inputs", ["Button", "IconButton", "Pressable", "Input", "TextArea", "Select", "SegmentedControl", "Slider", "Checkbox", "RadioGroup", "DatePicker", "Timer", "Form"]).with_notes(["- An input's first argument is its value, usually a $variable that other statements read.", "- Every input either changes something shown on the page, or sits in a Form whose submit Button sends a short first-person @ToAssistant (\"Help me choose a bike\"); the form values go with it. No dead inputs."]),
        Group::new("Media and data", ["SearchImage", "Image", "Icon", "Table", "Chart", "Map", "MapMarker", "YouTube"]),
        Group::new("Play", ["Flashcard", "Celebration"]),
        Group::new("Drawing", ["Svg", "G", "Circle", "Ellipse", "Rect", "Line", "Path", "Polyline", "Polygon", "SvgText", "HtmlView"]),
    ]
}

/// A slot of components, or a list of them from `@Each`. Printed as `Node[]`.
fn children() -> Schema {
    Schema::array(Schema::Any.named("Node"))
}

/// A colour token or a CSS colour, explained once in the Layout group's notes.
fn color() -> Schema {
    Schema::String.named("Color")
}

/// Points, or a CSS length such as "50%".
fn length() -> Schema {
    Schema::union([Schema::Number, Schema::String]).named("Length")
}

fn radius() -> Schema {
    Schema::union([Schema::enumeration(["sm", "md", "lg", "xl", "full"]), Schema::Number]).named("Radius")
}

/// Text, or a list mixing text and inline components.
fn string_or_nodes() -> Schema {
    Schema::union([Schema::String, children()])
}

fn option() -> Schema {
    Schema::union([Schema::String, Schema::object([Schema::String.field("value"), Schema::String.field("label")])])
}

fn align() -> Schema {
    Schema::enumeration(["start", "center", "end", "stretch"])
}

fn justify() -> Schema {
    Schema::enumeration(["start", "center", "end", "between"])
}

fn layout() -> Vec<ComponentDef> {
    vec![
        ComponentDef::new(
            "Col",
            "Vertical layout. Without gap it spaces blocks like an article.",
            [children().field("children"), Schema::Number.field("gap").optional(), align().field("align").optional()],
        ),
        ComponentDef::new(
            "Row",
            "Horizontal layout; items are vertically centered.",
            [
                children().field("children"),
                Schema::Number.field("gap").optional(),
                align().field("align").optional(),
                justify().field("justify").optional(),
                Schema::Boolean.field("wrap").optional(),
            ],
        ),
        ComponentDef::new(
            "Box",
            "Flex container, a column by default. radius also clips the content.",
            [
                children().field("children").optional(),
                Schema::enumeration(["row", "col"]).field("direction").optional(),
                Schema::Number.field("gap").optional(),
                Schema::Number.field("padding").optional(),
                color().field("background").optional(),
                radius().field("radius").optional(),
                Schema::Boolean.field("border").optional(),
                length().field("width").optional(),
                length().field("height").optional(),
                Schema::Number.field("flex").optional(),
                align().field("align").optional(),
                justify().field("justify").optional(),
                Schema::Boolean.field("wrap").optional(),
            ],
        ),
        ComponentDef::new(
            "Grid",
            "Equal columns (default 2). With minChildWidth (px), columns drop on narrow screens.",
            [
                children().field("children"),
                Schema::Number.field("columns").optional(),
                Schema::Number.field("minChildWidth").optional(),
                Schema::Number.field("gap").optional(),
            ],
        ),
        ComponentDef::new(
            "Card",
            "Panel with a border; soft is a filled panel for summaries.",
            [
                children().field("children"),
                Schema::enumeration(["outline", "soft"]).field("variant").optional(),
                Schema::Number.field("padding").optional(),
                Schema::Number.field("gap").optional(),
            ],
        ),
        ComponentDef::new(
            "Carousel",
            "Horizontal row that scrolls and snaps, each child an item itemWidth px wide (default 240): photos, cards, options, days.",
            [
                children().field("children"),
                Schema::Number.field("itemWidth").optional(),
                Schema::Number.field("gap").optional(),
            ],
        ),
        ComponentDef::new(
            "Popover",
            "Shows content in a small panel when the trigger is clicked: a definition, a source, details behind an Icon(\"info\", \"sm\").",
            [string_or_nodes().field("trigger"), string_or_nodes().field("content")],
        ),
        ComponentDef::new("Divider", "", []),
        ComponentDef::new("Spacer", "Fills the free space in a Row.", []),
    ]
}

fn text() -> Vec<ComponentDef> {
    vec![
        ComponentDef::new(
            "Title",
            "Heading; md for a card title, lg for a section, 2xl for a big number.",
            [
                Schema::String.field("text"),
                Schema::enumeration(["sm", "md", "lg", "xl", "2xl", "3xl"]).field("size").optional(),
                color().field("color").optional(),
            ],
        ),
        ComponentDef::new(
            "Text",
            "Paragraph; supports **bold**, *italic*, `code` and [links](https://…). text may be a list mixing strings and inline components: [Icon(\"clock\", \"sm\"), \" 25 min · \", Badge(\"Easy\")].",
            [
                string_or_nodes().field("text"),
                Schema::enumeration(["xs", "sm", "md", "lg"]).field("size").optional(),
                color().field("color").optional(),
                Schema::enumeration(["normal", "medium", "semibold"]).field("weight").optional(),
                Schema::Boolean.field("italic").optional(),
                Schema::Boolean.field("lineThrough").optional(),
                Schema::Boolean.field("tabularNums").optional(),
                Schema::Number.field("maxLines").optional(),
            ],
        ),
        ComponentDef::new(
            "Caption",
            "Small secondary text.",
            [Schema::String.field("text"), color().field("color").optional()],
        ),
        ComponentDef::new("Label", "Field or group label.", [Schema::String.field("text")]),
        ComponentDef::new(
            "Badge",
            "Pill tag; icon is an optional leading lucide name.",
            [
                Schema::String.field("text"),
                Schema::enumeration(["neutral", "info", "success", "warning", "danger", "purple", "pink"])
                    .field("color")
                    .optional(),
                Schema::String.field("icon").optional(),
            ],
        ),
        ComponentDef::new(
            "Link",
            "Opens an http(s) URL in a new tab.",
            [Schema::String.field("text"), Schema::String.field("url")],
        ),
        ComponentDef::new(
            "List",
            "timeline joins the markers with a line.",
            [
                Schema::array(Schema::union([Schema::String, Schema::component("ListItem")])).field("items"),
                Schema::enumeration(["bullet", "number", "none"]).field("marker").optional(),
                Schema::Boolean.field("timeline").optional(),
            ],
        ),
        ComponentDef::new(
            "ListItem",
            "marker replaces the bullet or number, e.g. a time or an emoji.",
            [
                Schema::String.field("title"),
                Schema::String.field("description").optional(),
                Schema::String.field("marker").optional(),
                children().field("children").optional(),
            ],
        ),
        ComponentDef::new(
            "Markdown",
            "Prose with headings, lists, quotes, code and tables.",
            [Schema::String.field("text")],
        ),
        ComponentDef::new(
            "Quote",
            "A quotation or a callout with a side rule; cite names the source.",
            [Schema::String.field("text"), Schema::String.field("cite").optional()],
        ),
        ComponentDef::new(
            "Code",
            "Code block with a copy button.",
            [Schema::String.field("code"), Schema::String.field("language").optional()],
        ),
        ComponentDef::new(
            "Math",
            "LaTeX formula, e.g. Math(\"E = mc^2\"); inline true to sit in a Text list.",
            [Schema::String.field("tex"), Schema::Boolean.field("inline").optional()],
        ),
    ]
}

fn inputs() -> Vec<ComponentDef> {
    vec![
        ComponentDef::new(
            "Button",
            "icon is a lucide name.",
            [
                Schema::String.field("label"),
                Schema::Action.field("action").optional(),
                Schema::enumeration(["solid", "outline", "soft", "ghost"]).field("variant").optional(),
                Schema::enumeration(["primary", "danger", "success"]).field("color").optional(),
                Schema::enumeration(["sm", "md", "lg"]).field("size").optional(),
                Schema::String.field("icon").optional(),
                Schema::Boolean.field("disabled").optional(),
                Schema::Boolean.field("block").optional(),
            ],
        ),
        ComponentDef::new(
            "IconButton",
            "Round icon-only button; label is read aloud.",
            [
                Schema::String.field("icon"),
                Schema::Action.field("action").optional(),
                Schema::String.field("label").optional(),
                Schema::enumeration(["outline", "ghost", "solid"]).field("variant").optional(),
            ],
        ),
        ComponentDef::new(
            "Pressable",
            "Makes any content clickable, such as a tile or a swatch.",
            [
                children().field("children"),
                Schema::Action.field("action").optional(),
                Schema::Boolean.field("selected").optional(),
                Schema::Number.field("padding").optional(),
                color().field("background").optional(),
                radius().field("radius").optional(),
            ],
        ),
        ComponentDef::new(
            "Input",
            "Text or number field; a number field stores numbers.",
            [
                Schema::union([Schema::String, Schema::Number]).field("value").bindable(),
                Schema::String.field("label").optional(),
                Schema::enumeration(["text", "number"]).field("type").optional(),
                Schema::String.field("placeholder").optional(),
            ],
        ),
        ComponentDef::new(
            "TextArea",
            "",
            [
                Schema::String.field("value").bindable(),
                Schema::String.field("label").optional(),
                Schema::String.field("placeholder").optional(),
                Schema::Number.field("rows").optional(),
            ],
        ),
        ComponentDef::new(
            "Select",
            "Dropdown, for long option lists.",
            [
                Schema::String.field("value").bindable(),
                Schema::array(option()).field("options"),
                Schema::String.field("label").optional(),
                Schema::String.field("placeholder").optional(),
            ],
        ),
        ComponentDef::new(
            "SegmentedControl",
            "2 to 4 short choices in a row.",
            [
                Schema::String.field("value").bindable(),
                Schema::array(option()).field("options"),
                Schema::Boolean.field("block").optional(),
            ],
        ),
        ComponentDef::new(
            "Slider",
            "Shows label and value; unit like \"%\" or \"$\" formats the value.",
            [
                Schema::Number.field("value").bindable(),
                Schema::Number.field("min"),
                Schema::Number.field("max"),
                Schema::Number.field("step").optional(),
                Schema::String.field("label").optional(),
                Schema::String.field("unit").optional(),
                Schema::array(Schema::object([Schema::Number.field("value"), Schema::String.field("label")]))
                    .field("marks")
                    .optional(),
            ],
        ),
        ComponentDef::new(
            "Checkbox",
            "lineThrough strikes the label when checked, for to-do lists.",
            [
                Schema::Boolean.field("checked").bindable(),
                Schema::String.field("label"),
                Schema::Boolean.field("lineThrough").optional(),
            ],
        ),
        ComponentDef::new(
            "RadioGroup",
            "pills for short choices; cards for choices that need a description line.",
            [
                Schema::String.field("value").bindable(),
                Schema::array(Schema::union([
                    Schema::String,
                    Schema::object([
                        Schema::String.field("value"),
                        Schema::String.field("label"),
                        Schema::String.field("description").optional(),
                    ]),
                ]))
                .field("options"),
                Schema::String.field("label").optional(),
                Schema::enumeration(["list", "pills", "cards"]).field("variant").optional(),
            ],
        ),
        ComponentDef::new(
            "DatePicker",
            "Dates are \"YYYY-MM-DD\".",
            [
                Schema::String.field("value").bindable(),
                Schema::String.field("label").optional(),
                Schema::String.field("min").optional(),
                Schema::String.field("max").optional(),
            ],
        ),
        ComponentDef::new(
            "Timer",
            "Countdown with start, pause and reset. It counts the bound seconds down to 0, so other statements can react, e.g. $left == 0.",
            [Schema::Number.field("seconds").bindable(), Schema::String.field("label").optional()],
        ),
        ComponentDef::new(
            "Form",
            "Groups fields; their values go with any @ToAssistant inside.",
            [children().field("children")],
        ),
    ]
}

fn media_and_data() -> Vec<ComponentDef> {
    vec![
        ComponentDef::new(
            "SearchImage",
            "A real photo the app finds for query; it needs no URL and no tool. Name the exact thing in 2 to 4 words: \"Belém Tower Lisbon\", \"iPhone 17\", \"pastel de nata\". Use one for every place, dish, product or thing with a look.",
            [
                Schema::String.field("query"),
                Schema::Number.field("aspectRatio").optional(),
                radius().field("radius").optional(),
                Schema::enumeration(["cover", "contain"]).field("fit").optional(),
            ],
        ),
        ComponentDef::new(
            "Image",
            "Photo from a URL the user or a tool gave; if it fails, a photo is found for alt.",
            [
                Schema::String.field("src"),
                Schema::String.field("alt"),
                Schema::Number.field("aspectRatio").optional(),
                radius().field("radius").optional(),
                Schema::enumeration(["cover", "contain"]).field("fit").optional(),
            ],
        ),
        ComponentDef::new(
            "Icon",
            "Lucide icon by name, e.g. \"map-pin\".",
            [
                Schema::String.field("name"),
                Schema::enumeration(["sm", "md", "lg", "xl"]).field("size").optional(),
                color().field("color").optional(),
            ],
        ),
        ComponentDef::new(
            "Table",
            "Each row lists its cells in column order; a cell may be a component. Number columns align right.",
            [Schema::array(Schema::String).field("columns"), Schema::array(Schema::array(Schema::Any)).field("rows")],
        ),
        ComponentDef::new(
            "Chart",
            "Rows of data; x is the category key (a number key for scatter), series the value keys (default: every number key). percent values are like 45; currency is a code like \"EUR\".",
            [
                Schema::enumeration(["bar", "line", "area", "pie", "donut", "scatter"]).field("type"),
                Schema::array(Schema::record(Schema::union([Schema::String, Schema::Number]))).field("data"),
                Schema::String.field("x").optional(),
                Schema::array(Schema::String).field("series").optional(),
                Schema::enumeration(["number", "currency", "percent"]).field("format").optional(),
                Schema::String.field("currency").optional(),
                Schema::Boolean.field("stacked").optional(),
                Schema::Number.field("height").optional(),
            ],
        ),
        ComponentDef::new(
            "Map",
            "Street map fitted to its markers. route draws the road path in marker order; selected holds the clicked marker's label.",
            [
                Schema::array(Schema::component("MapMarker")).field("markers"),
                Schema::Boolean.field("route").optional(),
                Schema::String.field("selected").optional().bindable(),
                Schema::Number.field("height").optional(),
            ],
        ),
        ComponentDef::new(
            "MapMarker",
            "A place on a Map; icon is one emoji, else the pin shows its number.",
            [
                Schema::String.field("label"),
                Schema::Number.field("lat"),
                Schema::Number.field("lng"),
                Schema::String.field("category").optional(),
                Schema::String.field("icon").optional(),
            ],
        ),
        ComponentDef::new(
            "YouTube",
            "Embedded YouTube video by id or URL. Only a video you know exists.",
            [Schema::String.field("video"), Schema::String.field("title").optional()],
        ),
    ]
}

fn play() -> Vec<ComponentDef> {
    vec![
        ComponentDef::new(
            "Flashcard",
            "Card that flips between front and back when tapped. Put several in a Carousel or Grid.",
            [
                string_or_nodes().field("front"),
                string_or_nodes().field("back"),
                Schema::String.field("hint").optional(),
            ],
        ),
        ComponentDef::new(
            "Celebration",
            "Bursts confetti when its condition turns true, e.g. Celebration($score == 5) at the end of a quiz.",
            [Schema::Boolean.field("when")],
        ),
    ]
}

fn drawing() -> Vec<ComponentDef> {
    vec![
        ComponentDef::new(
            "Svg",
            "A drawing for steps, parts, mechanisms, diagrams and layouts. Scales to its width; its shapes take expressions, so state can move them.",
            [
                children().field("children"),
                Schema::String.field("viewBox"),
                length().field("width").optional(),
                length().field("height").optional(),
            ],
        ),
        ComponentDef::new(
            "Circle",
            "",
            [
                Schema::Number.field("cx"),
                Schema::Number.field("cy"),
                Schema::Number.field("r"),
                color().field("fill").optional(),
                color().field("stroke").optional(),
                Schema::Number.field("strokeWidth").optional(),
                Schema::Number.field("opacity").optional(),
            ],
        ),
        ComponentDef::new(
            "Ellipse",
            "",
            [
                Schema::Number.field("cx"),
                Schema::Number.field("cy"),
                Schema::Number.field("rx"),
                Schema::Number.field("ry"),
                color().field("fill").optional(),
                color().field("stroke").optional(),
                Schema::Number.field("strokeWidth").optional(),
                Schema::Number.field("opacity").optional(),
            ],
        ),
        ComponentDef::new(
            "Rect",
            "",
            [
                Schema::Number.field("x"),
                Schema::Number.field("y"),
                Schema::Number.field("width"),
                Schema::Number.field("height"),
                color().field("fill").optional(),
                color().field("stroke").optional(),
                Schema::Number.field("strokeWidth").optional(),
                Schema::Number.field("opacity").optional(),
                Schema::Number.field("rx").optional(),
            ],
        ),
        ComponentDef::new(
            "Line",
            "",
            [
                Schema::Number.field("x1"),
                Schema::Number.field("y1"),
                Schema::Number.field("x2"),
                Schema::Number.field("y2"),
                color().field("stroke").optional(),
                Schema::Number.field("strokeWidth").optional(),
                Schema::Number.field("opacity").optional(),
            ],
        ),
        ComponentDef::new(
            "Path",
            "",
            [
                Schema::String.field("d"),
                color().field("fill").optional(),
                color().field("stroke").optional(),
                Schema::Number.field("strokeWidth").optional(),
                Schema::Number.field("opacity").optional(),
            ],
        ),
        ComponentDef::new(
            "Polyline",
            "points like \"0,0 10,5 20,0\".",
            [
                Schema::String.field("points"),
                color().field("stroke").optional(),
                Schema::Number.field("strokeWidth").optional(),
                color().field("fill").optional(),
                Schema::Number.field("opacity").optional(),
            ],
        ),
        ComponentDef::new(
            "Polygon",
            "",
            [
                Schema::String.field("points"),
                color().field("fill").optional(),
                color().field("stroke").optional(),
                Schema::Number.field("strokeWidth").optional(),
                Schema::Number.field("opacity").optional(),
            ],
        ),
        ComponentDef::new(
            "G",
            "Group; transform like \"rotate(30 50 50)\" or \"translate(10 0)\".",
            [
                children().field("children"),
                Schema::String.field("transform").optional(),
                Schema::Number.field("opacity").optional(),
            ],
        ),
        ComponentDef::new(
            "SvgText",
            "",
            [
                Schema::String.field("text"),
                Schema::Number.field("x"),
                Schema::Number.field("y"),
                Schema::Number.field("size").optional(),
                color().field("fill").optional(),
                Schema::enumeration(["start", "middle", "end"]).field("anchor").optional(),
                Schema::enumeration(["normal", "medium", "semibold"]).field("weight").optional(),
            ],
        ),
        ComponentDef::new(
            "HtmlView",
            "Sandboxed HTML page with inline JS, for games and canvas. No network except script CDNs. Fits its content unless height is set.",
            [
                Schema::String.field("html"),
                Schema::Number.field("height").optional(),
                Schema::String.field("title").optional(),
            ],
        ),
    ]
}
