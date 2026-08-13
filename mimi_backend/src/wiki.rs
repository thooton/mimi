// A read-only MediaWiki client that can read the wiki *as it was*.
//
// Everything fetched is pinned to one timestamp, so that a course assembled
// from a dozen pages is a coherent whole rather than a mix of states from
// either side of somebody's edit.
//
// MediaWiki gives us that pinning almost for free: `rvstart=<ts>&rvdir=older`
// returns the revision that was current at `<ts>`. The catch is that `rvstart`
// "may only be used on a single page", so a pinned read cannot be batched. That
// would be one request per page, which is why `at` below is written in two
// phases: the latest revision of every page is fetched in batches of fifty, and
// only the pages whose latest revision turns out to be *newer* than the
// snapshot need a second, single-page, pinned request. Since the snapshot is
// taken from the newest change on the wiki, that second phase is normally
// empty. Do not "simplify" this into one pinned request per page.
//
// Timestamps are compared as strings throughout. MediaWiki's are ISO 8601 in
// UTC with a fixed width, so lexicographic order *is* chronological order, and
// parsing them would buy nothing but a way to fail.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::time::Duration;

use http_body_util::{BodyExt, Empty};
use hyper::body::Bytes;
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;
use serde_json::Value;

// The four namespaces the editor defines. Declared here rather than fetched so
// that a wiki answering nonsense cannot quietly widen what we walk.
pub const NS_SKILL: i64 = 3000;
pub const NS_COURSE: i64 = 3002;
pub const NS_GLOSSARY: i64 = 3004;
pub const NS_TIPS: i64 = 3006;
pub const NAMESPACES: [i64; 4] = [NS_SKILL, NS_COURSE, NS_GLOSSARY, NS_TIPS];

// The API's limit for an ordinary (non-bot) account.
const BATCH: usize = 50;

// The wiki refused a request, or answered with something unusable. Most wiki
// failures can be repaired outside this process -- by bringing the service
// back, finishing a deployment, or correcting a page -- and are therefore
// retryable. `fatal` is reserved for requests that this process can never make
// successfully, such as a malformed endpoint or a definitive API refusal.
#[derive(Debug)]
pub struct WikiError {
    message: String,
    retryable: bool,
}

impl fmt::Display for WikiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for WikiError {}

fn err(message: impl Into<String>) -> WikiError {
    WikiError {
        message: message.into(),
        retryable: true,
    }
}

fn fatal(message: impl Into<String>) -> WikiError {
    WikiError {
        message: message.into(),
        retryable: false,
    }
}

impl WikiError {
    pub fn is_retryable(&self) -> bool {
        self.retryable
    }

    fn at(self, endpoint: &str) -> Self {
        Self {
            message: format!("{endpoint}: {}", self.message),
            ..self
        }
    }
}

// One page as it stood at the snapshot.
#[derive(Debug, Clone, PartialEq)]
pub struct Revision {
    pub title: String,
    pub revid: i64,
    pub timestamp: String,
    pub model: String,
    pub content: Value,
}

pub struct Wiki {
    api_url: String,
    client: Client<HttpConnector, Empty<Bytes>>,
    user_agent: String,
    timeout: Duration,
}

impl Wiki {
    pub fn new(api_url: impl Into<String>) -> Result<Self, WikiError> {
        let api_url = api_url.into();
        let uri = api_url
            .parse::<hyper::Uri>()
            .map_err(|error| fatal(format!("invalid wiki API URL '{api_url}': {error}")))?;
        if uri.scheme_str() != Some("http") || uri.authority().is_none() {
            return Err(fatal(format!(
                "wiki API URL must be an absolute http URL: '{api_url}'"
            )));
        }
        if uri.query().is_some() {
            return Err(fatal(format!(
                "wiki API URL must not contain a query string: '{api_url}'"
            )));
        }

        Ok(Wiki {
            api_url,
            client: Client::builder(TokioExecutor::new()).build_http(),
            user_agent: "mimi_backend/1.0".to_string(),
            timeout: Duration::from_secs(30),
        })
    }

    // --- transport ---

    async fn request(&self, params: &[(&str, String)]) -> Result<Value, WikiError> {
        self.get(&query_of(params)).await
    }

