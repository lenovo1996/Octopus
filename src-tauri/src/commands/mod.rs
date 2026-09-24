//! Thin IPC handlers by feature. Each handler validates its request,
//! delegates to services/engine (T03+), and returns `ApiResult<T>`.

pub mod branch;
pub mod commit;
pub mod commit_actions;
pub mod diff;
pub mod history;
pub mod merge;
pub mod preflight;
pub mod repos;
pub mod settings;
pub mod stage;
pub mod stash;
pub mod status;
pub mod sync;

pub mod prelude {
    pub use super::branch::prelude::*;
    pub use super::commit::prelude::*;
    pub use super::commit_actions::prelude::*;
    pub use super::diff::prelude::*;
    pub use super::history::prelude::*;
    pub use super::merge::prelude::*;
    pub use super::preflight::app_preflight;
    pub use super::repos::prelude::*;
    pub use super::settings::prelude::*;
    pub use super::stage::prelude::*;
    pub use super::stash::prelude::*;
    pub use super::status::prelude::*;
    pub use super::sync::prelude::*;
}
