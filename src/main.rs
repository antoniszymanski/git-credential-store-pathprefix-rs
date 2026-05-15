// SPDX-FileCopyrightText: 2026 Antoni Szymański
// SPDX-License-Identifier: MPL-2.0

use clap::{Parser, Subcommand};
use gitcredential::GitCredential;
use snafu::{ResultExt, Snafu};
use std::{
    env,
    fs::File,
    io::{self, Read},
    path::{Path, PathBuf},
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
    ParseCredential { source: gitcredential::FromReaderError },
    #[snafu(display("Failed to write credential to stdout"))]
    WriteCredential { source: io::Error },
    #[snafu(display("Failed to open the credentials file"))]
    OpenCredentials { source: io::Error },
    #[snafu(display("Failed to read credentials from {}", path.display()))]
    ReadCredentials { source: io::Error, path: PathBuf },
    #[snafu(display("Failed to parse credentials from {}", path.display()))]
    ParseCredentials { source: InvalidUrlError, path: PathBuf },
}

#[derive(Debug, Snafu)]
#[snafu(context(suffix(Ctx)))]
#[snafu(display("Failed to parse URL: {input:?}"))]
struct InvalidUrlError {
    source: url::ParseError,
    input: String,
}

#[snafu::report]
fn main() -> Result<(), Error> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Get => command_get(),
        Commands::Store | Commands::Erase => Ok(()),
    }
}

fn command_get() -> Result<(), Error> {
    let gc = GitCredential::from_reader(io::stdin()).context(ParseCredentialCtx)?;
    let (mut file, path) = match open_credentials().context(OpenCredentialsCtx)? {
        Some(v) => v,
        None => return Ok(()),
    };
    let mut content = String::new();
    file.read_to_string(&mut content)
        .context(ReadCredentialsCtx { path: &path })?;
    parse_credentials(&content, &path)?
        .into_iter()
        .find(|entry| is_match(&gc, entry))
        .map(|url| GitCredential::from_url(&url))
        .map_or_else(|| Ok(()), |gc| gc.to_writer(io::stdout()).context(WriteCredentialCtx))
}

fn open_credentials() -> Result<Option<(File, PathBuf)>, io::Error> {
    macro_rules! try_open {
        ($($source:expr),*) => {
            $(
                if let Some(path) = $source {
                    match File::open(&path) {
                        Ok(file) => return Ok(Some((file, path))),
                        Err(e) if e.kind() == io::ErrorKind::NotFound => (),
                        Err(e) => return Err(e),
                    }
                }
            )*
            return Ok(None);
        };
    }
    try_open!(
        env::var_os("GIT_CREDENTIALS")
            .filter(|s| !s.is_empty())
            .map(PathBuf::from),
        dirs::config_dir().map(|p| p.join("git").join("credentials.json")),
        dirs::home_dir().map(|p| p.join(".git-credentials.json"))
    );
}

fn parse_credentials(input: &str, path: &Path) -> Result<Vec<Url>, Error> {
    input
        .lines()
        .map(|input| Url::parse(input).context(InvalidUrlCtx { input }))
        .collect::<Result<Vec<Url>, InvalidUrlError>>()
        .context(ParseCredentialsCtx { path })
}

fn is_match(gc: &GitCredential, entry: &Url) -> bool {
    if gc.protocol.as_deref() != Some(entry.scheme()) && gc.host.as_deref() != entry.host_str() {
        return false;
    }
    if let (Some(expected), Some(actual)) = (
        gc.username.as_deref(), //
        Some(entry.username()).filter(|s| !s.is_empty()),
    ) && expected != actual
    {
        return false;
    }
    if let (Some(expected), actual) = (gc.path.as_deref(), trim_prefix(entry.path(), "/"))
        && !expected.starts_with(actual)
    {
        return false;
    }
    true
}

#[inline]
fn trim_prefix<'a>(s: &'a str, prefix: &'a str) -> &'a str {
    s.strip_prefix(prefix).unwrap_or(s)
}
