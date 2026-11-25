use motile_protocol::wire::{Item, Media};
use serde::Serialize;

use crate::render::rows::{Row, RowKind, file_name, rendered};

/// An image or a video of a thread, as its Media tab shows it: one the user attached or one a
/// reply showed. What tool calls showed their agent is left out.
#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct Shown {
    pub media: String,
    /// The row it is shown in, which Show in Chat brings into sight, and that row's item.
    pub row: String,
    pub item: String,
    pub name: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub video: bool,
    /// The image that stands for a video until it plays.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poster: Option<String>,
    /// In pixels, for an image, and for a video that has a poster.
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub size: u64,
    pub at: f64,
}

/// The thread's images and videos, the newest first.
pub fn gallery(items: &[Item], cwd: &str) -> Vec<Shown> {
    let mut shown: Vec<Shown> = items
        .iter()
        .filter(|item| item.parent.is_none() && item.media.iter().any(|media| !media.file))
        .flat_map(|item| rendered(item, cwd).into_iter().flat_map(move |row| of_row(item, &row)))
        .collect();
    shown.reverse();
    shown
}

/// Whether the row shows an image or a video the gallery has.
pub fn shows_media(row: &Row) -> bool {
    match &row.kind {
        RowKind::Media { .. } => true,
        RowKind::User { attachments, .. } => {
            attachments.iter().any(|attached| attached.media.is_some() && !attached.file)
        }
        _ => false,
    }
}

fn of_row(item: &Item, row: &Row) -> Vec<Shown> {
    let ids: Vec<&str> = match &row.kind {
        RowKind::Media { media, .. } => vec![media],
        RowKind::User { attachments, .. } => attachments
            .iter()
            .filter(|attached| !attached.file)
            .filter_map(|attached| attached.media.as_deref())
            .collect(),
        _ => Vec::new(),
    };
    let found = |id: &str| item.media.iter().find(|media| media.id == id && !media.file);
    ids.into_iter().filter_map(found).map(|media| shown(item, row, media)).collect()
}

fn shown(item: &Item, row: &Row, media: &Media) -> Shown {
    Shown {
        media: media.id.clone(),
        row: row.id.clone(),
        item: item.id.clone(),
        name: file_name(&media.src).to_string(),
        video: media.video,
        poster: media.poster.clone(),
        width: media.width,
        height: media.height,
        size: media.size,
        at: item.created_at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn item(id: &str, seq: u64, mut payload: serde_json::Value) -> Item {
        payload["id"] = id.into();
        payload["seq"] = seq.into();
        payload["rev"] = seq.into();
        payload["created_at"] = (seq as f64).into();
        serde_json::from_value(payload).unwrap()
    }

    fn media(id: &str, src: &str) -> serde_json::Value {
        json!({ "id": id, "src": src, "video": id.ends_with(".mp4"), "size": 10, "width": 4, "height": 3 })
    }

    fn ids(shown: &[Shown]) -> Vec<(&str, &str)> {
        shown.iter().map(|shown| (shown.media.as_str(), shown.row.as_str())).collect()
    }

    #[test]
    fn has_what_was_attached_and_shown_newest_first_but_not_files() {
        let items = [
            item(
                "u",
                1,
                json!({
                    "type": "user", "text": "Look", "attachments": ["/tmp/a.png", "/tmp/notes.txt"],
                    "media": [media("a.png", "/tmp/a.png"), { "id": "n.txt", "src": "/tmp/notes.txt", "video": false, "file": true, "size": 3 }],
                }),
            ),
            item(
                "r",
                2,
                json!({
                    "type": "assistant",
                    "text": "Here:\n\n![shot](/out/shot.png)\n\nand\n\n![clip](/out/clip.mp4)",
                    "media": [media("s.png", "/out/shot.png"), media("c.mp4", "/out/clip.mp4")],
                }),
            ),
        ];

        let shown = gallery(&items, "/tmp");

        assert_eq!(ids(&shown), [("c.mp4", "r/3"), ("s.png", "r/1"), ("a.png", "u/0")]);
        assert!(shown[0].video);
        assert_eq!((shown[2].name.as_str(), shown[2].at), ("a.png", 1.0));
    }

    #[test]
    fn leaves_out_what_agents_it_started_showed() {
        let mut child = item(
            "c",
            1,
            json!({
                "type": "assistant", "text": "![shot](/out/shot.png)", "media": [media("s.png", "/out/shot.png")],
            }),
        );
        child.parent = Some("t".into());

        assert!(gallery(&[child], "/tmp").is_empty());
    }
}
