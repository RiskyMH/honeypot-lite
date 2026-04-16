use crate::{CTX, config::save_guild_config};
use twilight_gateway::Event;
use twilight_http::request::AuditLogReason;
use twilight_model::{
    application::{
        command::{Command, CommandType},
        interaction::{
            InteractionContextType, InteractionData, application_command::CommandOptionValue,
        },
    },
    channel::ChannelType,
    guild::Permissions,
    http::interaction::{InteractionResponse, InteractionResponseData, InteractionResponseType},
    id::{Id, marker::UserMarker},
    oauth::ApplicationIntegrationType,
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
                panic!();
            };
            let kind: &str = data.name.as_str();
            let guild_id: u64 = interaction.guild_id.unwrap().into();

            if kind == "honeypot-set" {
                let channel_opt = data
                    .options
                    .iter()
                    .find(|option| option.name == "channel")
                    .unwrap();

                let CommandOptionValue::Channel(channel_id) = channel_opt.value else {
                    panic!();
                };

                let action_opt = data
                    .options
                    .iter()
                    .find(|option| option.name == "type")
                    .unwrap();

                let action = match &action_opt.value {
                    CommandOptionValue::String(action) => action.as_str(),
                    _ => panic!(),
                };

                let log_channel_id = data
                    .options
                    .iter()
                    .find(|option| option.name == "log_channel")
                    .map(|o| match o.value {
                        CommandOptionValue::Channel(log) => log,
                        _ => panic!(),
                    });

                let action_type = action.parse::<ActionType>().unwrap_or(ActionType::Ban);
                let guild_config_to_save = {
                    let mut guild_config = CTX.guild_config.write().await;
                    if action_type == ActionType::Disabled {
                        guild_config.remove(&guild_id);
                    } else {
                        guild_config.insert(
                            guild_id,
                            GuildConfigBody {
                                honeypot_channel: channel_id.into(),
                                log_channel: log_channel_id.map(|c| c.into()),
                                action_type,
                            },
                        );
                    }
                    guild_config.clone()
                };
                save_guild_config(&CTX.guild_config_file_path, &guild_config_to_save).unwrap();

                let response_content = if action_type == ActionType::Disabled {
                    Some(
                        "Honeypot configuration updated: Disabled honeypot for this server.".into(),
                    )
                } else {
                    Some(format!(
                        "Honeypot configuration updated: Will **{}** anyone who types in <#{}> {}.",
                        action_type.to_string(),
                        channel_id,
                        if let Some(log_channel_id) = log_channel_id {
                            format!("and log actions to <#{}>", log_channel_id)
                        } else {
                            String::from("and won't log actions")
                        }
                    ))
                };

                let response = InteractionResponse {
                    kind: InteractionResponseType::ChannelMessageWithSource,
                    data: Some(InteractionResponseData {
                        content: response_content,
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
            let guild_config_to_save = {
                let mut guild_config = CTX.guild_config.write().await;
                let guild_ids_to_remove: Vec<_> = guild_config
                    .iter()
                    .filter(|(_, config)| config.honeypot_channel == channel.id)
                    .map(|(guild_id, _)| *guild_id)
                    .collect();
                for guild_id in guild_ids_to_remove {
                    guild_config.remove(&guild_id);
                }
                guild_config.clone()
            };
            save_guild_config(&CTX.guild_config_file_path, &guild_config_to_save).unwrap();
        }
        Event::GuildDelete(guild) => {
            let guild_config_to_save = {
                let mut guild_config = CTX.guild_config.write().await;
                guild_config.remove(&guild.id.into());
                guild_config.clone()
            };
            save_guild_config(&CTX.guild_config_file_path, &guild_config_to_save).unwrap();
        }
        Event::MessageCreate(message) => {
            // bot messages dont matter, except for interactions from *other* bots where we count the invoker as msg author
            if message.author.bot
                && (message.interaction_metadata.is_none()
                    || message.application_id == Some(CTX.application_id))
            {
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
                let guild_config = CTX.guild_config.read().await;
                guild_config.get(&guild_id.into()).cloned()
            };

            let config = if let Some(config) = config {
                config
            } else {
                return;
            };

            if config.honeypot_channel != channel_id {
                return;
            }

            let user_id: Id<UserMarker> = message
                .interaction_metadata
                .as_ref()
                .map_or(message.author.id, |m| m.user.id);

            const DELETE_MESSAGE_SECONDS: u32 = 3600; // 1hr
            let mut failed = false;
            match config.action_type {
                ActionType::Ban => {
                    let res = CTX
                        .http
                        .create_ban(guild_id, user_id)
                        .delete_message_seconds(DELETE_MESSAGE_SECONDS)
                        .reason("User typed in #honeypot channel -> ban")
                        .await;

                    if let Err(error) = res {
                        tracing::warn!(
                            "Failed to ban user: {}",
                            twilight_http_error_string(&error)
                        );
                    }
                }
                ActionType::Softban => {
                    let res = CTX
                        .http
                        .create_ban(guild_id, user_id)
                        .delete_message_seconds(DELETE_MESSAGE_SECONDS)
                        .reason("User typed in #honeypot channel -> softban (1/2)")
                        .await;
                    if let Err(error) = res {
                        tracing::warn!(
                            "Failed to softban user: {}",
                            twilight_http_error_string(&error)
                        );
                        failed = true;
                    } else {
                        let res: Result<
                            twilight_http::Response<twilight_http::response::marker::EmptyBody>,
                            twilight_http::Error,
                        > = CTX
                            .http
                            .delete_ban(guild_id, user_id)
                            .reason("User typed in #honeypot channel -> softban (2/2)")
                            .await;
                        if let Err(error) = res {
                            tracing::warn!(
                                "Failed to delete ban for softban: {}",
                                twilight_http_error_string(&error)
                            );
                        }
                    }
                }
                ActionType::Disabled => return,
            }

            if failed {
                let action_name = match config.action_type {
                    ActionType::Ban => "ban",
                    ActionType::Softban => "softban",
                    ActionType::Disabled => "do nothing to",
                };

                let channel_id = if let Some(log_channel) = config.log_channel {
                    Id::new(log_channel)
                } else {
                    message.channel_id
                };

                let res = CTX.http.create_message(channel_id)
                    .content(&format!(
                        "User <@{}> triggered the honeypot but I **failed** to {} them, please check my permissions to ensure I can {} them.",
                        user_id, action_name, action_name
                    ))
                    .allowed_mentions(None)
                    .await;
                if let Err(error) = res {
                    tracing::warn!(
                        "Failed to create error message (due to {} fail): {}",
                        action_name,
                        twilight_http_error_string(&error)
                    );
                }
                return;
            } else if let Some(log_channel) = config.log_channel {
                let action_name = match config.action_type {
                    ActionType::Ban => "banned",
                    ActionType::Softban => "softbanned",
                    ActionType::Disabled => "nothinged?",
                };

                let res = CTX
                    .http
                    .create_message(Id::new(log_channel))
                    .content(&format!(
                        "User <@{}> was {} for triggering the honeypot in <#{}>",
                        user_id, action_name, channel_id
                    ))
                    .allowed_mentions(None)
                    .await;
                if let Err(error) = res {
                    tracing::warn!(
                        "Failed to create log message for {}: {}",
                        action_name,
                        twilight_http_error_string(&error)
                    );
                }
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
            "Set/update honeypot channel (note: this overrides previous config set)",
            CommandType::ChatInput,
        )
        .default_member_permissions(
            Permissions::BAN_MEMBERS | Permissions::MANAGE_GUILD | Permissions::MANAGE_CHANNELS,
        )
        .integration_types([ApplicationIntegrationType::GuildInstall])
        .contexts([InteractionContextType::Guild])
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
        .option(
            ChannelBuilder::new(
                "log_channel",
                "The channel to log actions in (if ommited, then it won't log anywhere)",
            )
            .required(false)
            .channel_types([
                ChannelType::GuildText,
                ChannelType::PublicThread,
                ChannelType::PrivateThread,
            ]),
        )
        .build(),
    ];
}

fn twilight_http_error_string(error: &twilight_http::Error) -> String {
    match error.kind() {
        twilight_http::error::ErrorType::Response {
            body,
            status,
            error: api_error,
        } => match &api_error {
            twilight_http::api_error::ApiError::General(g) => {
                format!("DiscordApiError[{}]: {}", g.code, g.message)
            }
            _ => {
                format!(
                    "DiscordApiError[{}]: {}",
                    status,
                    String::from_utf8_lossy(body),
                )
            }
        },
        other => {
            format!("TwilightHttpError: {:?}", other)
        }
    }
}
