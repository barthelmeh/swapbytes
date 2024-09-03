use simplelog::*;
use std::fs::{self, File};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn initialize() {
    // Create the "logs" directory if it doesn't exist
    let log_dir = "logs";
    if !Path::new(log_dir).exists() {
        fs::create_dir(log_dir).unwrap();
    }

    // Generate a unique filename using the current timestamp
    let start_time = SystemTime::now();
    let since_epoch = start_time.duration_since(UNIX_EPOCH).unwrap();
    let timestamp = since_epoch.as_secs();
    let log_file_path = format!("{}/swapbytes{}.log", log_dir, timestamp);

    CombinedLogger::init(vec![
        // Write logs to a unique file
        WriteLogger::new(
            LevelFilter::Info, // Set the logging level
            Config::default(),
            File::create(&log_file_path).unwrap(), // The log file
        ),
    ])
    .unwrap();
}

pub use log::{debug, error, info};
