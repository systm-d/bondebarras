//! Branch and tag endpoints.
//!
//! Branches and tags carry no numeric id from GitHub — unlike every other
//! resource family this crate handles, they are addressed by name alone.
//! `resource_id` derives a stable, effectively-unique `u64` from that name,
//! for `Resource.id` and the `(ResourceKind, u64)` key `App.selected` and
//! `clean::Progress` are built from (see `model::ResourceKind`'s own doc
//! comment). Deletion itself never goes through this id: `clean::execute`'s
//! branch and tag arms delete by `Resource.label` — the real name — instead,
//! precisely so a hash collision, vanishingly unlikely as it is, could never
//! delete the wrong ref. The hash is a selection key only.

use super::Client;
use crate::refs::BranchRef;
use anyhow::Result;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// GitHub's maximum page size for these endpoints.
const PAGE_SIZE: usize = 100;

/// Hard stop on pagination, mirroring `prs::MAX_PR_PAGES`: ten pages covers a
/// thousand branches or tags, well past any repository this tool will meet.
const MAX_PAGES: u32 = 10;

/// A stable, effectively-unique id for a branch or a tag, derived from its name.
///
/// `DefaultHasher::new()` seeds SipHash-1-3 with the fixed keys `(0, 0)` —
/// unlike `HashMap`'s own hasher, which draws a random seed per process
/// through `RandomState`. That randomness is exactly what would break
/// "stable across scans": a selection made from one scan's `Resource.id`
/// would no longer match the id a second scan recomputes for the same name,
/// the moment the process seed changed. `DefaultHasher` gives the same `u64`
/// for the same input on every call, every run, every machine — which is
/// what a selection surviving a refresh requires.
///
/// This is not a *proof* of uniqueness — no fixed-width hash can be one for
/// arbitrarily many inputs — only a probabilistic guarantee: two distinct
/// branch names would have to collide in a 64-bit space, which for the
/// number of branches or tags any real repository holds is astronomically
/// unlikely. `resource_id_does_not_collide_for_two_close_branch_names` below
/// exercises that against realistic, deliberately close names, not a proof
/// of the general case — which is exactly why deletion itself never trusts
/// this id (see the module doc comment above).
pub fn resource_id(name: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    name.hash(&mut hasher);
    hasher.finish()
}

/// Every branch of a repository, across as many pages as it takes.
///
/// `SecondBrain-io/monolith-back` alone crosses the 100-per-page ceiling, so
/// a single page would silently hide branches — the same defect
/// `prs::closed_prs` had before v0.1 fixed it.
pub async fn branches(client: &Client, owner: &str, repo: &str) -> Result<Vec<BranchRef>> {
    let mut out = Vec::new();

    for page in 1..=MAX_PAGES {
        let v = client
            .get_json(&format!(
                "/repos/{owner}/{repo}/branches?per_page={PAGE_SIZE}&page={page}"
            ))
            .await?;

        let Some(items) = v.as_array() else { break };
        // A branch with no usable name is one we cannot ever address again —
        // for deletion or for anything else — so it is dropped rather than
        // coerced to an empty name, the string equivalent of the "never
        // coerce a missing id to 0" rule the numeric-id families follow.
        out.extend(items.iter().filter_map(|item| {
            Some(BranchRef {
                name: item["name"].as_str()?.to_string(),
                protected: item["protected"].as_bool().unwrap_or(false),
            })
        }));

        // A short page is the last one.
        if items.len() < PAGE_SIZE {
            break;
        }
    }

    Ok(out)
}

/// Every tag name of a repository, across as many pages as it takes.
pub async fn tags(client: &Client, owner: &str, repo: &str) -> Result<Vec<String>> {
    let mut out = Vec::new();

    for page in 1..=MAX_PAGES {
        let v = client
            .get_json(&format!(
                "/repos/{owner}/{repo}/tags?per_page={PAGE_SIZE}&page={page}"
            ))
            .await?;

        let Some(items) = v.as_array() else { break };
        out.extend(
            items
                .iter()
                .filter_map(|item| item["name"].as_str().map(str::to_string)),
        );

        if items.len() < PAGE_SIZE {
            break;
        }
    }

    Ok(out)
}