    async fn get(&self, query: &BTreeMap<String, String>) -> Result<Value, WikiError> {
        let encoded: Vec<String> = query
            .iter()
            .map(|(k, v)| format!("{}={}", encode(k), encode(v)))
            .collect();
        let url = format!("{}?{}", self.api_url, encoded.join("&"));

        // Retry timing belongs to the startup/refresh loop in main.rs. Keeping
        // one transport attempt here means every kind of recoverable failure
        // follows the same fixed cadence instead of hiding a second backoff in
        // the HTTP client.
        let body = self
            .fetch(&url)
            .await
            .map_err(|error| error.at(&self.api_url))?;
        if let Some(error) = body.get("error") {
            let code = error.get("code").and_then(Value::as_str).unwrap_or("?");
            let info = error.get("info").and_then(Value::as_str).unwrap_or("?");
            return match code {
                // These describe the wiki's present state, not a bad request;
                // a later attempt can succeed unchanged.
                "maxlag" | "readonly" | "ratelimited" => Err(err(format!("{code}: {info}"))),
                _ => Err(fatal(format!("{code}: {info}"))),
            };
        }
        Ok(body)
    }

    async fn fetch(&self, url: &str) -> Result<Value, WikiError> {
        let uri = url
            .parse::<hyper::Uri>()
            .map_err(|e| fatal(format!("{url}: {e}")))?;
        let request = hyper::Request::builder()
            .uri(uri)
            .header(hyper::header::USER_AGENT, &self.user_agent)
            .body(Empty::<Bytes>::new())
            .map_err(|e| fatal(e.to_string()))?;

        let response = tokio::time::timeout(self.timeout, self.client.request(request))
            .await
            .map_err(|_| err("timed out"))?
            .map_err(|e| err(e.to_string()))?;
        let status = response.status();
        let body = tokio::time::timeout(self.timeout, response.into_body().collect())
            .await
            .map_err(|_| err("timed out reading the body"))?
            .map_err(|e| err(e.to_string()))?
            .to_bytes();
        if !status.is_success() {
            let message = format!("HTTP {status}");
            return if status.is_server_error()
                || status == hyper::StatusCode::REQUEST_TIMEOUT
                || status == hyper::StatusCode::TOO_MANY_REQUESTS
            {
                Err(err(message))
            } else {
                Err(fatal(message))
            };
        }
        serde_json::from_slice(&body).map_err(|e| err(e.to_string()))
    }

    // Follow the API's continuation until the result set is exhausted.
    //
    // The parameter map is keyed by `String` rather than `&'static str` because
    // the continuation keys are whatever the API chose to hand back. This runs
    // on a poll loop for the life of the process, so they cannot be leaked to
    // buy a borrowed key.
    async fn paged(&self, params: &[(&str, String)]) -> Result<Vec<Value>, WikiError> {
        let mut params = query_of(params);
        let mut pages = Vec::new();
        loop {
            let body = self.get(&params).await?;
            if let Some(query) = body.get("query") {
                pages.push(query.clone());
            }
            let Some(continuation) = body.get("continue").and_then(Value::as_object) else {
                return Ok(pages);
            };
            for (key, value) in continuation {
                let value = match value {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                params.insert(key.clone(), value);
            }
        }
    }

    // --- points in time ---

    // The wiki's own clock, which is the only one the timestamps mean.
    pub async fn server_time(&self) -> Result<String, WikiError> {
        let body = self
            .request(&[
                ("action", "query".into()),
                ("meta", "siteinfo".into()),
                ("siprop", "general".into()),
            ])
            .await?;
        body.pointer("/query/general/time")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| err("siteinfo did not report the server time"))
    }

    // When the wiki last changed, or None if it never has.
    pub async fn newest_change(&self) -> Result<Option<String>, WikiError> {
        self.edge("older").await
    }

    // The oldest change still retained.
    //
    // Recent changes are pruned after `$wgRCMaxAge` (90 days by default). If
    // our last snapshot predates what is still on the list, the list can no
    // longer tell us everything that went stale, and the only honest answer is
    // to fetch the wiki again from scratch.
    pub async fn oldest_change(&self) -> Result<Option<String>, WikiError> {
        self.edge("newer").await
    }

