//! What `watch` and its interactive version share on a terminal: how the screen is taken and given
//! back, whether colours are wanted, and one frame of the view.
use super::info::{self, FetchError, Info};
use super::view::{History, Style, render};

/// Takes the alternate screen and hides the cursor
pub const ENTER_SCREEN: &str = "\x1b[?1049h\x1b[?25l";

/// Gives the screen and the cursor back
pub const LEAVE_SCREEN: &[u8] = b"\x1b[?25h\x1b[?1049l";

/// Colours are wanted unless `NO_COLOR` is set to something
pub fn colors_wanted() -> bool {
    std::env::var("NO_COLOR").map_or(true, |value| value.is_empty())
}

/// One frame: the view of a sample, or why there is none
pub fn frame(style: &Style, url: &str, history: &mut History) -> Result<String, FetchError> {
    let info: Info = info::fetch(url)?;
    history.record(&info);
    Ok(render(style, url, &info, history))
}

/// The frame, or the reason there is none as the text to show instead
pub fn sample(style: &Style, url: &str, history: &mut History) -> String {
    frame(style, url, history).unwrap_or_else(|error| error.describe(url))
}
