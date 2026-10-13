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
    channel::{ChannelType, message::MessageFlags},
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

                #[rustfmt::skip]
                let _permission_check = {
                    let mut errors: Vec<String> = vec![];

                    let Some(resolved_channels) = data.resolved.as_ref().map(|r| &r.channels) else { return };

                    let app_permissions = interaction.app_permissions.unwrap_or(Permissions::empty());
                    let member_permissions = interaction.member.as_ref().and_then(|m| m.permissions).unwrap_or(Permissions::empty());

                    if !app_permissions.contains(Permissions::BAN_MEMBERS) {
                        errors.push("I need the **Ban Members** permission to function.".into());
                    }
                    if !member_permissions.contains(Permissions::BAN_MEMBERS) {
                        errors.push("You need the **Ban Members** permission to use this command.".into());
                    }

                    if let Some(resolved_honeypot) = resolved_channels.get(&channel_id) {
                        if !resolved_honeypot.permissions.contains(Permissions::VIEW_CHANNEL) {
                            errors.push(format!("You need **View Channel** permission in <#{channel_id}>."));
                        }
                        if !resolved_honeypot.app_permissions.unwrap_or(Permissions::empty()).contains(Permissions::VIEW_CHANNEL) {
                            errors.push(format!("I need the **View Channel** permission in <#{channel_id}>."));
                        }
                    } else {
                        errors.push(format!("Could not resolve channel <#{channel_id}> (honeypot channel)."));
                    }

                    if let Some(log_id) = log_channel_id
                        && let Some(resolved_log) = resolved_channels.get(&log_id) {
                            if !resolved_log.permissions.contains(Permissions::VIEW_CHANNEL | Permissions::SEND_MESSAGES) {
                                errors.push(format!("You need **View Channel** and **Send Messages** in <#{log_id}> (log channel)."));
                            }
                            if !resolved_log.app_permissions.unwrap_or(Permissions::empty()).contains(Permissions::VIEW_CHANNEL | Permissions::SEND_MESSAGES) {
                                errors.push(format!("I need **View Channel** and **Send Messages** in <#{log_id}> (log channel)."));
                            }
                        }

                    if !errors.is_empty() {
                        let _ = CTX.interaction().create_response(interaction.id, &interaction.token, &InteractionResponse {
                            kind: InteractionResponseType::ChannelMessageWithSource,
                            data: Some(InteractionResponseData {
                                content: Some(format!("Cannot set honeypot:\n- {}\n-# No settings have been changed.", errors.join("\n- "))),
                                flags: Some(MessageFlags::EPHEMERAL),
                                ..InteractionResponseData::default()
                            }),
                        }).await;
                        return;
                    }
                };

                {
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
                    save_guild_config(&CTX.guild_config_file_path, &guild_config.downgrade())
                        .unwrap();
                };

                let response_content = if action_type == ActionType::Disabled {
                    Some(
                        "Honeypot configuration updated: Disabled honeypot for this server.".into(),
                    )
                } else {
                    Some(format!(
                        "Honeypot configuration updated: Will **{}** anyone who types in <#{}> {}.",
                        action_type,
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
            let mut guild_config = CTX.guild_config.write().await;
            guild_config.retain(|_guild_id, config| {
                if config.honeypot_channel == channel.id {
                    false
                } else {
                    if config.log_channel == Some(channel.id.into()) {
                        config.log_channel = None;
                    }
                    true
                }
            });
            save_guild_config(&CTX.guild_config_file_path, &guild_config.downgrade()).unwrap();
        }
        Event::GuildDelete(guild) => {
            let mut guild_config = CTX.guild_config.write().await;
            guild_config.remove(&guild.id.into());
            save_guild_config(&CTX.guild_config_file_path, &guild_config.downgrade()).unwrap();
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
            let Some(guild_id) = message.guild_id else {
                return;
            };
            let channel_id = message.channel_id;

            let config = {
                let guild_config = CTX.guild_config.read().await;
                guild_config.get(&guild_id.into()).cloned()
            };
            let Some(config) = config else {
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
                        let res = CTX
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

                let channel_id = config.log_channel.map_or(message.channel_id, Id::new);

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
                return;
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
    vec![
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
    ]
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
