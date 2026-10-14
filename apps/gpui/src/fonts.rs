//! Inline code at the Mac's size. The Mac sets code in a paragraph at 12.5 points whatever the
//! paragraph's size, and GPUI sets a paragraph at one size. So the system's monospaced font is
//! made again for each size of paragraph and each weight, under a name of its own, its glyphs
//! drawn smaller by as much as the code is smaller than the paragraph: its em is made larger by
//! that much. The font is a variable one whose own weight is light, so each copy keeps only the
//! weight it is for, and CoreText, given the copy as a file, draws it at that weight.

use std::sync::OnceLock;

use gpui_kit::*;

use crate::theme;

/// The sizes of the transcript's paragraphs that have inline code: prose, tool titles and table
/// cells, and the headings.
const PARAGRAPH_SIZES: [u32; 5] = [13, 14, 15, 17, 20];

/// The system's monospaced font, whose glyphs the copies are.
const SYSTEM_MONO: &str = "/System/Library/Fonts/SFNSMono.ttf";

/// The weights of the copies: the font's own regular and semibold, by their `wght`.
const REGULAR: f32 = 400.;
const SEMIBOLD: f32 = 600.;

static LOADED: OnceLock<bool> = OnceLock::new();

/// Makes the copies of the monospaced font and has CoreText know them. Without them inline code
/// is set at its paragraph's size.
pub fn load() {
    LOADED.get_or_init(|| {
        let loaded = registered();
        if !loaded {
            tracing::warn!("inline code is set at its paragraph's size: its fonts couldn't be made");
        }
        loaded
    });
}

fn registered() -> bool {
    let Ok(font) = std::fs::read(SYSTEM_MONO) else { return false };
    let folder = std::env::temp_dir().join("motile-gpui-fonts");
    if std::fs::create_dir_all(&folder).is_err() {
        return false;
    }
    PARAGRAPH_SIZES.iter().all(|paragraph| {
        [(false, REGULAR), (true, SEMIBOLD)].into_iter().all(|(bold, weight)| {
            let family = family(*paragraph, bold);
            let scale = *paragraph as f32 / theme::CODE_SIZE;
            let Some(copy) = scaled(&font, scale, weight, &family) else { return false };
            let file = folder.join(format!("{}.ttf", postscript(&family)));
            std::fs::write(&file, copy).is_ok() && core_text::register(&file)
        })
    })
}

fn family(paragraph: u32, bold: bool) -> String {
    match bold {
        true => format!("Motile Inline Mono {paragraph} Semibold"),
        false => format!("Motile Inline Mono {paragraph}"),
    }
}

fn postscript(family: &str) -> String {
    family.chars().filter(|character| !character.is_whitespace()).collect()
}

/// How big inline code is drawn in a paragraph of `paragraph` points.
pub fn inline_code_size(paragraph: f32) -> f32 {
    match scaled_family(paragraph, false) {
        Some(_) => theme::CODE_SIZE,
        None => paragraph,
    }
}

fn scaled_family(paragraph: f32, bold: bool) -> Option<String> {
    let size = paragraph.round() as u32;
    let loaded = LOADED.get().copied().unwrap_or(false);
    (loaded && paragraph.fract() == 0. && PARAGRAPH_SIZES.contains(&size)).then(|| family(size, bold))
}

/// The font of inline code in a paragraph of `paragraph` points, semibold when it is bold.
pub fn inline_code(paragraph: f32, bold: bool) -> Font {
    let weight = if bold { FontWeight::SEMIBOLD } else { FontWeight::NORMAL };
    let family = scaled_family(paragraph, bold).unwrap_or_else(|| theme::MONO_FONT.to_string());
    Font { family: family.into(), features: FontFeatures::default(), fallbacks: None, weight, style: FontStyle::Normal }
}

/// A copy of a variable TrueType font whose glyphs are drawn `1 / scale` times as big, with only
/// the named weight nearest to `weight`, named `family`.
fn scaled(font: &[u8], scale: f32, weight: f32, family: &str) -> Option<Vec<u8>> {
    let tables = tables(font)?;
    let (_, name) = tables.iter().find(|(tag, _)| *tag == b"name")?;
    let original = name_records(name)?;
    let mut out: Vec<(&[u8; 4], Vec<u8>)> = Vec::with_capacity(tables.len());
    for (tag, data) in &tables {
        let data = match *tag {
            b"head" => {
                let mut head = data.to_vec();
                let em = u16_at(&head, 18)?;
                let larger = (f32::from(em) * scale).round().min(16384.) as u16;
                head[18..20].copy_from_slice(&larger.to_be_bytes());
                head[8..12].fill(0);
                head
            }
            b"fvar" => one_instance(data, weight)?,
            b"name" => renamed(&original, family),
            _ => data.to_vec(),
        };
        out.push((tag, data));
    }
    Some(assembled(font, out))
}

