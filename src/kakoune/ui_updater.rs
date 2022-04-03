use std::{collections::VecDeque, io, sync::Arc};

use tokio::{
    fs::{File, OpenOptions},
    io::AsyncWriteExt,
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
}

impl KakouneUIUpdater {
    pub fn new(session: Arc<Session>) -> Self {
        Self { session }
    }

    pub async fn process(&mut self, mut commands: VecDeque<DisplayCommand>) -> io::Result<()> {
        log::debug!("Processing {} UI commands", commands.len());

        while let Some(cmd) = commands.pop_front() {
            log::debug!("Received UI command {:?}", cmd);

            match cmd {
                DisplayCommand::ColorResult(richpp, append) => {
                    self.refresh_result_buffer_with(richpp, append).await?
                }
                DisplayCommand::AddToProcessed(range) => self.add_to_processed(range).await?,
                DisplayCommand::OutputGoals(fg, bg, gg) => self.output_goals(fg, bg, gg).await?,
                DisplayCommand::RemoveProcessed(range) => self.remove_processed(range).await?,
                DisplayCommand::RefreshErrorRange(range) => self.refresh_error_range(range).await?,
                DisplayCommand::RemoveToBeProcessed(range) => {
                    self.remove_to_be_processed(range).await?
                }
                DisplayCommand::GotoTip => self.goto_tip().await?,
            }
        }

        Ok(())
    }

    // ---------------------

    async fn goto_tip(&mut self) -> io::Result<()> {
        kak(
            &session_id(self.session.clone()),
            format!(
                r#"evaluate-commands -buffer '{}' %{{ coqide-goto-tip }}"#,
                edited_file(self.session.clone())
            ),
        )
        .await
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

    async fn refresh_error_range(&mut self, range: Option<Range>) -> io::Result<()> {
        let coq_file = edited_file(self.session.clone());

        kak(
            &session_id(self.session.clone()),
            match range {
                None => format!(
                    r#"evaluate-commands -buffer '{0}' %{{ coqide-remove-error-range }}"#,
                    coq_file
                ),
                Some(range) => format!(
                    r#"evaluate-commands -buffer '{0}' %{{ coqide-set-error-range '{1}' }}"#,
                    coq_file, range
                ),
            },
        )
        .await
    }

    async fn refresh_result_buffer_with(
        &mut self,
        richpp: ProtocolRichPP,
        append: bool,
    ) -> io::Result<()> {
        let result_buffer = result_file(&temporary_folder(self.session.clone()));

        let (mut content, colors) = extract_colors(richpp, 1usize, 1usize);

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
                r#"evaluate-commands -buffer '{0}' %{{ coqide-refresh-result-buffer "{1}" "{2}" }}"#,
                edited_file(self.session.clone()),
                result_buffer,
                colors.join("\" \"")
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
                r#"evaluate-commands -buffer '{0}' %{{ coqide-refresh-goal-buffer "{1}" "{2}" }}"#,
                edited_file(self.session.clone()),
                goal_buffer,
                colors.join("\" \"")
            ),
        )
        .await
    }
}

/// Extract the message and the colors from a [`ProtocolRichPP`] starting at the given line number.
fn extract_colors(
    richpp: ProtocolRichPP,
    starting_line: usize,
    starting_column: usize,
) -> (String, Vec<String>) {
    let ProtocolRichPP::RichPP(parts) = richpp;
    let mut message = String::new();
    let mut colors = Vec::new();
    let mut current_line = starting_line;
    let mut current_column = starting_column;

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
            | ProtocolRichPPPart::Path(txt)
            | ProtocolRichPPPart::Error(txt)
            | ProtocolRichPPPart::Warning(txt) => {
                let begin_column = current_column;
                let begin_line = current_line;

                for c in txt.chars() {
                    if c == '\n' {
                        current_line += 1;
                        current_column = 1;
                    } else {
                        current_column += 1;
                    }
                }

                message += txt.as_str();

                Some(format!(
                    "{}|coqide_{}",
                    Range::new(
                        begin_line as u64,
                        begin_column as u64,
                        current_line as u64,
                        current_column as u64
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
        ProtocolRichPPPart::Warning(_) => "warning",
        ProtocolRichPPPart::Error(_) => "error",
        _ => "unknown",
    }
    .to_string()
}

/// Transforms a [`ProtocolValue::Goal`] into its colored textual representation.
fn goal_to_string(goal: ProtocolValue, mut line: usize) -> (String, Vec<String>, usize) {
    if let ProtocolValue::Goal(box ProtocolValue::Str(name), hyps, ccl) = goal {
        let mut message = String::new();
        let mut colors = Vec::new();

        let mut max_size = 0usize;

        for hyp in hyps {
            let (msg, mut cols) = extract_colors(hyp, line, 2usize);
            line += 1;

            max_size = max_size.max(msg.len() + 2);

            message = if message.is_empty() {
                format!(" {} ", msg)
            } else {
                format!("{}\n {} ", message, msg)
            };
            colors.append(&mut cols);
        }
        let (msg, mut cols) = extract_colors(ccl, line + 1, 2usize);

        max_size = max_size.max(msg.len() + 2);
        let middle_line = "─".repeat(max_size);
        message = if message.is_empty() {
            line += 2;
            format!("{} ({})\n {} \n", middle_line, name, msg)
        } else {
            line += 2;
            format!("{}\n{} ({})\n {} \n", message, middle_line, name, msg)
        };
        colors.append(&mut cols);

        (message, colors, line)
    } else {
        unreachable!()
    }
}
