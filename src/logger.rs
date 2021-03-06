use std::{io, path::Path};

use log::LevelFilter;
use log4rs::{
    append::console::Target,
    append::{console::ConsoleAppender, file::FileAppender},
    config::{Appender, Root},
    encode::pattern::PatternEncoder,
    filter::threshold::ThresholdFilter,
    Config, Handle,
};

/// Creates a new logger and sets it to be the main one.
///
/// This logger dumps all messages whose level are greater than `WARN` to both
/// stderr and the logfile, and all other messages only to the log file.
///
/// If this behavior is not expected (e.g. you might want all messages on stderr too),
/// the [`Handle`] returned allows for later customisations.
pub fn init<P: AsRef<Path>>(path: P) -> io::Result<Handle> {
    let on_console = ConsoleAppender::builder()
        .encoder(Box::new(PatternEncoder::new(
            "{d(%Y-%m-%d %H:%M:%S)} {M} {h([{l}])} {m}{n}",
        )))
        .target(Target::Stderr)
        .build();
    let in_log_file = FileAppender::builder()
        .encoder(Box::new(PatternEncoder::new(
            "{d(%Y-%m-%d %H:%M:%S)} {M} {h([{l}])} {m}{n}",
        )))
        .build(path)?;

    let config = Config::builder()
        .appender(
            Appender::builder()
                .filter(Box::new(ThresholdFilter::new(LevelFilter::Debug)))
                .build("stderr", Box::new(on_console)),
        )
        .appender(
            Appender::builder()
                .filter(Box::new(ThresholdFilter::new(LevelFilter::Debug)))
                .build("log", Box::new(in_log_file)),
        )
        .build(
            Root::builder()
                .appender("log")
                .appender("stderr")
                .build(LevelFilter::Debug),
        )
        .map_err(|err| io::Error::new(io::ErrorKind::Other, format!("{:?}", err)))?;

    log4rs::init_config(config)
        .map_err(|err| io::Error::new(io::ErrorKind::Other, format!("{:?}", err)))
}
