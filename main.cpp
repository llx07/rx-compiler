mod diagnostic;
mod frontend;
mod generated;

use std::{error::Error, fs};
use std::fs::File;
use std::path::PathBuf;

use clap::{Parser, ValueEnum};

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
    println!("{:?} {:?}", args.file.to_str(), args.stage);

    let source = fs::read_to_string(args.file)?;
    
    let ast = match frontend::parse(source){
        Ok(ast) => ast,
        Err(diag) => {
            diag.show();
            std::process::exit(1);
        }
    };
    
    Ok(())
    // return compile();
}
