/// A decoder (parser) for user commands received from Kakoune.
pub mod decode;
/// Processes commands and sends calls to `coqidetop`.
pub mod processor;
/// A simple streaming channel which receives commands from Kakoune and
/// send them to the processor.
pub mod receiver;
/// Various types (mainly commands) used in this module.
pub mod types;
