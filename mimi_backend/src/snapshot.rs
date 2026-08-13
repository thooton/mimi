// The wiki state the server keeps between polls, and how it is refreshed.
//
// The thing worth understanding here is **why a timestamp is not enough state**.
// An incremental refresh learns which pages went stale, but it cannot turn
// those pages alone into a course: the vocabulary is assembled from the
// glossary *and* every skill in the course, so rebuilding it needs the pages
// that did not change just as much as the ones that did.
//
// So the whole page set is cached. The snapshot is the whole of the wiki the
// server cares about, as of one instant, and the course is a pure function of
// it (see convert.rs). An incremental refresh is then a small idea: patch the
// stale entries, move the instant forward, and rebuild exactly as a first run
// would. That is the property that makes polling trustworthy — patching three
// pages yields exactly what a full refetch would. If the rebuild ever comes to
// depend on anything outside the snapshot, that guarantee is gone.

use std::collections::{HashMap, HashSet};

use crate::wiki::{NS_COURSE, NS_GLOSSARY, Revision, Wiki, WikiError};

// Every page the server cares about, as of one instant.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub timestamp: String,
    pub pages: HashMap<String, Revision>,
}

// The course a page belongs to: its name, minus any skill subpage.
//
// The same rule as the wiki's own `CourseName::fromPage`. Names carry the data
// here — a skill is `Skill:<course>/<skill>`, a glossary is `Glossary:<course>`
// — and no page stores a pointer to another.
pub fn course_of(title: &str) -> String {
    let without_namespace = title.split_once(':').map_or(title, |(_, rest)| rest);
    without_namespace
        .split_once('/')
        .map_or(without_namespace, |(head, _)| head)
        .trim()
        .to_string()
}

