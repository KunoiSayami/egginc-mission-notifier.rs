use base64::Engine;
use chrono::TimeDelta;
use itertools::Itertools as _;
use reqwest::ClientBuilder;
use std::{
    sync::{Arc, LazyLock},
    time::Duration,
};

use teloxide::{
    payloads::{EditMessageTextSetters, SendMessageSetters},
    prelude::Requester as _,
    types::{CallbackQuery, ChatId, InlineKeyboardButton, InlineKeyboardMarkup, MessageId},
};

use anyhow::anyhow;

use crate::{
    bot::replace_all,
    egg::{decode_and_calc_score, encode_to_byte, query_coop_status},
    types::{fmt_time_delta_short, return_tf_emoji, timestamp_to_string, BASE64},
};

use super::{arg::NecessaryArg, functions::link_preview_options, BotType, EI_CHECKER_RE, SPACE_RE};

pub(super) static COOP_ID_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^[\w]+(-[\w\d]+)*$").unwrap());
pub(super) static ROOM_RE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^[\w\d][\-\w\d]*$").unwrap());
pub(super) static CONTRACT_WEBSITE_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(
        r"^https://eicoop-carpet.netlify.app/([\w\d][\-\w\d]*)/([\w\d][\-\w\d]*)(\?d)?$",
    )
    .unwrap()
});

#[derive(Clone)]
pub(super) enum ContractCommand {
    List {
        ei: String,
    },
    Calc {
        ei: String,
        id: String,
        detail: bool,
    },
    CalcRoom {
        id: String,
        room: String,
        detail: bool,
    },
    Control {
        enable: bool,
        ei: String,
    },
}

impl ContractCommand {
    fn parse(input: std::borrow::Cow<'_, str>) -> Option<Self> {
        let (first, second) = input.split_once(' ')?;

        if let Some((second, third)) = second.split_once(' ') {
            let (third, forth) = third.split_once(' ').unwrap_or((third, ""));
            match first {
                "calc" if EI_CHECKER_RE.is_match(second) && ROOM_RE.is_match(third) => {
                    Some(Self::Calc {
                        ei: second.into(),
                        id: third.into(),
                        detail: forth.eq("detail"),
                    })
                }
                "room" if COOP_ID_RE.is_match(second) && ROOM_RE.is_match(third) => {
                    Some(Self::CalcRoom {
                        id: second.into(),
                        room: third.into(),
                        detail: forth.eq("detail"),
                    })
                }
                _ => None,
            }
        } else {
            match first {
                "list" if EI_CHECKER_RE.is_match(second) => Some(Self::List { ei: second.into() }),
                "enable" | "disable" => {
                    if EI_CHECKER_RE.is_match(second) {
                        Some(Self::Control {
                            enable: first.eq("enable"),
                            ei: second.into(),
                        })
                    } else {
                        None
                    }
                }
                _ => None,
            }
        }
    }

    pub(super) fn new_room(contract_id: &str, room: &str, detail: bool) -> Self {
        Self::CalcRoom {
            id: contract_id.into(),
            room: room.into(),
            detail,
        }
    }

    fn keyboard(&self, detail: bool) -> InlineKeyboardMarkup {
        let detail = if detail { " detail" } else { "" };
        InlineKeyboardMarkup::new(match &self {
            ContractCommand::Calc { ei, id, .. } => [[
                InlineKeyboardButton::callback(
                    "Refresh",
                    format!("contract calc {ei} {id}{detail}"),
                ),
                InlineKeyboardButton::callback(
                    "Refresh inline",
                    format!("contract-i calc {ei} {id}{detail}"),
                ),
            ]],
            ContractCommand::CalcRoom { id, room, .. } => [[
                InlineKeyboardButton::callback(
                    "Refresh",
                    format!("contract room {id} {room}{detail}"),
                ),
                InlineKeyboardButton::callback(
                    "Refresh inline",
                    format!("contract-i room {id} {room}{detail}"),
                ),
            ]],
            _ => unreachable!(),
        })
    }
}

pub(super) async fn route_contract_command(
    bot: BotType,
    arg: Arc<NecessaryArg>,
    chat_id: ChatId,
    message_id: MessageId,
    cmd: String,
    inline: bool,
) -> anyhow::Result<()> {
    let filtered = SPACE_RE.replace_all(&cmd, " ");
    let Some(cmd) = ContractCommand::parse(filtered) else {
        bot.send_message(chat_id, "Invalid contract command\\.")
            .await?;
        return Ok(());
    };
    match cmd {
        ContractCommand::List { ei } => handle_list_contracts(bot, arg, chat_id, ei).await,
        ContractCommand::Calc { .. } | ContractCommand::CalcRoom { .. } => {
            handle_calc_score(bot, arg, chat_id, message_id, &cmd, inline).await
        }
        ContractCommand::Control { enable, ei } => {
            handle_enable_contract_tracker(bot, chat_id, arg, ei, enable).await
        }
    }
}

