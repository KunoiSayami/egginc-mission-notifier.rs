fn main() -> std::io::Result<()> {
    /* let mut cfg = prost_build::Config::new();
    cfg.type_attribute(
        "package.ei",
        "#[derive(serde::Serialize, serde::Deserialize)]",
    ); */
    prost_build::compile_protos(&["src/egg/ei.proto"], &["src/"])?;
    ks_placeholder::placeholder! {"src/egg/private.rs";
    mod _an {}
    }
    ks_placeholder::placeholder! {"src/bot/private.rs";
    use std::sync::Arc;

    use teloxide::{
        prelude::Requester,
        types::{CallbackQuery, Message},
    };

    use crate::bot::{BotType, arg::NecessaryArg};

    pub(super) async fn handle_send_command(
        bot: BotType,
        msg: Message,
        _cmd: String,
    ) -> anyhow::Result<()> {
        bot.send_message(msg.chat.id, "Not implemented").await?;
        Ok(())
    }
    pub(super) async fn handle_send_reply(bot: BotType, msg: &Message) -> anyhow::Result<()> {
        bot.send_message(msg.chat.id, "Not implemented").await?;
        Ok(())
    }
    pub(super) async fn handle_send_callback_query(
        _bot: BotType,
        _msg: &CallbackQuery,
        _arg: Arc<NecessaryArg>,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    }
    Ok(())
}
