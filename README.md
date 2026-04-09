# Honeypot (lite) Discord Bot 

A more light weight version of my normal Honeypot bot (https://github.com/RiskyMH/honeypot)

## Usage

1. [Invite the bot](https://discord.com/api/oauth2/authorize?client_id=1491864770490532061) to your server with appropriate permissions (Ban Members).
2. Set the channel you want to monitor with `/honeypot-set`.
3. Ensure the bot’s highest role is above any self-assignable (color/ping) roles.
4. Any user posting in the honeypot channel will be banned or softbanned (configurable).

## Getting Started (dev)

```sh
$ cargo build && DISCORD_TOKEN=your_token ./target/debug/honeypot-lite
```

## Deployment (production)

```sh
$ cargo build --release && DISCORD_TOKEN=your_token ./target/release/honeypot-lite

# or docker
$ docker compose build && DISCORD_TOKEN=your_token docker compose up
```
