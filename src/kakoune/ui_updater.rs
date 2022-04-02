use std::{io, sync::Arc};

use tokio::{
    fs::{File, OpenOptions},
    io::AsyncWriteExt,
    sync::{mpsc, watch},
};

use crate::{
    client::commands::types::DisplayCommand,
    coqtop::xml_protocol::types::{ProtocolRichPP, ProtocolRichPPPart, ProtocolValue},
    files::{goal_file, result_file},
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
                        DisplayCommand::ColorResult(richpp, append) => self.refresh_result_buffer_with(richpp, append).await?,
                        DisplayCommand::AddToProcessed(range) => self.add_to_processed(range).await?,
                        DisplayCommand::OutputGoals(fg, bg, gg) => self.output_goals(fg, bg, gg).await?,
                        DisplayCommand::RemoveProcessed(range) => self.remove_processed(range).await?,
                        _ => todo!(),
                    }
                }
            }
        }
    }

    // ---------------------

    async fn remove_processed(&mut self, range: Range) -> io::Result<()> {
        kak(
            &session_id(self.session.clone()),
            format!(
                r#"evaluate-commands -buffer '{}' %{{ coqide-remove-processed '{}' }}"#,
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

        let (mut content, colors) = extract_colors(richpp, 1);

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
            content += "\n";
            file.write_all(content.as_bytes()).await?;
        }
        kak(
            &session_id(self.session.clone()),
            format!(
                r#"evaluate-commands -buffer '{0}' %{{ coqide-refresh-result-buffer "{1}" {2} }}"#,
                edited_file(self.session.clone()),
                result_buffer,
                colors.join(" ")
            ),
        )
        .await?;

        Ok(())
    }

    /// Output all received goals to the goal buffer.
    async fn output_goals(
        &self,
        fg: Vec<ProtocolValue>,
        bg: Vec<(Vec<ProtocolValue>, Vec<ProtocolValue>)>,
        gg: Vec<ProtocolValue>,
    ) -> io::Result<()> {
        let goal_buffer = goal_file(&temporary_folder(self.session.clone()));

        let mut message: String;
        let mut colors: Vec<String> = Vec::new();

        if fg.is_empty() {
            if bg.is_empty() || bg.iter().all(|(lg, rg)| lg.is_empty() && rg.is_empty()) {
                if gg.is_empty() {
                    message = "No more subgoals.".to_string();
                } else {
                    message = "No more subgoals, but there are some given up goals:\n".to_string();
                    let mut line = 3usize;
                    for goal in gg.into_iter() {
                        let (txt, mut cols, i) = goal_to_string(goal, line);
                        message = format!("{}\n{}", message, txt);
                        colors.append(&mut cols);
                        line = i + 1;
                    }
                }
            } else {
                message = "The current subgoal is complete, but there are unfinished subgoals:\n"
                    .to_string();
                let mut line = 3usize;
                for (first, last) in bg.into_iter() {
                    for goal in first.into_iter().chain(last.into_iter()) {
                        let (txt, mut cols, i) = goal_to_string(goal, line);

                        message = format!("{}\n{}", message, txt);
                        colors.append(&mut cols);
                        line = i + 1;
                    }
                }
            }
        } else {
            message = format!("{} subgoal(s) remaining:\n", fg.len());
            let mut line = 3usize;
            for goal in fg.into_iter() {
                let (txt, mut cols, i) = goal_to_string(goal, line);

                message = format!("{}\n{}", message, txt);
                colors.append(&mut cols);
                line = i + 1;
            }
        }

        let mut file = File::create(&goal_buffer).await?;
        file.write_all(message.as_bytes()).await?;

        kak(
            &session_id(self.session.clone()),
            format!(
                r#"evaluate-commands -buffer '{0}' %{{ coqide-refresh-goal-buffer "{1}" {2} }}"#,
                edited_file(self.session.clone()),
                goal_buffer,
                colors.join(" ")
            ),
        )
        .await
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

/// Transforms a [`ProtocolValue::Goal`] into its colored textual representation.
fn goal_to_string(goal: ProtocolValue, mut line: usize) -> (String, Vec<String>, usize) {
    if let ProtocolValue::Goal(_, hyps, ccl) = goal {
        let mut message = String::new();
        let mut colors = Vec::new();

        let mut max_size = 0usize;

        for hyp in hyps {
            let (msg, mut cols) = extract_colors(hyp, line);
            line += 1;

            max_size = max_size.max(msg.len());

            message = if message.is_empty() {
                msg
            } else {
                format!("{}\n{}", message, msg)
            };
            colors.append(&mut cols);
        }
        let (msg, mut cols) = extract_colors(ccl, line + 1);

        max_size = max_size.max(msg.len());
        let middle_line = "─".repeat(max_size);
        message = if message.is_empty() {
            line += 1;
            format!("{}\n{}", middle_line, msg)
        } else {
            line += 2;
            format!("{}\n{}\n{}\n", message, middle_line, msg)
        };
        colors.append(&mut cols);

        (message, colors, line)
    } else {
        unreachable!()
    }
}
