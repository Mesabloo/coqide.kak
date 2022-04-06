use std::{convert::identity, fs::File, io::Read, path::Path};

use nom::{
    branch::alt,
    character::complete::{anychar, char, line_ending, multispace1, satisfy},
    combinator::{cut, map, peek, value},
    multi::{many1, many_till, separated_list0},
    sequence::{delimited, preceded},
    IResult,
};

pub async fn parse_file<P>(path: P) -> Vec<String>
where
    P: AsRef<Path>,
{
    let mut file = File::open(path).unwrap();
    tokio::task::block_in_place(move || {
        let mut content = String::new();
        file.read_to_string(&mut content).unwrap();
        parse(&content)
    })
}

pub fn parse(text: &str) -> Vec<String> {
    separated_list0(multispace1, parse_flag)(text)
        .unwrap()
        .1
        .into_iter()
        .filter_map(identity)
        .collect()
}

// --------------------------------------------------------------------------

fn parse_flag(input: &str) -> IResult<&str, Option<String>> {
    alt((parse_option, parse_string, parse_word, parse_comment))(input)
}

fn parse_option(input: &str) -> IResult<&str, Option<String>> {
    preceded(
        char('-'),
        cut(map(parse_word, |txt| txt.map(|txt| "-".to_string() + &txt))),
    )(input)
}

fn parse_word(input: &str) -> IResult<&str, Option<String>> {
    let not_whitespace = satisfy(char::is_whitespace);

    map(many1(not_whitespace), |txt| {
        Some(String::from_iter(txt.into_iter()))
    })(input)
}

fn parse_string(input: &str) -> IResult<&str, Option<String>> {
    map(
        delimited(
            char('"'),
            cut(map(many_till(anychar, peek(char('"'))), |s| s.0)),
            char('"'),
        ),
        |txt| Some(String::from_iter(txt.into_iter())),
    )(input)
}

fn parse_comment(input: &str) -> IResult<&str, Option<String>> {
    preceded(char('#'), cut(value(None, many_till(anychar, line_ending))))(input)
}
