use std::sync::Arc;

use anyhow::anyhow;
use itertools::Itertools as _;
use teloxide::{prelude::Requester as _, types::Message};

use crate::types::{return_tf_emoji, timestamp_to_string, SpaceShip};

use super::functions::replace_all;
use super::{arg::NecessaryArg, BotType, EI_CHECKER_RE};

pub(super) async fn handle_add_command(
    bot: BotType,
    arg: Arc<NecessaryArg>,
    msg: Message,
    ei: String,
) -> anyhow::Result<()> {
    if !arg.check_admin(msg.chat.id)
        && arg
            .database()
            .account_query(Some(msg.chat.id.0))
            .await
            .ok_or_else(|| anyhow!("Query player for user not response"))?
            .len()
            >= 4
    {
        bot.send_message(msg.chat.id, "You can't add more player")
            .await?;
        return Ok(());
    }

    if !EI_CHECKER_RE.is_match(&ei) {
        bot.send_message(msg.chat.id, "Skip invalid user").await?;
        return Ok(());
    }

    let result = arg
        .database()
        .account_add(ei.clone(), msg.chat.id.0)
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

pub(super) async fn handle_delete_command(
    bot: BotType,
    arg: Arc<NecessaryArg>,
    msg: Message,
    ei: String,
) -> anyhow::Result<()> {
    let Some(_account) = arg.database().account_query_ei(ei.clone()).await.flatten() else {
        bot.send_message(msg.chat.id, "User not found").await?;
        return Ok(());
    };
    let Some(account_map) = arg.database().account_query_users(ei.clone()).await else {
        bot.send_message(msg.chat.id, "User not found").await?;
        return Ok(());
    };

    if !account_map.users().iter().any(|x| x == &msg.chat.id.0) || !arg.check_admin(msg.chat.id) {
        bot.send_message(msg.chat.id, "Permission denied").await?;
        return Ok(());
    }

    arg.database().user_remove_account(msg.chat.id.0, ei).await;

    bot.send_message(msg.chat.id, "User deleted").await?;

    Ok(())
}

pub(super) async fn handle_list_command(
    bot: BotType,
    arg: Arc<NecessaryArg>,
    msg: Message,
    show_ei: bool,
) -> anyhow::Result<()> {
    let Some(ret) =
        (if arg.check_admin(msg.chat.id) && msg.text().is_some_and(|text| text.contains("all")) {
            arg.database().account_query(None).await
        } else {
            arg.database().account_query(Some(msg.chat.id.0)).await
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
        ret.into_iter()
            .map(|s| s.line(arg.username(), show_ei))
            .join("\n"),
    )
    .await?;

    Ok(())
}

// Credit: Asuna
fn iter_spaceships(
    spaceships: Vec<SpaceShip>,
    recent: bool,
) -> Box<dyn Iterator<Item = SpaceShip>> {
    if recent {
        Box::new(spaceships.into_iter().rev())
    } else {
        Box::new(spaceships.into_iter())
    }
}

pub(super) async fn handle_missions_command(
    bot: BotType,
    arg: Arc<NecessaryArg>,
    msg: Message,
    user: String,
    recent: bool,
) -> anyhow::Result<()> {
    let Some(ret) = arg
        .database()
        .mission_query_by_user(
            if arg.check_admin(msg.chat.id) && !user.is_empty() {
                user.parse()?
            } else {
                msg.chat.id.0
            },
            recent,
        )
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
        .filter(|(_, spaceships)| !spaceships.is_empty())
        .map(|(player, spaceships)| {
            format!(
                "*{}*:\n{}",
                replace_all(player.name()),
                iter_spaceships(spaceships, recent)
                    .map(|s| {
                        let delta = s.calc_time(&msg.date);
                        let delta = if delta.is_empty() {
                            delta
                        } else {
                            format!(" {} left", delta)
                        };
                        format!(
                            "{} \\({}\\) {} {}{delta}",
                            replace_all(s.name()),
                            s.duration_type(),
                            replace_all(&timestamp_to_string(s.land())),
                            return_tf_emoji(s.notified())
                        )
                    })
                    .join("\n")
            )
        })
        .join("\n\n");

    if text.is_empty() {
        bot.send_message(
            msg.chat.id,
            if recent {
                "Recent land mission is empty, try use \\/missions command to check all missions\\."
            } else {
                "Missions is empty, try again later\\."
            },
        )
        .await?;
        return Ok(());
    }

    bot.send_message(msg.chat.id, text).await?;

    Ok(())
}

pub(super) mod prelude {
    pub(in crate::bot) use super::{
        handle_add_command, handle_delete_command, handle_list_command, handle_missions_command,
    };
}
