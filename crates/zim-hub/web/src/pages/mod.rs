//! Pages — each a route target. Pages compose `components`, are wrapped by a
//! `layouts` shell (in `routes::switch`), and talk to the backend only
//! through `api`.

pub mod admin;
pub mod devices;
pub mod gate;
pub mod markdown_editor;
pub mod not_found;
pub mod settings;
pub mod vault;
pub mod vault_autosave;
pub mod vault_details;
pub mod vault_editor;
pub mod vault_file;
pub mod vault_history;
pub mod vault_history_view;
pub mod vault_tree;
pub mod workspace;