    async fn edge(&self, direction: &str) -> Result<Option<String>, WikiError> {
        let body = self
            .request(&[
                ("action", "query".into()),
                ("list", "recentchanges".into()),
                ("rcdir", direction.into()),
                ("rclimit", "1".into()),
                ("rcprop", "timestamp".into()),
            ])
            .await?;
        Ok(body
            .pointer("/query/recentchanges/0/timestamp")
            .and_then(Value::as_str)
            .map(str::to_owned))
    }

    // --- what changed ---

    // Every page in our namespaces touched in `since`..`until`.
    //
    // Both ends are inclusive. MediaWiki timestamps are only accurate to the
    // second, so an edit sharing its second with the snapshot boundary would
    // otherwise be able to fall through the gap between two runs. Re-reporting
    // a page we already have is free — refetching is idempotent — while missing
    // one leaves the course quietly wrong, so the window overlaps by a second
    // on purpose.
    pub async fn changed_between(
        &self,
        since: &str,
        until: &str,
    ) -> Result<HashSet<String>, WikiError> {
        let namespaces: Vec<String> = NAMESPACES.iter().map(|ns| ns.to_string()).collect();
        let pages = self
            .paged(&[
                ("action", "query".into()),
                ("list", "recentchanges".into()),
                ("rcstart", until.into()),
                ("rcend", since.into()),
                ("rcdir", "older".into()),
                ("rcnamespace", namespaces.join("|")),
                ("rcprop", "title|timestamp|loginfo".into()),
                ("rclimit", "500".into()),
            ])
            .await?;

        let mut titles = HashSet::new();
        for page in pages {
            let Some(changes) = page.get("recentchanges").and_then(Value::as_array) else {
                continue;
            };
            for change in changes {
                if let Some(title) = change.get("title").and_then(Value::as_str) {
                    titles.insert(title.to_string());
                }
                // A move leaves the old title behind and creates a new one, and
                // only the log entry knows the other half of the pair.
                if let Some(target) = change
                    .pointer("/logparams/target_title")
                    .and_then(Value::as_str)
                {
                    titles.insert(target.to_string());
                }
            }
        }
        Ok(titles)
    }

    // Every page that exists in a namespace *now*.
    //
    // Deliberately unpinned: it is only used to find the courses to start the
    // walk from, and a course that did not yet exist at the snapshot drops out
    // by itself when its pinned read comes back empty.
    pub async fn titles_in(&self, namespace: i64) -> Result<Vec<String>, WikiError> {
        let pages = self
            .paged(&[
                ("action", "query".into()),
                ("list", "allpages".into()),
                ("apnamespace", namespace.to_string()),
                ("aplimit", "500".into()),
            ])
            .await?;

        let mut titles = Vec::new();
        for page in pages {
            let Some(entries) = page.get("allpages").and_then(Value::as_array) else {
                continue;
            };
            titles.extend(
                entries
                    .iter()
                    .filter_map(|e| e.get("title").and_then(Value::as_str))
                    .map(str::to_owned),
            );
        }
        Ok(titles)
    }

    // --- reading pages ---