pub async fn delete_branch(client: &Client, owner: &str, repo: &str, name: &str) -> Result<()> {
    client
        .delete(&format!(
            "/repos/{owner}/{repo}/git/refs/heads/{}",
            encode_ref_name(name)
        ))
        .await
}

pub async fn delete_tag(client: &Client, owner: &str, repo: &str, name: &str) -> Result<()> {
    client
        .delete(&format!(
            "/repos/{owner}/{repo}/git/refs/tags/{}",
            encode_ref_name(name)
        ))
        .await
}

/// Percent-encode a branch or tag name for use inside a GitHub ref path,
/// leaving `/` intact.
///
/// `git check-ref-format` accepts `#`, `%`, `?`, `[`, `]` and more in a
/// refname — confirmed directly (`git check-ref-format feat/issue#42` exits
/// 0) — none of which build a path safely once interpolated raw: `#` opens
/// a URL fragment `http::Uri` drops silently before the request is ever
/// sent (no error — the DELETE just lands on a *different*, shorter,
/// possibly already-existing ref); `?` opens a query string, routing the
/// DELETE to whatever exists at the truncated path with the rest as a bogus
/// parameter; a bare `%` is not valid percent-encoding on its own and is
/// not guaranteed to survive unchanged. `/` is the one deliberate exception:
/// `feat/foo` is one ref name whose slash is part of it, and it must reach
/// the API as the path separator GitHub's own routing expects, not `%2F`.
///
/// Every byte outside RFC 3986's `unreserved` set (`ALPHA / DIGIT / "-" /
/// "." / "_" / "~"`) is escaped — a superset of what git or GitHub strictly
/// require, on purpose: a small, easily-audited allowlist is safer than
/// trying to enumerate every character that could reinterpret a path, and
/// there is no real ref name that comes out worse for the extra caution.
fn encode_ref_name(name: &str) -> String {
    name.split('/')
        .map(encode_segment)
        .collect::<Vec<_>>()
        .join("/")
}

/// Percent-encode one path segment (never containing `/` itself — the
/// caller already split on it).
fn encode_segment(segment: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(segment.len());
    for byte in segment.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char);
            }
            _ => {
                out.push('%');
                out.push(HEX[(byte >> 4) as usize] as char);
                out.push(HEX[(byte & 0x0F) as usize] as char);
            }
        }
    }
    out
}

