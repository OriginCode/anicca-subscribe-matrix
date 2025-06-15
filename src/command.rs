use anicca_subscribe::anicca::Anicca;
use eyre::Result;
use matrix_sdk::{
    Room,
    ruma::{
        UserId,
        events::room::message::{FormattedBody, RoomMessageEventContent},
    },
};
use pluralizer::pluralize;
use std::path::Path;

use crate::{bot::format_update_packages, config::Config, db::*};

pub const COMMAND_PREFIX: &str = "!anic";

pub fn parse_prefix_and_args(
    is_direct: bool,
    text: &str,
    bot_user_id: Option<&UserId>,
    display_name: &str,
) -> Option<Vec<String>> {
    let mut parsed_args = parse_args(text);
    let prefix = parsed_args.first().map(|x| x.as_str());
    if prefix == Some(COMMAND_PREFIX)
        || prefix == bot_user_id.map(|x| x.as_str())
        || prefix == bot_user_id.map(|x| format!("{}:", x)).as_deref()
    {
        Some(parsed_args.drain(1..).collect())
    } else if text.starts_with(&(display_name.to_owned() + ": ")) {
        Some(parse_args(
            text.strip_prefix(&(display_name.to_owned() + ": "))
                .unwrap(),
        ))
    } else if is_direct {
        Some(parsed_args)
    } else {
        None
    }
}

pub fn parse_args(text: &str) -> Vec<String> {
    text.split_whitespace().map(|s| s.to_owned()).collect()
}