    // Read pages as they stood at `timestamp`.
    //
    // A title maps to None when the page did not exist then — either because it
    // has since been deleted, or because it had not been created yet.
    pub async fn at(
        &self,
        titles: &[String],
        timestamp: &str,
    ) -> Result<HashMap<String, Option<Revision>>, WikiError> {
        let mut found: HashMap<String, Option<Revision>> = HashMap::new();
        let mut newer: HashSet<String> = HashSet::new();

        for batch in titles.chunks(BATCH) {
            // `prop=revisions` continuation is slightly deceptive: every
            // requested page appears in the first response, but only the
            // first run carries a `revisions` field. The rest are page stubs,
            // and their revisions arrive in the continued response. Treating
            // a stub as a missing page silently drops the tail of a large
            // split glossary, so consume the whole property result here just
            // as list queries do.
            let queries = self
                .paged(&[
                    ("action", "query".into()),
                    ("prop", "revisions".into()),
                    ("titles", batch.join("|")),
                    ("rvprop", "ids|timestamp|content|contentmodel".into()),
                    ("rvslots", "main".into()),
                ])
                .await?;

            // Titles come back normalized, so map them back to what we asked
            // for; the caller indexes the result by the name it handed in.
            let mut asked: HashMap<String, String> =
                batch.iter().map(|t| (normal_key(t), t.clone())).collect();
            for query in &queries {
                for entry in query
                    .get("normalized")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    let (Some(from), Some(to)) = (
                        entry.get("from").and_then(Value::as_str),
                        entry.get("to").and_then(Value::as_str),
                    ) else {
                        continue;
                    };
                    let original = asked
                        .get(&normal_key(from))
                        .cloned()
                        .unwrap_or_else(|| from.to_string());
                    asked.insert(normal_key(to), original);
                }
            }

            for query in &queries {
                absorb_revision_pages(query, &asked, timestamp, &mut found, &mut newer)?;
            }

            // A real missing page is represented explicitly above. Anything
            // still unresolved means MediaWiki returned neither that marker
            // nor a revision, and accepting the snapshot would make partial
            // course content look valid.
            if let Some(title) = batch
                .iter()
                .find(|title| !found.contains_key(*title) && !newer.contains(*title))
            {
                return Err(err(format!(
                    "{title}: the revision query returned no revision"
                )));
            }
        }

        for title in newer {
            let pinned = self.pinned(&title, timestamp).await?;
            found.insert(title, pinned);
        }
        Ok(found)
    }

    // The one revision of one page that was current at `timestamp`.
    async fn pinned(&self, title: &str, timestamp: &str) -> Result<Option<Revision>, WikiError> {
        let body = self
            .request(&[
                ("action", "query".into()),
                ("prop", "revisions".into()),
                ("titles", title.into()),
                ("rvprop", "ids|timestamp|content|contentmodel".into()),
                ("rvslots", "main".into()),
                ("rvstart", timestamp.into()),
                ("rvdir", "older".into()),
                ("rvlimit", "1".into()),
            ])
            .await?;
        let page = body
            .pointer("/query/pages")
            .and_then(Value::as_array)
            .and_then(|pages| pages.first());
        let Some(page) = page else {
            return Ok(None);
        };
        if page
            .get("missing")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            return Ok(None);
        }
        let revision = page
            .get("revisions")
            .and_then(Value::as_array)
            .and_then(|r| r.first());
        match revision {
            Some(revision) => Ok(Some(build_revision(title, revision)?)),
            None => Ok(None),
        }
    }
}

// Fold one page of a continued `prop=revisions` result into the current read.
// Existing pages without `revisions` are continuation stubs, not missing
// pages; another result page owns their revision and is allowed to fill it in.
fn absorb_revision_pages(
    query: &Value,
    asked: &HashMap<String, String>,
    timestamp: &str,
    found: &mut HashMap<String, Option<Revision>>,
    newer: &mut HashSet<String>,
) -> Result<(), WikiError> {
    let Some(pages) = query.get("pages").and_then(Value::as_array) else {
        return Ok(());
    };
    for page in pages {
        let reported = page
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let title = asked
            .get(&normal_key(reported))
            .cloned()
            .unwrap_or_else(|| reported.to_string());
        if page
            .get("missing")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            found.insert(title, None);
            continue;
        }
        let Some(revision) = page
            .get("revisions")
            .and_then(Value::as_array)
            .and_then(|revisions| revisions.first())
        else {
            continue;
        };
        let stamp = revision
            .get("timestamp")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if stamp > timestamp {
            // Edited since the snapshot was taken: its current text is not the
            // text we are entitled to see. A pinned read below supplies it.
            newer.insert(title);
            continue;
        }
        found.insert(title.clone(), Some(build_revision(&title, revision)?));
    }
    Ok(())
}

// The caller's parameters plus the two every request carries.
fn query_of(params: &[(&str, String)]) -> BTreeMap<String, String> {
    let mut query: BTreeMap<String, String> = params
        .iter()
        .map(|(k, v)| ((*k).to_string(), v.clone()))
        .collect();
    query.insert("format".into(), "json".into());
    query.insert("formatversion".into(), "2".into());
    query
}

