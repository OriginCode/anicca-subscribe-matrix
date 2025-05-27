use anicca_subscribe::anicca::Anicca;
use eyre::Result;
use matrix_sdk::ruma::UserId;
use rusqlite::Connection;
use std::{path::Path, sync::Arc};
use tokio::sync::Mutex;

pub const COMMAND_PREFIX: &str = "!anic";

pub fn parse_args(text: &str) -> Vec<String> {
    text.split(' ').map(|s| s.to_owned()).collect()
}

async fn get_packages(user_id: &UserId, db_conn: Arc<Mutex<Connection>>) -> Result<Vec<String>> {
    let db_conn = db_conn.lock().await;
    let mut stmt = db_conn.prepare("SELECT package FROM subscription WHERE user_id = ?1")?;
    let rows = stmt.query_map([user_id.as_str()], |row| row.get(0))?;
    let mut packages: Vec<String> = Vec::new();
    for row in rows {
        packages.push(row?);
    }
    Ok(packages)
}

pub async fn handle(
    args: &[String],
    data_dir: &Path,
    user_id: &UserId,
    db_conn: Arc<Mutex<Connection>>,
) -> Result<(String, Option<String>)> {
    if args.len() <= 1 {
        return Ok((
            "No command provided. Type '> help' for available commands.".to_string(),
            None,
        ));
    }

    match args[1].as_str() {
        "help" => {
            let html_help_message = "Available commands:<br/>\
                                <code>!anic help</code> - Show this help message<br/>\
                                <code>!anic list</code> - List subscribed packages<br/>\
                                <code>!anic subscribe &lt;packages&gt;</code> - Subscribe to packages<br/>\
                                <code>!anic unsubscribe &lt;packages&gt;</code> - Unsubscribe from packages<br/>\
                                <code>!anic updates</code> - Show package updates<br/>\
                                <code>!anic version</code> - Show the application version";
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
            let packages = get_packages(user_id, db_conn).await?;
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
            if args.len() < 3 {
                return Ok((
                    "Usage: `!anic subscribe <packages>`".to_owned(),
                    Some("Usage: <code>!anic subscribe &lt;packages&gt;</code>".to_owned()),
                ));
            }
            let packages: Vec<String> = args[2..].to_vec();
            let db_conn = db_conn.lock().await;
            let mut stmt =
                db_conn.prepare("INSERT INTO subscription (user_id, package) VALUES (?1, ?2)")?;
            for package in packages {
                stmt.execute([user_id.as_str(), &package])?;
            }
            Ok(("Subscribed.".to_owned(), None))
        }
        "unsubscribe" => {
            if args.len() < 3 {
                return Ok((
                    "Usage: `!anic unsubscribe <packages>`".to_owned(),
                    Some("Usage: <code>!anic unsubscribe &lt;packages&gt;</code>".to_owned()),
                ));
            }
            let packages: Vec<String> = args[2..].to_vec();
            let db_conn = db_conn.lock().await;
            let mut stmt =
                db_conn.prepare("DELETE FROM subscription WHERE user_id = ?1 AND package = ?2")?;
            for package in packages {
                stmt.execute([user_id.as_str(), &package])?;
            }
            Ok(("Unsubscribed.".to_owned(), None))
        }
        "updates" => {
            let packages = get_packages(user_id, db_conn).await?;
            let mut updates = Anicca::get_local_json(data_dir)
                .await?
                .get_updates(&packages)
                .await?;
            if updates.is_empty() {
                Ok(("No package update found.".to_owned(), None))
            } else {
                updates.sort_by(|a, b| a.name.cmp(&b.name));
                let update_list = updates
                    .iter()
                    .map(|update| {
                        format!(
                            "{}: {} -> {}{}",
                            update.name,
                            update.before,
                            update.after,
                            if update.warnings.len() > 1 {
                                format!(" ({})", &update.warnings[1])
                            } else {
                                String::new()
                            }
                        )
                    })
                    .collect::<Vec<String>>()
                    .join("\n");
                Ok((
                    format!(
                        "Found {} update{}:\n{}",
                        updates.len(),
                        if updates.len() >= 2 { "s" } else { "" },
                        update_list
                    ),
                    None,
                ))
            }
        }
        _ => Ok((format!("Unknown command: {}", args[1]), None)),
    }
}
