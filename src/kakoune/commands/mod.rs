/// A decoder (parser) for user commands received from Kakoune.
pub mod decode;
/// A simple streaming channel which receives commands from Kakoune and
/// send them to the processor.
pub mod receiver;

pub mod types;
