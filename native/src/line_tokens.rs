use crate::breakable_entry::{BreakableEntry, ConvertType};

#[derive(Debug, Clone)]
pub enum LineToken {
    // this is all bodil's fault
    CollapsingNewLine,
    HardNewLine,
    SoftNewline,
    Indent { depth: u32 },
    SoftIndent { depth: u32 },
    Keyword { keyword: String },
    DirectPart { part: String },
    CommaSpace,
    Comma,
    Space,
    Dot,
    ColonColon,
    LonelyOperator,
    OpenSquareBracket,
    CloseSquareBracket,
    OpenParen,
    CloseParen,
    BreakableEntry(BreakableEntry),
    Op { op: String },
    DoubleQuote,
    LTStringContent { content: String },
    SingleSlash,
    Comment { contents: String },
    Delim { contents: String },
    End,
}

impl LineToken {
    pub fn as_single_line(self) -> LineToken {
        match self {
            Self::CollapsingNewLine => LineToken::DirectPart {
                part: "".to_string(),
            },
            Self::SoftNewline => LineToken::Space,
            Self::SoftIndent { depth: _ } => LineToken::DirectPart {
                part: "".to_string(),
            },
            x => x,
        }
    }

    pub fn as_multi_line(self) -> LineToken {
        self
    }

    pub fn is_newline(&self) -> bool {
        match self {
            Self::HardNewLine => true,
            Self::SoftNewline => true,
            Self::CollapsingNewLine => true,
            Self::DirectPart { part } => {
                if part == "\n" {
                    panic!("shouldn't ever have a single newline direct part");
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    pub fn to_string(self) -> String {
        match self {
            Self::CollapsingNewLine => "\n".to_string(),
            Self::HardNewLine => "\n".to_string(),
            Self::SoftNewline => "\n".to_string(),
            Self::Indent { depth } => (0..depth).map(|_| ' ').collect(),
            Self::SoftIndent { depth } => (0..depth).map(|_| ' ').collect(),
            Self::Keyword { keyword } => keyword,
            Self::DirectPart { part } => part,
            Self::CommaSpace => ", ".to_string(),
            Self::Comma => ",".to_string(),
            Self::Space => " ".to_string(),
            Self::Dot => ".".to_string(),
            Self::ColonColon => "::".to_string(),
            Self::LonelyOperator => "&.".to_string(),
            Self::OpenSquareBracket => "[".to_string(),
            Self::CloseSquareBracket => "]".to_string(),
            Self::OpenParen => "(".to_string(),
            Self::CloseParen => ")".to_string(),
            Self::BreakableEntry(be) => {
                be.as_tokens(ConvertType::SingleLine).into_iter().fold("".to_string(), |accum, tok| {
                    format!("{}{}", accum, tok.to_string()).to_string()
                })
            }
            Self::Op { op } => op,
            Self::DoubleQuote => "\"".to_string(),
            Self::LTStringContent { content } => content,
            Self::SingleSlash => "\\".to_string(),
            Self::Comment { contents } => format!("{}\n", contents),
            Self::Delim { contents } => contents,
            Self::End => "end".to_string(),
        }
    }

    pub fn is_single_line_breakable_garbage(&self) -> bool {
        match self {
            Self::Comma => true,
            Self::Space => true,
            Self::SoftNewline => true,
            Self::DirectPart{part} => (part == &"".to_string()),
            _ => false,
        }
    }
}
