#![feature(never_type)]
use std::{env, sync::Arc};

use anyhow::Context;
use frontend::Parser;
use interpreter::{Interpreter, Value};

pub mod frontend;
pub mod interpreter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = env::args();
    // skip name
    let name = args.next().context("usage: scrapelect <filename> <url>")?;

    let filename = args
        .next()
        .with_context(|| format!("usage: {name} <filename> <url>"))?;

    let url = args
        .next()
        .with_context(|| format!("usage: {name} <filename = {filename}> <url>"))?;

    let pgm = std::fs::read_to_string(&filename)
        .with_context(|| format!("error reading file {filename}"))?;

    let parser = Parser::new(&pgm);

    let (ast, head) = match parser.parse() {
        Ok(x) => x,
        Err(e) => anyhow::bail!("Parse Error: {e}"),
    };

    let interpreter = Interpreter::new(&ast);

    let results = interpreter.interpret(url, head).await?;

    let results = Value::Structure(
        results
            .0
            .into_iter()
            .map(|(k, v)| (Arc::from(&*k), v))
            .collect(),
    );

    println!("{}", serde_json::to_string_pretty(&results)?);

    Ok(())
}
