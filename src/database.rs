use base64::{engine::general_purpose, Engine};
use openssl::{
    ssl::{SslConnector, SslMethod},
    x509::X509,
};
use postgres_openssl::MakeTlsConnector;

use crate::wrap;

/// Setup the database connection and return the client
///
/// # Returns
///
/// A `Result` containing a `tokio_postgres::Client`.
///
/// # Panics
///
/// If the database URL is not set.
///
/// If the database password is not set.
///
/// # Errors
///
/// If the connection to the database fails.
///
pub async fn setup() -> crate::Result<tokio_postgres::Client> {
    // Load the certificate from the environment variable
    let cert_path = std::env::var("CERT").map_err(|e| wrap!(e.into()))?;

    // Get the environment variable for the database URL
    let db_host = std::env::var("DB_HOST").map_err(|e| wrap!(e.into()))?;

    // Get the environment variable for the database password
    let db_password = std::env::var("DB_PASSWORD").map_err(|e| wrap!(e.into()))?;

    // Get the environment variable for the database port
    let db_port = std::env::var("DB_PORT").map_err(|e| wrap!(e.into()))?;

    // Get the environment variable for the database name
    let db_name = std::env::var("DB_NAME").map_err(|e| wrap!(e.into()))?;

    // Get the environment variable for the database user
    let db_user = std::env::var("DB_USER").map_err(|e| wrap!(e.into()))?;

    // Read the certificate
    let cert_data = std::fs::read_to_string(cert_path).map_err(|e| wrap!(e.into()))?;

    // Decode the base64-encoded certificate data
    let cert_bytes = general_purpose::STANDARD
        .decode(cert_data)
        .map_err(|e| wrap!(e.into()))?;

    // Load the certificate
    let cert = X509::from_pem(&cert_bytes).map_err(|e| wrap!(e.into()))?;

    // Load the certificate
    let mut builder = SslConnector::builder(SslMethod::tls()).map_err(|e| wrap!(e.into()))?;
    builder
        .cert_store_mut()
        .add_cert(cert)
        .map_err(|e| wrap!(e.into()))?;
    let connector = MakeTlsConnector::new(builder.build());

    let connection_string = format!("host={db_host} dbname={db_name} user={db_user} password={db_password} port={db_port} hostaddr={db_host} sslmode=require");

    // Connect to the database
    // https://docs.rs/tokio-postgres/latest/tokio_postgres/config/struct.Config.html
    let (client, connection) = tokio_postgres::connect(&connection_string, connector)
        .await
        .map_err(|e| wrap!(e.into()))?;

    // The connection object performs the actual communication with the database,
    // so spawn it off to run on its own.
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("connection error: {e}");
        }
    });

    Ok(client)
}

/// Create the schema and tables for the database
///
/// # Arguments
///
/// * `client` - A reference to a `tokio_postgres::Client`.
///
/// # Errors
///
/// If the query to the database fails.
///
/// If the creation of the schema or tables fails.
///
pub async fn create_schema(client: &tokio_postgres::Client) -> crate::Result<()> {
    let create_schema = "CREATE SCHEMA IF NOT EXISTS forc;";

    let create_runs_table = "CREATE TABLE IF NOT EXISTS forc.runs (
        id SERIAL PRIMARY KEY,
        date TIMESTAMP NOT NULL,
        benchmarks TEXT NOT NULL
    );";

    let create_benchmarks_table = "CREATE TABLE IF NOT EXISTS forc.benchmarks (
        id SERIAL PRIMARY KEY,
        total_time INTERVAL NOT NULL,
        system_specs TEXT NOT NULL, 
        benchmarks TEXT NOT NULL,
        forc_version TEXT NOT NULL,
        compiler_hash TEXT NOT NULL,
        benchmarks_datetime TEXT NOT NULL 
    );";

    let create_benchmark_table = "CREATE TABLE IF NOT EXISTS forc.benchmark (
        id SERIAL PRIMARY KEY,
        name VARCHAR NOT NULL,
        path TEXT NOT NULL,
        start_time INTERVAL,
        end_time INTERVAL,
        phases TEXT NOT NULL, 
        frames TEXT NOT NULL,
        asm_information TEXT NOT NULL,
        hyperfine TEXT NOT NULL
    );";

    let create_stats_table = "CREATE TABLE IF NOT EXISTS forc.stats (
        id SERIAL PRIMARY KEY,
        stats TEXT NOT NULL
    );";

    client
        .execute(create_schema, &[])
        .await
        .map_err(|e| wrap!(e.into()))?;

    client
        .execute(create_runs_table, &[])
        .await
        .map_err(|e| wrap!(e.into()))?;

    client
        .execute(create_benchmarks_table, &[])
        .await
        .map_err(|e| wrap!(e.into()))?;

    client
        .execute(create_benchmark_table, &[])
        .await
        .map_err(|e| wrap!(e.into()))?;

    client
        .execute(create_stats_table, &[])
        .await
        .map_err(|e| wrap!(e.into()))?;

    Ok(())
}

