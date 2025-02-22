use crate::DumpFileError;
use crate::DumpFileError::ReadError;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::str;

const LINE_SEPARATOR: char = ';';
const LINE_SEPARATOR_PLACEHOLDER: &str = "<<TMP>>";

pub fn list_queries_from_dump_file<'a, S, F>(
    dump_file_path: S,
    comment_chars: &str,
    query: F,
) -> Result<(), DumpFileError>
where
    S: Into<&'a str>,
    F: FnMut(&str),
{
    let file = match File::open(dump_file_path.into()) {
        Ok(file) => file,
        Err(_) => return Err(DumpFileError::DoesNotExist),
    };

    let reader = BufReader::new(file);
    list_queries_from_dump_reader(reader, comment_chars, query)
}

pub fn list_queries_from_dump_reader<R, F>(
    mut dump_reader: BufReader<R>,
    comment_chars: &str,
    mut query: F,
) -> Result<(), DumpFileError>
where
    R: Read,
    F: FnMut(&str),
{
    let mut count_empty_lines = 0;
    let mut buf_bytes: Vec<u8> = Vec::new();
    let mut is_comment_array = Vec::with_capacity(comment_chars.len());
    let mut line_buf_bytes: Vec<u8> = Vec::new();

    loop {
        let bytes = dump_reader.read_until(b'\n', &mut line_buf_bytes);
        let total_bytes = match bytes {
            Ok(bytes) => bytes,
            Err(err) => return Err(ReadError(err)),
        };

        let last_real_char_idx = if buf_bytes.len() > 1 {
            buf_bytes.len() - 2
        } else if buf_bytes.len() == 1 {
            1
        } else {
            0
        };

        // check end of line is a ';' char - it would mean it's the end of the query
        let is_last_by_end_of_query = match line_buf_bytes.get(last_real_char_idx) {
            Some(byte) => *byte == b';',
            None => false,
        };

        // E.g for Postgres comments starts with "--"
        // if the line starts with "--" then is_comment_array = [true, true] and is_comment is true
        // if the line starts only with "-" then is_comment_array = [true, false] and is_comment is false
        let _ = is_comment_array.clear();
        for (i, char_byte) in comment_chars.bytes().enumerate() {
            is_comment_array.insert(
                i,
                match line_buf_bytes.get(i) {
                    Some(byte) => *byte == char_byte,
                    None => false,
                },
            );
        }

        let is_comment_array_true = is_comment_array
            .iter()
            .filter(|x| **x == true)
            .collect::<Vec<_>>();

        let is_comment = is_comment_array_true.len() == is_comment_array.len();

        if is_comment {
            let comment_str = str::from_utf8(line_buf_bytes.as_slice()).unwrap(); // FIXME remove unwrap
            query(comment_str);
            line_buf_bytes.clear();
        } else {
            let _ = buf_bytes.append(&mut line_buf_bytes);
        }

        if total_bytes <= 1 || is_last_by_end_of_query {
            if count_empty_lines == 0 && buf_bytes.len() > 1 {
                let query_str = str::from_utf8(buf_bytes.as_slice()).unwrap(); // FIXME remove unwrap

                // split query_str by ';' in case of multiple queries are inside the string
                let query_string = query_str.replace(";'", LINE_SEPARATOR_PLACEHOLDER);
                let queries_str = query_string.split(";").collect::<Vec<&str>>();

                if queries_str.len() == 1 {
                    // there is a only one query inside the str
                    query(query_str);
                } else {
                    // iterate and send all queries one by one
                    for query_str in queries_str {
                        let query_str = query_str.trim().replace(LINE_SEPARATOR_PLACEHOLDER, ";'");
                        if !query_str.is_empty() {
                            let query_str = format!("{};", query_str);
                            query(query_str.as_str());
                        }
                    }
                }
            }

            let _ = buf_bytes.clear();
            count_empty_lines += 1;
        } else {
            count_empty_lines = 0;
        }

        // 49 is an empirical number -
        // not too large to avoid looping too much time, and not too small to avoid wrong end of query
        if count_empty_lines > 49 {
            // EOF?
            break;
        }
    }

    Ok(())
}
