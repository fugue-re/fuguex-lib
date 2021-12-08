use log::{Record, Level, Metadata, SetLoggerError, LevelFilter};
pub struct SimpleLogger;

// A simple logger to print logging info to stdout
impl log::Log for SimpleLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Trace
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            println!("{} - {}", record.level(), record.args());
        }
    }

    fn flush(&self) {}

}

static LOGGER: SimpleLogger = SimpleLogger;

pub fn init(level_option: Option<LevelFilter>) -> Result<(), SetLoggerError> {
    if let Some(level_filter) = level_option {
        log::set_logger(&LOGGER)
        .map(|()| log::set_max_level(level_filter))
    } else {
        log::set_logger(&LOGGER)
            .map(|()| log::set_max_level(LevelFilter::Info))
    }
}