/// Get the number of entries in a table
///
/// # Arguments
///
/// * `client` - A reference to a `tokio_postgres::Client`.
///
/// # Returns
///
/// A `Result` containing the number of entries in the table.
///
/// # Errors
///
/// If the query to the database fails.
///
pub async fn get_table_count(client: &tokio_postgres::Client) -> crate::Result<i64> {
    let query = "SELECT COUNT(*) FROM forc.runs;".to_string();

    let row = client
        .query_one(&query, &[])
        .await
        .map_err(|e| wrap!(e.into()))?;

    let count: i64 = row.get(0);

    Ok(count)
}

/// Insert the benchmark results into the database
///
/// # Arguments
///
/// * `client` - A reference to a `tokio_postgres::Client`.
///
/// * `benches` - A reference to a `crate::types::Benchmarks`.
///
/// # Errors
///
/// If the insertion into the database fails.
///
pub async fn insert_benchmarks(
    client: &tokio_postgres::Client,
    benches: &crate::types::Benchmarks,
) -> crate::Result<()> {
    let benchmarks_json = serde_json::to_string(benches).map_err(|e| wrap!(e.into()))?;

    client
        .execute(
            "INSERT INTO forc.runs (date, benchmarks) VALUES (NOW(), $1);",
            &[&benchmarks_json],
        )
        .await
        .map_err(|e| wrap!(e.into()))?;

    Ok(())
}

/// Get the latest benchmarks from the database
///
/// # Arguments
///
/// * `client` - A reference to a `tokio_postgres::Client`.
///
/// # Returns
///
/// A `Result` containing a `crate::types::Benchmarks`.
///
/// # Errors
///
/// If the query to the database fails.
///
/// If the deserialization of the benchmarks fails.
///
pub async fn get_latest_benchmarks(
    client: &tokio_postgres::Client,
) -> crate::Result<crate::types::Benchmarks> {
    let row = client
        .query_one("SELECT * FROM forc.runs ORDER BY date DESC LIMIT 1;", &[])
        .await
        .map_err(|e| wrap!(e.into()))?;

    let benchmarks: String = row.get("benchmarks");

    Ok(serde_json::from_str(&benchmarks).map_err(|e| wrap!(e.into()))?)
}

/// Insert the stats into the database
///
/// # Arguments
///
/// * `client` - A reference to a `tokio_postgres::Client`.
///
/// * `stats` - A reference to a `crate::stats::Stats`.
///
/// # Errors
///
/// If the parsing of the stats into a JSON string fails.
///
/// If the insertion into the database fails.
///
pub async fn insert_stats(
    client: &tokio_postgres::Client,
    stats: &crate::stats::Collection,
) -> crate::Result<()> {
    let stats_json = serde_json::to_string(stats).map_err(|e| wrap!(e.into()))?;

    client
        .execute(
            "INSERT INTO forc.stats (stats) VALUES ($1);",
            &[&stats_json],
        )
        .await
        .map_err(|e| wrap!(e.into()))?;

    Ok(())
}

