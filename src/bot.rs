use eyre::Result;
use matrix_sdk::{
    Client, Room,
    ruma::{UserId, events::room::message::RoomMessageEventContent},
};
use std::path::Path;
use tracing::info;

use crate::db::*;
use anicca_subscribe::anicca::{Anicca, Package};

pub fn format_update_packages(packages: &mut [Package]) -> (String, String) {
    packages.sort_by(|a, b| a.name.cmp(&b.name));
    let packages_list = packages
        .iter()
        .map(|package| {
            format!(
                "<li><code>{}: {} -> {}</code>{}</li>",
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
        .collect::<String>();
    let html_output = format!(
        "<strong>Found {} update{}</strong><br/><ul>{}</ul>",
        packages.len(),
        if packages.len() >= 2 { "s" } else { "" },
        packages_list
    );
    let plain_output = html_output
        .replace("<code>", "")
        .replace("</code>", "")
        .replace("<br/>", "\n")
        .replace("<ul>", "")
        .replace("</ul>", "")
        .replace("<li>", "- ")
        .replace("</li>", "\n")
        .replace("<strong>", "")
        .replace("</strong>", ": ");
    (plain_output, html_output)
}

async fn dm_or_create(client: Client, user_id: &UserId) -> Result<Room> {
    if let Some(room) = client.get_dm_room(user_id) {
        return Ok(room);
    }
    Ok(client.create_dm(user_id).await?)
}

async fn notify_user(
    client: Client,
    user_id: &UserId,
    db: DatabaseImpl,
    data_dir: &Path,
) -> Result<()> {
    let room = dm_or_create(client.clone(), user_id).await?;
    info!("Notifying user: {}", user_id);
    let anicca_diff = Anicca::get_diff(data_dir).await?;
    let packages = db.get_packages(user_id).await?;
    let mut updates = anicca_diff.get_subscription_updates(&packages)?;

    if !updates.is_empty() {
        let (plain_updates, html_updates) = format_update_packages(&mut updates);
        let header = "(Hourly Notification)";
        let plain_updates = format!("{header}\n{plain_updates}");
        let html_updates = format!("{header}<br/>{html_updates}");
        let content = RoomMessageEventContent::notice_html(plain_updates, html_updates);
        room.send(content).await?;
    }

    Ok(())
}

pub async fn notify(client: Client, db: DatabaseImpl, data_dir: &Path) -> Result<()> {
    let targets = db.notification_targets().await?;
    for user_id in targets.iter() {
        notify_user(client.clone(), user_id, db.clone(), data_dir).await?;
    }

    Ok(())
}
