mod config;
mod context;
mod dispatch;
mod event_handler;
mod resume;

use crate::config::GuildConfigMap;

pub use self::{
    context::CTX,
    dispatch::{ShardHandle, ShardRestartKind},
    resume::{ConfigBuilderExt, Info as ResumeInfo},
};

use dashmap::DashMap;
use std::sync::Mutex;
use std::{env, time::Duration};
use tokio::signal;
use twilight_gateway::{ConfigBuilder, EventTypeFlags, Intents, queue::InMemoryQueue};
use twilight_http::Client;

#[rustfmt::skip]
const EVENT_TYPES: EventTypeFlags = EventTypeFlags::empty().union(EventTypeFlags::INTERACTION_CREATE).union(EventTypeFlags::MESSAGE_CREATE).union(EventTypeFlags::CHANNEL_DELETE).union(EventTypeFlags::GUILD_DELETE).union(EventTypeFlags::READY);
const INTENTS: Intents = Intents::GUILDS.union(Intents::GUILD_MESSAGES);

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let token = env::var("DISCORD_TOKEN")?;
    let config_file_path =
        env::var("CONFIG_FILE_PATH").unwrap_or_else(|_| ".guild_config.csv".into());

    let http = Client::new(token.clone());
    let app = async { anyhow::Ok(http.current_user_application().await?.model().await?) }.await?;
    let info = async { anyhow::Ok(http.gateway().authed().await?.model().await?) }.await?;
    async {
        http.interaction(app.id)
            .set_global_commands(&event_handler::global_commands())
            .await?;
        anyhow::Ok(())
    }
    .await?;
    let shards = DashMap::new();
    let guild_config: GuildConfigMap = config::load_guild_config(&config_file_path);
    context::init(
        app.id,
        http,
        shards,
        Mutex::new(guild_config),
        config_file_path,
    );

    // The queue defaults are static and may be incorrect for large or newly
    // restarted bots.
    let queue = InMemoryQueue::new(
        info.session_start_limit.max_concurrency,
        info.session_start_limit.remaining,
        Duration::from_millis(info.session_start_limit.reset_after),
        info.session_start_limit.total,
    );
    let config = ConfigBuilder::new(token, INTENTS).queue(queue).build();

    let shards = resume::restore(config, info.shards).await;

    let tasks = shards
        .into_iter()
        .map(|shard| {
            tokio::spawn(dispatch::run(
                event_handler::event_handler,
                shard,
                |_shard| (),
            ))
        })
        .collect::<Vec<_>>();

    signal::ctrl_c().await?;
    tracing::info!("shutting down; press CTRL-C to abort");

    let join_all_tasks = async {
        let mut resume_info = Vec::with_capacity(tasks.len());
        for task in tasks {
            resume_info.push(task.await?);
        }
        anyhow::Ok(resume_info)
    };
    let resume_info = tokio::select! {
        _ = signal::ctrl_c() => Vec::new(),
        resume_info = join_all_tasks => resume_info?,
    };

    // Save shard information to be restored.
    resume::save(&resume_info).await?;

    Ok(())
}