fn normal_key(title: &str) -> String {
    title.replace('_', " ")
}

fn build_revision(title: &str, revision: &Value) -> Result<Revision, WikiError> {
    let slot = revision
        .pointer("/slots/main")
        .ok_or_else(|| err(format!("{title}: the revision has no main slot")))?;
    let text = slot.get("content").and_then(Value::as_str).unwrap_or("");
    let content = serde_json::from_str(text)
        .map_err(|e| err(format!("{title} does not hold valid JSON: {e}")))?;
    Ok(Revision {
        title: title.to_string(),
        revid: revision.get("revid").and_then(Value::as_i64).unwrap_or(0),
        timestamp: revision
            .get("timestamp")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        model: slot
            .get("contentmodel")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        content,
    })
}

// Percent-encode one query component. Page titles carry spaces, slashes and
// accents, all of which have to survive the round trip intact — a title is the
// only thing linking a skill to its tips, so mangling one silently loses a page.
fn encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_endpoint_that_cannot_be_requested_is_not_retried() {
        assert!(Wiki::new("http://wiki.example/api.php").is_ok());
        for endpoint in [
            "not-a-url",
            "https://wiki.example/api.php",
            "http://wiki.example/api.php?old=query",
        ] {
            let Err(error) = Wiki::new(endpoint) else {
                panic!("accepted invalid endpoint {endpoint}");
            };
            assert!(!error.is_retryable());
        }
    }

    #[test]
    fn encoding_survives_the_characters_a_title_actually_uses() {
        assert_eq!(encode("Skill:Spanish/Family"), "Skill%3ASpanish%2FFamily");
        assert_eq!(encode("Buenos días"), "Buenos%20d%C3%ADas");
        assert_eq!(encode("a|b"), "a%7Cb");
        // unreserved characters are left alone, so ordinary titles stay legible
        assert_eq!(encode("food_1-2.3~4"), "food_1-2.3~4");
    }

    #[test]
    fn underscores_and_spaces_are_the_same_title() {
        assert_eq!(normal_key("Skill:A_b"), normal_key("Skill:A b"));
    }

    // Timestamps are compared as strings, which only works because MediaWiki's
    // are fixed-width UTC. Guard the property the rest of the file rests on.
    #[test]
    fn timestamps_compare_chronologically_as_strings() {
        assert!("2026-08-09T18:56:00Z" > "2026-08-09T18:55:59Z");
        assert!("2026-01-01T00:00:00Z" > "2025-12-31T23:59:59Z");
    }

    // MediaWiki returns every requested page on every continuation, but only
    // the slice owned by that response carries `revisions`. In particular, a
    // metadata-only page is not evidence that the page is missing.
    #[test]
    fn revision_continuation_stubs_do_not_erase_pages() {
        let revision = |id| {
            json!({
                "revid": id,
                "timestamp": "2026-08-09T18:55:59Z",
                "slots": {"main": {
                    "contentmodel": "json",
                    "content": "{\"entries\":[]}"
                }}
            })
        };
        let first = json!({"pages": [
            {"title": "Glossary:A", "revisions": [revision(1)]},
            {"title": "Glossary:P"}
        ]});
        let second = json!({"pages": [
            {"title": "Glossary:A"},
            {"title": "Glossary:P", "revisions": [revision(2)]}
        ]});
        let asked = HashMap::from([
            (normal_key("Glossary:A"), "Glossary:A".to_string()),
            (normal_key("Glossary:P"), "Glossary:P".to_string()),
        ]);
        let mut found = HashMap::new();
        let mut newer = HashSet::new();

        absorb_revision_pages(
            &first,
            &asked,
            "2026-08-09T18:56:00Z",
            &mut found,
            &mut newer,
        )
        .unwrap();
        absorb_revision_pages(
            &second,
            &asked,
            "2026-08-09T18:56:00Z",
            &mut found,
            &mut newer,
        )
        .unwrap();

        assert_eq!(found["Glossary:A"].as_ref().unwrap().revid, 1);
        assert_eq!(found["Glossary:P"].as_ref().unwrap().revid, 2);
        assert!(newer.is_empty());
    }
}
