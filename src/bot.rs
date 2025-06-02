use deadpool_sqlite::Pool;
use eyre::Result;
use matrix_sdk::{
    Client, Room,
    ruma::{OwnedUserId, UserId, events::room::message::RoomMessageEventContent},
};
use std::path::Path;
use tracing::info;

use anicca_subscribe::anicca::{Anicca, Package};

pub fn format_update_packages(packages: &mut [Package]) -> (String, String) {
    packages.sort_by(|a, b| a.name.cmp(&b.name));
    let packages_list = packages
        .iter()
        .map(|package| {
            format!(
                "<code>{}: {} -> {}</code>{}",
                package.name,
                package.before,
                package.after,
                if package.warnings.len() > 1 {
                    format!(" ({})", &package.warnings[1])
                } else {
                    String::new()
                }
            )
        })
        .collect::<Vec<String>>()
        .join("<br/>");
    let html_output = format!(
        "Found {} update{}:<br/>{}",
        packages.len(),
        if packages.len() >= 2 { "s" } else { "" },
        packages_list
    );
    let plain_output = html_output
        .replace("<code>", "")
        .replace("</code>", "")
        .replace("<br/>", "\n");
    (plain_output, html_output)
}

pub async fn get_packages(user_id: &UserId, pool: Pool) -> Result<Vec<String>> {
    let db_conn = pool.get().await?;
    let user_id_str = user_id.to_string();
    let packages = db_conn
        .interact(move |db_conn| {
            let mut stmt =
                db_conn.prepare("SELECT package FROM subscription WHERE user_id = ?1")?;
            let rows = stmt.query_map([&user_id_str], |row| row.get(0))?;
            let mut packages: Vec<String> = Vec::new();
            for row in rows {
                packages.push(row?);
            }
            Ok::<Vec<String>, rusqlite::Error>(packages)
        })
        .await
        .unwrap()?;
    Ok(packages)
}

async fn dm_or_create(client: Client, user_id: &UserId) -> Result<Room> {
    if let Some(room) = client.get_dm_room(user_id) {
        return Ok(room);
    }
    Ok(client.create_dm(user_id).await?)
}

async fn notify_user(client: Client, user_id: &UserId, pool: Pool, data_dir: &Path) -> Result<()> {
    let room = dm_or_create(client.clone(), user_id).await?;
    info!("Notifying user: {}", user_id);
    let anicca_diff = Anicca::get_diff(data_dir).await?;
    let packages = get_packages(user_id, pool.clone()).await?;
    let mut updates = anicca_diff.get_subscription_updates(&packages)?;

    if !updates.is_empty() {
        let (plain_updates, html_updates) = format_update_packages(&mut updates);
        let content = RoomMessageEventContent::notice_html(plain_updates, html_updates);
        room.send(content).await?;
    }

    Ok(())
}

pub async fn notify(client: Client, pool: Pool, data_dir: &Path) -> Result<()> {
    let db_conn = pool.get().await?;
    let targets = db_conn
        .interact(|db_conn| {
            let mut stmt = db_conn.prepare("SELECT user_id FROM notification")?;
            let mut rows = stmt.query([])?;

            let mut targets = Vec::new();
            while let Some(row) = rows.next()? {
                let user_id = UserId::parse(row.get::<_, String>(0)?).unwrap();
                targets.push(user_id);
            }
            Ok::<Vec<OwnedUserId>, rusqlite::Error>(targets)
        })
        .await
        .unwrap()?;

    for user_id in targets.iter() {
        notify_user(client.clone(), user_id, pool.clone(), data_dir).await?;
    }

    Ok(())
}
