use anicca_subscribe::anicca::Anicca;
use deadpool_sqlite::Pool;
use eyre::Result;
use matrix_sdk::ruma::UserId;
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
) -> Result<(String, Option<String>)> {
    if args.is_empty() {
        return Ok((
            "No command provided. Type `!anic help` for available commands.".to_owned(),
            Some(
                "No command provided. Type <code>!anic help</code> for available commands."
                    .to_owned(),
            ),
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
                                <code>!anic version</code> - Show the application version
                                <code>!anic enable-notification</code> - Enable hourly notification
                                <code>!anic disable-notification</code> - Disable hourly notification";
            let plain_help_message = html_help_message
                .replace("<code>", "`")
                .replace("</code>", "`")
                .replace("&lt;", "<")
                .replace("&gt;", ">")
                .replace("<br/>", "\n");
            Ok((plain_help_message, Some(html_help_message.to_owned())))
        }
        "version" => {
            let version = env!("CARGO_PKG_VERSION");
            Ok((version.to_owned(), None))
        }
        "ping" => Ok(("pong".to_string(), None)),
        "list" => {
            let packages = get_packages(user_id, pool).await?;
            if packages.is_empty() {
                Ok(("No package subscribed.".to_owned(), None))
            } else {
                let package_list = packages.join(", ");
                Ok((
                    format!(
                        "Subscribed {} package{}: {}",
                        packages.len(),
                        if packages.len() >= 2 { "s" } else { "" },
                        package_list
                    ),
                    None,
                ))
            }
        }
        "subscribe" => {
            if args.len() < 2 {
                return Ok((
                    "Usage: `!anic subscribe <packages>`".to_owned(),
                    Some("Usage: <code>!anic subscribe &lt;packages&gt;</code>".to_owned()),
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
            Ok(("Subscribed.".to_owned(), None))
        }
        "unsubscribe" => {
            if args.len() < 2 {
                return Ok((
                    "Usage: `!anic unsubscribe <packages>`".to_owned(),
                    Some("Usage: <code>!anic unsubscribe &lt;packages&gt;</code>".to_owned()),
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
            Ok(("Unsubscribed.".to_owned(), None))
        }
        "updates" => {
            let packages = get_packages(user_id, pool).await?;
            let mut updates = Anicca::get_local_json(data_dir)
                .await?
                .get_subscription_updates(&packages)?;
            if updates.is_empty() {
                Ok(("No package update found.".to_owned(), None))
            } else {
                Ok((format_update_packages(&mut updates), None))
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
                return Ok(("Hourly notification already enabled.".to_owned(), None));
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
            Ok(("Enabled hourly notification.".to_owned(), None))
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
            Ok(("Hourly notification disabled.".to_owned(), None))
        }
        _ => Ok((format!("Unknown command: {}", args[0]), None)),
    }
}
