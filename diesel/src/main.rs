#[macro_use]
extern crate diesel;
#[macro_use]
extern crate diesel_migrations;
extern crate r2d2;

pub mod models;
pub mod schema;

use models::NewCounter;

use diesel::prelude::*;

use std::thread;
use std::time::Duration;

use diesel::connection::SimpleConnection;
use diesel::r2d2::ConnectionManager;
use diesel::result::Error;
use diesel::sqlite::SqliteConnection;
use diesel::RunQueryDsl;
use r2d2::PooledConnection;
use schema::counter::dsl::{counter, value};

const DATABASE_URL: &str = "db.sqlite";
const NUM_THREADS: u32 = 2000;

#[derive(Debug)]
struct Customizer;

impl diesel::r2d2::CustomizeConnection<SqliteConnection, diesel::r2d2::Error> for Customizer {
    fn on_acquire(&self, conn: &mut SqliteConnection) -> Result<(), diesel::r2d2::Error> {
        Ok((|| {
            conn.batch_execute("
                PRAGMA busy_timeout = 5;
                PRAGMA journal_mode = WAL;
                PRAGMA synchronous = NORMAL;
            ")?;
            Ok(())
        })().map_err(diesel::r2d2::Error::QueryError)?)
    }
}

fn main() {
    let manager = ConnectionManager::<SqliteConnection>::new(DATABASE_URL);
    let pool = r2d2::Pool::builder()
        .connection_customizer(Box::new(Customizer))
        .max_size(10)
        .connection_timeout(Duration::from_secs(1))
        .build(manager)
        .expect("Could not create database connection pool.");

    // Run migrations.
    let conn = pool.get().unwrap();
    embed_migrations!("migrations");
    embedded_migrations::run(&conn).unwrap();

    // Delete all rows.
    diesel::delete(counter).execute(&conn).unwrap();

    // Start the new counter at zero.
    diesel::insert_into(counter)
        .values(&NewCounter { value: 0 })
        .execute(&conn)
        .unwrap();

    let mut handles = Vec::with_capacity(NUM_THREADS as usize);
    for _ in 0..NUM_THREADS {
        let pool = pool.clone();
        handles.push(thread::spawn(move || {
            let conn = pool.get().unwrap();

            retry_transaction::<_, Error, _>(&conn, || {
                let val = counter.select(value).get_result::<i64>(&conn).unwrap();
                let mut val = val as u64;
                val += 1;

                diesel::update(counter)
                    .set(value.eq(val as i64))
                    .execute(&conn)
            })
            .unwrap();
        }));
    }
    for handle in handles {
        handle.join().unwrap();
    }

    // Check counter after all threads have finished.
    let val = counter.select(value).get_result::<i64>(&conn).unwrap();
    println!("{}", val);
}

pub fn retry_transaction<T, E, F>(conn: &PooledConnection<ConnectionManager<SqliteConnection>>, f: F) -> Result<T, E>
where
    F: Clone + FnOnce() -> Result<T, E>,
    E: From<Error>,
{
    for i in 0..5 {
        let r = conn.immediate_transaction::<T, E, F>(f.clone());
        if r.is_ok() || i == 4 {
            return r;
        } else {
            std::thread::sleep(std::time::Duration::from_millis(1 * 2u64.pow(i)));
            continue;
        }
    };
    panic!("Should never reach this point."); 
}