/// A repository's default branch name.
///
/// Not part of the brief's own verified route list — added here because
/// `refs::branch_is_dead` (task 1) needs a real default-branch name to
/// exclude, and nothing else in this crate fetches one. `GET
/// /repos/{owner}/{repo}` is GitHub's standard single-repository lookup and
/// its response carries `default_branch`. Colocated in this module rather
/// than `api::repos` because it exists solely to serve `branch_is_dead`,
/// which is this module's whole reason to fetch branches in the first
/// place; `api::repos` otherwise only ever lists many repositories, never
/// one.
pub async fn default_branch(client: &Client, owner: &str, repo: &str) -> Result<String> {
    let v = client.get_json(&format!("/repos/{owner}/{repo}")).await?;
    Ok(v["default_branch"].as_str().unwrap_or_default().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn branches_carries_name_and_protected() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/branches"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "name": "main", "protected": true },
                { "name": "claude/landing-3jbqk4", "protected": false }
            ])))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let out = branches(&client, "systm-d", "claudine").await.unwrap();

        assert_eq!(out.len(), 2);
        assert_eq!(out[0].name, "main");
        assert!(out[0].protected);
        assert_eq!(out[1].name, "claude/landing-3jbqk4");
        assert!(!out[1].protected);
    }

    /// `SecondBrain-io/monolith-back` alone crosses the 100-per-page ceiling —
    /// a single page would silently hide branches, the exact defect
    /// `prs::closed_prs` had before v0.1 fixed it. Modeled on
    /// `prs::closed_prs_follows_pagination`.
    #[tokio::test]
    async fn branches_follows_pagination() {
        let server = MockServer::start().await;
        let full: Vec<serde_json::Value> = (1..=100)
            .map(|n| serde_json::json!({ "name": format!("branch-{n}"), "protected": false }))
            .collect();
        Mock::given(method("GET"))
            .and(path("/repos/SecondBrain-io/monolith-back/branches"))
            .and(query_param("page", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(full))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/SecondBrain-io/monolith-back/branches"))
            .and(query_param("page", "2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "name": "main", "protected": true }
            ])))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let out = branches(&client, "SecondBrain-io", "monolith-back")
            .await
            .unwrap();

        assert_eq!(out.len(), 101, "the second page must not be dropped");
        assert!(out.iter().any(|b| b.name == "main" && b.protected));
    }

    #[tokio::test]
    async fn a_branch_without_a_usable_name_is_dropped() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/branches"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "name": "main", "protected": true },
                { "protected": false }
            ])))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let out = branches(&client, "systm-d", "claudine").await.unwrap();

        assert_eq!(out.len(), 1, "the nameless entry must be dropped");
        assert_eq!(out[0].name, "main");
    }

    #[tokio::test]
    async fn tags_carries_names() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/tags"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "name": "v0.1.3" },
                { "name": "v0.1.2" }
            ])))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let out = tags(&client, "systm-d", "claudine").await.unwrap();

        assert_eq!(out, vec!["v0.1.3".to_string(), "v0.1.2".to_string()]);
    }

    #[tokio::test]
    async fn tags_follows_pagination() {
        let server = MockServer::start().await;
        let full: Vec<serde_json::Value> = (1..=100)
            .map(|n| serde_json::json!({ "name": format!("v0.{n}.0") }))
            .collect();
        Mock::given(method("GET"))
            .and(path("/repos/SecondBrain-io/monolith-back/tags"))
            .and(query_param("page", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(full))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/repos/SecondBrain-io/monolith-back/tags"))
            .and(query_param("page", "2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "name": "v0.101.0" }
            ])))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let out = tags(&client, "SecondBrain-io", "monolith-back")
            .await
            .unwrap();

        assert_eq!(out.len(), 101, "the second page must not be dropped");
        assert!(out.contains(&"v0.101.0".to_string()));
    }

    #[tokio::test]
    async fn a_tag_without_a_usable_name_is_dropped() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine/tags"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                { "name": "v0.1.3" },
                { "no_name_field": true }
            ])))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let out = tags(&client, "systm-d", "claudine").await.unwrap();

        assert_eq!(out, vec!["v0.1.3".to_string()]);
    }

    #[tokio::test]
    async fn delete_branch_targets_the_heads_endpoint() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path(
                "/repos/systm-d/claudine/git/refs/heads/claude/landing-3jbqk4",
            ))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        delete_branch(&client, "systm-d", "claudine", "claude/landing-3jbqk4")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn delete_tag_targets_the_tags_endpoint() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/repos/systm-d/claudine/git/refs/tags/v0.1.2"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        delete_tag(&client, "systm-d", "claudine", "v0.1.2")
            .await
            .unwrap();
    }

    /// `git check-ref-format feat/issue#42` exits 0 — a legal branch name.
    /// Interpolated raw, `#` opens a URL fragment that `http::Uri` silently
    /// drops before the request is ever sent (verified directly:
    /// `"...feat/issue#42".parse::<http::Uri>()` yields a path ending in
    /// `.../feat/issue`, no error) — the DELETE would land on
    /// `.../git/refs/heads/feat/issue`, a **different, shorter, and possibly
    /// already-existing** ref. A fixture using only well-formed names, like
    /// `delete_branch_targets_the_heads_endpoint` above, cannot catch this:
    /// every character in `claude/landing-3jbqk4` is already
    /// percent-encoding-safe, so that test would pass unchanged whether or
    /// not encoding ever happened. This one only passes if `delete_branch`
    /// actually escapes the name before it reaches the client.
    #[tokio::test]
    async fn delete_branch_percent_encodes_a_hash_in_the_name() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path(
                "/repos/systm-d/claudine/git/refs/heads/feat/issue%2342",
            ))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        delete_branch(&client, "systm-d", "claudine", "feat/issue#42")
            .await
            .expect("the escaped path must reach the mock, not a truncated one");
    }

    /// Same defect, a different terminator: `?` opens a query string rather
    /// than a fragment, so an unescaped one would route the DELETE to
    /// `.../git/refs/tags/weird` with `name` as a query parameter — again a
    /// different ref than the one named.
    #[tokio::test]
    async fn delete_tag_percent_encodes_a_question_mark_in_the_name() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/repos/systm-d/claudine/git/refs/tags/weird%3Fname"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        delete_tag(&client, "systm-d", "claudine", "weird?name")
            .await
            .expect("a `?` must not truncate the ref name into a query string");
    }

    /// The encoding helper itself, isolated from the HTTP plumbing: the four
    /// cases the fix calls for, in one place. `feat/issue#42` and
    /// `weird?name` are also proven end-to-end above; `%` and `[`/`]` are
    /// only proven here, since GitHub's mock-path matcher already covers the
    /// wiring for the other two.
    #[test]
    fn encode_ref_name_escapes_reserved_characters_but_leaves_slash_and_a_plain_name_alone() {
        assert_eq!(encode_ref_name("feat/issue#42"), "feat/issue%2342");
        assert_eq!(
            encode_ref_name("50%tag"),
            "50%25tag",
            "a literal % must itself be escaped, not passed through as if already encoded"
        );
        assert_eq!(encode_ref_name("weird?name"), "weird%3Fname");
        assert_eq!(
            encode_ref_name("release[2]"),
            "release%5B2%5D",
            "brackets must be escaped too"
        );
        assert_eq!(
            encode_ref_name("feat/foo"),
            "feat/foo",
            "a well-formed name must pass through unchanged, slash included"
        );
    }

    #[tokio::test]
    async fn default_branch_reads_the_field_from_the_repo_endpoint() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/systm-d/claudine"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "name": "claudine",
                "default_branch": "main"
            })))
            .mount(&server)
            .await;

        let client = Client::with_base("t0ken", &server.uri()).unwrap();
        let out = default_branch(&client, "systm-d", "claudine")
            .await
            .unwrap();

        assert_eq!(out, "main");
    }

    /// Two names close enough to plausibly trip up a weak hash: same prefix,
    /// one character apart at the end — exactly the shape of the real
    /// `claude/landing-3jbqk4`-style branch names this codebase's own PR
    /// fixtures use. `a` and `b` would prove nothing; these are meant to
    /// stress it.
    #[test]
    fn resource_id_does_not_collide_for_two_close_branch_names() {
        let a = resource_id("claude/landing-3jbqk4");
        let b = resource_id("claude/landing-3jbqk5");
        assert_ne!(
            a, b,
            "two distinct branch names must not hash to the same id"
        );

        // A second pair, distinguished only by a trailing path segment, and a
        // third pair distinguished by a length difference — different ways a
        // naive scheme (e.g. summing bytes) could still collide.
        let c = resource_id("release/2.0");
        let d = resource_id("release/2.0.1");
        assert_ne!(c, d);

        let e = resource_id("feature/rejected");
        let f = resource_id("feature/rejected-2");
        assert_ne!(e, f);
    }

    /// A selection made before a refresh must still target the same branch
    /// after it — `resource_id` has to be a pure function of the name, not
    /// something seeded per-process the way `HashMap`'s own default hasher
    /// is (which would make `App.selected`, built from one scan, mismatch
    /// `Resource.id` recomputed on the next).
    #[test]
    fn resource_id_is_stable_for_the_same_name() {
        assert_eq!(
            resource_id("claude/landing-3jbqk4"),
            resource_id("claude/landing-3jbqk4")
        );
    }
}
