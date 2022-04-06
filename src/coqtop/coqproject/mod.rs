use std::{io, path::Path};

pub mod parser;

pub const COQPROJECT: &'static str = "_CoqProject";

pub async fn find_and_parse_from(file: String) -> io::Result<Vec<String>> {
    let file_path = Path::new(&file).canonicalize()?;
    let directory = file_path.parent().unwrap();

    for dir in directory.ancestors() {
        let dir = dir.join(COQPROJECT);
        if dir.exists() {
            let file_args = parser::parse_file(&dir).await;
            let mut args = vec![];

            let mut iterator = file_args.into_iter();
            while let Some(opt) = iterator.next() {
                match opt.as_str() {
                    "-R" => {
                        args.append(&mut vec![
                            opt,
                            iterator.next().ok_or(io::ErrorKind::InvalidData)?,
                            iterator.next().ok_or(io::ErrorKind::InvalidData)?,
                        ]);
                    }
                    "-I" => {
                        args.append(&mut vec![
                            opt,
                            iterator.next().ok_or(io::ErrorKind::InvalidData)?,
                        ]);
                    }
                    "-Q" => {
                        args.append(&mut vec![
                            opt,
                            iterator.next().ok_or(io::ErrorKind::InvalidData)?,
                            iterator.next().ok_or(io::ErrorKind::InvalidData)?,
                        ]);
                    }
                    "-arg" => {
                        args.append(&mut parser::parse(
                            &iterator.next().ok_or(io::ErrorKind::InvalidData)?,
                        ));
                    }
                    _ => {}
                }
            }

            return Ok(args);
        }
    }

    Ok(vec![])
}
