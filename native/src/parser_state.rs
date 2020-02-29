use crate::comment_block::CommentBlock;
use crate::format::{format_inner_string, StringType};
use crate::line_metadata::LineMetadata;
use crate::delimiters::BreakableDelims;
use crate::line_tokens::*;
use crate::render_queue_writer::RenderQueueWriter;
use crate::ripper_tree_types::StringContentPart;
use crate::types::{ColNumber, LineNumber};
use crate::breakable_entry::BreakableEntry;
use std::io::{self, Cursor, Write};
use std::mem;
use std::str;

fn insert_at<T>(idx: usize, target: &mut Vec<T>, input: &mut Vec<T>) {
    let drain = input.drain(..);
    let mut idx = idx;
    for item in drain {
        target.insert(idx, item);
        idx += 1;
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FormattingContext {
    Main,
    Assign,
    Binary,
    ClassOrModule,
    Def,
    CurlyBlock,
    ArgsList,
    Heredoc
}

#[derive(Clone, Copy)]
struct IndentDepth {
    depth: ColNumber,
}

impl IndentDepth {
    fn new() -> Self {
        IndentDepth { depth: 0 }
    }

    fn increment(&mut self) {
        self.depth += 1;
    }

    fn decrement(&mut self) {
        self.depth -= 1;
    }

    fn get(self) -> ColNumber {
        self.depth
    }
}

pub struct HeredocString {
    symbol: String,
    squiggly: bool,
    buf: Vec<u8>,
}

impl HeredocString {
    pub fn new(symbol: String, squiggly: bool, buf: Vec<u8>) -> Self {
        HeredocString {
            symbol,
            squiggly,
            buf,
        }
    }
}

pub struct ParserState {
    depth_stack: Vec<IndentDepth>,
    start_of_line: Vec<bool>,
    surpress_comments_stack: Vec<bool>,
    render_queue: Vec<LineToken>,
    current_orig_line_number: LineNumber,
    comments_hash: LineMetadata,
    heredoc_strings: Vec<HeredocString>,
    comments_to_insert: CommentBlock,
    breakable_entry_stack: Vec<BreakableEntry>,
    formatting_context: Vec<FormattingContext>,
    absorbing_indents: i32,
}

impl ParserState {
    pub fn new(lm: LineMetadata) -> Self {
        ParserState {
            depth_stack: vec![IndentDepth::new()],
            start_of_line: vec![true],
            surpress_comments_stack: vec![false],
            render_queue: vec![],
            current_orig_line_number: 0,
            comments_hash: lm,
            heredoc_strings: vec![],
            comments_to_insert: CommentBlock::new(vec![]),
            breakable_entry_stack: vec![],
            formatting_context: vec![FormattingContext::Main],
            absorbing_indents: 0,
        }
    }

    fn consume_to_render_queue(self) -> Vec<LineToken> {
        self.render_queue
    }

    pub fn on_line(&mut self, line_number: LineNumber) {
        if line_number < self.current_orig_line_number {
            return;
        }

        if line_number - self.current_orig_line_number >= 2 {
            self.insert_extra_newline_at_last_newline();
        }

        if let Some(be) = self.breakable_entry_stack.last_mut() {
            be.push_line_number(line_number);
        }

        self.current_orig_line_number = line_number;

        let comments = self.comments_hash.extract_comments_to_line(line_number);
        if comments.is_none() {
            return;
        }

        if !self
            .surpress_comments_stack
            .last()
            .expect("comments stack is never empty")
        {
            self.insert_comment_collection(
                comments.expect("we checked it was none at the top of the function"),
            )
        }
    }

    fn insert_extra_newline_at_last_newline(&mut self) {
        let idx = self.index_of_prev_hard_newline();
        let insert_idx = match idx {
            Some(idx) => idx + 1,
            None => 0,
        };

        if self.formatting_context.last() != Some(&FormattingContext::Heredoc) {
            eprintln!("actually writing extra newline {:?}", self.formatting_context);
            insert_at(
                insert_idx,
                &mut self.render_queue,
                &mut vec!(LineToken::HardNewLine)
            );
        }
    }

    pub fn insert_comment_collection(&mut self, comments: CommentBlock) {
        self.comments_to_insert.merge(comments);
    }

    pub fn emit_indent(&mut self) {
        self.push_token(LineToken::Indent {
            depth: self.current_spaces(),
        });
    }

    pub fn emit_op(&mut self, op: String) {
        self.push_token(LineToken::Op { op: op });
    }

    pub fn emit_double_quote(&mut self) {
        self.push_token(LineToken::DoubleQuote);
    }

    pub fn emit_string_content(&mut self, s: String) {
        self.push_token(LineToken::LTStringContent { content: s });
    }

    fn current_spaces(&self) -> ColNumber {
        2 * self
            .depth_stack
            .last()
            .expect("depth stack is never empty")
            .get()
    }

    pub fn emit_ident(&mut self, ident: String) {
        self.push_token(LineToken::DirectPart { part: ident });
    }

    pub fn emit_keyword(&mut self, kw: String) {
        self.push_token(LineToken::Keyword { keyword: kw });
    }

    pub fn emit_def_keyword(&mut self) {
        self.push_token(LineToken::Keyword {
            keyword: "def".to_string(),
        });
    }

    pub fn emit_case_keyword(&mut self) {
        self.push_token(LineToken::Keyword {
            keyword: "case".to_string(),
        });
    }

    pub fn emit_when_keyword(&mut self) {
        self.push_token(LineToken::Keyword {
            keyword: "when".to_string(),
        });
    }

    pub fn emit_do_keyword(&mut self) {
        self.push_token(LineToken::Keyword {
            keyword: "do".to_string(),
        });
    }

    pub fn emit_class_keyword(&mut self) {
        self.push_token(LineToken::Keyword {
            keyword: "class".to_string(),
        });
    }

    pub fn emit_module_keyword(&mut self) {
        self.push_token(LineToken::Keyword {
            keyword: "module".to_string(),
        });
    }

    pub fn emit_rescue(&mut self) {
        self.push_token(LineToken::Keyword {
            keyword: "rescue".to_string(),
        });
    }

    pub fn emit_ensure(&mut self) {
        self.push_token(LineToken::Keyword {
            keyword: "ensure".to_string(),
        });
    }

    pub fn emit_begin(&mut self) {
        self.push_token(LineToken::Keyword {
            keyword: "begin".to_string(),
        });
    }

    pub fn emit_soft_indent(&mut self) {
        self.push_token(LineToken::SoftIndent {
            depth: self.current_spaces(),
        });
    }

    pub fn emit_comma(&mut self) {
        self.push_token(LineToken::Comma);
    }

    pub fn emit_soft_newline(&mut self) {
        self.push_token(LineToken::SoftNewline);
    }

    pub fn emit_collapsing_newline(&mut self) {
        self.push_token(LineToken::CollapsingNewLine);
    }

    pub fn emit_def(&mut self, def_name: String) {
        self.emit_def_keyword();
        self.push_token(LineToken::DirectPart {
            part: format!(" {}", def_name),
        });
    }

    pub fn emit_newline(&mut self) {
        self.shift_comments();
        self.push_token(LineToken::HardNewLine);
        self.render_heredocs(false);
    }

    pub fn emit_end(&mut self) {
        if !self
            .render_queue
            .last()
            .map(|x| x.is_newline())
            .unwrap_or(false)
        {
            self.emit_newline();
        }
        if self.at_start_of_line() {
            self.emit_indent();
        }
        self.push_token(LineToken::End);
    }

    pub fn shift_comments(&mut self) {
        let idx_of_prev_hard_newline = self.index_of_prev_hard_newline();

        if self.comments_to_insert.has_comments() {
            let insert_index = match idx_of_prev_hard_newline {
                Some(idx) => idx + 1,
                None => 0,
            };

            let mut new_comments = CommentBlock::new(vec![]);
            mem::swap(&mut new_comments, &mut self.comments_to_insert);

            insert_at(
                insert_index,
                &mut self.render_queue,
                &mut new_comments.into_line_tokens(),
            );
            self.comments_to_insert = CommentBlock::new(vec![]);
        }
    }

    pub fn index_of_prev_hard_newline(&self) -> Option<usize> {
        self.render_queue.iter().rposition(|v| v.is_newline())
    }

    pub fn emit_else(&mut self) {
        self.push_token(LineToken::Keyword {
            keyword: "else".into(),
        });
    }

    pub fn emit_comma_space(&mut self) {
        self.push_token(LineToken::CommaSpace)
    }

    pub fn emit_space(&mut self) {
        self.push_token(LineToken::Space);
    }

    pub fn emit_dot(&mut self) {
        self.push_token(LineToken::Dot);
    }

    pub fn emit_colon_colon(&mut self) {
        self.push_token(LineToken::ColonColon);
    }

    pub fn emit_lonely_operator(&mut self) {
        self.push_token(LineToken::LonelyOperator);
    }

    pub fn with_surpress_comments<F>(&mut self, surpress: bool, f: F)
    where
        F: FnOnce(&mut ParserState),
    {
        self.surpress_comments_stack.push(surpress);
        f(self);
        self.surpress_comments_stack.pop();
    }

    pub fn with_formatting_context<F>(&mut self, fc: FormattingContext, f: F)
    where
        F: FnOnce(&mut ParserState),
    {
        self.formatting_context.push(fc);
        f(self);
        self.formatting_context.pop();
    }

    pub fn with_absorbing_indent_block<F>(&mut self, f: F)
    where
        F: FnOnce(&mut ParserState),
    {
        let was_absorving = self.absorbing_indents != 0;
        self.absorbing_indents += 1;
        if was_absorving {
            f(self);
        } else {
            self.new_block(f);
        }
        self.absorbing_indents -= 1;
    }

    pub fn new_block<F>(&mut self, f: F)
    where
        F: FnOnce(&mut ParserState),
    {
        let ds_length = self.depth_stack.len();
        self.depth_stack[ds_length - 1].increment();
        f(self);
        self.depth_stack[ds_length - 1].decrement();
    }

    pub fn dedent<F>(&mut self, f: F)
    where
        F: FnOnce(&mut ParserState),
    {
        let ds_length = self.depth_stack.len();
        self.depth_stack[ds_length - 1].decrement();
        f(self);
        self.depth_stack[ds_length - 1].increment();
    }

    pub fn with_start_of_line<F>(&mut self, start_of_line: bool, f: F)
    where
        F: FnOnce(&mut ParserState),
    {
        self.start_of_line.push(start_of_line);
        f(self);
        self.start_of_line.pop();
    }

    pub fn at_start_of_line(&self) -> bool {
        *self
            .start_of_line
            .last()
            .expect("start of line is never_empty")
    }

    pub fn current_formatting_context(&self) -> FormattingContext {
        *self
            .formatting_context
            .last()
            .expect("formatting context is never empty")
    }

    pub fn new_with_depth_stack_from(ps: &ParserState) -> Self {
        let mut next_ps = ParserState::new(LineMetadata::new());
        next_ps.depth_stack = ps.depth_stack.clone();
        next_ps.current_orig_line_number = ps.current_orig_line_number;
        next_ps
    }

    pub fn render_with_blank_state<F>(ps: &mut ParserState, f: F) -> ParserState
    where
        F: FnOnce(&mut ParserState),
    {
        let mut next_ps = ParserState::new_with_depth_stack_from(ps);
        f(&mut next_ps);
        next_ps
    }

    pub fn push_heredoc_content(
        &mut self,
        symbol: String,
        is_squiggly: bool,
        parts: Vec<StringContentPart>,
    ) {
        let mut next_ps = ParserState::render_with_blank_state(self, |n| {
            format_inner_string(n, parts, StringType::Heredoc);
            n.emit_newline();
        });

        for hs in next_ps.heredoc_strings.drain(0..) {
            self.heredoc_strings.push(hs);
        }

        let data = next_ps.render_to_buffer();
        self.heredoc_strings
            .push(HeredocString::new(symbol, is_squiggly, data));
    }

    pub fn will_render_as_multiline<F>(&mut self, f: F) -> bool
    where
        F: FnOnce(&mut ParserState),
    {
        let mut next_ps = ParserState::new_with_depth_stack_from(self);
        f(&mut next_ps);
        let data = next_ps.render_to_buffer();

        // unsafe because we got the source code from the ruby parser
        // and only in wildly exceptional circumstances will it not be
        // valid utf8 and also we're only using this to newline match
        // which should be very hard to break. The unsafe conversion
        // here skips a utf8 check which is faster.
        unsafe {
            let s = str::from_utf8_unchecked(&data).to_string();
            s.trim().chars().any(|v| v == '\n')
        }
    }

    fn render_to_buffer(self) -> Vec<u8> {
        let mut bufio = Cursor::new(Vec::new());
        self.write(&mut bufio).expect("in memory io cannot fail");
        bufio.set_position(0);
        bufio.into_inner()
    }

    pub fn render_heredocs(&mut self, skip: bool) {
        while !self.heredoc_strings.is_empty() {
            let mut next_heredoc = self.heredoc_strings.pop().expect("we checked it's there");
            let want_newline = match self.render_queue.last() {
                Some(x) => !x.is_newline(),
                None => true,
            };
            if want_newline {
                self.push_token(LineToken::HardNewLine);
            }

            if let Some(b'\n') = next_heredoc.buf.last() {
                next_heredoc.buf.pop();
            };

            if let Some(b'\n') = next_heredoc.buf.last() {
                next_heredoc.buf.pop();
            };

            self.with_formatting_context(FormattingContext::Heredoc, |ps| {
                ps.wind_n_lines(next_heredoc.buf.iter().filter(|c| c == &&b'\n').count());
            });
            self.push_token(LineToken::DirectPart {
                part: String::from_utf8(next_heredoc.buf).expect("hereoc is utf8"),
            });
            self.emit_newline();
            if next_heredoc.squiggly {
                self.emit_indent();
            }

            self.emit_ident(next_heredoc.symbol.replace("'", ""));
            if !skip {
                self.emit_newline();
            }
        }
    }

    pub fn breakable_of<F>(&mut self, delims: BreakableDelims, f: F)
    where
        F: FnOnce(&mut ParserState),
    {
        eprintln!("called: {:?}", delims);
        let mut be = BreakableEntry::new(
            self.current_spaces(),
            delims,
        );
        be.push_line_number(self.current_orig_line_number);

        self.breakable_entry_stack.push(be);

        self.emit_collapsing_newline();
        self.new_block(|ps| {
            f(ps);
        });

        self.emit_soft_indent();

        let insert_be = self
            .breakable_entry_stack
            .pop()
            .expect("cannot have empty here because we just pushed");
        self.push_token(LineToken::BreakableEntry(insert_be));
    }

    pub fn emit_open_square_bracket(&mut self) {
        self.push_token(LineToken::OpenSquareBracket);
    }

    pub fn emit_close_square_bracket(&mut self) {
        self.push_token(LineToken::CloseSquareBracket);
    }

    pub fn emit_slash(&mut self) {
        self.push_token(LineToken::SingleSlash);
    }

    pub fn emit_open_paren(&mut self) {
        self.push_token(LineToken::OpenParen);
    }

    pub fn emit_close_paren(&mut self) {
        self.push_token(LineToken::CloseParen);
    }

    pub fn write<W: Write>(self, writer: &mut W) -> io::Result<()> {
        let rqw = RenderQueueWriter::new(self.consume_to_render_queue());
        rqw.write(writer)
    }

    pub fn push_token(&mut self, t: LineToken) {
        if self.breakable_entry_stack.is_empty() {
            self.render_queue.push(t);
        } else {
            self.breakable_entry_stack
                .last_mut()
                .expect("we checked it wasn't empty")
                .push(t);
        }
    }

    pub fn is_absorbing_indents(&self) -> bool {
        self.absorbing_indents >= 1
    }

    pub fn wind_line_forward(&mut self) {
        self.on_line(self.current_orig_line_number + 1);
    }

    pub fn wind_n_lines(&mut self, n: usize) {
        self.on_line(self.current_orig_line_number + (n as u64));
    }
}
