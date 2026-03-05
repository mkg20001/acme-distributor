use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, Pool, PooledConnection};
use diesel::SqliteConnection;

use super::models::{Certificate, NewCertificate};
use super::schema::certificates;

pub type DbPool = Pool<ConnectionManager<SqliteConnection>>;
pub type DbConn = PooledConnection<ConnectionManager<SqliteConnection>>;

pub fn create_pool(database_url: &str) -> DbPool {
    let manager = ConnectionManager::<SqliteConnection>::new(database_url);
    Pool::builder()
        .max_size(10)
        .build(manager)
        .expect("Failed to create database pool")
}

pub fn get_certificate(conn: &mut SqliteConnection, cert_id: &str) -> Option<Certificate> {
    certificates::table
        .find(cert_id)
        .first(conn)
        .optional()
        .expect("Error loading certificate")
}

pub fn get_all_certificates(conn: &mut SqliteConnection) -> Vec<Certificate> {
    certificates::table
        .load(conn)
        .expect("Error loading certificates")
}

pub fn upsert_certificate(conn: &mut SqliteConnection, cert: NewCertificate) {
    let now = chrono::Utc::now().timestamp_millis();

    diesel::insert_into(certificates::table)
        .values(&cert)
        .on_conflict(certificates::id)
        .do_update()
        .set((
            certificates::source.eq(&cert.source),
            certificates::provider.eq(&cert.provider),
            certificates::names.eq(&cert.names),
            certificates::cert_pem.eq(&cert.cert_pem),
            certificates::key_pem.eq(&cert.key_pem),
            certificates::ca_pem.eq(&cert.ca_pem),
            certificates::chain_pem.eq(&cert.chain_pem),
            certificates::expires_at.eq(&cert.expires_at),
            certificates::prefer_renew_before.eq(&cert.prefer_renew_before),
            certificates::prefer_renew_after.eq(&cert.prefer_renew_after),
            certificates::requested_at.eq(&cert.requested_at),
            certificates::updated_at.eq(now),
        ))
        .execute(conn)
        .expect("Error upserting certificate");
}

pub fn update_requested_at(conn: &mut SqliteConnection, cert_id: &str) {
    let now = chrono::Utc::now().timestamp_millis();

    diesel::update(certificates::table.find(cert_id))
        .set((
            certificates::requested_at.eq(now),
            certificates::updated_at.eq(now),
        ))
        .execute(conn)
        .expect("Error updating requested_at");
}

pub fn delete_certificate(conn: &mut SqliteConnection, cert_id: &str) {
    diesel::delete(certificates::table.find(cert_id))
        .execute(conn)
        .expect("Error deleting certificate");
}
