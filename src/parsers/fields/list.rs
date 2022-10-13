/*
 * Copyright Stalwart Labs Ltd. See the COPYING
 * file at the top-level directory of this distribution.
 *
 * Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
 * https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
 * <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
 * option. This file may not be copied, modified, or distributed
 * except according to those terms.
 */

use std::borrow::Cow;

use crate::{parsers::MessageStream, HeaderValue};

struct ListParser<'x> {
    token_start: usize,
    token_end: usize,
    is_token_start: bool,
    tokens: Vec<Cow<'x, str>>,
    list: Vec<Cow<'x, str>>,
}

impl<'x> ListParser<'x> {
    fn add_token(&mut self, stream: &MessageStream<'x>, add_space: bool) {
        if self.token_start > 0 {
            if !self.tokens.is_empty() {
                self.tokens.push(" ".into());
            }
            self.tokens.push(String::from_utf8_lossy(
                &stream.data[self.token_start - 1..self.token_end],
            ));

            if add_space {
                self.tokens.push(" ".into());
            }

            self.token_start = 0;
            self.is_token_start = true;
        }
    }

    fn add_tokens_to_list(&mut self) {
        if !self.tokens.is_empty() {
            self.list.push(if self.tokens.len() == 1 {
                self.tokens.pop().unwrap()
            } else {
                let value = self.tokens.concat();
                self.tokens.clear();
                value.into()
            });
        }
    }
}

impl<'x> MessageStream<'x> {
    pub fn parse_comma_separared(&mut self) -> HeaderValue<'x> {
        let mut parser = ListParser {
            token_start: 0,
            token_end: 0,
            is_token_start: true,
            tokens: Vec::new(),
            list: Vec::new(),
        };

        while let Some(ch) = self.next() {
            match ch {
                b'\n' => {
                    parser.add_token(self, false);
                    if !self.try_next_is_space() {
                        parser.add_tokens_to_list();

                        return match parser.list.len() {
                            1 => HeaderValue::Text(parser.list.pop().unwrap()),
                            0 => HeaderValue::Empty,
                            _ => HeaderValue::TextList(parser.list),
                        };
                    } else {
                        continue;
                    }
                }
                b' ' | b'\t' => {
                    if !parser.is_token_start {
                        parser.is_token_start = true;
                    }
                    continue;
                }
                b'=' if parser.is_token_start && self.peek_char(b'?') => {
                    self.checkpoint();
                    if let Some(token) = self.decode_rfc2047() {
                        parser.add_token(self, true);
                        parser.tokens.push(token.into());
                        continue;
                    }
                    self.restore();
                }
                b',' => {
                    parser.add_token(self, false);
                    parser.add_tokens_to_list();
                    continue;
                }
                b'\r' => continue,
                _ => (),
            }

            if parser.is_token_start {
                parser.is_token_start = false;
            }

            if parser.token_start == 0 {
                parser.token_start = self.offset();
            }

            parser.token_end = self.offset();
        }

        HeaderValue::Empty
    }
}
#[cfg(test)]
mod tests {
    use crate::{parsers::MessageStream, HeaderValue};

    #[test]
    fn parse_comma_separated_text() {
        let inputs = [
            (" one item  \n", vec!["one item"]),
            ("simple, list\n", vec!["simple", "list"]),
            (
                "multi \r\n list, \r\n with, cr lf  \r\n",
                vec!["multi list", "with", "cr lf"],
            ),
            (
                "=?iso-8859-1?q?this is some text?=, in, a, list, \n",
                vec!["this is some text", "in", "a", "list"],
            ),
            (
                concat!(
                    " =?ISO-8859-1?B?SWYgeW91IGNhbiByZWFkIHRoaXMgeW8=?=\n     ",
                    "=?ISO-8859-2?B?dSB1bmRlcnN0YW5kIHRoZSBleGFtcGxlLg==?=\n",
                    " , but, in a list, which, is, more, fun!\n"
                ),
                vec![
                    "If you can read this you understand the example.",
                    "but",
                    "in a list",
                    "which",
                    "is",
                    "more",
                    "fun!",
                ],
            ),
            (
                "=?ISO-8859-1?Q?a?= =?ISO-8859-1?Q?b?=\n , listed\n",
                vec!["ab", "listed"],
            ),
            (
                "ハロー・ワールド, and also, ascii terms\n",
                vec!["ハロー・ワールド", "and also", "ascii terms"],
            ),
        ];

        for input in inputs {
            let str = input.0.to_string();

            match MessageStream::new(str.as_bytes()).parse_comma_separared() {
                HeaderValue::TextList(ids) => {
                    assert_eq!(ids, input.1, "Failed to parse '{:?}'", input.0);
                }
                HeaderValue::Text(id) => {
                    assert!(input.1.len() == 1, "Failed to parse '{:?}'", input.0);
                    assert_eq!(id, input.1[0], "Failed to parse '{:?}'", input.0);
                }
                _ => panic!("Unexpected result"),
            }
        }
    }
}
