use smallvec::SmallVec;

#[derive(Debug)]
pub struct RichTextParser<'a> {
    text: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RichTextToken<'a> {
    Text(&'a str),
    TagOpen { raw: &'a str, tag_name: &'a str, value: SmallVec<[&'a str; 1]> },
    TagClose { raw: &'a str, tag_name: Option<&'a str> },
}

pub fn parse(text: &str) -> RichTextParser {
    RichTextParser { text }
}

impl<'a> Iterator for RichTextParser<'a> {
    type Item = RichTextToken<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut chars = self.text.char_indices().peekable();
        match chars.next()? {
            (_, '<') => match chars.peek() {
                Some((_, '/')) => 'close: {
                    chars.next();
                    while chars.next_if(|(_, c)| c.is_whitespace()).is_some() {}
                    let Some(&(tag_name_start, _)) = chars.peek() else {
                        break 'close;
                    };
                    while chars.next_if(|(_, c)| matches!(c, 'a'..='z' | 'A'..='Z' | '0'..='9' | '_')).is_some() {}
                    let Some(&(tag_name_end, _)) = chars.peek() else {
                        break 'close;
                    };
                    while chars.next_if(|(_, c)| c.is_whitespace()).is_some() {}
                    let Some((i, '>')) = chars.next() else {
                        break 'close;
                    };
                    let (head, tail) = self.text.split_at(i + 1);
                    self.text = tail;
                    return Some(RichTextToken::TagClose {
                        raw: head,
                        tag_name: (tag_name_start < tag_name_end).then_some(&head[tag_name_start..tag_name_end]),
                    });
                }
                Some((_, c)) if matches!(c, 'a'..='z' | 'A'..='Z' | '0'..='9' | '_') || c.is_whitespace() => 'open: {
                    while chars.next_if(|(_, c)| c.is_whitespace()).is_some() {}
                    let Some((tag_name_start, 'a'..='z' | 'A'..='Z' | '0'..='9' | '_')) = chars.next() else {
                        break 'open;
                    };
                    while chars.next_if(|(_, c)| matches!(c, 'a'..='z' | 'A'..='Z' | '0'..='9' | '_')).is_some() {}
                    let Some(&(tag_name_end, _)) = chars.peek() else {
                        break 'open;
                    };
                    while chars.next_if(|(_, c)| c.is_whitespace()).is_some() {}
                    match chars.next() {
                        Some((i, '=')) => {
                            let mut value_start = i + 1;
                            let mut value = SmallVec::new();
                            loop {
                                while chars.next_if(|&(_, c)| c != '\\' && c != '>').is_some() {}
                                let Some((value_end, c)) = chars.next() else {
                                    break 'open;
                                };
                                value.push(&self.text[value_start..value_end]);
                                match c {
                                    '\\' => {
                                        let Some((i, _)) = chars.next() else {
                                            break 'open;
                                        };
                                        value_start = i;
                                    }
                                    '>' => {
                                        let (head, tail) = self.text.split_at(value_end + 1);
                                        self.text = tail;
                                        return Some(RichTextToken::TagOpen {
                                            raw: head,
                                            tag_name: &head[tag_name_start..tag_name_end],
                                            value,
                                        });
                                    }
                                    _ => unreachable!(),
                                }
                            }
                        }
                        Some((i, '>')) => {
                            let (head, tail) = self.text.split_at(i + 1);
                            self.text = tail;
                            return Some(RichTextToken::TagOpen {
                                raw: head,
                                tag_name: &head[tag_name_start..tag_name_end],
                                value: SmallVec::new(),
                            });
                        }
                        None | Some((_, _)) => {}
                    }
                }
                None | Some((_, _)) => {}
            },
            (_, '\\') => {
                let text = if chars.next().is_some() {
                    let i = chars.find_map(|(i, c)| matches!(c, '<' | '\\').then_some(i)).unwrap_or(self.text.len());
                    let (head, tail) = self.text.split_at(i);
                    self.text = tail;
                    &head[1..]
                } else {
                    "\\"
                };
                return Some(RichTextToken::Text(text));
            }
            (_, _) => {}
        }
        let i = chars.find_map(|(i, c)| matches!(c, '<' | '\\').then_some(i)).unwrap_or(self.text.len());
        let (head, tail) = self.text.split_at(i);
        self.text = tail;
        Some(RichTextToken::Text(head))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use smallvec::smallvec;

    #[test]
    fn test_parse() {
        assert_eq!(
            parse("Hello, <b>world</b>!").collect::<Vec<_>>(),
            vec![
                RichTextToken::Text("Hello, "),
                RichTextToken::TagOpen { raw: "<b>", tag_name: "b", value: smallvec![] },
                RichTextToken::Text("world"),
                RichTextToken::TagClose { raw: "</b>", tag_name: Some("b") },
                RichTextToken::Text("!"),
            ]
        );
        assert_eq!(
            parse("<color=white></color>").collect::<Vec<_>>(),
            vec![
                RichTextToken::TagOpen {
                    raw: "<color=white>",
                    tag_name: "color",
                    value: smallvec!["white"],
                },
                RichTextToken::TagClose { raw: "</color>", tag_name: Some("color") },
            ]
        );
        assert_eq!(
            parse("<color=white>\\<\\</color>").collect::<Vec<_>>(),
            vec![
                RichTextToken::TagOpen {
                    raw: "<color=white>",
                    tag_name: "color",
                    value: smallvec!["white"],
                },
                RichTextToken::Text("<"),
                RichTextToken::Text("</color>"),
            ]
        );
        assert_eq!(
            parse("< text_color = wh\\>ite>\\<</ text_color ></>").collect::<Vec<_>>(),
            vec![
                RichTextToken::TagOpen {
                    raw: "< text_color = wh\\>ite>",
                    tag_name: "text_color",
                    value: smallvec![" wh", ">ite"],
                },
                RichTextToken::Text("<"),
                RichTextToken::TagClose { raw: "</ text_color >", tag_name: Some("text_color") },
                RichTextToken::TagClose { raw: "</>", tag_name: None },
            ]
        );
    }
}
