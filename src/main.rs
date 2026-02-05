// SPDX-FileCopyrightText: 2026 Antoni Szymański
// SPDX-License-Identifier: MPL-2.0

use clap::{Parser, Subcommand};
use gitcredential::GitCredential;
use snafu::{OptionExt, ResultExt, Snafu};
use std::{
    env,
    fs::File,
    io::{self, BufRead, BufReader},
    path::PathBuf,
};
use url::Url;

#[derive(Parser)]
#[command(version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Return a matching credential, if any exists.
    Get,
    /// Store the credential.
    Store,
    /// Remove matching credentials, if any, from the storage.
    Erase,
}

#[derive(Debug, Snafu)]
#[snafu(context(suffix(Ctx)))]
enum Error {
    #[snafu(display("Failed to parse credential from stdin"))]
    Parse { source: gitcredential::FromReaderError },
    #[snafu(display("Failed to lookup credential"))]
    Lookup { source: LookupError },
    #[snafu(display("Failed to write credential to stdout"))]
    Write { source: io::Error },
}

#[snafu::report]
fn main() -> Result<(), Error> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Get => {
            let input = GitCredential::from_reader(io::stdin()).context(ParseCtx)?;
            if let Some(output) = lookup_credential(&input).context(LookupCtx)? {
                output.to_writer(io::stdout()).context(WriteCtx)?;
            }
        }
        Commands::Store | Commands::Erase => {}
    }
    Ok(())
}

#[derive(Debug, Snafu)]
#[snafu(context(suffix(Ctx)))]
enum LookupError {
    #[snafu(display("Failed to locate the .git-credentials file"))]
    LocateGitCredentials,
    #[snafu(display("Failed to open the .git-credentials file"))]
    OpenGitCredentials { source: io::Error, path: PathBuf },
    #[snafu(display("Failed to read line from input reader"))]
    ReadLine { source: io::Error },
    #[snafu(display("Failed to parse URL: {input:?}"))]
    InvalidUrl { source: url::ParseError, input: String },
}

fn lookup_credential(gc: &GitCredential) -> Result<Option<GitCredential>, LookupError> {
    let path = locate_git_credentials().context(LocateGitCredentialsCtx)?;
    let file = match File::open(&path) {
        Ok(v) => v,
        Err(e) => {
            if e.kind() == io::ErrorKind::NotFound {
                return Ok(None);
            }
            return Err(LookupError::OpenGitCredentials { source: e, path });
        }
    };
    let buf_reader = BufReader::new(file);
    for line in buf_reader.lines() {
        let line = line.context(ReadLineCtx)?;
        let url = Url::parse(&line).context(InvalidUrlCtx { input: line })?;
        if gc.protocol.as_deref() != Some(url.scheme()) && gc.host.as_deref() != url.host_str() {
            continue;
        }
        if let (Some(expected), Some(actual)) = (
            gc.username.as_deref(), //
            Some(url.username()).filter(|s| !s.is_empty()),
        ) && expected != actual
        {
            continue;
        }
        if let (Some(expected), actual) = (gc.path.as_deref(), trim_prefix(url.path(), "/"))
            && !expected.starts_with(actual)
        {
            continue;
        }
        return Ok(Some(GitCredential::from_url(&url)));
    }
    Ok(None)
}

fn locate_git_credentials() -> Option<PathBuf> {
    match env::var_os("GIT_CREDENTIALS").filter(|s| !s.is_empty()) {
        Some(path) => Some(path.into()),
        None => env::home_dir().map(|home| home.join(".git-credentials")),
    }
}

#[inline]
fn trim_prefix<'a>(s: &'a str, prefix: &'a str) -> &'a str {
    s.strip_prefix(prefix).unwrap_or(s)
}