/// Get the latest stats from the database
///
/// # Arguments
///
/// * `client` - A reference to a `tokio_postgres::Client`.
///
/// # Returns
///
/// A `Result` containing a `crate::stats::Stats`.
///
/// # Errors
///
/// If the query to the database fails.
///
/// If the deserialization of the stats fails.
///
pub async fn get_latest_stats(
    client: &tokio_postgres::Client,
) -> crate::Result<crate::stats::Stats> {
    let row = client
        .query_one("SELECT * FROM forc.stats ORDER BY id DESC LIMIT 1;", &[])
        .await
        .map_err(|e| wrap!(e.into()))?;

    let stats: String = row.get("stats");

    Ok(serde_json::from_str(&stats).map_err(|e| wrap!(e.into()))?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Result;
    use crate::types::Benchmarks;
    use tokio;
    use tokio_postgres::NoTls;

    /// Setup the test database connection and return the client
    async fn make_setup() -> crate::Result<tokio_postgres::Client> {
        // Connect to the docker database container
        // https://docs.rs/tokio-postgres/latest/tokio_postgres/config/struct.Config.html
        let (client, connection) = tokio_postgres::connect(
            "host=localhost user=postgres dbname=forc password=forc port=5432",
            NoTls,
        )
        .await
        .map_err(|e| wrap!(e.into()))?;

        // The connection object performs the actual communication with the database,
        // so spawn it off to run on its own.
        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("connection error: {e}");
            }
        });

        create_schema(&client).await.map_err(|e| wrap!(e))?;

        Ok(client)
    }

    #[tokio::test]
    async fn test_setup() -> Result<()> {
        let client = make_setup().await.map_err(|e| wrap!(e))?;

        let row = client
            .query_one("SELECT 1", &[])
            .await
            .map_err(|e| wrap!(e.into()))?;

        let value: i32 = row.get(0);

        assert_eq!(value, 1);

        Ok(())
    }

    #[tokio::test]
    async fn test_insert_and_get_benches() -> Result<()> {
        let client = make_setup().await.map_err(|e| wrap!(e))?;

        let benchmark1 = Benchmarks {
            total_time: std::time::Duration::from_secs(1),
            system_specs: crate::types::SystemSpecs::default(),
            benchmarks: vec![crate::types::Benchmark {
                name: "dyno1".to_string(),
                path: "path/to/bench".to_string().into(),
                start_time: None,
                end_time: None,
                phases: vec![],
                frames: std::sync::Arc::new(std::sync::Mutex::new(vec![])),
                asm_information: None,
                hyperfine: None,
            }],
            forc_version: "0.1.0".to_string(),
            compiler_hash: "123456".to_string(),
            benchmarks_datetime: "2021-01-01T00:00:00".to_string(),
        };

        insert_benchmarks(&client, &benchmark1)
            .await
            .map_err(|e| wrap!(e))?;

        let benchmark2 = Benchmarks {
            total_time: std::time::Duration::from_secs(1),
            system_specs: crate::types::SystemSpecs::default(),
            benchmarks: vec![crate::types::Benchmark {
                name: "dyno2".to_string(),
                path: "path/to/bench".to_string().into(),
                start_time: None,
                end_time: None,
                phases: vec![],
                frames: std::sync::Arc::new(std::sync::Mutex::new(vec![])),
                asm_information: None,
                hyperfine: None,
            }],
            forc_version: "0.1.0".to_string(),
            compiler_hash: "123456".to_string(),
            benchmarks_datetime: "2021-01-01T00:00:00".to_string(),
        };

        insert_benchmarks(&client, &benchmark2)
            .await
            .map_err(|e| wrap!(e))?;

        let latest_benchmarks = get_latest_benchmarks(&client).await.map_err(|e| wrap!(e))?;

        assert!(latest_benchmarks.benchmarks[0].name == "dyno2");

        Ok(())
    }

    /// Helper function to clear the database
    #[tokio::test]
    async fn reset_database() -> Result<()> {
        let client = make_setup().await.map_err(|e| wrap!(e))?;
        client
            .execute("DROP SCHEMA forc CASCADE;", &[])
            .await
            .map_err(|e| wrap!(e.into()))?;
        create_schema(&client).await.map_err(|e| wrap!(e))?;
        Ok(())
    }

    #[tokio::test]
    async fn test_get_table_count() -> Result<()> {
        let client = make_setup().await.map_err(|e| wrap!(e))?;
        println!("Table count : {}", get_table_count(&client).await?);
        Ok(())
    }

    #[tokio::test]
    async fn test_get_latest_benchmarks() -> Result<()> {
        let client = make_setup().await.map_err(|e| wrap!(e))?;
        let benchmarks = get_latest_benchmarks(&client).await.map_err(|e| wrap!(e))?;
        println!("Benchmarks : {:#?}", benchmarks.benchmarks[0].name);
        Ok(())
    }
}
