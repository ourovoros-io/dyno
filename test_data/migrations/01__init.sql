CREATE SCHEMA IF NOT EXISTS forc;

CREATE TABLE
    forc.runs (
        id SERIAL PRIMARY KEY,
        date TIMESTAMP NOT NULL,
        benchmarks TEXT NOT NULL
    );

CREATE TABLE
    forc.benchmarks (
        id SERIAL PRIMARY KEY,
        total_time INTERVAL NOT NULL,
        system_specs TEXT NOT NULL,
        benchmarks TEXT NOT NULL,
        forc_version TEXT NOT NULL,
        compiler_hash TEXT NOT NULL,
        benchmarks_datetime TEXT NOT NULL 
    );

CREATE TABLE
    forc.benchmark (
        id SERIAL PRIMARY KEY,
        name VARCHAR NOT NULL,
        path TEXT NOT NULL,
        start_time INTERVAL,
        end_time INTERVAL,
        phases TEXT NOT NULL,
        frames TEXT NOT NULL,
        asm_information TEXT NOT NULL,
        hyperfine TEXT NOT NULL
    );

CREATE TABLE
    forc.stats (
        id SERIAL PRIMARY KEY,
        stats TEXT NOT NULL
    );