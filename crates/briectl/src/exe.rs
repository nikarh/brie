use std::{borrow::Cow, path::PathBuf};

pub fn path() -> Cow<'static, str> {
    std::env::current_exe()
        .ok()
        .as_ref()
        .and_then(|p| p.parent())
        .map(|p| p.join("brie"))
        .map(PathBuf::into_os_string)
        .and_then(|p| p.into_string().ok())
        .map_or(Cow::Borrowed("brie"), Cow::Owned)
}
