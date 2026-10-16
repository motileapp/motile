//! The panel beside the thread: the changes in its folder, its files, the files opened from
//! either, and the agents the thread started.

pub mod code_view;
pub mod line_comment;
pub mod linear;
pub mod pull_request;
pub mod pull_request_list;
pub mod ricrac;
pub mod state;
pub mod view;

/// The space between the right panel's list rows, split around each: it looks empty but is the
/// row's.
pub const ROW_MARGIN: f32 = 1.;
/// The room under the end of what the right panel scrolls through, so that it never ends on the
/// window's edge.
pub const END_ROOM: f32 = 64.;