// The pages a page reaches: the next step of the walk.
//
// A course reaches the skills its layout places and its glossary; a skill
// reaches its tips, which are the same page name in another namespace.
pub fn references(page: &Revision) -> Vec<String> {
    if page.title.starts_with("Course:") {
        // `skills` is the authored list and `rows` is where they are placed;
        // the wiki validates that the two agree, and the course only has a use
        // for a skill that sits somewhere, so rows are what is followed.
        let mut out: Vec<String> = page
            .content
            .get("rows")
            .and_then(|rows| rows.as_array())
            .map(|rows| {
                rows.iter()
                    .filter_map(|row| row.as_array())
                    .flatten()
                    .filter_map(|skill| skill.as_str())
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default();
        out.push(format!("Glossary:{}", course_of(&page.title)));
        return out;
    }
    if let Some(rest) = page.title.strip_prefix("Skill:") {
        return vec![format!("Tips:{rest}")];
    }
    Vec::new()
}

// Bring the cached wiki state up to a new instant.
//
// The first refresh and every one after it differ in one respect only — which
// cached pages are treated as stale — and share the walk that follows.
pub async fn refresh(
    wiki: &Wiki,
    previous: Option<&Snapshot>,
    report: &mut (dyn FnMut(&str) + Send),
) -> Result<Snapshot, WikiError> {
    let until = match wiki.newest_change().await? {
        Some(timestamp) => timestamp,
        None => {
            // A wiki with nothing on its recent-changes list still has pages:
            // the seeder suppresses its own edits. With no change to pin to,
            // the server's own clock is the most recent instant we can honestly
            // claim.
            let now = wiki.server_time().await?;
            report(&format!(
                "no recent changes; taking the snapshot at the wiki clock, {now}"
            ));
            now
        }
    };

    refresh_at(wiki, previous, until, report).await
}

// Refresh at a timestamp the caller has already obtained. The server's poller
// asks for `newest_change` once per tick so an idle wiki costs exactly one
// request; when it did move, passing that answer here avoids asking twice.
pub async fn refresh_at(
    wiki: &Wiki,
    previous: Option<&Snapshot>,
    until: String,
    report: &mut (dyn FnMut(&str) + Send),
) -> Result<Snapshot, WikiError> {
    let mut previous = previous;
    let mut stale: HashSet<String> = HashSet::new();
    match previous {
        None => report(&format!(
            "first refresh; fetching the whole wiki as of {until}"
        )),
        Some(last) => {
            let oldest = wiki.oldest_change().await?;
            if oldest
                .as_deref()
                .is_some_and(|o| o > last.timestamp.as_str())
            {
                // Recent changes have been pruned past our last snapshot, so
                // the list can no longer say what went stale in between.
                report(&format!(
                    "recent changes only reach back to {}, later than the last snapshot at {}; \
                     refetching everything",
                    oldest.unwrap_or_default(),
                    last.timestamp
                ));
                previous = None;
            } else if until == last.timestamp {
                report(&format!("no changes since {}", last.timestamp));
            } else {
                stale = wiki.changed_between(&last.timestamp, &until).await?;
                report(&format!(
                    "{} page(s) changed between {} and {until}",
                    stale.len(),
                    last.timestamp
                ));
            }
        }
    }

    let have: HashMap<&str, &Revision> = previous.map_or_else(HashMap::new, |last| {
        last.pages
            .iter()
            .filter(|(title, _)| !stale.contains(title.as_str()))
            .map(|(title, page)| (title.as_str(), page))
            .collect()
    });

    // Start from every course that exists now, and from every course we already
    // knew about, so that a course deleted since the snapshot is still visited
    // and can be seen to be gone.
    let mut seeds: HashSet<String> = wiki.titles_in(NS_COURSE).await?.into_iter().collect();
    // And from every glossary page, because a large glossary is spread over
    // `Glossary:<course>/<letter>` subpages and nothing says which exist: a
    // course names its glossary, and a glossary's segments are found by sitting
    // beneath it. One extra listing per refresh is what that costs; asking is
    // the only way to learn of a segment somebody added.
    seeds.extend(wiki.titles_in(NS_GLOSSARY).await?);
    if let Some(last) = previous {
        seeds.extend(
            last.pages
                .keys()
                .filter(|title| title.starts_with("Course:"))
                .cloned(),
        );
    }

    let mut pages: HashMap<String, Revision> = HashMap::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut frontier: Vec<String> = seeds.into_iter().collect();
    frontier.sort();
    let mut fetched = 0usize;

    while !frontier.is_empty() {
        let batch: Vec<String> = frontier
            .iter()
            .filter(|title| !seen.contains(*title))
            .cloned()
            .collect();
        seen.extend(batch.iter().cloned());
        let wanted: Vec<String> = batch
            .iter()
            .filter(|title| !have.contains_key(title.as_str()))
            .cloned()
            .collect();
        let arrived = if wanted.is_empty() {
            HashMap::new()
        } else {
            wiki.at(&wanted, &until).await?
        };
        fetched += arrived.values().filter(|page| page.is_some()).count();

        frontier = Vec::new();
        for title in batch {
            let page = match have.get(title.as_str()) {
                Some(cached) => Some((*cached).clone()),
                None => arrived.get(&title).cloned().flatten(),
            };
            // Did not exist at the snapshot. A red link in a layout is the
            // ordinary case: the wiki invites authors to write what is missing,
            // so it is the rebuild's job to cope, not to complain.
            let Some(page) = page else { continue };
            frontier.extend(references(&page));
            pages.insert(title, page);
        }
    }

    report(&format!(
        "snapshot at {until}: {} page(s), {fetched} fetched",
        pages.len()
    ));
    Ok(Snapshot {
        timestamp: until,
        pages,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn page(title: &str, content: serde_json::Value) -> Revision {
        Revision {
            title: title.to_string(),
            revid: 1,
            timestamp: "2026-01-01T00:00:00Z".to_string(),
            model: "json".to_string(),
            content,
        }
    }

    #[test]
    fn a_course_is_read_off_the_page_name() {
        assert_eq!(
            course_of("Course:Spanish for English speakers"),
            "Spanish for English speakers"
        );
        assert_eq!(
            course_of("Skill:Spanish for English speakers/Food"),
            "Spanish for English speakers"
        );
        assert_eq!(course_of("Glossary:Toki"), "Toki");
        assert_eq!(course_of("Bare"), "Bare");
    }

    #[test]
    fn a_course_reaches_its_placed_skills_and_its_glossary() {
        let course = page(
            "Course:Spanish for English speakers",
            json!({"rows": [["Skill:Spanish for English speakers/Intro"], ["Skill:Spanish for English speakers/Food"]]}),
        );
        assert_eq!(
            references(&course),
            [
                "Skill:Spanish for English speakers/Intro",
                "Skill:Spanish for English speakers/Food",
                "Glossary:Spanish for English speakers",
            ]
        );
    }

    // A skill's tips are the same page name in another namespace, and that is
    // the whole of the link between them.
    #[test]
    fn a_skill_reaches_its_tips() {
        let skill = page("Skill:Spanish for English speakers/Food", json!({}));
        assert_eq!(
            references(&skill),
            ["Tips:Spanish for English speakers/Food"]
        );
    }

    #[test]
    fn a_course_with_no_rows_still_reaches_its_glossary() {
        let course = page("Course:Toki", json!({}));
        assert_eq!(references(&course), ["Glossary:Toki"]);
    }

    #[test]
    fn a_glossary_reaches_nothing() {
        assert!(references(&page("Glossary:Toki", json!({}))).is_empty());
    }
}
