use teloxide::types::LinkPreviewOptions;

use super::TELEGRAM_ESCAPE_RE;

pub fn replace_all(s: &str) -> std::borrow::Cow<'_, str> {
    TELEGRAM_ESCAPE_RE.replace_all(s, "\\$1")
}

pub(super) fn link_preview_options(enable: bool) -> LinkPreviewOptions {
    LinkPreviewOptions {
        is_disabled: !enable,
        prefer_large_media: false,
        prefer_small_media: false,
        url: None,
        show_above_text: false,
    }
}
