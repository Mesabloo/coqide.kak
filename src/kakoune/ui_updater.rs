use std::{
    collections::VecDeque,
    io,
    sync::{Arc, RwLock},
};

use tokio::{
    fs::{File, OpenOptions},
    io::AsyncWriteExt,
};

use crate::{
    client::commands::types::DisplayCommand,
    coqtop::xml_protocol::types::{ProtocolRichPP, ProtocolRichPPPart, ProtocolValue},
    files::{goal_file, result_file},
    range::Range,
    session::{client_name, edited_file, session_id, temporary_folder, Session},
    state::{ErrorState, State},
};

use super::command_line::kak;

pub struct KakouneUIUpdater {
    session: Arc<Session>,
    state: Arc<RwLock<State>>,
    current_buffer_line: usize,
}

impl KakouneUIUpdater {
    pub fn new(session: Arc<Session>, state: Arc<RwLock<State>>) -> Self {
        Self {
            session,
            state,
            current_buffer_line: 1,
        }
    }

    pub async fn process(&mut self, mut commands: VecDeque<DisplayCommand>) -> io::Result<()> {
        log::debug!("Processing {} UI commands", commands.len());

        while let Some(cmd) = commands.pop_front() {
            log::debug!("Received UI command {:?}", cmd);

            let error_state = self.state.read().unwrap().error_state;

            match cmd {
                DisplayCommand::ColorResult(richpp, append) => {
                    self.refresh_result_buffer_with(richpp, append).await?
                }
                DisplayCommand::AddToProcessed(range) => self.add_to_processed(range).await?,
                DisplayCommand::OutputGoals(fg, bg, gg, sg) => {
                    self.output_goals(fg, bg, gg, sg).await?
                }
                DisplayCommand::RemoveProcessed(range) => self.remove_processed(range).await?,
                DisplayCommand::RefreshErrorRange(range, force)
                    if force || error_state != ErrorState::Ok =>
                {
                    self.refresh_error_range(range).await?
                } // TODO: do we need to do this only if it is Ok to continue?
                DisplayCommand::RemoveToBeProcessed(range) => {
                    self.remove_to_be_processed(range).await?
                }
                DisplayCommand::GotoTip => self.goto_tip().await?,
                DisplayCommand::AddAxiom(range) => self.add_axiom(range).await?,
                DisplayCommand::RemoveAxiom(range) => self.remove_axiom(range).await?,
                DisplayCommand::ShowStatus(path, proof_name) => {
                    self.show_status(path, proof_name).await?
                }
                _ => {}
            }
        }

        Ok(())
    }

    // ---------------------

    async fn show_status(&mut self, path: String, proof_name: String) -> io::Result<()> {
        kak(
            &session_id(self.session.clone()),
            format!(
                r#"evaluate-commands -buffer '{}' %{{ coqide-show-status "{}" "{}" "{}" }}"#,
                edited_file(self.session.clone()),
                client_name(self.session.clone()),
                path,
                proof_name
            ),
        )
        .await
    }

    async fn add_axiom(&mut self, range: Range) -> io::Result<()> {
        kak(
            &session_id(self.session.clone()),
            format!(
                r#"evaluate-commands -buffer '{}' %{{ coqide-push-axiom "{}" }}"#,
                edited_file(self.session.clone()),
                range
            ),
        )
        .await
    }

    async fn remove_axiom(&mut self, range: Range) -> io::Result<()> {
        kak(
            &session_id(self.session.clone()),
            format!(
                r#"evaluate-commands -buffer '{}' %{{ coqide-remove-axiom "{}" }}"#,
                edited_file(self.session.clone()),
                range
            ),
        )
        .await
    }

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

        let (added_lines, colors) = {
            let mut file = if !append {
                self.current_buffer_line = 1;
                File::create(&result_buffer).await?
            } else {
                OpenOptions::new()
                    .append(true)
                    .create(true)
                    .open(&result_buffer)
                    .await?
            };

            let (mut content, colors) =
                extract_colors(richpp, self.current_buffer_line, 1usize, true);

            let mut added_lines = 0;
            if !content.is_empty() {
                content += "\n";
                added_lines = content.matches("\n").count();
                self.current_buffer_line += added_lines;
                file.write_all(content.as_bytes()).await?;
                file.flush().await?;
            }
            file.shutdown().await?;

            (added_lines, colors)
        };

        kak(
            &session_id(self.session.clone()),
            format!(
                r#"evaluate-commands -buffer '{0}' %{{ coqide-refresh-result-buffer "{1}" "{2}" "{3}" }}"#,
                edited_file(self.session.clone()),
                result_buffer,
                if append { format!("{}", added_lines) } else { "".to_string() },
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
        sg: Vec<ProtocolValue>,
    ) -> io::Result<()> {
        let goal_buffer = goal_file(&temporary_folder(self.session.clone()));

        let mut message: String;
        let mut colors: Vec<String> = Vec::new();

        if fg.is_empty() {
            if bg.is_empty() || bg.iter().all(|(lg, rg)| lg.is_empty() && rg.is_empty()) {
                if gg.is_empty() {
                    if sg.is_empty() {
                        message = "There are no more subgoals.\nProof is complete.".to_string();
                    } else {
                        message = "There are no more subgoals, but some goals remain sheleved:\n"
                            .to_string();
                        let mut line = 3usize;
                        for goal in sg.into_iter() {
                            let (txt, mut cols, i) = goal_to_string(goal, line);
                            message = format!("{}\n{}", message, txt);
                            colors.append(&mut cols);
                            line = i + 1;
                        }
                    }
                } else {
                    message = "There are no more subgoals, but there are some given up goals:\n"
                        .to_string();
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
    align: bool,
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
                        current_column = if align { starting_column } else { 1 };
                    } else {
                        current_column += 1;
                    }
                }

                let tmp = format!("\n{}", " ".repeat(starting_column - 1));
                message += txt.replace("\n", tmp.as_str()).as_str();

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
                        current_column = if align { starting_column } else { 1 };
                    } else {
                        current_column += 1;
                    }
                }

                let tmp = format!("\n{}", " ".repeat(starting_column - 1));
                message += txt.replace("\n", tmp.as_str()).as_str();

                Some(format!(
                    "{}|coqide_{}",
                    Range::new(
                        begin_line as u64,
                        begin_column as u64,
                        current_line as u64,
                        (current_column - 1) as u64
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
            let (msg, mut cols) = extract_colors(hyp, line, 2usize, true);
            line += 1;

            max_size = max_size.max(msg.lines().map(|l| l.len()).max().unwrap_or(0) + 2);

            message = if message.is_empty() {
                format!(" {} ", msg)
            } else {
                format!("{}\n {} ", message, msg)
            };
            colors.append(&mut cols);
        }
        let (msg, mut cols) = extract_colors(ccl, line + 1, 2usize, true);

        max_size = max_size.max(msg.lines().map(|l| l.len()).max().unwrap_or(0) + 2);
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
