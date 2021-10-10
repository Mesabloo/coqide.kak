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

pub fn init<P: AsRef<Path>>(path: P) -> io::Result<Handle> {
    let on_console = ConsoleAppender::builder()
        .encoder(Box::new(PatternEncoder::new(
            "{h([{d(%Y-%m-%d %H:%M:%S)} {M} {l}])} {m}{n}",
        )))
        .target(Target::Stderr)
        .build();
    let in_log_file = FileAppender::builder()
        .encoder(Box::new(PatternEncoder::new(
            "[{d(%Y-%m-%d %H:%M:%S)} {M} {l}] {m}{n}",
        )))
        .build(path)?;

    let config = Config::builder()
        .appender(
            Appender::builder()
                .filter(Box::new(ThresholdFilter::new(LevelFilter::Warn)))
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
