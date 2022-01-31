#[macro_use]

mod config;
mod process;

use colored::*;
use ctrlc;
use log::error;
use std::collections::{HashMap, HashSet};
use std::sync::{atomic::AtomicBool, Arc, Mutex};
use std::time::{Duration, Instant};
use std::vec::Vec;
use url::Url;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const APP_NAME: &str = env!("CARGO_PKG_NAME");
pub const ABOUT: &str = env!("CARGO_PKG_DESCRIPTION");
pub const AUTHORS: &str = env!("CARGO_PKG_AUTHORS");

fn main() {
    env_logger::init();

    let requested_stop = Arc::new(AtomicBool::new(false));

    let rt = requested_stop.clone();
    let _ctrlc = ctrlc::set_handler(move || {
        rt.store(true, std::sync::atomic::Ordering::Relaxed);
        println!("{}", "Stopping in-flight requests...".yellow());
    });

    let config = config::Config::new();
    let concurrent = config.concurrent;

    println!("*** {} - {} ***", APP_NAME.green(), VERSION.yellow());
    println!(
        "Mode: {:?} with {} concurrent requests",
        config.mode,
        config.concurrent.to_string().magenta(),
    );

    if config.requests.is_some() && config.duration.is_some() {
        println!(
            "Running for {} requests or {} seconds",
            config.requests.unwrap().to_string().magenta(),
            config.duration.unwrap().as_secs().to_string().magenta()
        );
    } else {
        if let Some(requests) = config.requests {
            println!("Running for {} requests", requests.to_string().magenta(),);
        } else if let Some(duration) = config.duration {
            println!(
                "Running for {} seconds",
                duration.as_secs().to_string().magenta(),
            );
        }
    }

    println!("");

    if !config.no_delayed_start {
        println!("{}", "Starting in 1.5 seconds...".yellow());
        std::thread::sleep(Duration::from_millis(1500));
    }

    let started = Instant::now();

    let results = process::run(config, requested_stop);

    let results = results.lock().unwrap();

    println!("");

    println!(
        "*** Processed a total of {} requests in {:.2} seconds!",
        results.len().to_string().green(),
        (started.elapsed().as_secs_f32())
    );

    let mut errors = 0;
    let mut http_responses = 0;
    let mut status_codes: HashMap<u16, usize> = HashMap::new();
    let mut mean_response_time = 0.0;
    let mut median_response_time = 0.0;
    let mut total_length = 0;

    for result in results.iter() {
        match result {
            process::HttpResult::Response(val) => {
                http_responses += 1;
                let count = status_codes.entry(val.status.as_u16()).or_insert(0);
                *count += 1;

                total_length += val.length;

                mean_response_time += val.duration.as_millis() as f32;

                if median_response_time == 0.0 {
                    median_response_time = val.duration.as_millis() as f32;
                } else {
                    median_response_time =
                        (median_response_time as f32 + val.duration.as_millis() as f32) as f32 / 2.0
                }
            }
            process::HttpResult::Error(_) => {
                errors += 1;
            }
        }
    }

    mean_response_time /= http_responses as f32;

    let percentage_responses = (http_responses as f32 / results.len() as f32) * 100.0;
    let percentage_failures = 100.0 - percentage_responses;

    println!(
        "*** Received {} HTTP responses ({:.2}%) while {} requests failed ({:.2}%).\n",
        http_responses.to_string().green(),
        percentage_responses,
        errors.to_string().red(),
        percentage_failures,
    );

    let mut status_codes: Vec<_> = status_codes.iter().collect();
    status_codes.sort_by_key(|a| a.1);
    status_codes.reverse();

    for (status, count) in status_codes.iter() {
        let percentage = (**count as f32 / http_responses as f32) * 100.0;

        println!(
            "* [status {}] : {} requests ({:.2}%)",
            match status {
                200 => status.to_string().green(),
                301 => status.to_string().yellow(),
                _ => status.to_string().red(),
            },
            count.to_string().green(),
            percentage,
        );
    }

    println!("");

    println!("* Concurrency level: {}", concurrent);

    println!(
        "* Requests per second: {:.2} [#/sec] (mean)",
        (http_responses as f32 / started.elapsed().as_secs_f32())
    );

    println!("* Mean response time per request: {mean_response_time:.2}ms",);

    println!("* Median response time per request: {median_response_time:.2}ms",);

    println!("* Total content body length of responses: {total_length} bytes",);

    println!("");

    let mut p: Vec<_> = results
        .iter()
        .filter_map(|r| match r {
            process::HttpResult::Response(val) => Some(val.duration.as_millis()),
            _ => None,
        })
        .collect();

    p.sort();

    let percentile_95 = p[(p.len() as f32 * 0.95) as usize];
    let percentile_99 = p[(p.len() as f32 * 0.99) as usize];

    println!(
        "* 95th percentile response time: {}ms",
        percentile_95.to_string().green()
    );

    println!(
        "* 99th percentile response time: {}ms",
        percentile_99.to_string().green()
    );

    print!("\n");
}
