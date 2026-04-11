use crate::{ShardHandle, config::GuildConfigMap};
use dashmap::DashMap;
use tokio::sync::RwLock;
use std::{
    ops::Deref,
    sync::{OnceLock},
};
use twilight_http::{Client, client::InteractionClient};
use twilight_model::id::{Id, marker::ApplicationMarker};

pub static CTX: Ref = Ref(OnceLock::new());

#[derive(Debug)]
pub struct Context {
    pub application_id: Id<ApplicationMarker>,
    pub http: Client,
    pub shards: DashMap<u32, ShardHandle>,
    pub guild_config: RwLock<GuildConfigMap>,
    pub guild_config_file_path: String,
}

impl Context {
    pub fn interaction(&self) -> InteractionClient<'_> {
        self.http.interaction(self.application_id)
    }
}

pub fn init(
    application_id: Id<ApplicationMarker>,
    http: Client,
    shards: DashMap<u32, ShardHandle>,
    guild_config: RwLock<GuildConfigMap>,
    guild_config_file_path: String,
) {
    let context = Context {
        application_id,
        http,
        shards,
        guild_config,
        guild_config_file_path,
    };
    assert!(CTX.0.set(context).is_ok());
}

pub struct Ref(OnceLock<Context>);

impl Deref for Ref {
    type Target = Context;

    fn deref(&self) -> &Self::Target {
        self.0.get().unwrap()
    }
}
