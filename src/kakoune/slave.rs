use std::io::SeekFrom;

use tokio::{
    fs::{File, OpenOptions},
    io::{self, AsyncSeekExt, AsyncWriteExt},
    sync::{mpsc, watch},
};

use crate::{
    coqtop::xml_protocol::types::{ProtocolRichPP, ProtocolRichPPPart, ProtocolValue},
    files::{goal_file, result_file},
    kakoune::command_line::kak,
    state::CodeSpan,
};

use super::types::DisplayCommand;

/// A simple abstraction of Kakoune used to send commands to it.
pub struct KakSlave {
    /// The receiving end of the channel used to send commands to be sent to Kakoune.
    cmd_rx: mpsc::UnboundedReceiver<DisplayCommand>,
    /// The session identifier of the Kakoune session to connect to.
    kak_session: String,
    /// The path to the goal file output in the goal buffer.
    kak_goal: String,
    /// The path to the result file output in the result buffer.
    kak_result: String,
    /// The file currently being edited.
    coq_file: String,
}

impl KakSlave {
    /// Initialises a new Kakoune slave.
    ///
    /// The 4th argument is used to automatically deduce both goal and results files
    /// using [`goal_file`] and [`result_file`].
    pub fn new(
        cmd_rx: mpsc::UnboundedReceiver<DisplayCommand>,
        kak_session: String,
        coq_file: String,
        tmp_dir: &String,
    ) -> Self {
        let kak_goal = goal_file(&tmp_dir);
        let kak_result = result_file(&tmp_dir);

        Self {
            cmd_rx,
            kak_session,
            kak_goal,
            kak_result,
            coq_file,
        }
    }

    /// Runs the processing loop of the kakoune slave until a message is received
    /// on its second parameter.
    ///
    /// Commands to be sent are received asynchronously, and direcctly dispatched (with minor formatting
    /// to execute commands in the correct buffer).
    pub async fn process(&mut self, mut stop_rx: watch::Receiver<()>) -> io::Result<()> {
        loop {
            tokio::select! {
                Ok(_) = stop_rx.changed() => break Ok(()),
                Some(cmd) = self.cmd_rx.recv() => {
                    log::debug!("Sending command `{:?}` to Kakoune", cmd);

                    self.process_command(cmd).await?;
                }
            }
        }
    }

    /// Process a [`DisplayCommand`] and send it to Kakoune.
    async fn process_command(&self, cmd: DisplayCommand) -> io::Result<()> {
        use DisplayCommand::*;

        match cmd {
            RefreshProcessedRange(range) => self.refresh_processed_range(range).await,
            RefreshErrorRange(range) => self.refresh_error_range(range).await,
            ColorResult(richpp) => self.output_result(richpp).await,
            OutputGoals(fg, bg, gg) => self.output_goals(fg, bg, gg).await,
        }
    }

    /// Refresh the range of processed kakoune inside the Coq buffer.
    async fn refresh_processed_range(&self, range: CodeSpan) -> io::Result<()> {
        kak(
            &self.kak_session,
            format!(
                r#"evaluate-commands -buffer '{}' %{{
                  set-option buffer coqide_processed_range %val{{timestamp}} '{}|coqide_processed'
                }}"#,
                self.coq_file, range
            ),
        )
        .await
    }

    /// Refresh the error range inside the Coq buffer.
    async fn refresh_error_range(&self, range: Option<CodeSpan>) -> io::Result<()> {
        kak(
            &self.kak_session,
            format!(
                r#"evaluate-commands -buffer '{}' %{{
                  set-option buffer coqide_error_range {}
                }}"#,
                self.coq_file,
                match range {
                    Some(range) => format!("%val{{timestamp}} '{}|coqide_errors'", range),
                    None => "%val{timestamp}".to_string(),
                }
            ),
        )
        .await
    }

    /// Extract colors from the [`ProtocolRichPP`] message and output both the colors and the message to
    /// the result buffer.
    async fn output_result(&self, richpp: ProtocolRichPP) -> io::Result<()> {
        let (message, colors) = tokio::task::block_in_place(|| extract_colors(richpp, 1));

        overwrite_file(&self.kak_result, message, true).await?;

        kak(
            &self.kak_session,
            format!(
                r#"evaluate-commands -buffer '{}' %{{
                  evaluate-commands -buffer "%opt{{coqide_result_buffer}}" %{{
                    execute-keys %{{ %|cat<space>{}<ret> }}
                    set-option buffer coqide_result_highlight %val{{timestamp}} {}
                  }}
                }}"#,
                self.coq_file,
                self.kak_result,
                colors.join(" ")
            ),
        )
        .await
    }

    /// Output all received goals to the goal buffer.
    async fn output_goals(
        &self,
        fg: Vec<ProtocolValue>,
        bg: Vec<(Vec<ProtocolValue>, Vec<ProtocolValue>)>,
        gg: Vec<ProtocolValue>,
    ) -> io::Result<()> {
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

        overwrite_file(&self.kak_goal, message, true).await?;

        kak(
            &self.kak_session,
            format!(
                r#"evaluate-commands -buffer '{}' %{{
                  evaluate-commands -buffer "%opt{{coqide_goal_buffer}}" %{{
                    execute-keys %{{ %|cat<space>{}<ret> }}
                    set-option buffer coqide_goal_highlight %val{{timestamp}} {}
                  }}
                }}"#,
                self.coq_file,
                self.kak_goal,
                colors.join(" ")
            ),
        )
        .await
    }
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
                    CodeSpan::new(
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

/// Empty the file at the given path if the content is empty, else append the content at the end.
async fn overwrite_file(path: &String, content: String, must_overwrite: bool) -> io::Result<()> {
    let mut file = if content.is_empty() || must_overwrite {
        File::create(path).await?
    } else {
        OpenOptions::new()
            .append(true)
            .create(true)
            .open(path)
            .await?
    };

    // file.set_len(0).await?;
    // file.seek(SeekFrom::Start(0)).await?;
    let cnt = format!("{}\n", content);
    file.write_all(if content.is_empty() {
        &[]
    } else {
        cnt.as_bytes()
    })
    .await?;
    file.flush().await
}