fn u16_at(data: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(data.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(data.get(at..at + 4)?.try_into().ok()?))
}

/// The font's tables, by tag, in the order its directory lists them.
fn tables(font: &[u8]) -> Option<Vec<(&[u8; 4], &[u8])>> {
    let count = u16_at(font, 4)? as usize;
    (0..count)
        .map(|index| {
            let record = 12 + index * 16;
            let tag: &[u8; 4] = font.get(record..record + 4)?.try_into().ok()?;
            let offset = u32_at(font, record + 8)? as usize;
            let length = u32_at(font, record + 12)? as usize;
            Some((tag, font.get(offset..offset + length)?))
        })
        .collect()
}

/// A font file of the tables, with the original's header.
fn assembled(original: &[u8], tables: Vec<(&[u8; 4], Vec<u8>)>) -> Vec<u8> {
    let directory = 12 + 16 * tables.len();
    let mut file = original[..12].to_vec();
    file.resize(directory, 0);
    for (index, (tag, data)) in tables.iter().enumerate() {
        let offset = file.len();
        file.extend_from_slice(data);
        file.resize(file.len().next_multiple_of(4), 0);
        let sum = checksum(&file[offset..]);
        let record = 12 + index * 16;
        file[record..record + 4].copy_from_slice(*tag);
        file[record + 4..record + 8].copy_from_slice(&sum.to_be_bytes());
        file[record + 8..record + 12].copy_from_slice(&(offset as u32).to_be_bytes());
        file[record + 12..record + 16].copy_from_slice(&(data.len() as u32).to_be_bytes());
    }
    file
}

fn checksum(data: &[u8]) -> u32 {
    data.chunks(4).fold(0u32, |sum, word| {
        let mut bytes = [0; 4];
        bytes[..word.len()].copy_from_slice(word);
        sum.wrapping_add(u32::from_be_bytes(bytes))
    })
}

/// The `fvar` table with only the named instance whose weight is nearest to `weight`.
fn one_instance(fvar: &[u8], weight: f32) -> Option<Vec<u8>> {
    let (axes_at, axes, axis_size) = (u16_at(fvar, 4)? as usize, u16_at(fvar, 8)? as usize, u16_at(fvar, 10)? as usize);
    let (instances, instance_size) = (u16_at(fvar, 12)? as usize, u16_at(fvar, 14)? as usize);
    let tag_of = |axis: usize| fvar.get(axes_at + axis * axis_size..axes_at + axis * axis_size + 4);
    let wght = (0..axes).position(|axis| tag_of(axis) == Some(b"wght"))?;
    let first = axes_at + axes * axis_size;
    let weight_of = |instance: usize| {
        let at = first + instance * instance_size + 4 + wght * 4;
        u32_at(fvar, at).map(|fixed| fixed as i32 as f32 / 65536.)
    };
    let nearest = (0..instances)
        .filter_map(|instance| Some((instance, (weight_of(instance)? - weight).abs())))
        .min_by(|a, b| a.1.total_cmp(&b.1))?
        .0;
    let mut table = fvar.get(..first + instance_size)?.to_vec();
    table[first..].copy_from_slice(fvar.get(first + nearest * instance_size..first + (nearest + 1) * instance_size)?);
    table[12..14].copy_from_slice(&1u16.to_be_bytes());
    Some(table)
}

/// The records of a `name` table: platform, encoding, language and name, and the bytes.
fn name_records(name: &[u8]) -> Option<Vec<([u16; 4], Vec<u8>)>> {
    let count = u16_at(name, 2)? as usize;
    let storage = u16_at(name, 4)? as usize;
    (0..count)
        .map(|index| {
            let at = 6 + index * 12;
            let ids = [u16_at(name, at)?, u16_at(name, at + 2)?, u16_at(name, at + 4)?, u16_at(name, at + 6)?];
            let (length, offset) = (u16_at(name, at + 8)? as usize, u16_at(name, at + 10)? as usize);
            Some((ids, name.get(storage + offset..storage + offset + length)?.to_vec()))
        })
        .collect()
}