pub async fn handle(
    config: Config,
    data_dir: &Path,
    db: DatabaseImpl,
    user_id: &UserId,
    room: Room,
    args: &[String],
) -> Result<RoomMessageEventContent> {
    let unknown_command =
        RoomMessageEventContent::notice_plain(format!("Unknown command: {}", args[0]));

    if args.is_empty() {
        return Ok(RoomMessageEventContent::notice_html(
            "No command provided. Type `!anic help` for available commands.".to_owned(),
            "No command provided. Type <code>!anic help</code> for available commands.".to_owned(),
        ));
    }

    match args[0].as_str() {
        "help" => {
            let html_help_message = "Available commands:<br/>\
                                Prefixing commands with <code>!anic</code> is not required for direct messages.<br/>\
                                <code>!anic help</code> - Show this help message<br/>\
                                <code>!anic list</code> - List subscribed packages<br/>\
                                <code>!anic subscribe &lt;packages&gt;</code> - Subscribe to packages<br/>\
                                <code>!anic unsubscribe &lt;packages&gt;</code> - Unsubscribe from packages<br/>\
                                <code>!anic updates</code> - Show package updates<br/>\
                                <code>!anic enable-notification</code> - Enable hourly notification<br/>\
                                <code>!anic disable-notification</code> - Disable hourly notification<br/>\
                                <code>!anic version</code> - Show the bot version<br/>\
                                <code>!anic changelog</code> - Show the bot changelog";
            let plain_help_message = html_help_message
                .replace("<code>", "`")
                .replace("</code>", "`")
                .replace("&lt;", "<")
                .replace("&gt;", ">")
                .replace("<br/>", "\n");
            Ok(RoomMessageEventContent::notice_html(
                plain_help_message,
                html_help_message.to_owned(),
            ))
        }
        "version" => {
            let version = env!("CARGO_PKG_VERSION");
            #[cfg(feature = "sqlite")]
            let backend = "SQLite";
            #[cfg(feature = "rocksdb")]
            let backend = "RocksDB";
            Ok(RoomMessageEventContent::notice_html(
                version.to_owned(),
                format!(
                    "<a href=\"https://factoria.origincode.me/OriginCode/anicca-subscribe-matrix/-/tree/v{version}?ref_type=tags\">{version}</a> ({backend} backend)",
                ),
            ))
        }
        "changelog" => {
            let changelog = include_str!("../CHANGELOG.md");
            Ok(RoomMessageEventContent::notice_html(
                changelog.to_owned(),
                format!(
                    "<details><summary>Click to see the changelog</summary>{}</details>",
                    FormattedBody::markdown(changelog).unwrap().body
                ),
            ))
        }
        "ping" => Ok(RoomMessageEventContent::notice_plain("pong".to_string())),
        "list" => {
            let packages = db.get_packages(user_id).await?;
            if packages.is_empty() {
                Ok(RoomMessageEventContent::notice_plain(
                    "No package subscribed.".to_owned(),
                ))
            } else {
                Ok(RoomMessageEventContent::notice_plain(format!(
                    "Subscribed {}: {}",
                    pluralize("package", packages.len() as isize, true),
                    packages.join(", ")
                )))
            }
        }
        "subscribe" => {
            if args.len() < 2 {
                return Ok(RoomMessageEventContent::notice_html(
                    "Usage: `!anic subscribe <packages>`".to_owned(),
                    "Usage: <code>!anic subscribe &lt;packages&gt;</code>".to_owned(),
                ));
            }
            let packages: Vec<String> = args[1..].to_vec();
            db.subscribe(user_id, packages).await?;
            Ok(RoomMessageEventContent::notice_plain(
                "Subscribed.".to_owned(),
            ))
        }
        "unsubscribe" => {
            if args.len() < 2 {
                return Ok(RoomMessageEventContent::notice_html(
                    "Usage: `!anic unsubscribe <packages>`".to_owned(),
                    "Usage: <code>!anic unsubscribe &lt;packages&gt;</code>".to_owned(),
                ));
            }
            let packages: Vec<String> = args[1..].to_vec();
            db.unsubscribe(user_id, packages).await?;
            Ok(RoomMessageEventContent::notice_plain(
                "Unsubscribed.".to_owned(),
            ))
        }
        "updates" => {
            let packages = db.get_packages(user_id).await?;
            let mut updates = Anicca::get_local_json(data_dir)
                .await?
                .get_subscription_updates(&packages)?;
            if updates.is_empty() {
                Ok(RoomMessageEventContent::notice_plain(
                    "No package update found.".to_owned(),
                ))
            } else {
                let (plain_updates, html_updates) = format_update_packages(&mut updates);
                Ok(RoomMessageEventContent::notice_html(
                    plain_updates,
                    html_updates,
                ))
            }
        }
        "enable-notification" => {
            if db.is_notification_enabled(user_id).await? {
                return Ok(RoomMessageEventContent::notice_plain(
                    "Hourly notification already enabled.".to_owned(),
                ));
            }
            db.enable_notification(user_id).await?;
            Ok(RoomMessageEventContent::notice_plain(
                "Enabled hourly notification.".to_owned(),
            ))
        }
        "disable-notification" => {
            db.disable_notification(user_id).await?;
            Ok(RoomMessageEventContent::notice_plain(
                "Hourly notification disabled.".to_owned(),
            ))
        }
        "+users" => {
            if room.is_direct().await? && config.is_admin(user_id) {
                let users = db.users().await?;
                let notification_targets = db.notification_targets().await?;
                Ok(RoomMessageEventContent::notice_plain(format!(
                    "{}: {}",
                    pluralize("user", users.len() as isize, true),
                    users
                        .iter()
                        .map(|id| {
                            if notification_targets.contains(id) {
                                format!("{} [✓]", id.as_str())
                            } else {
                                id.as_str().to_owned()
                            }
                        })
                        .collect::<Vec<String>>()
                        .join(", ")
                )))
            } else {
                Ok(unknown_command)
            }
        }
        "+list" => {
            if args.len() < 2 {
                return Ok(RoomMessageEventContent::notice_html(
                    "Usage: `!anic list-subscriptions [userid]`".to_owned(),
                    "Usage: <code>!anic list-subscriptions [userid]</code>".to_owned(),
                ));
            }

            if room.is_direct().await? && config.is_admin(user_id) {
                let packages = db.get_packages(&UserId::parse(&args[1])?).await?;
                if packages.is_empty() {
                    Ok(RoomMessageEventContent::notice_plain(
                        "No package subscribed.".to_owned(),
                    ))
                } else {
                    Ok(RoomMessageEventContent::notice_plain(format!(
                        "Subscribed {}: {}",
                        pluralize("package", packages.len() as isize, true),
                        packages.join(", ")
                    )))
                }
            } else {
                Ok(unknown_command)
            }
        }
        _ => Ok(unknown_command),
    }
}