async fn handle_list_contracts(
    bot: BotType,
    arg: Arc<NecessaryArg>,
    chat_id: ChatId,
    ei: String,
) -> anyhow::Result<()> {
    if !arg.check_admin(chat_id)
        && !arg
            .database()
            .account_query(Some(chat_id.0))
            .await
            .ok_or_else(|| anyhow!("Query user error"))?
            .iter()
            .any(|x| x.ei().eq(&ei))
    {
        bot.send_message(chat_id, "Permission denied").await?;
        return Ok(());
    }

    let contracts = arg
        .database()
        .account_query_contract(ei)
        .await
        .ok_or_else(|| anyhow!("Query user contract error"))?;

    let res = contracts
        .into_iter()
        .map(|contract| {
            format!(
                "`{}` `{}` {} [{}](t.me/{}?start={})[📋](t.me/{}?start={})",
                replace_all(contract.id()),
                replace_all(contract.room()),
                replace_all(&{
                    if let Some(start_time) = contract.start_time() {
                        timestamp_to_string(start_time as i64)
                    } else {
                        "Unknown".into()
                    }
                }),
                return_tf_emoji(contract.finished()),
                arg.username(),
                BASE64.encode(
                    format!("contract room {} {}", contract.id(), contract.room()).as_bytes()
                ),
                arg.username(),
                BASE64.encode(
                    format!("contract room {} {} detail", contract.id(), contract.room())
                        .as_bytes()
                )
            )
        })
        .join("\n");

    if res.is_empty() {
        bot.send_message(chat_id, "Contract not found").await?;
    } else {
        bot.send_message(chat_id, res).await?;
    }

    Ok(())
}

pub(super) async fn handle_calc_score(
    bot: BotType,
    arg: Arc<NecessaryArg>,
    chat_id: ChatId,
    message_id: MessageId,
    event: &ContractCommand,
    inline: bool,
) -> anyhow::Result<()> {
    let detail = match event {
        ContractCommand::CalcRoom { detail, .. } => detail,
        ContractCommand::Calc { ei, detail, .. } => {
            if !arg
                .database()
                .account_query(Some(chat_id.0))
                .await
                .ok_or(anyhow!("Query user error"))?
                .iter()
                .any(|x| x.ei().eq(ei))
            {
                bot.send_message(chat_id, "Permission denied").await?;

                return Ok(());
            }
            detail
        }
        _ => unreachable!(),
    };

    match process_calc(arg, event, *detail, inline).await {
        Ok(res) => {
            if inline {
                bot.edit_message_text(chat_id, message_id, res)
                    .link_preview_options(link_preview_options(false))
                    .reply_markup(event.keyboard(*detail))
                    .await
            } else {
                bot.send_message(chat_id, res)
                    .link_preview_options(link_preview_options(false))
                    .reply_markup(event.keyboard(*detail))
                    .await
            }
        }
        Err(e) => {
            log::error!("Calc function error: {e:?}");
            bot.send_message(chat_id, "Got error in calc score, check console\\.")
                .await
        }
    }?;

    Ok(())
}

