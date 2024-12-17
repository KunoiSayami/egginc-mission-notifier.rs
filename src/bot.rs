use std::sync::{Arc, LazyLock};

use admin::handle_admin_command;
use anyhow::anyhow;
use itertools::Itertools;
use teloxide::{
    adaptors::DefaultParseMode,
    dispatching::{HandlerExt as _, UpdateFilterExt as _},
    dptree,
    prelude::{Dispatcher, Requester as _, RequesterExt as _},
    types::{ChatId, Message, ParseMode, Update},
    utils::command::BotCommands,
    Bot,
};

use crate::{
    config::Config,
    database::DatabaseHelper,
    egg::monitor::{MonitorHelper, LAST_QUERY},
    types::{return_tf_emoji, timestamp_to_string},
};

pub type BotType = DefaultParseMode<Bot>;

static TELEGRAM_ESCAPE_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"([_*\[\]\(\)~>#\+\-=|\{}\.!])").unwrap());
pub static USERNAME_CHECKER_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^EI\d{16}$").unwrap());

/* fn accept_two_digits(input: String) -> Result<(u8,), ParseError> {
    match input.len() {
        2 => {
            let num = input
                .parse::<u8>()
                .map_err(|e| ParseError::IncorrectFormat(e.into()))?;
            Ok((num,))
        }
        len => Err(ParseError::Custom(
            format!("Only 2 digits allowed, not {}", len).into(),
        )),
    }
} */

pub fn replace_all<'a>(s: &'a str) -> std::borrow::Cow<'a, str> {
    TELEGRAM_ESCAPE_RE.replace_all(s, "\\$1")
    /* .replace("\\*\\*", "**")
    .replace("\\_\\_", "__")
    .replace("\\~\\~", "~~") */
}

mod admin {

    use std::str::FromStr;

    use super::*;

    #[derive(Clone, Debug)]
    pub(super) enum AdminCommand {
        Query { ei: Option<String> },
        /* Test, */
    }

    impl FromStr for AdminCommand {
        type Err = &'static str;

        fn from_str(s: &str) -> Result<Self, Self::Err> {
            if s.contains(' ') {
                let (first, second) = s.split_once(' ').unwrap();

                match first {
                    "query" => Ok(Self::Query {
                        ei: Some(second.to_string()),
                    }),
                    _ => Err("Invalid command"),
                }
            } else {
                match s {
                    "query" => Ok(Self::Query { ei: None }),
                    /* "test" => Ok(Self::Test), */
                    _ => Err("Invalid command"),
                }
            }
        }
    }
    pub(crate) async fn handle_admin_command(
        bot: BotType,
        arg: Arc<NecessaryArg>,
        msg: Message,
        line: String,
    ) -> anyhow::Result<()> {
        if !arg.check_admin(msg.chat.id) {
            return Ok(());
        }

        let command: Result<AdminCommand, &str> = line.parse();

        match command {
            Ok(AdminCommand::Query { ei }) => {
                if let Some(ei) = ei {
                    arg.database().player_timestamp_reset(ei).await;
                }
                arg.monitor().new_client().await;
                bot.send_message(msg.chat.id, "Request sent").await
            }
            /* Ok(AdminCommand::Test) => {
                bot.send_message(msg.chat.id, "_te*st_\n*te_st*\n__test__")
                    .await
            } */
            Err(e) => bot.send_message(msg.chat.id, e).await,
        }?;

        Ok(())
    }
}

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase")]
enum Command {
    Add { ei: String },
    Delete { ei: String },
    List,
    Missions { user: String },
    Admin { line: String },
    Ping,
}

#[derive(Clone, Debug)]
pub struct NecessaryArg {
    database: DatabaseHelper,
    admin: Vec<ChatId>,
    monitor: MonitorHelper,
}

impl NecessaryArg {
    pub fn new(database: DatabaseHelper, admin: Vec<ChatId>, monitor: MonitorHelper) -> Self {
        Self {
            database,
            admin,
            monitor,
        }
    }

    pub fn database(&self) -> &DatabaseHelper {
        &self.database
    }

    /* pub fn admin(&self) -> &[ChatId] {
        &self.admin
    } */

    pub fn check_admin(&self, id: ChatId) -> bool {
        self.admin.iter().any(|x| &id == x)
    }

    pub fn monitor(&self) -> &MonitorHelper {
        &self.monitor
    }
}

pub fn bot(config: &Config) -> anyhow::Result<BotType> {
    let bot = Bot::new(config.telegram().api_key());
    Ok(match config.telegram().api_server() {
        Some(url) => bot.set_api_url(url.parse()?),
        None => bot,
    }
    .parse_mode(ParseMode::MarkdownV2))
}

