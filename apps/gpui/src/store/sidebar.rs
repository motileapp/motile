//! What the sidebar keeps that no thread owns.

#[derive(Default)]
pub struct SidebarState {
    /// The thread its pull request's end last marked done, which the sidebar then shows.
    pub settled_thread_id: Option<String>,
}
