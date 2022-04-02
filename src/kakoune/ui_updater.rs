use std::{io, path::Path, sync::Arc};

use tokio::{
    fs::{File, OpenOptions},
    io::AsyncWriteExt,
    sync::{mpsc, watch},
};

use crate::{
    client::commands::types::DisplayCommand,
    coqtop::xml_protocol::types::{ProtocolRichPP, ProtocolRichPPPart},
    files::result_file,
    range::Range,
    session::{edited_file, session_id, temporary_folder, Session},
};

use super::command_line::kak;

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
                    match cmd {
                        DisplayCommand::ColorResult(richpp) => self.refresh_result_buffer_with(richpp, false).await?,
                        DisplayCommand::AddToProcessed(range) => self.add_to_processed(range).await?,
                        DisplayCommand::RemoveToBeProcessed(range) => self.remove_to_be_processed(range).await?,
                        _ => todo!(),
                    }
                }
            }
        }
    }

    // ---------------------

    async fn refresh_processed_range(&mut self) -> io::Result<()> {
        Ok(())
    }

    async fn remove_to_be_processed(&mut self, range: Range) -> io::Result<()> {
        kak(
            &session_id(self.session.clone()),
            format!(
                r#"evaluate-commands -buffer '{}' %{{ coqide-remove-to-be-processed '{}' }}"#,
                edited_file(self.session.clone()),
                range
            ),
        )
        .await
    }

    async fn add_to_processed(&mut self, range: Range) -> io::Result<()> {
        kak(
            &session_id(self.session.clone()),
            format!(
                r#"evaluate-commands -buffer '{}' %{{ coqide-add-to-processed '{}' }}"#,
                edited_file(self.session.clone()),
                range
            ),
        )
        .await
    }

    async fn refresh_result_buffer_with(
        &mut self,
        richpp: ProtocolRichPP,
        append: bool,
    ) -> io::Result<()> {
        let result_buffer = result_file(&temporary_folder(self.session.clone()));

        let (content, colors) = extract_colors(richpp, 1);

        let mut file = if content.is_empty() || !append {
            File::create(&result_buffer).await?
        } else {
            OpenOptions::new()
                .append(true)
                .create(true)
                .open(&result_buffer)
                .await?
        };

        if !content.is_empty() {
            let coq_file = edited_file(self.session.clone());

            file.write_all(content.as_bytes()).await?;
            kak(
                &session_id(self.session.clone()),
                format!(
                    r#"evaluate-commands -buffer '{0}' %{{ coqide-refresh-result-buffer "{1}" "{2}" }}"#,
                    coq_file,
                    result_buffer,
                    colors.join(" ")
                ),
            )
            .await?;
        }

        Ok(())
    }
}

/// Extract the message and the colors from a [`ProtocolRichPP`] starting at the given line number.
fn extract_colors(richpp: ProtocolRichPP, starting_line: usize) -> (String, Vec<String>) {
    let ProtocolRichPP::RichPP(parts) = richpp;
    let mut message = String::new();
    let mut colors = Vec::new();
    let mut current_line = starting_line;
    let mut current_column = 1usize;

    for part in parts {
        let color_name = color_name(&part);

        let color = match part {
            ProtocolRichPPPart::Raw(txt) => {
                for c in txt.chars() {
                    if c == '\n' {
                        current_line += 1;
                        current_column = 1;
                    } else {
                        current_column += 1;
                    }
                }
                message += txt.as_str();
                None
            }
            // NOTE: there should be no \n in any of those remaining
            ProtocolRichPPPart::Keyword(txt)
            | ProtocolRichPPPart::Evar(txt)
            | ProtocolRichPPPart::Type(txt)
            | ProtocolRichPPPart::Notation(txt)
            | ProtocolRichPPPart::Variable(txt)
            | ProtocolRichPPPart::Reference(txt)
            | ProtocolRichPPPart::Path(txt) => {
                let begin = current_column;
                let end = begin + txt.len();
                message += txt.as_str();

                current_column = end;
                Some(format!(
                    "{}|coqide_{}",
                    Range::new(
                        current_line as u64,
                        begin as u64,
                        current_line as u64,
                        end as u64
                    ),
                    color_name,
                ))
            }
        };

        if let Some(color) = color {
            colors.push(color);
        }
    }

    (message, colors)
}

/// Retrieves the name of the color corresponding to a RichPP node.
fn color_name(part: &ProtocolRichPPPart) -> String {
    match part {
        ProtocolRichPPPart::Keyword(_) => "keyword",
        ProtocolRichPPPart::Evar(_) => "evar",
        ProtocolRichPPPart::Type(_) => "type",
        ProtocolRichPPPart::Notation(_) => "notation",
        ProtocolRichPPPart::Variable(_) => "variable",
        ProtocolRichPPPart::Reference(_) => "reference",
        ProtocolRichPPPart::Path(_) => "path",
        _ => "unknown",
    }
    .to_string()
}
