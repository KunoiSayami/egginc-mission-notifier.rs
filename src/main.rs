mod bot;
mod config;
mod database;
mod egg;
mod types;

use std::sync::OnceLock;

use bot::{bot, bot_run};
use clap::arg;
use config::Config;
use database::DatabaseHandle;
use egg::monitor::Monitor;

static FETCH_PERIOD: OnceLock<i64> = OnceLock::new();
static CHECK_PERIOD: OnceLock<i64> = OnceLock::new();

//const STATIC_DATA: &[u8] = include_bytes!("../out1.data");

async fn async_main(config_file: &str) -> anyhow::Result<()> {
    let config = Config::read(config_file).await?;
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
            arg!(--"check-period" <second> "Override check period")
                .default_value("240")
                .value_parser(clap::value_parser!(i64)),
            arg!(--"fetch-period" <second> "Override minium fetch period per-use (second)")
                .long_help("Default set to 1800, set to long can reduce game server pressure")
                .default_value("1800")
                .value_parser(clap::value_parser!(i64)),
            arg!(-v --verbose ... "More verbose log output"),
        ])
        .get_matches();

    enable_log(matches.get_count("verbose"));

    FETCH_PERIOD
        .set(*matches.get_one::<i64>("fetch-period").unwrap())
        .unwrap();
    CHECK_PERIOD
        .set(*matches.get_one::<i64>("check-period").unwrap())
        .unwrap();

    log::info!(
        "Version: {}, fetch period: {}, check period: {}",
        env!("CARGO_PKG_VERSION"),
        FETCH_PERIOD.get().unwrap(),
        CHECK_PERIOD.get().unwrap()
    );

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async_main(matches.get_one::<String>("CONFIG").unwrap()))
}
