# Egg, Inc. Mission Notifier

This project is help user got notification when rocket is land in [Egg, Inc.](https://en.wikipedia.org/wiki/Egg,_Inc.)

## Setup

### Requirements

* Rust build environment
* Telegram bot token

### Build

```bash
$ cargo build --release
```

### Configure

A configure file named `config.toml` (or specify filename in argument) should be located in directory.

Here is a sample configure file:
```toml
# Administrators group, can use admin command
admin = [114514191]
[telegram]
# Telegram bot token
api_key = "1145141919:BAABAABAA"
# Custom telegram bot API server
#api-server = "http://localhost:8081"
# Bot username
username = "egg_bot"
```

### Run

Use `cargo run --release` to run this bot, you can specify environment variable `RUST_LOG=debug` to show more logs


## License

[![](https://www.gnu.org/graphics/agplv3-155x51.png "AGPL v3 logo")](https://www.gnu.org/licenses/agpl-3.0.txt)

Copyright (C) 2024-2025 KunoiSayami

This program is free software: you can redistribute it and/or modify it under the terms of the GNU Affero General Public License as published by the Free Software Foundation, either version 3 of the License, or any later version.

This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more details.

You should have received a copy of the GNU Affero General Public License along with this program. If not, see <https://www.gnu.org/licenses/>.
