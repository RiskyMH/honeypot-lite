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
    pub channel: u64,
    pub action_type: ActionType,
}
pub type GuildConfigMap = HashMap<u64, GuildConfigBody>;

// save via csv:
// guild_id, channel_id, action_type
// 1234567890123456789, 234567890123456789, ban

pub fn load_guild_config(file: &str) -> GuildConfigMap {
    let mut guild_config = GuildConfigMap::new();

    if let Ok(contents) = std::fs::read_to_string(file) {
        for line in contents.lines() {
            let parts: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
            if parts.len() == 3 {
                if parts[0] == "guild_id" {
                    continue; // skip header
                }
                if let (Ok(guild_id), Ok(channel_id)) =
                    (parts[0].parse::<u64>(), parts[1].parse::<u64>())
                {
                    let action_type = match parts[2].parse::<ActionType>() {
                        Ok(action_type) => action_type,
                        Err(_) => continue,
                    };
                    guild_config.insert(
                        guild_id,
                        GuildConfigBody {
                            channel: channel_id,
                            action_type,
                        },
                    );
                }
            }
        }
    }

    guild_config
}

pub fn save_guild_config(file: &str, guild_config: &GuildConfigMap) -> std::io::Result<()> {
    let mut contents = String::from("guild_id,channel_id,action_type\n");
    for (guild_id, config) in guild_config {
        contents.push_str(&format!(
            "{},{},{}\n",
            guild_id, config.channel, config.action_type
        ));
    }
    std::fs::write(file, contents)
}
