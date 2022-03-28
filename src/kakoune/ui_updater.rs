use std::{io, sync::Arc};

use tokio::sync::{mpsc, watch};

use crate::{client::commands::types::DisplayCommand, session::Session};

pub struct KakouneUIUpdater {
    session: Arc<Session>,
    kakoune_display_rx: mpsc::UnboundedReceiver<DisplayCommand>,
}

impl KakouneUIUpdater {
    pub fn new(
        session: Arc<Session>,
        kakoune_display_rx: mpsc::UnboundedReceiver<DisplayCommand>,
    ) -> Self {
        Self {
            session,
            kakoune_display_rx,
        }
    }

    pub async fn update_until(&mut self, mut stop: watch::Receiver<()>) -> io::Result<()> {
        loop {
            tokio::select! {
                Ok(_) = stop.changed() => break Ok(()),
                Some(cmd) = self.kakoune_display_rx.recv() => {
                    todo!()
                }
            }
        }
    }
}
