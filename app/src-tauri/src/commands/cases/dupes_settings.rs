//! Settings of the duplicate finder. The settings type, its file (`duplicate-settings.json` in the
//! app's config dir) and how it is read and written are `pm_formats::ops_dupes`, shared with
//! `pm duplicates`; this keeps the old path for the finder.

pub use pm_formats::ops_dupes::{load, save, DupSettings};
