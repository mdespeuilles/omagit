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

/// Which forges this machine can be asked about: whether there is a `gh`,
/// and the GitLab hosts `glab` is configured for.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Forges {
    pub github: bool,
    pub gitlab: Vec<String>,
}

pub fn forges() -> Forges {
    Forges {
        github: locate("gh").is_some(),
        gitlab: if locate("glab").is_some() {
            glab_config()
                .map(|text| gitlab_hosts(&text))
                .unwrap_or_default()
        } else {
            Vec::new()
        },
    }
}

/// `glab`'s configuration file, where its own documentation says it lives.
fn glab_config() -> Option<String> {
    let dir = std::env::var_os("GLAB_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("XDG_CONFIG_HOME").map(|dir| PathBuf::from(dir).join("glab-cli"))
        })
        .or_else(|| {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config/glab-cli"))
        })?;
    std::fs::read_to_string(dir.join("config.yml")).ok()
}

/// The keys under `hosts:` in `glab`'s YAML. Read by indentation rather than
/// with a YAML parser: the file's shape is `glab`'s own and fixed, and a whole
/// parser for one list of names would be most of a dependency for nothing.
fn gitlab_hosts(config: &str) -> Vec<String> {
    let mut hosts = Vec::new();
    let mut inside = false;
    let mut indent = None;
    for line in config.lines() {
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let depth = line.len() - line.trim_start().len();
        if depth == 0 {
            inside = line.trim_end() == "hosts:";
            indent = None;
            continue;
        }
        if !inside {
            continue;
        }
        let level = *indent.get_or_insert(depth);
        if depth == level
            && let Some(name) = line.trim().strip_suffix(':')
        {
            hosts.push(name.trim_matches(['"', '\'']).to_owned());
        }
    }
    hosts
}

/// Every project the signed-in user is a member of on `host`, most recently
/// active first.
pub fn gitlab(host: &str, journal: &Journal, cancel: &Cancel) -> Result<Hosted, String> {
    let glab = locate("glab").ok_or_else(|| "glab-missing".to_owned())?;
    let home = std::env::temp_dir();
    let output = cli::program(&glab, journal, &home)
        .args([
            "api",
            "--hostname",
            host,
            "--paginate",
            "projects?membership=true&per_page=100&order_by=last_activity_at",
        ])
        .timeout(DEADLINE)
        .run(cancel)
        .map_err(|error| error.to_string())?;
    let repos = parse_gitlab(&output.text())?;
    let protocol = cli::program(&glab, journal, &home)
        .args(["config", "get", "git_protocol", "--host", host])
        .timeout(Duration::from_secs(5))
        .run(cancel)
        .map(|output| output.text().trim().to_owned())
        .unwrap_or_default();
    Ok(Hosted {
        host: host.to_owned(),
        ssh: protocol != "https",
        repos,
    })
}

/// GitLab's projects, as one or several JSON arrays back to back — which is
/// what `--paginate` prints, a page at a time.
fn parse_gitlab(text: &str) -> Result<Vec<HostedRepo>, String> {
    #[derive(Deserialize)]
    struct Project {
        path_with_namespace: String,
        description: Option<String>,
        visibility: Option<String>,
        #[serde(default)]
        archived: bool,
        forked_from_project: Option<serde_json::Value>,
        last_activity_at: Option<String>,
        ssh_url_to_repo: String,
        http_url_to_repo: String,
    }
    let mut repos = Vec::new();
    for page in serde_json::Deserializer::from_str(text).into_iter::<Vec<Project>>() {
        for project in page.map_err(|error| error.to_string())? {
            repos.push(HostedRepo {
                full_name: project.path_with_namespace,
                description: project.description.filter(|text| !text.trim().is_empty()),
                private: project.visibility.as_deref() != Some("public"),
                fork: project
                    .forked_from_project
                    .is_some_and(|value| !value.is_null()),
                archived: project.archived,
                pushed_at: project.last_activity_at,
                ssh_url: project.ssh_url_to_repo,
                clone_url: project.http_url_to_repo,
            });
        }
    }
    Ok(repos)
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
    fn reads_gitlab_pages_printed_back_to_back() {
        let text = concat!(
            r#"[{"path_with_namespace":"team/api","description":"","visibility":"private","archived":false,"last_activity_at":"2026-11-01T10:00:00Z","ssh_url_to_repo":"git@git.example.com:team/api.git","http_url_to_repo":"https://git.example.com/team/api.git"}]"#,
            r#"[{"path_with_namespace":"me/fork","description":"Mine","visibility":"public","archived":true,"forked_from_project":{"id":1},"last_activity_at":null,"ssh_url_to_repo":"git@git.example.com:me/fork.git","http_url_to_repo":"https://git.example.com/me/fork.git"}]"#,
            "\n",
        );
        let repos = parse_gitlab(text).unwrap();
        assert_eq!(repos.len(), 2);
        assert_eq!(repos[0].full_name, "team/api");
        assert!(repos[0].private && !repos[0].fork);
        assert_eq!(repos[0].description, None, "an empty description is none");
        assert!(!repos[1].private && repos[1].fork && repos[1].archived);
    }

    #[test]
    fn finds_the_hosts_in_glabs_configuration() {
        let config = "git_protocol: ssh\nhosts:\n    gitlab.com:\n        token: x\n        api_protocol: https\n    git.example.com:\n        token: y\nno_prompt: false\n";
        assert_eq!(gitlab_hosts(config), ["gitlab.com", "git.example.com"]);
        assert!(gitlab_hosts("editor: vim\n").is_empty());
    }

    #[test]
    fn a_program_that_is_nowhere_is_not_found() {
        assert!(locate("omagit-no-such-program-anywhere").is_none());
    }
}
