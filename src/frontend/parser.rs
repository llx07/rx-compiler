use std::sync::{Arc, Mutex};

use antlr4_runtime::{CommonTokenStream, ErrorListener, InputStream, Recognizer, SyntaxErrorEvent};

use crate::diagnostic::{CompileResult, Diagnostic};
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

pub fn parse(tokens: CommonTokenStream<RxLexer<InputStream>>) -> CompileResult<()> {
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
        Err(_) => {
            return Err(Diagnostic::new("unkown error at parser", 0..0));
        }
    };

    return Ok(());
}
