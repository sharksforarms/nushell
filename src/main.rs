#![feature(crate_visibility_modifier)]
#![feature(in_band_lifetimes)]
#![feature(async_await)]
#![feature(try_trait)]
#![feature(bind_by_move_pattern_guards)]

mod cli;
mod commands;
mod context;
mod env;
mod errors;
mod evaluate;
mod format;
mod git;
mod object;
mod parser;
mod prelude;
mod shell;
mod stream;

use clap::{App, Arg};
use log::LevelFilter;
use std::error::Error;

crate use parser::parse::text::Text;

fn main() -> Result<(), Box<dyn Error>> {
    let matches = App::new("nu shell")
        .version("0.5")
        .arg(
            Arg::with_name("loglevel")
                .short("l")
                .long("loglevel")
                .value_name("LEVEL")
                .possible_values(&["error", "warn", "info", "debug", "trace"])
                .takes_value(true),
        )
        .arg(
            Arg::with_name("develop")
                .long("develop")
                .multiple(true)
                .takes_value(true),
        )
        .get_matches();

    let loglevel = match matches.value_of("loglevel") {
        None => LevelFilter::Warn,
        Some("error") => LevelFilter::Error,
        Some("warn") => LevelFilter::Warn,
        Some("info") => LevelFilter::Info,
        Some("debug") => LevelFilter::Debug,
        Some("trace") => LevelFilter::Trace,
        _ => unreachable!(),
    };

    let mut builder = pretty_env_logger::formatted_builder();

    if let Ok(s) = std::env::var("RUST_LOG") {
        builder.parse_filters(&s);
    }

    builder.filter_module("nu", loglevel);

    match matches.values_of("develop") {
        None => {}
        Some(values) => {
            for item in values {
                builder.filter_module(&format!("nu::{}", item), LevelFilter::Trace);
            }
        }
    }

    builder.try_init()?;

    futures::executor::block_on(crate::cli::cli())?;
    Ok(())
}
