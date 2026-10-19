//! open-intelligent-ui's function tests (`src/lib/functions/__tests__`): every row of their tables,
//! and the cases beside them.

use super::functions;
use crate::js;
use crate::library::{Args, FixedEnv};
use crate::value::Value;

/// The rows' `ROWS`, as the language writes them and as JSON.
pub(crate) const ROWS: &str = r#"[{region: "West", product: "A", amount: 10, date: "2026-10-01"}, {region: "East", product: "B", amount: 5, date: "2026-10-02"}, {region: "West", product: "B", amount: 7, date: "2026-10-09"}]"#;
const ROWS_JSON: &str = r#"[{"region": "West", "product": "A", "amount": 10, "date": "2026-10-01"}, {"region": "East", "product": "B", "amount": 5, "date": "2026-10-02"}, {"region": "West", "product": "B", "amount": 7, "date": "2026-10-09"}]"#;

/// The 253 rows: the expression (with `ROWS` for [`ROWS`]), the function it calls with the
/// arguments its literals evaluate to, what the expression does with the result (`.field` plucks,
/// `round n` is the `@Round(…, n)` around the call), and the value expected, as JSON.
pub(crate) const CASES: &[(&str, &str, &str, &str, &str)] = &[
    // lists.test.ts
    ("@Append([1, 2], 3)", "Append", "[[1, 2], 3]", "", "[1, 2, 3]"),
    (r#"@Append([], "a")"#, "Append", r#"[[], "a"]"#, "", r#"["a"]"#),
    ("@Append([1], [2])", "Append", "[[1], [2]]", "", "[1, [2]]"),
    ("@Append(null, 1)", "Append", "[null, 1]", "", "null"),
    (r#"@Append("x", 1)"#, "Append", r#"["x", 1]"#, "", "null"),
    ("@Prepend([2, 3], 1)", "Prepend", "[[2, 3], 1]", "", "[1, 2, 3]"),
    (r#"@Prepend([], "a")"#, "Prepend", r#"[[], "a"]"#, "", r#"["a"]"#),
    ("@Prepend(5, 1)", "Prepend", "[5, 1]", "", "null"),
    ("@Concat([1], [2, 3])", "Concat", "[[1], [2, 3]]", "", "[1, 2, 3]"),
    ("@Concat([], [])", "Concat", "[[], []]", "", "[]"),
    ("@Concat([1], 2)", "Concat", "[[1], 2]", "", "null"),
    (r#"@Remove(["a", "b", "a"], "", "a")"#, "Remove", r#"[["a", "b", "a"], "", "a"]"#, "", r#"["b"]"#),
    (r#"@Remove([{id: 1}, {id: 2}], "id", 2)"#, "Remove", r#"[[{"id": 1}, {"id": 2}], "id", 2]"#, "", r#"[{"id": 1}]"#),
    (r#"@Remove([{id: 1}], "id", "1")"#, "Remove", r#"[[{"id": 1}], "id", "1"]"#, "", "[]"),
    (
        r#"@Remove([{p: {id: 1}}, {p: {id: 2}}], "p.id", 1)"#,
        "Remove",
        r#"[[{"p": {"id": 1}}, {"p": {"id": 2}}], "p.id", 1]"#,
        "",
        r#"[{"p": {"id": 2}}]"#,
    ),
    (r#"@Remove([], "id", 1)"#, "Remove", r#"[[], "id", 1]"#, "", "[]"),
    (r#"@Remove(["a"], "", "z")"#, "Remove", r#"[["a"], "", "z"]"#, "", r#"["a"]"#),
    ("@Remove([1], 5, 1)", "Remove", "[[1], 5, 1]", "", "null"),
    (r#"@RemoveAt(["a", "b", "c"], 1)"#, "RemoveAt", r#"[["a", "b", "c"], 1]"#, "", r#"["a", "c"]"#),
    (r#"@RemoveAt(["a", "b", "c"], -1)"#, "RemoveAt", r#"[["a", "b", "c"], -1]"#, "", r#"["a", "b"]"#),
    (r#"@RemoveAt(["a", "b", "c"], 9)"#, "RemoveAt", r#"[["a", "b", "c"], 9]"#, "", r#"["a", "b", "c"]"#),
    ("@RemoveAt([], 0)", "RemoveAt", "[[], 0]", "", "[]"),
    (r#"@RemoveAt(["a"], "0")"#, "RemoveAt", r#"[["a"], "0"]"#, "", "null"),
    (
        r#"@Update([{id: 1, done: false}, {id: 2, done: false}], "id", 2, "done", true)"#,
        "Update",
        r#"[[{"id": 1, "done": false}, {"id": 2, "done": false}], "id", 2, "done", true]"#,
        "",
        r#"[{"id": 1, "done": false}, {"id": 2, "done": true}]"#,
    ),
    (
        r#"@Update([1, {id: 1}], "id", 1, "x", 5)"#,
        "Update",
        r#"[[1, {"id": 1}], "id", 1, "x", 5]"#,
        "",
        r#"[1, {"id": 1, "x": 5}]"#,
    ),
    (r#"@Update([{id: 1}], "id", 3, "x", 5)"#, "Update", r#"[[{"id": 1}], "id", 3, "x", 5]"#, "", r#"[{"id": 1}]"#),
    (r#"@Update([], "id", 1, "x", 5)"#, "Update", r#"[[], "id", 1, "x", 5]"#, "", "[]"),
    (r#"@Update([{id: 1}], "id", 1, null, 5)"#, "Update", r#"[[{"id": 1}], "id", 1, null, 5]"#, "", "null"),
    (r#"@Toggle(["a"], "b")"#, "Toggle", r#"[["a"], "b"]"#, "", r#"["a", "b"]"#),
    (r#"@Toggle(["a", "b"], "a")"#, "Toggle", r#"[["a", "b"], "a"]"#, "", r#"["b"]"#),
    ("@Toggle([], 1)", "Toggle", "[[], 1]", "", "[1]"),
    (r#"@Toggle([1, 2], "2")"#, "Toggle", r#"[[1, 2], "2"]"#, "", "[1]"),
    ("@Includes([1, 2], 2)", "Includes", "[[1, 2], 2]", "", "true"),
    ("@Includes([1, 2], 3)", "Includes", "[[1, 2], 3]", "", "false"),
    (r#"@Includes("Hello world", "world")"#, "Includes", r#"["Hello world", "world"]"#, "", "true"),
    (r#"@Includes("Hello", "z")"#, "Includes", r#"["Hello", "z"]"#, "", "false"),
    (r#"@Includes([], "a")"#, "Includes", r#"[[], "a"]"#, "", "false"),
    ("@Includes(5, 5)", "Includes", "[5, 5]", "", "null"),
    (
        r#"@Find([{n: "a", v: 1}, {n: "b", v: 2}], "n", "b")"#,
        "Find",
        r#"[[{"n": "a", "v": 1}, {"n": "b", "v": 2}], "n", "b"]"#,
        "",
        r#"{"n": "b", "v": 2}"#,
    ),
    (r#"@Find([{n: "a"}], "n", "z")"#, "Find", r#"[[{"n": "a"}], "n", "z"]"#, "", "null"),
    (r#"@Find([], "n", "z")"#, "Find", r#"[[], "n", "z"]"#, "", "null"),
    (r#"@IndexOf(["a", "b"], "b")"#, "IndexOf", r#"[["a", "b"], "b"]"#, "", "1"),
    (r#"@IndexOf(["a", "b"], "z")"#, "IndexOf", r#"[["a", "b"], "z"]"#, "", "-1"),
    ("@IndexOf([], 1)", "IndexOf", "[[], 1]", "", "-1"),
    (
        "@IndexOf([{a: 1, b: [2]}, {a: 2}], {b: [2], a: 1})",
        "IndexOf",
        r#"[[{"a": 1, "b": [2]}, {"a": 2}], {"b": [2], "a": 1}]"#,
        "",
        "0",
    ),
    ("@Take([1, 2, 3], 2)", "Take", "[[1, 2, 3], 2]", "", "[1, 2]"),
    ("@Take([1, 2, 3], -1)", "Take", "[[1, 2, 3], -1]", "", "[3]"),
    ("@Take([1, 2, 3], 0)", "Take", "[[1, 2, 3], 0]", "", "[]"),
    ("@Take([], 5)", "Take", "[[], 5]", "", "[]"),
    ("@Slice([1, 2, 3, 4], 1, 3)", "Slice", "[[1, 2, 3, 4], 1, 3]", "", "[2, 3]"),
    ("@Slice([1, 2, 3], -2)", "Slice", "[[1, 2, 3], -2]", "", "[2, 3]"),
    ("@Slice([], 0, 2)", "Slice", "[[], 0, 2]", "", "[]"),
    (r#"@Slice("Saturday", 0, 3)"#, "Slice", r#"["Saturday", 0, 3]"#, "", r#""Sat""#),
    (r#"@Slice("report.pdf", -3)"#, "Slice", r#"["report.pdf", -3]"#, "", r#""pdf""#),
    (r#"@Slice([1, 2], "a")"#, "Slice", r#"[[1, 2], "a"]"#, "", "null"),
    (r#"@Unique([1, 1, 2, "a", "a"])"#, "Unique", r#"[[1, 1, 2, "a", "a"]]"#, "", r#"[1, 2, "a"]"#),
    (
        "@Unique([{a: 1}, {a: 1}, {a: 2}])",
        "Unique",
        r#"[[{"a": 1}, {"a": 1}, {"a": 2}]]"#,
        "",
        r#"[{"a": 1}, {"a": 2}]"#,
    ),
    ("@Unique([])", "Unique", "[[]]", "", "[]"),
    ("@Reverse([1, 2, 3])", "Reverse", "[[1, 2, 3]]", "", "[3, 2, 1]"),
    ("@Reverse([])", "Reverse", "[[]]", "", "[]"),
    ("@Flatten([[1, 2], [3], 4])", "Flatten", "[[[1, 2], [3], 4]]", "", "[1, 2, 3, 4]"),
    ("@Flatten([])", "Flatten", "[[]]", "", "[]"),
    ("@Chunk([1, 2, 3, 4, 5], 2)", "Chunk", "[[1, 2, 3, 4, 5], 2]", "", "[[1, 2], [3, 4], [5]]"),
    ("@Chunk([1], 0)", "Chunk", "[[1], 0]", "", "null"),
    ("@Range(0, 3)", "Range", "[0, 3]", "", "[0, 1, 2]"),
    ("@Range(1, 10, 3)", "Range", "[1, 10, 3]", "", "[1, 4, 7]"),
    ("@Range(3, 0, -1)", "Range", "[3, 0, -1]", "", "[3, 2, 1]"),
    ("@Range(0, 0.3, 0.1)", "Range", "[0, 0.3, 0.1]", "", "[0, 0.1, 0.2]"),
    ("@Range(5, 0)", "Range", "[5, 0]", "", "[]"),
    ("@Range(0, 5, 0)", "Range", "[0, 5, 0]", "", "null"),
    ("@Range(0, 100000)", "Range", "[0, 100000]", "", "null"),
    ("@Range(0)", "Range", "[0]", "", "null"),
    // numbers-text.test.ts
    ("@Pow(2, 10)", "Pow", "[2, 10]", "", "1024"),
    ("@Pow(9, 0.5)", "Pow", "[9, 0.5]", "", "3"),
    ("@Pow(2, -1)", "Pow", "[2, -1]", "", "0.5"),
    ("@Pow(0, -1)", "Pow", "[0, -1]", "", "null"),
    ("@Pow(-8, 1 / 3)", "Pow", "[-8, 0.3333333333333333]", "", "null"),
    (r#"@Pow("2", 2)"#, "Pow", r#"["2", 2]"#, "", "null"),
    ("@Sqrt(9)", "Sqrt", "[9]", "", "3"),
    ("@Sqrt(0)", "Sqrt", "[0]", "", "0"),
    ("@Sqrt(-1)", "Sqrt", "[-1]", "", "null"),
    ("@Sqrt(null)", "Sqrt", "[null]", "", "null"),
    ("@Clamp(5, 0, 3)", "Clamp", "[5, 0, 3]", "", "3"),
    ("@Clamp(-1, 0, 3)", "Clamp", "[-1, 0, 3]", "", "0"),
    ("@Clamp(2, 0, 3)", "Clamp", "[2, 0, 3]", "", "2"),
    ("@Clamp(2, 3, 0)", "Clamp", "[2, 3, 0]", "", "2"),
    ("@Clamp(2, 0)", "Clamp", "[2, 0]", "", "null"),
    ("@Percent(3, 4)", "Percent", "[3, 4]", "", "75"),
    ("@Percent(1, 0)", "Percent", "[1, 0]", "", "0"),
    ("@Percent(-1, 4)", "Percent", "[-1, 4]", "", "-25"),
    ("@Percent(1, null)", "Percent", "[1, null]", "", "null"),
    ("@Lerp(0, 10, 0.25)", "Lerp", "[0, 10, 0.25]", "", "2.5"),
    ("@Lerp(10, 0, 1)", "Lerp", "[10, 0, 1]", "", "0"),
    ("@Lerp(0, 10)", "Lerp", "[0, 10]", "", "null"),
    ("@Allocate(100, [1, 1, 1])", "Allocate", "[100, [1, 1, 1]]", "", "[33.34, 33.33, 33.33]"),
    ("@Allocate(10, [1, 2, 1])", "Allocate", "[10, [1, 2, 1]]", "", "[2.5, 5, 2.5]"),
    ("@Allocate(-1, [1, 1, 1])", "Allocate", "[-1, [1, 1, 1]]", "", "[-0.34, -0.33, -0.33]"),
    ("@Allocate(7, [1, 1], 0)", "Allocate", "[7, [1, 1], 0]", "", "[4, 3]"),
    ("@Allocate(5, [0, 0])", "Allocate", "[5, [0, 0]]", "", "[0, 0]"),
    ("@Allocate(5, [1, -1])", "Allocate", "[5, [1, -1]]", "", "null"),
    (r##"@MixColor("#ffffff", "#000000", 0.5)"##, "MixColor", r##"["#ffffff", "#000000", 0.5]"##, "", r##""#808080""##),
    (r##"@MixColor("#fff", "#0285ff", 1)"##, "MixColor", r##"["#fff", "#0285ff", 1]"##, "", r##""#0285ff""##),
    (r##"@MixColor("#ff0000", "#0000ff", 2)"##, "MixColor", r##"["#ff0000", "#0000ff", 2]"##, "", r##""#0000ff""##),
    (r##"@MixColor("red", "#000000", 0.5)"##, "MixColor", r##"["red", "#000000", 0.5]"##, "", "null"),
    ("@Format(12345.678)", "Format", "[12345.678]", "", r#""12,345.68""#),
    (r#"@Format(12345.6, "number")"#, "Format", r#"[12345.6, "number"]"#, "", r#""12,345.6""#),
    (r#"@Format(2.5, "number", 0)"#, "Format", r#"[2.5, "number", 0]"#, "", r#""3""#),
    (r#"@Format(1234.5, "currency")"#, "Format", r#"[1234.5, "currency"]"#, "", r#""$1,234.50""#),
    (r#"@Format(1234.5, "currency", 0, "EUR")"#, "Format", r#"[1234.5, "currency", 0, "EUR"]"#, "", r#""€1,235""#),
    (r#"@Format(1, "currency", 2, "nope")"#, "Format", r#"[1, "currency", 2, "nope"]"#, "", r#""$1.00""#),
    (r#"@Format(45, "percent")"#, "Format", r#"[45, "percent"]"#, "", r#""45%""#),
    (r#"@Format(12.345, "percent", 1)"#, "Format", r#"[12.345, "percent", 1]"#, "", r#""12.3%""#),
    (r#"@Format(600, "number", 1)"#, "Format", r#"[600, "number", 1]"#, "", r#""600""#),
    (r#"@Format(12, "currency", 2)"#, "Format", r#"[12, "currency", 2]"#, "", r#""$12.00""#),
    (r#"@Format(1234567, "compact")"#, "Format", r#"[1234567, "compact"]"#, "", r#""1.2M""#),
    (r#"@Format(1234567, "compact", null, "USD")"#, "Format", r#"[1234567, "compact", null, "USD"]"#, "", r#""$1.2M""#),
    (r#"@Format(999, "compact")"#, "Format", r#"[999, "compact"]"#, "", r#""999""#),
    (r#"@Format(-5, "currency")"#, "Format", r#"[-5, "currency"]"#, "", r#""-$5.00""#),
    (r#"@Format("12", "number")"#, "Format", r#"["12", "number"]"#, "", "null"),
    (r#"@Format(1234.5, "money")"#, "Format", r#"[1234.5, "money"]"#, "", r#""1,234.5""#),
    (r#"@Join(["a", "b"])"#, "Join", r#"[["a", "b"]]"#, "", r#""a, b""#),
    (r#"@Join(["a", "b"], " / ")"#, "Join", r#"[["a", "b"], " / "]"#, "", r#""a / b""#),
    (r#"@Join([{n: "x"}, {n: "y"}], "\n", "n")"#, "Join", r#"[[{"n": "x"}, {"n": "y"}], "\n", "n"]"#, "", r#""x\ny""#),
    (r#"@Join(["a", null, "b"], "-")"#, "Join", r#"[["a", null, "b"], "-"]"#, "", r#""a-b""#),
    ("@Join([1, 2.5])", "Join", "[[1, 2.5]]", "", r#""1, 2.5""#),
    ("@Join([])", "Join", "[[]]", "", r#""""#),
    (r#"@Join("ab")"#, "Join", r#"["ab"]"#, "", "null"),
    (r#"@Plural(1, "guest")"#, "Plural", r#"[1, "guest"]"#, "", r#""1 guest""#),
    (r#"@Plural(3, "guest")"#, "Plural", r#"[3, "guest"]"#, "", r#""3 guests""#),
    (r#"@Plural(0, "item")"#, "Plural", r#"[0, "item"]"#, "", r#""0 items""#),
    (r#"@Plural(2, "person", "people")"#, "Plural", r#"[2, "person", "people"]"#, "", r#""2 people""#),
    (r#"@Plural(1500, "step")"#, "Plural", r#"[1500, "step"]"#, "", r#""1,500 steps""#),
    (r#"@Plural("2", "cat")"#, "Plural", r#"["2", "cat"]"#, "", "null"),
    ("@Pad(7, 2)", "Pad", "[7, 2]", "", r#""07""#),
    (r#"@Pad("5", 3, " ")"#, "Pad", r#"["5", 3, " "]"#, "", r#""  5""#),
    ("@Pad(123, 2)", "Pad", "[123, 2]", "", r#""123""#),
    ("@Pad(7)", "Pad", "[7]", "", "null"),
    ("@Round(@Payment(300000, 0.5, 360), 2)", "Payment", "[300000, 0.5, 360]", "round 2", "1798.65"),
    ("@Payment(1200, 0, 12)", "Payment", "[1200, 0, 12]", "", "100"),
    ("@Payment(1000, 1, 0)", "Payment", "[1000, 1, 0]", "", "null"),
    (r#"@Round(@Convert(10, "km", "mi"), 3)"#, "Convert", r#"[10, "km", "mi"]"#, "round 3", "6.214"),
    (r#"@Convert(100, "c", "f")"#, "Convert", r#"[100, "c", "f"]"#, "", "212"),
    (r#"@Round(@Convert(2, "CUP", "ml"))"#, "Convert", r#"[2, "CUP", "ml"]"#, "round 0", "473"),
    (r#"@Convert(1, "kg", "m")"#, "Convert", r#"[1, "kg", "m"]"#, "", "null"),
    (r#"@Convert(1, "parsec", "m")"#, "Convert", r#"[1, "parsec", "m"]"#, "", "null"),
    (r#"@Trim("  hi there ")"#, "Trim", r#"["  hi there "]"#, "", r#""hi there""#),
    ("@Trim(3)", "Trim", "[3]", "", "null"),
    (r#"@Capitalize("new york-city trip")"#, "Capitalize", r#"["new york-city trip"]"#, "", r#""New York-City Trip""#),
    (r#"@Capitalize("élan vital")"#, "Capitalize", r#"["élan vital"]"#, "", r#""Élan Vital""#),
    (r#"@Split("a, b,,c")"#, "Split", r#"["a, b,,c"]"#, "", r#"["a", "b", "c"]"#),
    ("@Split(\"eggs\nmilk\", \"\n\")", "Split", r#"["eggs\nmilk", "\n"]"#, "", r#"["eggs", "milk"]"#),
    (r#"@Split("")"#, "Split", r#"[""]"#, "", "[]"),
    (r#"@Replace("a-b-c", "-", " / ")"#, "Replace", r#"["a-b-c", "-", " / "]"#, "", r#""a / b / c""#),
    (r#"@Replace("abc", "", "x")"#, "Replace", r#"["abc", "", "x"]"#, "", r#""abc""#),
    ("@Duration(4800)", "Duration", "[4800]", "", r#""1h 20m""#),
    ("@Duration(245)", "Duration", "[245]", "", r#""4m 05s""#),
    ("@Duration(45)", "Duration", "[45]", "", r#""45s""#),
    (r#"@Duration(4805, "clock")"#, "Duration", r#"[4805, "clock"]"#, "", r#""1:20:05""#),
    (r#"@Duration(65, "clock")"#, "Duration", r#"[65, "clock"]"#, "", r#""1:05""#),
    ("@Duration(-60)", "Duration", "[-60]", "", r#""-1m""#),
    (r#"@Default("", 0)"#, "Default", r#"["", 0]"#, "", "0"),
    (r#"@Default(null, "n/a")"#, "Default", r#"[null, "n/a"]"#, "", r#""n/a""#),
    ("@Default(0, 5)", "Default", "[0, 5]", "", "0"),
    ("@Default(false, true)", "Default", "[false, true]", "", "false"),
    (r#"@Upper("abc")"#, "Upper", r#"["abc"]"#, "", r#""ABC""#),
    (r#"@Upper("")"#, "Upper", r#"[""]"#, "", r#""""#),
    ("@Upper(5)", "Upper", "[5]", "", "null"),
    (r#"@Lower("AbC")"#, "Lower", r#"["AbC"]"#, "", r#""abc""#),
    ("@Lower(null)", "Lower", "[null]", "", "null"),
    // dates-data.test.ts
    (r#"@FormatDate("2026-10-09")"#, "FormatDate", r#"["2026-10-09"]"#, "", r#""Oct 9, 2026""#),
    (r#"@FormatDate("2026-10-09", "short")"#, "FormatDate", r#"["2026-10-09", "short"]"#, "", r#""Oct 9""#),
    (
        r#"@FormatDate("2026-10-09", "long")"#,
        "FormatDate",
        r#"["2026-10-09", "long"]"#,
        "",
        r#""Friday, October 9, 2026""#,
    ),
    (r#"@FormatDate("2026-10-09", "weekday")"#, "FormatDate", r#"["2026-10-09", "weekday"]"#, "", r#""Fri""#),
    (r#"@FormatDate("2026-10-09", "month")"#, "FormatDate", r#"["2026-10-09", "month"]"#, "", r#""Oct 2026""#),
    (r#"@FormatDate("2026-10-09", "iso")"#, "FormatDate", r#"["2026-10-09", "iso"]"#, "", r#""2026-10-09""#),
    (
        r#"@FormatDate("2026-10-09T21:30:00Z", "time")"#,
        "FormatDate",
        r#"["2026-10-09T21:30:00Z", "time"]"#,
        "",
        r#""9:30 PM""#,
    ),
    (
        r#"@FormatDate("2026-10-09T21:30:00Z", "datetime")"#,
        "FormatDate",
        r#"["2026-10-09T21:30:00Z", "datetime"]"#,
        "",
        r#""Oct 9, 9:30 PM""#,
    ),
    (r#"@FormatDate(0, "iso")"#, "FormatDate", r#"[0, "iso"]"#, "", r#""1970-01-01""#),
    (r#"@FormatDate("2026-02-30")"#, "FormatDate", r#"["2026-02-30"]"#, "", "null"),
    (r#"@FormatDate("soon")"#, "FormatDate", r#"["soon"]"#, "", "null"),
    (r#"@AddDays("2026-10-09", 3)"#, "AddDays", r#"["2026-10-09", 3]"#, "", r#""2026-10-12""#),
    (r#"@AddDays("2026-12-30", 5)"#, "AddDays", r#"["2026-12-30", 5]"#, "", r#""2027-01-04""#),
    (r#"@AddDays("2026-03-01", -1)"#, "AddDays", r#"["2026-03-01", -1]"#, "", r#""2026-02-28""#),
    (r#"@AddDays("2026-10-09T23:00:00Z", 1)"#, "AddDays", r#"["2026-10-09T23:00:00Z", 1]"#, "", r#""2026-10-10""#),
    (r#"@AddDays("bad", 1)"#, "AddDays", r#"["bad", 1]"#, "", "null"),
    (r#"@DaysBetween("2026-10-09", "2026-12-25")"#, "DaysBetween", r#"["2026-10-09", "2026-12-25"]"#, "", "77"),
    (r#"@DaysBetween("2026-12-25", "2026-10-09")"#, "DaysBetween", r#"["2026-12-25", "2026-10-09"]"#, "", "-77"),
    (r#"@DaysBetween("2026-10-09", "2026-10-09")"#, "DaysBetween", r#"["2026-10-09", "2026-10-09"]"#, "", "0"),
    (r#"@DaysBetween("2026-10-09", null)"#, "DaysBetween", r#"["2026-10-09", null]"#, "", "null"),
    (r#"@AddMonths("2026-01-31", 1)"#, "AddMonths", r#"["2026-01-31", 1]"#, "", r#""2026-02-28""#),
    (r#"@AddMonths("2026-10-09", -12)"#, "AddMonths", r#"["2026-10-09", -12]"#, "", r#""2025-10-09""#),
    (r#"@AddMonths("2026-11-15", 3)"#, "AddMonths", r#"["2026-11-15", 3]"#, "", r#""2027-02-15""#),
    (r#"@MonthsBetween("1990-05-20", "2026-05-19")"#, "MonthsBetween", r#"["1990-05-20", "2026-05-19"]"#, "", "431"),
    (r#"@MonthsBetween("1990-05-20", "2026-05-20")"#, "MonthsBetween", r#"["1990-05-20", "2026-05-20"]"#, "", "432"),
    (r#"@MonthsBetween("2026-03-15", "2026-01-20")"#, "MonthsBetween", r#"["2026-03-15", "2026-01-20"]"#, "", "-1"),
    (r#"@StartOf("2026-10-09", "week")"#, "StartOf", r#"["2026-10-09", "week"]"#, "", r#""2026-10-05""#),
    (r#"@StartOf("2026-10-05", "week")"#, "StartOf", r#"["2026-10-05", "week"]"#, "", r#""2026-10-05""#),
    (r#"@StartOf("2026-10-11", "week")"#, "StartOf", r#"["2026-10-11", "week"]"#, "", r#""2026-10-05""#),
    (r#"@StartOf("2026-10-09", "month")"#, "StartOf", r#"["2026-10-09", "month"]"#, "", r#""2026-10-01""#),
    (r#"@StartOf("2026-10-09", "year")"#, "StartOf", r#"["2026-10-09", "year"]"#, "", r#""2026-01-01""#),
    (
        r#"@GroupBy(ROWS, "region", "amount")"#,
        "GroupBy",
        r#"[ROWS, "region", "amount"]"#,
        "",
        r#"[{"key": "West", "value": 17}, {"key": "East", "value": 5}]"#,
    ),
    (
        r#"@GroupBy(ROWS, "region")"#,
        "GroupBy",
        r#"[ROWS, "region"]"#,
        "",
        r#"[{"key": "West", "value": 2}, {"key": "East", "value": 1}]"#,
    ),
    (
        r#"@GroupBy(ROWS, "region", "amount", "avg")"#,
        "GroupBy",
        r#"[ROWS, "region", "amount", "avg"]"#,
        "",
        r#"[{"key": "West", "value": 8.5}, {"key": "East", "value": 5}]"#,
    ),
    (
        r#"@GroupBy(ROWS, "region", "amount", "max")"#,
        "GroupBy",
        r#"[ROWS, "region", "amount", "max"]"#,
        "",
        r#"[{"key": "West", "value": 10}, {"key": "East", "value": 5}]"#,
    ),
    (
        r#"@GroupBy([{a: 1}, {b: 2}], "a")"#,
        "GroupBy",
        r#"[[{"a": 1}, {"b": 2}], "a"]"#,
        "",
        r#"[{"key": 1, "value": 1}, {"key": "Unknown", "value": 1}]"#,
    ),
    (r#"@GroupBy([], "region")"#, "GroupBy", r#"[[], "region"]"#, "", "[]"),
    (
        r#"@Pivot(ROWS, "region", "product", "amount")"#,
        "Pivot",
        r#"[ROWS, "region", "product", "amount"]"#,
        "",
        r#"[{"key": "West", "A": 10, "B": 7}, {"key": "East", "A": 0, "B": 5}]"#,
    ),
    (
        r#"@Pivot(ROWS, "product", "region")"#,
        "Pivot",
        r#"[ROWS, "product", "region"]"#,
        "",
        r#"[{"key": "A", "West": 1, "East": 0}, {"key": "B", "West": 1, "East": 1}]"#,
    ),
    (r#"@Pivot([], "a", "b")"#, "Pivot", r#"[[], "a", "b"]"#, "", "[]"),
    (r#"@Top(ROWS, "amount", 2).amount"#, "Top", r#"[ROWS, "amount", 2]"#, ".amount", "[10, 7]"),
    (r#"@Top(ROWS, "amount", 1, "asc").amount"#, "Top", r#"[ROWS, "amount", 1, "asc"]"#, ".amount", "[5]"),
    (
        r#"@Top([{v: "x"}, {v: 2}, {v: 9}], "v", 3).v"#,
        "Top",
        r#"[[{"v": "x"}, {"v": 2}, {"v": 9}], "v", 3]"#,
        ".v",
        r#"[9, 2, "x"]"#,
    ),
    (r#"@Top(ROWS, "amount", 0)"#, "Top", r#"[ROWS, "amount", 0]"#, "", "[]"),
    (r#"@Top([], "amount", 3)"#, "Top", r#"[[], "amount", 3]"#, "", "[]"),
    (
        r#"@Between(ROWS, "date", "2026-10-02", "2026-10-09").amount"#,
        "Between",
        r#"[ROWS, "date", "2026-10-02", "2026-10-09"]"#,
        ".amount",
        "[5, 7]",
    ),
    (
        r#"@Between(ROWS, "date", "2026-10-02").amount"#,
        "Between",
        r#"[ROWS, "date", "2026-10-02"]"#,
        ".amount",
        "[5, 7]",
    ),
    (
        r#"@Between(ROWS, "date", null, "2026-10-01").amount"#,
        "Between",
        r#"[ROWS, "date", null, "2026-10-01"]"#,
        ".amount",
        "[10]",
    ),
    (r#"@Between(ROWS, "amount", 6, 10).amount"#, "Between", r#"[ROWS, "amount", 6, 10]"#, ".amount", "[10, 7]"),
    (
        r#"@Between([{v: 9}, {v: 10}, {w: 1}], "v", 9, 9.5)"#,
        "Between",
        r#"[[{"v": 9}, {"v": 10}, {"w": 1}], "v", 9, 9.5]"#,
        "",
        r#"[{"v": 9}]"#,
    ),
    (r#"@Between([], "date", "2026-01-01")"#, "Between", r#"[[], "date", "2026-01-01"]"#, "", "[]"),
    ("@Between(ROWS, 3, 1)", "Between", "[ROWS, 3, 1]", "", "null"),
    (
        r#"@Bucket(ROWS, "date", "week", "amount")"#,
        "Bucket",
        r#"[ROWS, "date", "week", "amount"]"#,
        "",
        r#"[{"key": "2026-09-28", "value": 15}, {"key": "2026-10-05", "value": 7}]"#,
    ),
    (
        r#"@Bucket(ROWS, "date", "month", "amount")"#,
        "Bucket",
        r#"[ROWS, "date", "month", "amount"]"#,
        "",
        r#"[{"key": "2026-10", "value": 22}]"#,
    ),
    (
        r#"@Bucket(ROWS, "date", "day").value"#,
        "Bucket",
        r#"[ROWS, "date", "day"]"#,
        ".value",
        "[1, 1, 0, 0, 0, 0, 0, 0, 1]",
    ),
    (
        r#"@Bucket(ROWS, "date", "day", "amount", "avg").key"#,
        "Bucket",
        r#"[ROWS, "date", "day", "amount", "avg"]"#,
        ".key",
        r#"["2026-10-01", "2026-10-02", "2026-10-09"]"#,
    ),
    (
        r#"@Bucket([{d: "nope"}, {d: "2026-01-31"}, {d: "2026-03-02"}], "d", "month").key"#,
        "Bucket",
        r#"[[{"d": "nope"}, {"d": "2026-01-31"}, {"d": "2026-03-02"}], "d", "month"]"#,
        ".key",
        r#"["2026-01", "2026-02", "2026-03"]"#,
    ),
    (r#"@Bucket([], "date", "day")"#, "Bucket", r#"[[], "date", "day"]"#, "", "[]"),
    (r#"@Bucket(ROWS, "date", "year")"#, "Bucket", r#"[ROWS, "date", "year"]"#, "", "null"),
    (r#"@Cumulative([1, 2, 3], "")"#, "Cumulative", r#"[[1, 2, 3], ""]"#, "", "[1, 3, 6]"),
    (
        r#"@Cumulative([{d: 1, v: 2}, {d: 2, v: 3}], "v")"#,
        "Cumulative",
        r#"[[{"d": 1, "v": 2}, {"d": 2, "v": 3}], "v"]"#,
        "",
        r#"[{"d": 1, "v": 2}, {"d": 2, "v": 5}]"#,
    ),
    (r#"@Cumulative([{v: "x"}, {v: 4}], "v").v"#, "Cumulative", r#"[[{"v": "x"}, {"v": 4}], "v"]"#, ".v", "[0, 4]"),
    (r#"@Cumulative([], "v")"#, "Cumulative", r#"[[], "v"]"#, "", "[]"),
    (r#"@MovingAvg([2, 4, 6, 8], "", 2)"#, "MovingAvg", r#"[[2, 4, 6, 8], "", 2]"#, "", "[2, 3, 5, 7]"),
    (
        r#"@MovingAvg([{v: 3}, {v: 6}, {v: 9}], "v", 3).v"#,
        "MovingAvg",
        r#"[[{"v": 3}, {"v": 6}, {"v": 9}], "v", 3]"#,
        ".v",
        "[3, 4.5, 6]",
    ),
    (r#"@MovingAvg([1, 2], "", 0)"#, "MovingAvg", r#"[[1, 2], "", 0]"#, "", "null"),
    (r#"@MovingAvg([], "", 7)"#, "MovingAvg", r#"[[], "", 7]"#, "", "[]"),
    ("@Change(120, 100)", "Change", "[120, 100]", "", "20"),
    ("@Change(80, 100)", "Change", "[80, 100]", "", "-20"),
    ("@Change(-50, -100)", "Change", "[-50, -100]", "", "50"),
    ("@Change(5, 0)", "Change", "[5, 0]", "", "null"),
    (
        "@Histogram([0.1, 0.2, 0.3, 0.9], 2, 0, 1)",
        "Histogram",
        "[[0.1, 0.2, 0.3, 0.9], 2, 0, 1]",
        "",
        r#"[{"key": "0", "value": 3, "start": 0, "end": 0.5}, {"key": "0.5", "value": 1, "start": 0.5, "end": 1}]"#,
    ),
    ("@Histogram([0, 1, 2], 2, 0, 1).value", "Histogram", "[[0, 1, 2], 2, 0, 1]", ".value", "[1, 1]"),
    (
        "@Histogram([0.1, 0.6], 4, 0, 1).key",
        "Histogram",
        "[[0.1, 0.6], 4, 0, 1]",
        ".key",
        r#"["0", "0.25", "0.5", "0.75"]"#,
    ),
    ("@Histogram([1200, 1350, 1990], 2).key", "Histogram", "[[1200, 1350, 1990], 2]", ".key", r#"["1200", "1595"]"#),
    ("@Histogram([], 3).value", "Histogram", "[[], 3]", ".value", "[0, 0, 0]"),
    ("@Histogram([5, 5], 1).value", "Histogram", "[[5, 5], 1]", ".value", "[2]"),
    ("@Histogram([1, 2], 0)", "Histogram", "[[1, 2], 0]", "", "null"),
    ("@Histogram([{v: 0.2}, {w: 1}].v, 2, 0, 1).value", "Histogram", "[[0.2, null], 2, 0, 1]", ".value", "[1, 0]"),
    (
        "@Stats([2, 4, 4, 4, 5, 5, 7, 9])",
        "Stats",
        "[[2, 4, 4, 4, 5, 5, 7, 9]]",
        "",
        r#"{"count": 8, "min": 2, "max": 9, "mean": 5, "median": 4.5, "stdev": 2}"#,
    ),
    ("@Stats([3, 1, 2]).median", "Stats", "[[3, 1, 2]]", ".median", "2"),
    ("@Stats([])", "Stats", "[[]]", "", r#"{"count": 0, "min": 0, "max": 0, "mean": 0, "median": 0, "stdev": 0}"#),
    (r#"@Stats("1, 2")"#, "Stats", r#"["1, 2"]"#, "", "null"),
    ("@Stats([{v: 1}, {w: 2}, {v: 3}].v).mean", "Stats", "[[1, null, 3]]", ".mean", "2"),
];

/// `@Name(args)` as the evaluator calls it: null when the function fails.
fn call(name: &str, args: &[Value], env: &FixedEnv) -> Value {
    let function =
        functions().into_iter().find(|function| function.name == name).unwrap_or_else(|| panic!("no @{name}"));
    let run = function.run.expect("a function with a body");
    run(&Args::new(&function.params, args, env)).unwrap_or_default()
}

fn json(text: &str) -> Value {
    Value::from_json_str(&text.replace("ROWS", ROWS_JSON)).unwrap_or_else(|error| panic!("{text}: {error}"))
}

fn args(text: &str) -> Vec<Value> {
    json(text).as_array().expect("a list of arguments").to_vec()
}

fn then(value: Value, step: &str) -> Value {
    if let Some(field) = step.strip_prefix('.') {
        return js::member(&value, field);
    }
    let Some(decimals) = step.strip_prefix("round ") else { return value };
    let Some(number) = value.as_number() else { return Value::Null };
    let scale = 10f64.powf(decimals.parse().expect("a number of decimals"));
    Value::Number(js::round(number * scale) / scale)
}

fn utc() -> FixedEnv {
    FixedEnv::default()
}

#[test]
fn passes_every_table_row() {
    assert_eq!(CASES.len(), 253);
    let mut failures = Vec::new();
    for (expression, name, arguments, step, expected) in CASES {
        let value = then(call(name, &args(arguments), &utc()), step);
        if value != json(expected) {
            failures.push(format!("{expression}: got {}, expected {expected}", value.to_json_string()));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn has_the_functions_of_every_row() {
    let names: Vec<String> = functions().into_iter().map(|function| function.name).collect();
    assert_eq!(names.len(), 59);
    for (expression, name, ..) in CASES {
        assert!(expression.starts_with('@') && names.iter().any(|known| known == name), "{expression}");
        assert!(!expression.replace("ROWS", ROWS).contains("ROWS"));
    }
}

fn random(draw: f64) -> FixedEnv {
    FixedEnv { random: draw, ..FixedEnv::default() }
}

#[test]
fn defaults_empty_inputs() {
    assert_eq!(call("Default", &args("[null, 0]"), &utc()), Value::Number(0.0));
    assert_eq!(call("Default", &args(r#"["", 0]"#), &utc()), Value::Number(0.0));
    assert_eq!(call("Default", &args("[4, 0]"), &utc()), Value::Number(4.0));
    assert_eq!(call("Default", &args("[5, null]"), &utc()), Value::Null);
}

#[test]
fn draws_random_numbers_from_the_env() {
    assert_eq!(call("Random", &args("[0, 10]"), &random(0.5)), Value::Number(5.0));
    assert_eq!(call("Random", &args("[10, 0]"), &random(0.5)), Value::Number(5.0));
    assert_eq!(call("Random", &args("[1, 6, true]"), &random(0.999)), Value::Number(6.0));
    assert_eq!(call("Random", &args("[1, 6, true]"), &random(0.0)), Value::Number(1.0));
    assert_eq!(call("Random", &args("[0, 2, true]"), &random(0.4)), Value::Number(1.0));
    assert_eq!(call("Random", &args("[0.2, 0.8, true]"), &random(0.5)), Value::Null);
    assert_eq!(call("Random", &args("[1]"), &random(0.5)), Value::Null);
}

#[test]
fn picks_and_shuffles_with_the_env() {
    assert_eq!(call("Pick", &args(r#"[["a", "b", "c"]]"#), &random(0.5)), Value::from("b"));
    assert_eq!(call("Pick", &args("[[]]"), &random(0.5)), Value::Null);
    assert_eq!(call("Pick", &args(r#"["abc"]"#), &random(0.5)), Value::Null);
    assert_eq!(call("Shuffle", &args("[[1, 2, 3, 4]]"), &random(0.0)), json("[2, 3, 4, 1]"));
    let shuffled = call("Shuffle", &args("[[1, 2, 3, 4, 5]]"), &random(0.7));
    let mut numbers: Vec<f64> = shuffled.as_array().unwrap().iter().filter_map(Value::as_number).collect();
    numbers.sort_by(f64::total_cmp);
    assert_eq!(numbers, [1.0, 2.0, 3.0, 4.0, 5.0]);
    assert_eq!(call("Shuffle", &args("[[]]"), &random(0.7)), json("[]"));
}

#[test]
fn fails_on_bad_arguments() {
    assert_eq!(call("Take", &args(r#"["abc", 1]"#), &utc()), Value::Null);
    assert_eq!(call("Append", &args(r#"["not a list", 1]"#), &utc()), Value::Null);
}

#[test]
fn reads_the_clock_and_the_time_zone() {
    let noon = FixedEnv { now: 20_735.0 * 86_400_000.0 + 43_200_000.0, ..FixedEnv::default() };
    let now = call("Now", &[], &noon);
    assert_eq!(now, Value::from("2026-10-09T12:00:00.000Z"));
    let tomorrow = call("AddDays", &[now, Value::Number(1.0)], &noon);
    assert_eq!(call("FormatDate", &[tomorrow, Value::from("short")], &noon), Value::from("Oct 10"));

    let late = FixedEnv { now: 20_735.0 * 86_400_000.0 + 84_600_000.0, ..FixedEnv::default() };
    assert_eq!(call("Today", &[], &late), Value::from("2026-10-09"));
    let new_york =
        FixedEnv { now: 20_736.0 * 86_400_000.0 + 12_600_000.0, utc_offset_minutes: -240.0, ..FixedEnv::default() };
    assert_eq!(call("Today", &[], &new_york), Value::from("2026-10-09"));
    let evening = args(r#"["2026-10-10T01:30:00Z", "datetime"]"#);
    assert_eq!(call("FormatDate", &evening, &new_york), Value::from("Oct 9, 9:30 PM"));
    assert_eq!(call("FormatDate", &args(r#"["2026-10-10"]"#), &new_york), Value::from("Oct 10, 2026"));
}

#[test]
fn drops_an_invalid_optional_argument() {
    assert_eq!(call("FormatDate", &args(r#"["2026-10-09", "fancy"]"#), &utc()), Value::from("Oct 9, 2026"));
    let rows = args(r#"[ROWS, "region", "amount", "median"]"#);
    assert_eq!(call("GroupBy", &rows, &utc()), json(r#"[{"key": "West", "value": 17}, {"key": "East", "value": 5}]"#));
}

#[test]
fn summarizes_without_a_bad_item() {
    assert_eq!(
        call("Stats", &args(r#"[[1, "two", 3]]"#), &utc()),
        json(r#"{"count": 2, "min": 1, "max": 3, "mean": 2, "median": 2, "stdev": 1}"#)
    );
}

#[test]
fn joins_lines_for_the_clipboard() {
    assert_eq!(call("Join", &args(r#"[["a", "b"], "\n"]"#), &utc()), Value::from("a\nb"));
}

#[test]
fn caps_what_it_builds() {
    assert_eq!(call("Range", &args("[0, 10000]"), &utc()).as_array().map(<[Value]>::len), Some(10_000));
    assert_eq!(call("Range", &args("[0, 10001]"), &utc()), Value::Null);
    assert_eq!(call("Split", &[Value::from(",".repeat(10_001) + "x"), Value::Null], &utc()), json(r#"["x"]"#));
    let long = Value::Array(vec![Value::Number(1.0); 6_000]);
    assert_eq!(call("Concat", &[long.clone(), long], &utc()), Value::Null);
}

#[test]
fn keeps_keys_in_javascript_order() {
    let rows = args(r#"[[{"m": "Jan", "y": "2026", "v": 2}, {"m": "Jan", "y": "2025", "v": 1}], "m", "y", "v"]"#);
    let pivot = call("Pivot", &rows, &utc());
    assert_eq!(pivot.to_json_string(), r#"[{"2025":1,"2026":2,"key":"Jan"}]"#);
}
