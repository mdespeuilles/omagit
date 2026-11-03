//! The repositories somebody has on a forge, for the clone dialog to search.
//!
//! Through the forge's own command-line client rather than its HTTP API: `gh`
//! already holds the login — in the keyring, with SSO and two-factor done — and
//! asking for a token to paste would be a second copy of a secret that exists.
//! `glab` will fit the same shape for self-hosted GitLab.
//!
//! `gh` is looked for on the `PATH` and then in the places installers put it.
//! An app started from a desktop launcher does not get the `PATH` a terminal
//! builds, and a version manager's shims are exactly what that leaves out.

use std::path::{Path, PathBuf};
use std::time::Duration;

use omagit_git::journal::Journal;
use omagit_git::{Cancel, cli};
use serde::{Deserialize, Serialize};

/// Two hundred repositories is three pages, and a page is a second or two.
const DEADLINE: Duration = Duration::from_secs(60);

/// One repository, as the dialog lists it.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct HostedRepo {
    pub full_name: String,
    pub description: Option<String>,
    pub private: bool,
    pub fork: bool,
    pub archived: bool,
    /// ISO 8601, as the API gives it. Sorted on, never shown raw.
    pub pushed_at: Option<String>,
    pub ssh_url: String,
    pub clone_url: String,
}

/// What the dialog gets: the list, and which URL the user's `gh` clones with.
#[derive(Clone, Debug, Serialize)]
pub struct Hosted {
    pub host: String,
    pub ssh: bool,
    pub repos: Vec<HostedRepo>,
}

/// Every repository the signed-in user owns, collaborates on, or reaches
/// through an organisation, most recently pushed first.
pub fn github(journal: &Journal, cancel: &Cancel) -> Result<Hosted, String> {
    let gh = locate("gh").ok_or_else(|| "gh-missing".to_owned())?;
    let home = std::env::temp_dir();
    let output = cli::program(&gh, journal, &home)
        .args([
            "api",
            "--paginate",
            "/user/repos?per_page=100&affiliation=owner,collaborator,organization_member&sort=pushed",
            "--jq",
            ".[] | {full_name, description, private, fork, archived, pushed_at, ssh_url, clone_url}",
        ])
        .timeout(DEADLINE)
        .run(cancel)
        .map_err(|error| error.to_string())?;
    let repos = parse(&output.text())?;

    // What `gh repo clone` would use. Asked rather than assumed: somebody who
    // set `https` has a credential helper for it and perhaps no key at all.
    let protocol = cli::program(&gh, journal, &home)
        .args(["config", "get", "git_protocol", "-h", "github.com"])
        .timeout(Duration::from_secs(5))
        .run(cancel)
        .map(|output| output.text().trim().to_owned())
        .unwrap_or_default();

    Ok(Hosted {
        host: "github.com".to_owned(),
        ssh: protocol != "https",
        repos,
    })
}

/// One JSON object per line, which is what `--jq` with `--paginate` prints.
fn parse(text: &str) -> Result<Vec<HostedRepo>, String> {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).map_err(|error| format!("{error}: {line}")))
        .collect()
}

/// A program by name: on the `PATH`, or where an installer usually puts it.
fn locate(name: &str) -> Option<PathBuf> {
    let on_path = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .unwrap_or_default();
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let usual = [
        home.as_ref().map(|home| home.join(".local/bin")),
        home.as_ref()
            .map(|home| home.join(".local/share/mise/shims")),
        home.as_ref().map(|home| home.join(".nix-profile/bin")),
        Some(PathBuf::from("/opt/homebrew/bin")),
        Some(PathBuf::from("/usr/local/bin")),
        Some(PathBuf::from("/home/linuxbrew/.linuxbrew/bin")),
        Some(PathBuf::from("/usr/bin")),
    ];
    on_path
        .into_iter()
        .chain(usual.into_iter().flatten())
        .map(|dir| dir.join(name))
        .find(|candidate| is_program(candidate))
}

fn is_program(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        path.metadata()
            .map(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_one_repository_per_line() {
        let text = concat!(
            r#"{"full_name":"me/a","description":null,"private":true,"fork":false,"archived":false,"pushed_at":"2026-11-01T10:00:00Z","ssh_url":"git@github.com:me/a.git","clone_url":"https://github.com/me/a.git"}"#,
            "\n\n",
            r#"{"full_name":"org/b","description":"B","private":false,"fork":true,"archived":true,"pushed_at":null,"ssh_url":"git@github.com:org/b.git","clone_url":"https://github.com/org/b.git"}"#,
            "\n",
        );
        let repos = parse(text).unwrap();
        assert_eq!(repos.len(), 2);
        assert_eq!(repos[0].full_name, "me/a");
        assert!(repos[1].fork && repos[1].archived);
        assert_eq!(repos[1].description.as_deref(), Some("B"));
    }

    #[test]
    fn a_line_that_is_not_a_repository_is_an_error_that_quotes_it() {
        let error = parse("not json\n").unwrap_err();
        assert!(error.contains("not json"), "{error}");
    }

    /// Against the account `gh` is signed in to. Ignored: it needs the
    /// network and a login, which is a machine and not a test.
    #[test]
    #[ignore]
    fn lists_the_signed_in_account() {
        let hosted = github(&Journal::new(), &Cancel::new()).unwrap();
        assert!(!hosted.repos.is_empty());
        eprintln!("{} repositories, ssh: {}", hosted.repos.len(), hosted.ssh);
    }

    #[test]
    fn a_program_that_is_nowhere_is_not_found() {
        assert!(locate("omagit-no-such-program-anywhere").is_none());
    }
}
