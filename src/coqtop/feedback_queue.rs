use std::io;

use tokio::sync::{mpsc, watch};

use crate::kakoune::commands::types::DisplayCommand;

use super::xml_protocol::types::{FeedbackContent, ProtocolValue, MessageType};

pub type Feedback = (ProtocolValue, FeedbackContent);

pub struct FeedbackQueue {
    feedback_rx: mpsc::UnboundedReceiver<Feedback>,
    disp_cmd_tx: mpsc::UnboundedSender<DisplayCommand>,
}

impl FeedbackQueue {
    pub fn new(
        feedback_rx: mpsc::UnboundedReceiver<Feedback>,
        disp_cmd_tx: mpsc::UnboundedSender<DisplayCommand>,
    ) -> Self {
        Self {
            feedback_rx,
            disp_cmd_tx,
        }
    }

    pub async fn process(&mut self, mut stop_rx: watch::Receiver<()>) -> io::Result<()> {
        loop {
            tokio::select! {
                Ok(_) = stop_rx.changed() => break Ok(()),
                Some(feedback) = self.feedback_rx.recv() => {
                    self.process_feedback(feedback).await?;
                }
            }
        }
    }

    async fn process_feedback(&mut self, feedback: Feedback) -> io::Result<()> {
        if let (ProtocolValue::StateId(state_id), content) = feedback {
            match content {
                FeedbackContent::Message(level, message) => {
                    if let MessageType::Info | MessageType::Notice = level {
                        self.disp_cmd_tx
                            .send(DisplayCommand::ColorResult(message))
                            .map_err(|err| io::Error::new(io::ErrorKind::BrokenPipe, err))?;
                    } else {
                        log::debug!("{:?}", message.strip());
                    }
                }
                FeedbackContent::Processed => {},
                FeedbackContent::WorkerStatus(_) => {},
                FeedbackContent::Processing(_) => {},
                FeedbackContent::AddedAxiom => {},
            }
        } else {
            unreachable!();
        }

        Ok(())
    }
}
