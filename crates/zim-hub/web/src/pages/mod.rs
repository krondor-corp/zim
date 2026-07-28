//! Pages — each a route target. Pages compose `components`, are wrapped by a
//! `layouts` shell (in `routes::switch`), and talk to the backend only
//! through `api`.

pub mod admin;
pub mod device_pair;
pub mod devices;
pub mod gate;
pub mod not_found;
pub mod settings;
pub mod vault;
pub mod vault_editor;
pub mod vault_tree;
pub mod workspace;
