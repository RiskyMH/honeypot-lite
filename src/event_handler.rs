use crate::{CTX, config::save_guild_config};
use twilight_gateway::Event;
use twilight_http::request::AuditLogReason;
use twilight_model::{
    application::{
        command::{Command, CommandType},
        interaction::{
            InteractionData,
            application_command::{CommandDataOption, CommandOptionValue},
        },
    },
    channel::ChannelType,
    guild::Permissions,
    http::interaction::{InteractionResponse, InteractionResponseData, InteractionResponseType},
};
use twilight_util::builder::command::{ChannelBuilder, CommandBuilder, StringBuilder};

use crate::config::{ActionType, GuildConfigBody};

pub async fn event_handler(event: Event, _state: ()) {
    #[allow(clippy::single_match)]
    match event {
        Event::InteractionCreate(interaction) => {
            if interaction.guild_id.is_none() {
                return;
            }
            let InteractionData::ApplicationCommand(data) = interaction.data.as_ref().unwrap()
            else {
                unreachable!();
            };
            let kind: &str = data.name.as_str();
            let guild_id: u64 = interaction.guild_id.unwrap().into();

            if kind == "honeypot-set" {
                let channel_id = if let CommandDataOption {
                    value: CommandOptionValue::Channel(channel_id),
                    ..
                } = data
                    .options
                    .iter()
                    .find(|option| option.name == "channel")
                    .unwrap()
                {
                    *channel_id
                } else {
                    unreachable!();
                };
                let action = if let CommandDataOption {
                    value: CommandOptionValue::String(action),
                    ..
                } = data
                    .options
                    .iter()
                    .find(|option| option.name == "type")
                    .unwrap()
                {
                    action.as_str()
                } else {
                    unreachable!();
                };

                let action_type = action.parse::<ActionType>().unwrap_or(ActionType::Ban);
                {
                    let mut guild_config = CTX.guild_config.lock().unwrap();
                    if action_type == ActionType::Disabled {
                        guild_config.remove(&guild_id);
                    } else {
                        guild_config.insert(
                            guild_id,
                            GuildConfigBody {
                                channel: channel_id.into(),
                                action_type,
                            },
                        );
                    }
                    save_guild_config(&CTX.guild_config_file_path, &guild_config.clone()).unwrap();
                }

                let response = InteractionResponse {
                    kind: InteractionResponseType::ChannelMessageWithSource,
                    data: Some(InteractionResponseData {
                        content: Some("Honeypot configuration updated.".to_string()),
                        ..InteractionResponseData::default()
                    }),
                };

                let _ = CTX
                    .interaction()
                    .create_response(interaction.id, &interaction.token, &response)
                    .await;
            }
        }
        Event::ChannelDelete(channel) => {
            let mut guild_config = CTX.guild_config.lock().unwrap();
            let guild_ids_to_remove: Vec<_> = guild_config
                .iter()
                .filter(|(_, config)| config.channel == channel.id)
                .map(|(guild_id, _)| *guild_id)
                .collect();
            for guild_id in guild_ids_to_remove {
                guild_config.remove(&guild_id);
            }
            save_guild_config(&CTX.guild_config_file_path, &guild_config.clone()).unwrap();
        }
        Event::GuildDelete(guild) => {
            let mut guild_config = CTX.guild_config.lock().unwrap();
            guild_config.remove(&guild.id.into());
            save_guild_config(&CTX.guild_config_file_path, &guild_config.clone()).unwrap();
        }
        Event::MessageCreate(message) => {
            if message.author.bot {
                return;
            }

            // check if msg is in a honeypot channel
            let guild_id = if let Some(guild_id) = message.guild_id {
                guild_id
            } else {
                return;
            };

            let channel_id = message.channel_id;

            let config = {
                let guild_config = CTX.guild_config.lock().unwrap();
                guild_config.get(&guild_id.into()).cloned()
            };

            let config = if let Some(config) = config {
                config
            } else {
                return;
            };

            if config.channel != channel_id {
                return;
            }

            const DELETE_MESSAGE_SECONDS: u32 = 3600; // 1hr
            match config.action_type {
                ActionType::Ban => {
                    let res = CTX
                        .http
                        .create_ban(guild_id, message.author.id)
                        .delete_message_seconds(DELETE_MESSAGE_SECONDS)
                        .reason("User typed in #honeypot channel -> ban")
                        .await;
                    if let Err(error) = res {
                        tracing::warn!("failed to ban user: {:?}", error);
                    }
                }
                ActionType::Softban => {
                    let res = CTX
                        .http
                        .create_ban(guild_id, message.author.id)
                        .delete_message_seconds(DELETE_MESSAGE_SECONDS)
                        .reason("User typed in #honeypot channel -> softban (1/2)")
                        .await;
                    if let Err(error) = res {
                        tracing::warn!("failed to softban user: {:?}", error);
                        return;
                    }

                    let res = CTX
                        .http
                        .delete_ban(guild_id, message.author.id)
                        .reason("User typed in #honeypot channel -> softban (2/2)")
                        .await;
                    if let Err(error) = res {
                        tracing::warn!("failed to delete ban for softban: {:?}", error);
                    }
                }
                ActionType::Disabled => {}
            }
        }
        Event::Ready(ready) => {
            tracing::info!(
                "[shard {}] {}#{} is ready",
                ready.shard.map(|s| s.number()).unwrap_or(0),
                ready.user.name,
                ready.user.discriminator
            );
        }
        _ => {
            tracing::info!("unhandled event: {:?}", event.kind());
        }
    }
}

pub fn global_commands() -> Vec<Command> {
    return vec![
        CommandBuilder::new(
            "honeypot-set",
            "Set/update honeypot channel",
            CommandType::ChatInput,
        )
        .default_member_permissions(
            Permissions::BAN_MEMBERS | Permissions::MANAGE_GUILD | Permissions::MANAGE_CHANNELS,
        )
        .option(
            ChannelBuilder::new("channel", "The channel to ban people that message in it")
                .required(true)
                .channel_types([ChannelType::GuildText]),
        )
        .option(
            StringBuilder::new(
                "type",
                "The action to take when someone messages in the honeypot channel",
            )
            .required(true)
            .choices([
                ("Ban", "ban"),
                ("Softban", "softban"),
                ("Disabled", "disabled"),
            ]),
        )
        .build(),
    ];
}
