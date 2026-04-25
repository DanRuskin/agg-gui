//! Text-related and layout demo windows.
//!
//! Public facade for split text demo modules.

mod dialogs;
mod multi_touch;
mod strip_table;
mod text_layout;

pub use dialogs::{modals_demo, undo_redo, window_options};
pub use multi_touch::multi_touch;
pub use strip_table::{strip_demo, table_demo};
pub use text_layout::text_layout;
