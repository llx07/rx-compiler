mod diagnostic;
mod frontend;
mod generated;

use std::path::PathBuf;
use std::{error::Error, fs};

use antlr4_runtime::Token as _;
use clap::{Parser, ValueEnum};

use crate::generated::rx_lexer;

#[derive(Parser)]

struct Args {
    file: PathBuf,
    #[arg(long, short, value_enum)]
    stage: CompileStage,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]

enum CompileStage {
    Lexer,
    Parser,
    Semantic,
    CodeGen,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    // println!("{:?} {:?}", args.file.to_str(), args.stage);

    let source = fs::read_to_string(&args.file)?;

    let tokens= match frontend::lexer_parse(&source) {
        Ok(tokens) => tokens,
        Err(diag) => {
            diag.show(args.file.to_str().unwrap(), &source);
            std::process::exit(1);
        }
    };

    if args.stage == CompileStage::Lexer {
        let vocabulary = rx_lexer::metadata().vocabulary();

        for token in tokens.tokens() {
            println!(
                "type={} channel={} text={:?}",
                vocabulary.display_name(token.token_type()),
                token.channel(),
                token.text(),
            );
        }
        return Ok(());
    }

    let ast = match frontend::parser_parse(tokens) {
        Ok(ast) => ast,
        Err(diag) => {
            diag.show(args.file.to_str().unwrap(), &source);
            std::process::exit(1);
        }
    };

    Ok(())
    // return compile();
}
