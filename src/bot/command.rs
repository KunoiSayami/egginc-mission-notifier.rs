use std::sync::Arc;

use super::{BotType, admin::handle_admin_command, replace_all};
use base64::Engine;
use itertools::Itertools;
use teloxide::{
    Bot,
    dispatching::{HandlerExt as _, UpdateFilterExt as _},
    dptree,
    prelude::{Dispatcher, Requester as _, RequesterExt as _},
    types::{CallbackQuery, ChatId, Message, ParseMode, Update},
    utils::command::BotCommands,
};

use crate::{
    CHECK_PERIOD, FETCH_PERIOD,
    bot::arg::NecessaryArg,
    config::Config,
    database::DatabaseHelper,
    egg::monitor::{LAST_QUERY, MonitorHelper},
    types::{BASE64, timestamp_to_string},
};

use super::contract::{CONTRACT_WEBSITE_RE, COOP_ID_RE, ContractCommand, ROOM_RE, prelude::*};
use super::missions::prelude::*;

#[derive(BotCommands, Clone)]
#[command(rename_rule = "snake_case")]
enum Command {
    Add { ei: String },
    Delete { ei: String },
    List { detail: String },
    Contract { cmd: String },
    Missions { user: String },
    Recent { user: String },
    Admin { line: String },
    Start { args: String },
    Help,
    Ping,
}

impl Command {
    fn decode_inner(s: String) -> Option<Self> {
        let content = BASE64
            .decode(s.as_bytes())
            .ok()
            .and_then(|x| String::from_utf8(x).ok())?;
        let (first, second) = content.split_once(' ')?;
        match first {
            "contract" | "c" => Some(Self::Contract { cmd: second.into() }),
            _ => None,
        }
    }

    fn decode(s: String) -> Self {
        Self::decode_inner(s.clone()).unwrap_or(Self::Start { args: s })
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
        config.telegram().username().to_string(),
    ));

    let handle_command_message = Update::filter_message().branch(
        dptree::entry()
            .filter(|msg: Message| msg.chat.is_private())
            .filter_command::<Command>()
            .endpoint(
                |msg: Message, bot: BotType, arg: Arc<NecessaryArg>, cmd: Command| {
                    let cmd = if let Command::Start { args } = cmd {
                        Command::decode(args)
                    } else {
                        cmd
                    };
                    async move {
                        match cmd {
                            Command::Ping => handle_ping(bot, msg, arg).await,
                            Command::Add { ei } => handle_add_command(bot, arg, msg, ei).await,
                            Command::Delete { ei } => {
                                handle_delete_command(bot, arg, msg, ei).await
                            }
                            Command::List { detail } => {
                                handle_list_command(bot, arg, msg, detail.eq("ei")).await
                            }
                            Command::Missions { user } => {
                                handle_missions_command(bot, arg, msg, user, false).await
                            }
                            Command::Recent { user } => {
                                handle_missions_command(bot, arg, msg, user, true).await
                            }
                            Command::Admin { line } => {
                                handle_admin_command(bot, arg, msg, line).await
                            }
                            Command::Contract { cmd } => {
                                route_contract_command(bot, arg, msg.chat.id, msg.id, cmd, false)
                                    .await
                            }
                            Command::Help => handle_help(bot, msg).await,
                            Command::Start { args: _ } => {
                                bot.send_message(
                                    msg.chat.id,
                                    "Welcome, use /help to show more information\\.",
                                )
                                .await?;
                                Ok(())
                            }
                        }
                    }
                },
            ),
    );

    let handle_message = Update::filter_message()
        .filter(|msg: Message| msg.chat.is_private())
        .endpoint(
            |msg: Message, bot: BotType, arg: Arc<NecessaryArg>| async move {
                let Some(text) = msg.text() else {
                    return Ok(());
                };

                if let Some(group) = CONTRACT_WEBSITE_RE.captures(text) {
                    let event = ContractCommand::new_room(
                        group.get(1).unwrap().as_str(),
                        group.get(2).unwrap().as_str(),
                        group.get(3).is_some(),
                    );
                    return handle_calc_score(bot, arg, msg.chat.id, msg.id, &event, false).await;
                }

                let groups = text.split_whitespace().collect_vec();

                if groups.len() >= 2 {
                    let first = groups[0];
                    let second = groups[1];
                    let detail = groups.get(2).map(|x| x.eq(&"d")).unwrap_or_default();

                    if COOP_ID_RE.is_match(first) && ROOM_RE.is_match(second) {
                        let event = ContractCommand::new_room(first, second, detail);
                        return handle_calc_score(bot, arg, msg.chat.id, msg.id, &event, false)
                            .await;
                    }
                }

                Ok(())
            },
        );

    let handle_callback_query = Update::filter_callback_query()
        .filter(|q: CallbackQuery| q.data.is_some())
        .endpoint(
            |q: CallbackQuery, bot: BotType, arg: Arc<NecessaryArg>| async move {
                handle_callback_query(bot, q, arg).await
            },
        );

    let dispatcher = Dispatcher::builder(
        bot,
        dptree::entry()
            .branch(handle_command_message)
            .branch(handle_message)
            .branch(handle_callback_query),
    )
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

async fn handle_ping(bot: BotType, msg: Message, arg: Arc<NecessaryArg>) -> anyhow::Result<()> {
    bot.send_message(
        msg.chat.id,
        format!(
            "Chat id: `{id}`\nLast system query: `{last_query}`\nCheck period: {check_period}s\nFetch period: {fetch_period}s\nIs admin: {is_admin}\nVersion: `{version}`",
            id = msg.chat.id.0,
            last_query = replace_all(&timestamp_to_string(
                LAST_QUERY.load(std::sync::atomic::Ordering::Relaxed) as i64
            )),
            check_period = CHECK_PERIOD.get().unwrap(),
            fetch_period = FETCH_PERIOD.get().unwrap(),
            is_admin = arg.check_admin(msg.chat.id),
            version = replace_all(env!("CARGO_PKG_VERSION"))
        ),
    )
    .await?;
    Ok(())
}

async fn handle_help(bot: BotType, msg: Message) -> anyhow::Result<()> {
    bot.send_message(msg.chat.id, "Usage:\n\
    /add `\\<EI\\>` Add your account to this bot\\.\n\
    /list `\\[ei\\]` List all EI belong your telegram account\\.\n\
    /missions Display recent 6 rocket missions\\.\n\
    /recent Display recent 1 hour land missions\\.\n\
    /remove `\\<EI\\>` Remove your account from this bot\\.\n\n\
    Contract rated:\n\
    `/contract list` List your recent contracts, only available when contract tracker enabled\\.\n\
    `/contract calc \\<EI\\> \\<contract\\-id\\>` Calculate user's contract score\\.\n\
    `/contract room \\<contract\\-id\\> \\<room\\-id\\> \\[detail\\]` Calculate contract score by specify room ID\\.\n\
    `/contract enable\\|disable <EI>` Enable / Disable contract tracker \\(After add to bot\\)\\.\n\n\
    Note:\n\
    `\\[\\.\\.\\.\\]` means optional string\\.
    ").await?;
    Ok(())
}
