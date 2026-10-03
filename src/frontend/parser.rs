use std::sync::{Arc, Mutex};

use antlr4_runtime::{
    CommonTokenStream, ErrorListener, InputStream, NodeId, Recognizer, SyntaxErrorEvent,
};

use crate::diagnostic::{CompileResult, Diagnostic};
use crate::frontend::ast::Crate;
use crate::frontend::ast_builder::build_ast;
use crate::generated::rx_lexer::RxLexer;
use crate::generated::rx_parser::RxParser;

struct DiagnosticListener {
    diagnostic: Arc<Mutex<Option<Diagnostic>>>,
}

impl<R> ErrorListener<R> for DiagnosticListener
where
    R: Recognizer + ?Sized,
{
    fn syntax_error(&mut self, _recognizer: &R, event: &SyntaxErrorEvent<'_>) {
        if self.diagnostic.lock().unwrap().is_some() {
            return;
        }
        *(self.diagnostic.lock().unwrap()) =
            Some(Diagnostic::new(event.message, event.span.clone().unwrap()));
    }
}

pub fn parse(tokens: CommonTokenStream<RxLexer<InputStream>>) -> CompileResult<Crate> {
    let mut parser = RxParser::new(tokens);
    parser.remove_error_listeners();

    let diagnostic: Arc<Mutex<Option<Diagnostic>>> = Arc::new(Mutex::new(None));
    parser.add_error_listener(DiagnosticListener {
        diagnostic: Arc::clone(&diagnostic),
    });

    let result = parser.crate_();

    let diagnostic = diagnostic.lock().unwrap().take();
    if let Some(diagnostic) = diagnostic {
        return Err(diagnostic);
    }

    let root = match result {
        Ok(root) => root,
        Err(err) => {
            return Err(Diagnostic::new(format!("parser error: {err}"), 0..0));
        }
    };

    let parsed = parser.into_parsed_file(root);

    build_ast(&parsed)
}
