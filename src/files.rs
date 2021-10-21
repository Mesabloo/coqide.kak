pub static COQTOP: &'static str = "coqidetop";

pub fn goal_file(tmp_dir: &String) -> String {
    format!("{}/goal", tmp_dir)
}

pub fn result_file(tmp_dir: &String) -> String {
    format!("{}/result", tmp_dir)
}

pub fn command_file(tmp_dir: &String) -> String {
    format!("{}/cmd.sock", tmp_dir)
}

