use bincode::{Decode, Encode, config};
use eyre::{Result, bail};
use rocksdb::{DB, IteratorMode, Options};
use rusqlite::Connection;
use std::path::Path;

#[derive(Encode, Decode, Debug, Clone)]
struct User {
    packages: Vec<String>,
    notification_enabled: bool,
}

fn sqlite_to_rocksdb<T: AsRef<Path>>(path: T) -> Result<()> {
    let path = path.as_ref();
    let sqlite_db = Connection::open(path.join("anicca.db"))?;
    let mut opts = Options::default();
    opts.create_if_missing(true);
    let rocksdb_db = DB::open(&opts, path.join("anicca"))?;
    let bincode_config = config::standard();

    let mut stmt = sqlite_db.prepare("SELECT DISTINCT user_id FROM subscription")?;
    let rows = stmt.query_map([], |row| row.get(0))?;
    let mut users: Vec<String> = Vec::new();
    for row in rows {
        users.push(row?);
    }

    let mut stmt = sqlite_db.prepare("SELECT user_id FROM notification")?;
    let rows = stmt.query_map([], |row| row.get(0))?;
    let mut notification_users: Vec<String> = Vec::new();
    for row in rows {
        notification_users.push(row?);
    }

    for user_id in users {
        let mut stmt = sqlite_db.prepare("SELECT package FROM subscription WHERE user_id = ?1")?;
        let rows = stmt.query_map([&user_id], |row| row.get(0))?;
        let mut packages: Vec<String> = Vec::new();
        for row in rows {
            packages.push(row?);
        }

        let user = User {
            packages,
            notification_enabled: notification_users.contains(&user_id),
        };

        let encoded = bincode::encode_to_vec(&user, bincode_config)?;
        rocksdb_db.put(user_id.as_bytes(), encoded)?;
    }

    Ok(())
}

fn rocksdb_to_sqlite<T: AsRef<Path>>(path: T) -> Result<()> {
    let path = path.as_ref();
    let sqlite_db = Connection::open(path.join("anicca.db"))?;
    let mut opts = Options::default();
    opts.create_if_missing(true);
    let rocksdb_db = DB::open(&opts, path.join("anicca"))?;
    let bincode_config = config::standard();

    sqlite_db.execute(
        "CREATE TABLE IF NOT EXISTS subscription ( user_id TEXT NOT NULL, package TEXT NOT NULL )",
        (),
    )?;
    sqlite_db.execute(
        "CREATE TABLE IF NOT EXISTS notification ( user_id TEXT )",
        (),
    )?;

    let iter = rocksdb_db.iterator(IteratorMode::Start);
    for item in iter {
        let (key, val) = item?;
        let user_id = str::from_utf8(&key)?;
        let (user, _) = bincode::decode_from_slice::<User, _>(&val, bincode_config)?;

        let mut stmt =
            sqlite_db.prepare("INSERT INTO subscription (user_id, package) VALUES (?1, ?2)")?;
        for package in user.packages {
            stmt.execute([&user_id, &package.as_str()])?;
        }

        if user.notification_enabled {
            sqlite_db.execute("INSERT INTO notification (user_id) VALUES (?1)", [&user_id])?;
        }
    }

    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let path = &args[1];
    let target = &args[2];

    if target == "rocksdb" {
        sqlite_to_rocksdb(path)?;
    } else if target == "sqlite" {
        rocksdb_to_sqlite(path)?;
    } else {
        bail!("Unknown database target");
    }

    Ok(())
}
