mod bot;
mod config;
mod database;
mod egg;
mod types;

use bot::{bot, bot_run};
use clap::arg;
use config::Config;
use database::DatabaseHandle;
use egg::monitor::Monitor;

const FETCH_PERIOD: i64 = 1800;
const CHECK_PERIOD: i64 = 240;

//const STATIC_DATA: &[u8] = include_bytes!("../out1.data");

async fn async_main(config_file: &String) -> anyhow::Result<()> {
    let config = Config::read(&config_file).await?;
    let (database_thread, database_helper) = DatabaseHandle::connect("spaceship.db").await?;

    let bot = bot(&config)?;

    let (monitor, monitor_helper) = Monitor::create(database_helper.clone(), bot.clone());

    bot_run(bot, config, database_helper.clone(), monitor_helper.clone()).await?;

    monitor_helper.exit().await;
    database_helper.terminate().await;

    monitor.join().await?;
    database_thread.wait().await?;

    Ok(())
}

fn enable_log(verbose: u8) {
    let mut builder = env_logger::Builder::from_default_env();
    if verbose < 3 {
        builder
            .filter_module("tracing", log::LevelFilter::Warn)
            .filter_module("hyper", log::LevelFilter::Warn)
            .filter_module("reqwest", log::LevelFilter::Warn);
    }

    if verbose < 2 {
        builder.filter_module("teloxide", log::LevelFilter::Debug);
    }
    if verbose < 1 {
        builder.filter_module("sqlx", log::LevelFilter::Warn);
    }
    builder.init();
}

fn main() -> anyhow::Result<()> {
    let matches = clap::command!()
        .args(&[
            arg!([CONFIG] "Configure file to read").default_value("config.toml"),
            arg!(-v --verbose ... "More verbose output"),
        ])
        .get_matches();

    enable_log(matches.get_count("verbose"));

    log::info!("Version: {}", env!("CARGO_PKG_VERSION"));

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async_main(matches.get_one("CONFIG").unwrap()))
}
