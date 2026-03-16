// SPDX-FileCopyrightText: 2026 Antoni Szymański
// SPDX-License-Identifier: MPL-2.0

use clap::{Parser, Subcommand};
use gitcredential::GitCredential;
use snafu::{OptionExt, ResultExt, Snafu};
use std::{env, fs, io, path::PathBuf};
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
    #[snafu(display("Failed to read the .git-credentials file"))]
    ReadGitCredentials { source: io::Error, path: PathBuf },
    #[snafu(display("Failed to parse URL: {input:?}"))]
    InvalidUrl { source: url::ParseError, input: String },
}

fn lookup_credential(gc: &GitCredential) -> Result<Option<GitCredential>, LookupError> {
    let path = locate_git_credentials().context(LocateGitCredentialsCtx)?;
    let content = match fs::read_to_string(&path) {
        Ok(v) => v,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        e => e.context(ReadGitCredentialsCtx { path })?,
    };
    let entries = parse_git_credentials(&content)?;
    for entry in entries {
        if gc.protocol.as_deref() != Some(entry.scheme()) && gc.host.as_deref() != entry.host_str() {
            continue;
        }
        if let (Some(expected), Some(actual)) = (
            gc.username.as_deref(), //
            Some(entry.username()).filter(|s| !s.is_empty()),
        ) && expected != actual
        {
            continue;
        }
        if let (Some(expected), actual) = (gc.path.as_deref(), trim_prefix(entry.path(), "/"))
            && !expected.starts_with(actual)
        {
            continue;
        }
        return Ok(Some(GitCredential::from_url(&entry)));
    }
    Ok(None)
}

fn locate_git_credentials() -> Option<PathBuf> {
    match env::var_os("GIT_CREDENTIALS").filter(|s| !s.is_empty()) {
        Some(path) => Some(path.into()),
        None => env::home_dir().map(|home| home.join(".git-credentials")),
    }
}

fn parse_git_credentials(input: &str) -> Result<Vec<Url>, LookupError> {
    input
        .lines()
        .map(|input| Url::parse(input).context(InvalidUrlCtx { input }))
        .collect()
}

#[inline]
fn trim_prefix<'a>(s: &'a str, prefix: &'a str) -> &'a str {
    s.strip_prefix(prefix).unwrap_or(s)
}
