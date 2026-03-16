use anyhow::{anyhow, Result};
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "frisk")]
pub struct Cli {

    #[arg(index = 1, default_value = "default")]
    pub mirror: String,

    #[arg(index = 2, default_value = "-C", allow_hyphen_values = true)]
    pub action: String,

    #[arg(index = 3)]
    pub package: Option<String>,
}

impl Cli {
    pub fn require_package(&self) -> Result<String> {
        self.package
        .clone()
        .ok_or_else(|| anyhow!("this command requires a package name"))
    }
}
