use simplelog::*;
use std::fs::File;

pub fn initialize() {
    CombinedLogger::init(vec![
        // Write logs to a file
        WriteLogger::new(
            LevelFilter::Info, // Set the logging level
            Config::default(),
            File::create("app.log").unwrap(), // The log file
        ),
    ])
    .unwrap();
}
pub use log::{debug, error, info};
