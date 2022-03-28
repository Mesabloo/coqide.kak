// NOTE: changes to paths in this file must be reflected back in the
// `rc/coqide.kak` file.

/// Retrieves the path to the goal file given the path to its directory.
pub fn goal_file(tmp_dir: &String) -> String {
    format!("{}/goal", tmp_dir)
}

/// Retrieves the path to the result file given its dirname.
pub fn result_file(tmp_dir: &String) -> String {
    format!("{}/result", tmp_dir)
}

/// Retrieves the file where all logging is done.
pub fn log_file(tmp_dir: &String) -> String {
    format!("{}/log", tmp_dir)
}
