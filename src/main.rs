mod diagnostic;
mod frontend;
mod generated;
mod id_alloc;
mod ir;
mod lower;
mod semantic;

use std::path::{Path, PathBuf};
use std::{
    error::Error,
    fs,
    io::{self, Write},
};

use antlr4_runtime::Token as _;
use clap::{Parser, ValueEnum};

use crate::generated::rx_lexer;
use crate::semantic::semantic_check::semantic_check;

#[derive(Parser)]

struct Args {
    file: PathBuf,
    #[arg(long, short, value_enum)]
    stage: CompileStage,
    #[arg(short, long, value_name = "FILE")]
    output: Option<PathBuf>,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]

enum CompileStage {
    Lexer,
    Parser,
    Semantic,
    CodeGen,
}

fn output_writer(path: Option<&Path>) -> io::Result<io::BufWriter<Box<dyn Write>>> {
    let writer: Box<dyn Write> = match path {
        Some(path) => Box::new(fs::File::create(path)?),
        None => Box::new(io::stdout().lock()),
    };
    Ok(io::BufWriter::new(writer))
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    let source = fs::read_to_string(&args.file)?;

    let tokens = match frontend::lexer::parse(&source) {
        Ok(tokens) => tokens,
        Err(diag) => {
            diag.show(args.file.to_str().unwrap(), &source);
            std::process::exit(1);
        }
    };

    if args.stage == CompileStage::Lexer {
        let vocabulary = rx_lexer::metadata().vocabulary();
        let mut output = output_writer(args.output.as_deref())?;

        for token in tokens.tokens() {
            writeln!(
                output,
                "type={} channel={} text={:?}",
                vocabulary.display_name(token.token_type()),
                token.channel(),
                token.text(),
            )?;
        }
        output.flush()?;
        return Ok(());
    }

    let ast = match frontend::parser::parse(tokens) {
        Ok(ast) => ast,
        Err(diag) => {
            diag.show(args.file.to_str().unwrap(), &source);
            std::process::exit(1);
        }
    };

    if args.stage == CompileStage::Parser {
        let mut output = output_writer(args.output.as_deref())?;
        writeln!(output, "{:#?}", ast)?;
        output.flush()?;
        return Ok(());
    }

    let ctx = match semantic_check(&ast) {
        Ok(ctx) => ctx,
        Err(diags) => {
            for diag in diags {
                diag.show(args.file.to_str().unwrap(), &source);
            }
            std::process::exit(1);
        }
    };

    if args.stage == CompileStage::Semantic {
        let mut output = output_writer(args.output.as_deref())?;
        writeln!(output, "{:#?}", ast)?;
        output.flush()?;
        return Ok(());
    }

    Ok(())
}
