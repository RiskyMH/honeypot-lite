use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionType {
    Ban,
    Softban,
    Disabled,
}

use std::fmt;
use std::str::FromStr;

impl fmt::Display for ActionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            ActionType::Ban => "ban",
            ActionType::Softban => "softban",
            ActionType::Disabled => "disabled",
        };
        write!(f, "{}", s)
    }
}

impl FromStr for ActionType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "ban" => Ok(ActionType::Ban),
            "softban" => Ok(ActionType::Softban),
            "disabled" => Ok(ActionType::Disabled),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct GuildConfigBody {
    pub honeypot_channel: u64,
    pub log_channel: Option<u64>,
    pub action_type: ActionType,
}
pub type GuildConfigMap = HashMap<u64, GuildConfigBody>;

// save via csv:
// guild_id,honeypot_channel_id,log_channel_id,action_type
// 1234567890123456789,234567890123456789,345678901234567890,ban
// 1234567890123456789,234567890123456789,,softban

pub fn load_guild_config(file: &str) -> GuildConfigMap {
    let mut guild_config = GuildConfigMap::new();

    if let Ok(contents) = std::fs::read_to_string(file) {
        for line in contents.lines() {
            let parts: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
            if parts.len() == 4 {
                if parts[0] == "guild_id" {
                    continue; // skip header
                }
                if let (Ok(guild_id), Ok(honeypot_channel)) =
                    (parts[0].parse::<u64>(), parts[1].parse::<u64>())
                {
                    let log_channel: Option<u64> = if parts[2].is_empty() {
                        None
                    } else {
                        match parts[2].parse::<u64>() {
                            Ok(id) => Some(id),
                            Err(_) => None,
                        }
                    };
                    let action_type = match parts[3].parse::<ActionType>() {
                        Ok(action_type) => action_type,
                        Err(_) => continue,
                    };
                    guild_config.insert(
                        guild_id,
                        GuildConfigBody {
                            honeypot_channel,
                            log_channel,
                            action_type,
                        },
                    );
                } else {
                    tracing::info!("skipping invalid line in config file: {}", line);
                }
            }
        }
    }

    guild_config
}

pub fn save_guild_config(file: &str, guild_config: &GuildConfigMap) -> std::io::Result<()> {
    let mut contents = String::from("guild_id,honeypot_channel_id,log_channel_id,action_type\n");
    for (guild_id, config) in guild_config {
        if config.action_type == ActionType::Disabled {
            continue; // skip disabled configs
        }
        let log_channel = config
            .log_channel
            .map(|c| c.to_string())
            .unwrap_or_else(String::new);
        contents.push_str(&format!(
            "{},{},{},{}\n",
            guild_id, config.honeypot_channel, log_channel, config.action_type
        ));
    }
    std::fs::write(file, contents)
}
