use clap::Parser;
use command::parse_prefix_and_args;
use eyre::Result;
use matrix_sdk::{
    Client, Room, RoomState,
    config::SyncSettings,
    event_handler::Ctx,
    room::Receipts,
    ruma::{
        OwnedEventId,
        api::client::filter::FilterDefinition,
        events::{
            relation::{InReplyTo, Thread},
            room::{
                encrypted::SyncRoomEncryptedEvent,
                member::{MembershipState, StrippedRoomMemberEvent, SyncRoomMemberEvent},
                message::{MessageType, OriginalSyncRoomMessageEvent, Relation},
            },
        },
    },
};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use tracing::{debug, error, info, instrument, warn};
use tracing_subscriber::{EnvFilter, prelude::*};

mod bot;
mod cli;
mod command;
mod db;

use cli::{Cli, Subcommands};
use db::*;

#[derive(Clone)]
struct Payload {
    db: DatabaseImpl,
    data_dir: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    matrixbot_ezlogin::DuplexLog::init();
    tracing_subscriber::registry()
        .with(tracing_error::ErrorLayer::default())
        .with({
            let mut filter = EnvFilter::new("info,matrixbot_ezlogin=info");
            if let Some(env) = std::env::var_os(EnvFilter::DEFAULT_ENV) {
                for segment in env.to_string_lossy().split(',') {
                    if let Ok(directive) = segment.parse() {
                        filter = filter.add_directive(directive);
                    }
                }
            }
            filter
        })
        .with(
            tracing_subscriber::fmt::layer().with_writer(matrixbot_ezlogin::DuplexLog::get_writer),
        )
        .init();

    let args = Cli::parse();

    match args.subcommands {
        Subcommands::Setup {
            data_dir,
            device_name,
        } => {
            db::DatabaseImpl::new(&data_dir)?.init().await?;
            drop(matrixbot_ezlogin::setup_interactive(&data_dir, &device_name).await?)
        }
        Subcommands::Run { data_dir } => run(&data_dir).await?,
        Subcommands::Logout { data_dir } => matrixbot_ezlogin::logout(&data_dir).await?,
    }

    Ok(())
}

async fn run(data_dir: &Path) -> Result<()> {
    let (client, sync_helper) = matrixbot_ezlogin::login(data_dir).await?;

    let database = DatabaseImpl::new(data_dir)?;
    let notify_client = client.clone();
    let data_dir_owned = data_dir.to_path_buf();
    let notify_db = database.clone();
    tokio::spawn(async move {
        loop {
            info!("Fetching anicca pkgsupdate.json");
            if let Err(e) = anicca_subscribe::anicca::Anicca::fetch_json(&data_dir_owned).await {
                warn!("Unable to fetch anicca pkgsupdate.json: {}", e);
            }
            info!("Sending hourly notifications");
            if let Err(e) =
                bot::notify(notify_client.clone(), notify_db.clone(), &data_dir_owned).await
            {
                warn!("Unable to send hourly notifications: {}", e);
            }
            tokio::time::sleep(Duration::from_secs(3600)).await;
        }
    });

    // We don't ignore joining and leaving events happened during downtime.
    client.add_event_handler(on_invite);
    client.add_event_handler(on_leave);

    // Enable room members lazy-loading, it will speed up the initial sync a lot with accounts in lots of rooms.
    // https://spec.matrix.org/v1.6/client-server-api/#lazy-loading-room-members
    let sync_settings =
        SyncSettings::default().filter(FilterDefinition::with_lazy_loading().into());

    info!(
        "Skipping messages since last logout. May take longer depending on the number of rooms joined."
    );
    sync_helper
        .sync_once(&client, sync_settings.clone())
        .await?;

    client.add_event_handler_context(Payload {
        db: database.clone(),
        data_dir: data_dir.to_owned(),
    });
    client.add_event_handler(on_message);
    client.add_event_handler(on_utd);

    // Forget rooms that we already left
    let left_rooms = client.left_rooms();
    tokio::spawn(async move {
        for room in left_rooms {
            info!("Forgetting room {}.", room.room_id());
            match room.forget().await {
                Ok(_) => info!("Forgot room {}.", room.room_id()),
                Err(err) => error!("Failed to forget room {}: {:?}", room.room_id(), err),
            }
        }
    });

    info!("Starting sync.");
    sync_helper.sync(&client, sync_settings).await?;

    Ok(())
}

#[instrument(skip_all)]
fn set_read_marker(room: Room, event_id: OwnedEventId) {
    tokio::spawn(async move {
        if let Err(err) = room
            .send_multiple_receipts(
                Receipts::new()
                    .fully_read_marker(event_id.clone())
                    .public_read_receipt(event_id.clone()),
            )
            .await
        {
            error!(
                "Failed to set the read marker of room {} to event {}: {:?}",
                room.room_id(),
                event_id,
                err
            );
        }
    });
}