pub async fn bot_run(
    bot: BotType,
    config: Config,
    database: DatabaseHelper,
    monitor: MonitorHelper,
) -> anyhow::Result<()> {
    let arg = Arc::new(NecessaryArg::new(
        database,
        config.admin().iter().map(|u| ChatId(*u)).collect(),
        monitor,
    ));

    let handle_message = Update::filter_message().branch(
        dptree::entry()
            .filter(|msg: Message| msg.chat.is_private())
            .filter_command::<Command>()
            .endpoint(
                |msg: Message, bot: BotType, arg: Arc<NecessaryArg>, cmd: Command| async move {
                    match cmd {
                        Command::Ping => handle_ping(bot, msg, arg).await,
                        Command::Add { ei } => handle_add_command(bot, arg, msg, ei).await,
                        Command::Delete { ei } => handle_delete_command(bot, arg, msg, ei).await,
                        Command::List => handle_list_command(bot, arg, msg).await,
                        Command::Missions { user } => {
                            handle_missions_command(bot, arg, msg, user).await
                        }
                        Command::Admin { line } => handle_admin_command(bot, arg, msg, line).await,
                    }
                    .inspect_err(|e| log::error!("Handle command error: {e:?}"))
                },
            ),
    );
    let dispatcher = Dispatcher::builder(bot, dptree::entry().branch(handle_message))
        .dependencies(dptree::deps![arg])
        .default_handler(|_| async {});

    #[cfg(not(debug_assertions))]
    dispatcher.enable_ctrlc_handler().build().dispatch().await;

    #[cfg(debug_assertions)]
    tokio::select! {
        _ = async move {
            dispatcher.build().dispatch().await
        } => {}
        _ = tokio::signal::ctrl_c() => {}
    }
    Ok(())
}

pub async fn handle_add_command(
    bot: BotType,
    arg: Arc<NecessaryArg>,
    msg: Message,
    ei: String,
) -> anyhow::Result<()> {
    if !arg.check_admin(msg.chat.id)
        && arg
            .database()
            .player_query(Some(msg.chat.id.0))
            .await
            .ok_or_else(|| anyhow!("Query player for user not response"))?
            .len()
            >= 4
    {
        bot.send_message(msg.chat.id, "You can't add more player")
            .await?;
        return Ok(());
    }

    if !USERNAME_CHECKER_RE.is_match(&ei) {
        bot.send_message(msg.chat.id, "Skip invalid user").await?;
        return Ok(());
    }

    let result = arg
        .database()
        .player_add(ei.clone(), msg.chat.id.0)
        .await
        .unwrap_or(false);

    bot.send_message(
        msg.chat.id,
        if result {
            format!("Player {ei} added")
        } else {
            "Can't add player, please contact administrator".into()
        },
    )
    .await?;

    if result {
        arg.monitor().new_client().await;
    }

    Ok(())
}

async fn handle_delete_command(
    bot: BotType,
    arg: Arc<NecessaryArg>,
    msg: Message,
    ei: String,
) -> anyhow::Result<()> {
    let Some(player) = arg.database().player_query_ei(ei.clone()).await.flatten() else {
        bot.send_message(msg.chat.id, "User not found").await?;
        return Ok(());
    };

    if player.user() != msg.chat.id.0 || !arg.check_admin(msg.chat.id) {
        bot.send_message(msg.chat.id, "Permission denied").await?;
        return Ok(());
    }

    arg.database().player_remove(ei).await;

    bot.send_message(msg.chat.id, "Deleted").await?;

    Ok(())
}

async fn handle_ping(bot: BotType, msg: Message, arg: Arc<NecessaryArg>) -> anyhow::Result<()> {
    bot.send_message(
        msg.chat.id,
        &format!(
            "Chat id: `{id}`\nLast query: `{last_query}`\nIs admin: {is_admin}\nVersion: {version}",
            id = msg.chat.id.0,
            last_query = replace_all(&timestamp_to_string(
                LAST_QUERY.load(std::sync::atomic::Ordering::Relaxed) as i64
            )),
            is_admin = arg.check_admin(msg.chat.id),
            version = replace_all(env!("CARGO_PKG_VERSION"))
        ),
    )
    .await?;
    Ok(())
}

async fn handle_list_command(
    bot: BotType,
    arg: Arc<NecessaryArg>,
    msg: Message,
) -> anyhow::Result<()> {
    let Some(ret) =
        (if arg.check_admin(msg.chat.id) && msg.text().is_some_and(|text| text.contains("all")) {
            arg.database().player_query(None).await
        } else {
            arg.database().player_query(Some(msg.chat.id.0)).await
        })
    else {
        log::warn!("Query result is None");
        return Ok(());
    };

    if ret.is_empty() {
        bot.send_message(msg.chat.id, "Nothing found").await?;
        return Ok(());
    }

    bot.send_message(
        msg.chat.id,
        ret.into_iter().map(|s| s.to_string()).join("\n"),
    )
    .await?;

    Ok(())
}

async fn handle_missions_command(
    bot: BotType,
    arg: Arc<NecessaryArg>,
    msg: Message,
    user: String,
) -> anyhow::Result<()> {
    let Some(ret) = arg
        .database()
        .mission_query_by_user(if arg.check_admin(msg.chat.id) && !user.is_empty() {
            user.parse()?
        } else {
            msg.chat.id.0
        })
        .await
    else {
        log::warn!("Query mission result is None");
        return Ok(());
    };

    if ret.is_empty() {
        bot.send_message(msg.chat.id, "Nothing found").await?;
        return Ok(());
    }

    let text = ret
        .into_iter()
        .map(|(player, spaceships)| {
            format!(
                "*{}*:\n{}",
                replace_all(player.name()),
                spaceships
                    .into_iter()
                    .map(|s| format!(
                        "{} {} {}",
                        replace_all(s.name()),
                        replace_all(&timestamp_to_string(s.land())),
                        return_tf_emoji(s.notified())
                    ))
                    .join("\n")
            )
        })
        .join("\n\n");

    bot.send_message(msg.chat.id, text).await?;

    Ok(())
}
