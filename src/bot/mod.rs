mod admin;
mod arg;
mod command;
mod contract;
mod functions;
mod missions;

use std::sync::LazyLock;

use teloxide::{adaptors::DefaultParseMode, Bot};

pub type BotType = DefaultParseMode<Bot>;

static TELEGRAM_ESCAPE_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"([_*\[\]\(\)~>#\+\-=|\{}\.!])").unwrap());
pub static EI_CHECKER_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^EI\d{16}$").unwrap());
static SPACE_RE: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"[ \t]+").unwrap());

pub use command::{bot, bot_run};
pub use functions::replace_all;
