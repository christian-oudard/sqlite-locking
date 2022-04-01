#[macro_use]
extern crate rocket;
#[macro_use]
extern crate rocket_sync_db_pools;
#[macro_use]
extern crate diesel_migrations;
#[macro_use]
extern crate diesel;

use std::thread::sleep;
use std::time::Duration;

use rocket::fairing::AdHoc;
use rocket::response::Debug;
use rocket::{Build, Rocket};

use self::diesel::prelude::*;
use diesel::result::Error;

#[database("sqlite_database")]
struct Db(diesel::SqliteConnection);

type Result<T, E = Debug<diesel::result::Error>> = std::result::Result<T, E>;

#[derive(Debug, Clone, Queryable, Insertable)]
#[table_name = "counter"]
struct Counter {
    id: i32,
    value: i64,
}

table! {
    counter (id) {
        id -> Integer,
        value -> BigInt,
    }
}

const BASE_DELAY_MS: u32 = 10;
const NUM_RETRIES: u32 = 5;
pub fn transaction<T, E, F>(conn: &SqliteConnection, f: F) -> Result<T, E>
where
    F: Clone + FnOnce() -> Result<T, E>,
    E: From<Error>,
{
    for i in 0..NUM_RETRIES {
        let r = conn.immediate_transaction::<T, E, F>(f.clone());
        if r.is_ok() || i == (NUM_RETRIES - 1) {
            return r;
        }
        sleep(Duration::from_millis((BASE_DELAY_MS * 2_u32.pow(i)) as u64));
    }
    panic!("Should never reach this point.");
}

#[post("/")]
async fn increment(db: Db) -> Result<()> {
    db.run(move |conn| {
        transaction::<_, diesel::result::Error, _>(&conn, || {
            let val: u64 = counter::table
                .select(counter::value)
                .get_result::<i64>(conn)? as u64;
            diesel::update(counter::table)
                .set(counter::value.eq((val + 1) as i64))
                .execute(&*conn)
        })
    })
    .await?;
    Ok(())
}

#[get("/")]
async fn get(db: Db) -> Result<String> {
    let value: u64 = db
        .run(move |conn| {
            counter::table
                .select(counter::value)
                .get_result::<i64>(conn)
        })
        .await? as u64;
    Ok(value.to_string())
}

async fn run_migrations(rocket: Rocket<Build>) -> Rocket<Build> {
    embed_migrations!("migrations");

    let db = Db::get_one(&rocket).await.expect("database connection");
    db.run(|conn| embedded_migrations::run(conn))
        .await
        .expect("diesel migrations");

    // Delete all rows and start the new counter at zero.
    db.run(|conn| {
        diesel::delete(counter::table).execute(conn).unwrap();
        diesel::insert_into(counter::table)
            .values(&Counter { id: 1, value: 0 })
            .execute(conn)
            .unwrap();
    })
    .await;
    rocket
}

pub fn diesel_sqlite_stage() -> AdHoc {
    AdHoc::on_ignite("Diesel SQLite Stage", |rocket| async {
        rocket
            .attach(Db::fairing())
            .attach(AdHoc::on_ignite("Diesel Migrations", run_migrations))
            .mount("/", routes![increment, get])
    })
}

#[launch]
fn rocket() -> _ {
    rocket::build().attach(diesel_sqlite_stage())
}