async fn process_calc(
    arg: Arc<NecessaryArg>,
    event: &ContractCommand,
    detail: bool,
    inline: bool,
) -> anyhow::Result<String> {
    let contract_id = match event {
        ContractCommand::Calc { id, .. } | ContractCommand::CalcRoom { id, .. } => id,
        _ => unreachable!(),
    };

    let Some(contract_spec) = arg
        .database()
        .contract_query_spec(contract_id.to_string())
        .await
        .ok_or_else(|| anyhow!("Query contract spec error"))?
    else {
        return Err(anyhow!("Contract spec not found"));
    };

    let current_time = kstool::time::get_current_second() as i64;

    let (timestamp, room, body) = match event {
        ContractCommand::Calc { ei, .. } => {
            let Some(user_contract) = arg
                .database()
                .contract_query_single(contract_id.to_string(), ei.to_string())
                .await
                .ok_or_else(|| anyhow!("Query user contract room error"))?
            else {
                return Err(anyhow!("User room not found"));
            };

            let Some(contract_cache) = arg
                .database()
                .contract_cache_query(contract_id.to_string(), user_contract.room().to_string())
                .await
                .ok_or_else(|| anyhow!("Query contract cache error"))?
            else {
                return Err(anyhow!("Contract cache not found"));
            };
            (
                contract_cache.timestamp(),
                contract_cache.room().to_string(),
                contract_cache.extract(),
            )
        }
        ContractCommand::CalcRoom { room, .. } => {
            match arg
                .database()
                .contract_cache_query(contract_id.to_string(), room.to_string())
                .await
                .ok_or_else(|| anyhow!("Query contract cache error"))?
            {
                Some(cache) if cache.recent() => {
                    (cache.timestamp(), cache.room().to_string(), cache.extract())
                }
                _ => {
                    let client = ClientBuilder::new()
                        .timeout(Duration::from_secs(10))
                        .build()
                        .unwrap();
                    let raw = query_coop_status(&client, contract_id, room, None).await?;

                    let bytes = encode_to_byte(&raw);
                    arg.database()
                        .contract_cache_insert(
                            contract_id.into(),
                            room.into(),
                            bytes.clone(),
                            raw.cleared_for_exit() || raw.all_members_reporting(),
                            None,
                            None,
                        )
                        .await;
                    (current_time, room.clone(), bytes)
                }
            }
        }
        _ => unreachable!(),
    };

    let score = decode_and_calc_score(contract_spec, &body, false)?;

    let sub_title = if !score.is_finished() {
        let expect = current_time + score.expect_finish_time(Some(timestamp)) as i64;

        format!(
            "Expect complete: {}\n{}",
            replace_all(&timestamp_to_string(
                current_time + score.expect_finish_time(Some(timestamp)) as i64,
            )),
            if current_time > expect {
                "⚠️*Warning: The contract will be completed beyond the estimated time\\.*\n"
            } else {
                ""
            }
        )
    } else {
        "".into()
    };

    let users = score
        .member()
        .iter()
        .map(|member| {
            member
                .print(detail, Some(timestamp), score.is_cleared(), replace_all)
                .unwrap()
        })
        .join("\n");

    let result = format!(
        "[*\\({grade}\\)*](https://eicoop-carpet.netlify.app/{contract_id}/{room}) `{contract}` \\[`{room_id}`\\] {current_status}\n\
        Target: {amount}/{target} ELR: _{elr}_ Buff: _{buff}_\n\
        Contract timestamp: _{completion_time}_ / _{remain}_ remain\n\
        {sub_title}\n{users}\n\n\
        Contract last update: {last_update}\n\
        {msg_update}\
        {footer}",
        contract = replace_all(contract_id),
        room_id = replace_all(&room),
        grade = score.grade_str(),
        current_status = score.emoji(),
        elr = replace_all(&score.total_known_elr()),
        buff = replace_all(&score.display_buff()),
        completion_time = fmt_time_delta_short(TimeDelta::seconds(score.completion_time() as i64)),
        amount = replace_all(&score.current_amount()),
        target = replace_all(&score.target_amount()),
        remain = replace_all(&fmt_time_delta_short(TimeDelta::seconds(
            score.contract_remain_time(Some(timestamp)) as i64
        ))),
        last_update = replace_all(&timestamp_to_string(timestamp)),
        msg_update = if inline {
            format!(
                "Score update timestamp: {}\n",
                replace_all(&timestamp_to_string(current_time))
            )
        } else {
            String::new()
        },
        footer = if score.is_finished() && !score.is_cleared() {
            "This score is included your offline contributions, but not included your teamwork score\\.\n"
        } else {
            "This score not included your teamwork score\\."
        }
    );

    Ok(result)
}

async fn handle_enable_contract_tracker(
    bot: BotType,
    chat_id: ChatId,
    arg: Arc<NecessaryArg>,
    ei: String,
    enable: bool,
) -> anyhow::Result<()> {
    if !arg.check_admin(chat_id)
        && !arg
            .database()
            .account_query(Some(chat_id.0))
            .await
            .is_some_and(|x| x.iter().any(|account| account.ei().eq(&ei)))
    {
        bot.send_message(chat_id, "Permission denied\\.").await?;
        return Ok(());
    }

    arg.database().account_contract_update(ei, enable).await;
    bot.send_message(
        chat_id,
        format!(
            "Set contract tracker to {}",
            if enable { "enabled" } else { "disabled" }
        ),
    )
    .await?;
    Ok(())
}

pub(super) async fn handle_callback_query(
    bot: BotType,
    msg: CallbackQuery,
    arg: Arc<NecessaryArg>,
) -> anyhow::Result<()> {
    //log::trace!("Callback data: {:?}", msg.data);
    let Some((first, second)) = msg.data.as_ref().and_then(|text| text.split_once(' ')) else {
        bot.answer_callback_query(msg.id).await?;
        return Ok(());
    };

    match first {
        "contract" | "contract-i" => {
            if let Some(msg) = second
                .contains(' ')
                .then_some(msg.message.as_ref())
                .flatten()
            {
                route_contract_command(
                    bot.clone(),
                    arg,
                    msg.chat().id,
                    msg.id(),
                    second.to_string(),
                    first.eq("contract-i"),
                )
                .await?;
                /* if let Ok(result) =
                    process_calc(arg, &ContractCommand::parse(second.to_string()).unwrap()).await
                {
                    bot.edit_message_text(msg.chat().id, msg.id(), result)
                        .await?;
                }; */
                if !first.ends_with("-i") {
                    bot.edit_message_reply_markup(msg.chat().id, msg.id())
                        .await?;
                }
            }
        }
        _ => {}
    }
    bot.answer_callback_query(msg.id).await?;
    Ok(())
}

pub(super) mod prelude {
    pub(in crate::bot) use super::{
        handle_calc_score, handle_callback_query, route_contract_command,
    };
}