/// The `name` table with the family's names in place of the font's own. The names its weights
/// are known by start with the family's PostScript name instead of the system font's, which
/// they would otherwise be taken for.
fn renamed(records: &[([u16; 4], Vec<u8>)], family: &str) -> Vec<u8> {
    const REPLACED: [u16; 7] = [1, 3, 4, 6, 16, 17, 25];
    const SYSTEM: &str = ".SFNSMono";
    let postscript = postscript(family);
    let utf16 = |words: &str| -> Vec<u8> { words.encode_utf16().flat_map(u16::to_be_bytes).collect() };
    let words_of = |bytes: &[u8]| {
        String::from_utf16(&bytes.as_chunks::<2>().0.iter().map(|pair| u16::from_be_bytes(*pair)).collect::<Vec<_>>())
            .ok()
    };
    let mut kept: Vec<([u16; 4], Vec<u8>)> = records
        .iter()
        // Names in a language given by a tag, which a table of the first format doesn't keep.
        .filter(|(ids, _)| !REPLACED.contains(&ids[3]) && ids[2] < 0x8000)
        .map(|(ids, bytes)| {
            let system = match ids[0] {
                1 => bytes.strip_prefix(SYSTEM.as_bytes()).map(|rest| [postscript.as_bytes(), rest].concat()),
                _ => words_of(bytes)
                    .and_then(|words| words.strip_prefix(SYSTEM).map(|rest| utf16(&format!("{postscript}{rest}")))),
            };
            (*ids, system.unwrap_or_else(|| bytes.clone()))
        })
        .collect();
    let own = [
        (1, family.to_string()),
        (3, postscript.clone()),
        (4, family.to_string()),
        (6, format!("{postscript}-Regular")),
        (16, family.to_string()),
        (25, postscript),
    ];
    kept.extend(own.into_iter().map(|(id, words)| ([3, 1, 0x409, id], utf16(&words))));
    kept.sort_by_key(|(ids, _)| *ids);
    let mut table = Vec::new();
    table.extend_from_slice(&0u16.to_be_bytes());
    table.extend_from_slice(&(kept.len() as u16).to_be_bytes());
    table.extend_from_slice(&((6 + kept.len() * 12) as u16).to_be_bytes());
    let mut strings = Vec::new();
    for (ids, bytes) in &kept {
        for id in ids {
            table.extend_from_slice(&id.to_be_bytes());
        }
        table.extend_from_slice(&(bytes.len() as u16).to_be_bytes());
        table.extend_from_slice(&(strings.len() as u16).to_be_bytes());
        strings.extend_from_slice(bytes);
    }
    table.extend_from_slice(&strings);
    table
}

#[cfg(target_os = "macos")]
mod core_text {
    use std::ffi::c_void;
    use std::path::Path;

    use core_foundation::base::TCFType;
    use core_foundation::url::{CFURL, CFURLRef};

    /// The fonts are known to this process only.
    const SCOPE_PROCESS: u32 = 1;

    #[link(name = "CoreText", kind = "framework")]
    unsafe extern "C" {
        fn CTFontManagerRegisterFontsForURL(url: CFURLRef, scope: u32, error: *mut *mut c_void) -> bool;
    }

    /// Has CoreText know the font in the file, as if it were installed, while the app runs.
    pub fn register(file: &Path) -> bool {
        let Some(url) = CFURL::from_path(file, false) else { return false };
        unsafe { CTFontManagerRegisterFontsForURL(url.as_concrete_TypeRef(), SCOPE_PROCESS, std::ptr::null_mut()) }
    }
}

#[cfg(not(target_os = "macos"))]
mod core_text {
    pub fn register(_: &std::path::Path) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::{SEMIBOLD, SYSTEM_MONO, scaled, tables, u16_at, u32_at};

    #[test]
    fn a_copy_draws_one_weight_smaller_under_its_own_name() {
        let Ok(font) = std::fs::read(SYSTEM_MONO) else { return };
        let copy = scaled(&font, 14. / 12.5, SEMIBOLD, "Motile Inline Mono 14 Semibold").expect("scaled");
        let (original, copied) = (tables(&font).expect("tables"), tables(&copy).expect("tables"));
        let table = |tables: &[(&[u8; 4], &[u8])], wanted: &[u8; 4]| {
            tables.iter().find(|(tag, _)| *tag == wanted).map(|(_, data)| data.to_vec()).expect("table")
        };
        let em = |head: Vec<u8>| u16_at(&head, 18).unwrap() as f32;
        assert!((em(table(&copied, b"head")) / em(table(&original, b"head")) - 14. / 12.5).abs() < 0.001);
        let fvar = table(&copied, b"fvar");
        assert_eq!(u16_at(&fvar, 12), Some(1));
        let first = u16_at(&fvar, 4).unwrap() as usize + u16_at(&fvar, 8).unwrap() as usize * 20;
        let weight = |axis: usize| u32_at(&fvar, first + 4 + axis * 4).unwrap() as i32 as f32 / 65536.;
        assert!((0..2).any(|axis| (weight(axis) - SEMIBOLD).abs() < 50.));
        let name = table(&copied, b"name");
        let utf16 = |words: &str| -> Vec<u8> { words.encode_utf16().flat_map(u16::to_be_bytes).collect() };
        let has = |words: &str| name.windows(utf16(words).len()).any(|window| window == utf16(words).as_slice());
        assert!(has("MotileInlineMono14Semibold") && !has(".SFNSMono"));
        assert_eq!(copied.len(), original.len());
    }
}