// https://spec.matrix.org/v1.14/client-server-api/#mroommessage
#[instrument(skip_all)]
async fn on_message(
    event: OriginalSyncRoomMessageEvent,
    room: Room,
    client: Client,
    context: Ctx<Payload>,
) -> Result<()> {
    if event.sender == client.user_id().unwrap() {
        // Ignore my own message
        return Ok(());
    }
    debug!("room = {}, event = {:?}", room.room_id(), event);
    if room.state() != RoomState::Joined {
        info!(
            "Ignoring room {}: Current room state is {:?}.",
            room.room_id(),
            room.state()
        );
        return Ok(());
    }
    if let Some(Relation::Replacement(_)) = event.content.relates_to {
        info!(
            "Ignoring event {}: This event is an edit operation.",
            event.event_id
        );
        return Ok(());
    }
    if !matches!(event.content.msgtype, MessageType::Text(_)) {
        info!(
            "Ignoring event {}: Message type is {}.",
            event.event_id,
            event.content.msgtype()
        );
        return Ok(());
    }

    let MessageType::Text(ref text) = event.content.msgtype else {
        unreachable!()
    };

    let is_direct = room.is_direct().await?;
    let display_name = client
        .account()
        .get_display_name()
        .await?
        .unwrap_or("anicca".to_owned());
    let parsed_args = parse_prefix_and_args(is_direct, &text.body, client.user_id(), &display_name);
    let mut reply = if let Some(args) = parsed_args {
        set_read_marker(room.clone(), event.event_id.clone());
        command::handle(&args, &context.data_dir, &event.sender, context.db.clone()).await?
    } else {
        debug!("Ignoring: Not command: {:?}.", text);
        return Ok(());
    };

    // We should use make_reply_to, but it embeds the original message body, which I don't want
    reply.relates_to = match reply.relates_to {
        Some(Relation::Replacement(_)) => unreachable!(),
        Some(Relation::Thread(thread)) => Some(Relation::Thread(Thread::reply(
            thread.event_id,
            event.event_id.to_owned(),
        ))),
        _ => Some(Relation::Reply {
            in_reply_to: InReplyTo::new(event.event_id.to_owned()),
        }),
    };

    tokio::spawn(async move {
        info!("Sending a reply message to {}.", event.event_id);
        match room.send(reply).await {
            Ok(_) => info!("Sent a reply message to {}.", event.event_id),
            Err(err) => error!(
                "Failed to send a reply message to {}: {:?}",
                event.event_id, err
            ),
        }
    });

    Ok(())
}

// The SDK documentation said nothing about how to catch unable-to-decrypt (UTD) events.
// But it seems this handler can capture them.
//
// https://spec.matrix.org/v1.14/client-server-api/#mroomencrypted
#[instrument(skip_all)]
async fn on_utd(event: SyncRoomEncryptedEvent, room: Room) {
    info!("room = {}, event = {:?}", room.room_id(), event);
    error!("Unable to decrypt message {}.", event.event_id());
}

// Whenever someone invites me to a room, join if it is a direct chat.
//
// https://spec.matrix.org/v1.14/client-server-api/#mroommember
// https://spec.matrix.org/v1.14/client-server-api/#stripped-state
#[instrument(skip_all)]
async fn on_invite(event: StrippedRoomMemberEvent, room: Room, client: Client) {
    let user_id = client.user_id().unwrap();
    if event.sender == user_id {
        return;
    }
    debug!("room = {}, event = {:?}", room.room_id(), event);
    // The user for which a membership applies is represented by the state_key.
    if event.state_key != user_id {
        info!(
            "Ignoring room {}: Someone else was invited.",
            room.room_id()
        );
        return;
    }
    if room.state() != RoomState::Invited {
        info!(
            "Ignoring room {}: Current room state is {:?}.",
            room.room_id(),
            room.state()
        );
        return;
    }

    tokio::spawn(async move {
        for retry in 0.. {
            info!("Joining room {}.", room.room_id());
            match room.join().await {
                Ok(_) => {
                    info!("Joined room {}.", room.room_id());
                    return;
                }
                Err(err) => {
                    // https://github.com/matrix-org/synapse/issues/4345
                    if retry >= 16 {
                        error!("Failed to join room {}: {:?}", room.room_id(), err);
                        error!("Too many retries, giving up after 1 hour.");
                        return;
                    } else {
                        const BASE: f64 = 1.6180339887498947;
                        let duration = BASE.powi(retry);
                        warn!("Failed to join room {}: {:?}", room.room_id(), err);
                        warn!("This is common, will retry in {:.1}s.", duration);
                        tokio::time::sleep(Duration::from_secs_f64(duration)).await;
                    }
                }
            }
        }
    });
}

// Whenever someone leaves a room, check whether I am the last remaining member.
// If so, leave the room, then forget the empty room from the account data.
//
// https://spec.matrix.org/v1.14/client-server-api/#mroommember
// Each m.room.member event occurs twice in SyncResponse, one as state event, another as timeline event.
// As of matrix_sdk-0.11.0, this event handler matching SyncRoomMemberEvent is actually called twice whenever such an event happens.
// (Reference: matrix_sdk::Client::call_sync_response_handlers, https://github.com/matrix-org/matrix-rust-sdk/pull/4947)
// Thankfully, leaving a room twice does not return errors.
#[instrument(skip_all)]
async fn on_leave(event: SyncRoomMemberEvent, room: Room) {
    if !matches!(
        event.membership(),
        MembershipState::Leave | MembershipState::Ban
    ) {
        return;
    }
    debug!("room = {}, event = {:?}", room.room_id(), event);

    match room.state() {
        RoomState::Joined => {
            tokio::spawn(async move {
                if let Err(err) = room.sync_members().await {
                    warn!("Failed to sync members of {}: {:?}", room.room_id(), err);
                }
                // Only I remain in the room.
                if room.joined_members_count() <= 1 {
                    info!("Leaving room {}.", room.room_id());
                    match room.leave().await {
                        Ok(_) => info!("Left room {}.", room.room_id()),
                        Err(err) => error!("Failed to leave room {}: {:?}", room.room_id(), err),
                    }
                }
            });
        }
        RoomState::Banned | RoomState::Left => {
            // Either I successfully left the room, or someone kicked me out.
            tokio::spawn(async move {
                info!("Forgetting room {}.", room.room_id());
                match room.forget().await {
                    Ok(_) => info!("Forgot room {}.", room.room_id()),
                    Err(err) => error!("Failed to forget room {}: {:?}", room.room_id(), err),
                }
            });
        }
        _ => (),
    }
}
