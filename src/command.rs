use anicca_subscribe::anicca::Anicca;
use deadpool_sqlite::Pool;
use eyre::Result;
use matrix_sdk::ruma::{
    UserId,
    events::room::message::{FormattedBody, RoomMessageEventContent},
};
use std::path::Path;

use crate::bot::{format_update_packages, get_packages};

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
    args: &[String],
    data_dir: &Path,
    user_id: &UserId,
    pool: Pool,
) -> Result<RoomMessageEventContent> {
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
            Ok(RoomMessageEventContent::notice_html(
                version.to_owned(),
                format!(
                    "<a href=\"https://factoria.origincode.me/OriginCode/anicca-subscribe-matrix/-/tree/v{version}?ref_type=tags\">{version}</a>"
                ),
            ))
        }
        "changelog" => {
            let changelog = include_str!("../CHANGELOG.md");
            Ok(RoomMessageEventContent::notice_html(
                changelog.to_owned(),
                FormattedBody::markdown(format!(
                    "<details><summary>Click to see the changelog</summary>{changelog}</details>",
                ))
                .unwrap()
                .body,
            ))
        }
        "ping" => Ok(RoomMessageEventContent::notice_plain("pong".to_string())),
        "list" => {
            let packages = get_packages(user_id, pool).await?;
            if packages.is_empty() {
                Ok(RoomMessageEventContent::notice_plain(
                    "No package subscribed.".to_owned(),
                ))
            } else {
                let package_list = packages.join(", ");
                Ok(RoomMessageEventContent::notice_plain(format!(
                    "Subscribed {} package{}: {}",
                    packages.len(),
                    if packages.len() >= 2 { "s" } else { "" },
                    package_list
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
            let db_conn = pool.get().await?;
            let user_id_str = user_id.to_string();
            db_conn
                .interact(move |db_conn| {
                    let mut stmt = db_conn
                        .prepare("INSERT INTO subscription (user_id, package) VALUES (?1, ?2)")?;
                    for package in packages {
                        stmt.execute([&user_id_str, &package])?;
                    }
                    Ok::<(), rusqlite::Error>(())
                })
                .await
                .unwrap()?;
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
            let db_conn = pool.get().await?;
            let user_id_str = user_id.to_string();
            db_conn
                .interact(move |db_conn| {
                    let mut stmt = db_conn
                        .prepare("DELETE FROM subscription WHERE user_id = ?1 AND package = ?2")?;
                    for package in packages {
                        stmt.execute([&user_id_str, &package])?;
                    }
                    Ok::<(), rusqlite::Error>(())
                })
                .await
                .unwrap()?;
            Ok(RoomMessageEventContent::notice_plain(
                "Unsubscribed.".to_owned(),
            ))
        }
        "updates" => {
            let packages = get_packages(user_id, pool).await?;
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
            let db_conn = pool.get().await?;
            let user_id_str = user_id.as_str().to_owned();
            let count: i32 = db_conn
                .interact(move |db_conn| {
                    let mut stmt =
                        db_conn.prepare("SELECT COUNT(*) FROM notification WHERE user_id = ?1")?;
                    stmt.query_row([&user_id_str], |row| row.get(0))
                })
                .await
                .unwrap()?;
            if count > 0 {
                return Ok(RoomMessageEventContent::notice_plain(
                    "Hourly notification already enabled.".to_owned(),
                ));
            }
            let user_id_str = user_id.to_string();
            db_conn
                .interact(move |db_conn| {
                    db_conn.execute(
                        "INSERT INTO notification (user_id) VALUES (?1)",
                        [&user_id_str],
                    )
                })
                .await
                .unwrap()?;
            Ok(RoomMessageEventContent::notice_plain(
                "Enabled hourly notification.".to_owned(),
            ))
        }
        "disable-notification" => {
            let db_conn = pool.get().await?;
            let user_id_str = user_id.to_string();
            db_conn
                .interact(move |db_conn| {
                    db_conn.execute(
                        "DELETE FROM notification WHERE user_id = ?1",
                        [&user_id_str],
                    )
                })
                .await
                .unwrap()?;
            Ok(RoomMessageEventContent::notice_plain(
                "Hourly notification disabled.".to_owned(),
            ))
        }
        _ => Ok(RoomMessageEventContent::notice_plain(format!(
            "Unknown command: {}",
            args[0]
        ))),
    }
}
