mod api;
mod auth;
mod card;
mod convert;
mod course;
mod dictionary;
mod exercise;
mod gloss;
mod leaderboard;
mod lesson;
mod loader;
mod messages;
mod position;
mod profile;
mod quests;
mod sentence;
mod server;
mod skill;
mod snapshot;
mod store;
mod user;
mod vocab;
mod wiki;
mod word;

use std::collections::HashMap;
use std::io;
use std::sync::Arc;
use std::time::Duration;

// where user data is persisted
const DB_PATH: &str = "mimi.db";
const WIKI_API: &str = "http://mimi.localhost:4771/api.php";
const POLL_EVERY: Duration = Duration::from_secs(5);

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api = std::env::var("WIKI_API").unwrap_or_else(|_| WIKI_API.to_string());
    let language_codes = language_codes()?;
    let wiki = Arc::new(wiki::Wiki::new(&api));

    let mut report = |message: &str| println!("wiki: {message}");
    let snapshot = snapshot::refresh(&wiki, None, &mut report).await?;
    let courses = content_of(&snapshot, &language_codes).map_err(io::Error::other)?;
    println!("loaded {} course(s) from {api}", courses.len());
    for (id, (course, _)) in &courses {
        println!(
            "  '{id}': {} words, {} skills, {} sentences ({} questions)",
            course.vocab.len(),
            course.skills().len(),
            course.sentences.len(),
            course.sentences.len() * exercise::Ask::ALL.len()
        );
    }

    let db_path = std::env::var("DB_PATH").unwrap_or_else(|_| DB_PATH.to_string());
    let store = store::Store::open(&db_path)?;
    println!("using database {db_path}");

    // A fresh database has no accounts in it, and that is now the whole
    // truth: there is no invented example learner. Everything the profile
    // page and the leaderboard show is something somebody actually did.
    let auth_url =
        std::env::var("MIMI_AUTH_URL").unwrap_or_else(|_| "http://127.0.0.1:4770".to_string());
    let secure_cookies = std::env::var("MIMI_SECURE_COOKIES")
        .is_ok_and(|value| matches!(value.as_str(), "1" | "true" | "yes"));
    let frontend_origin = std::env::var("MIMI_FRONTEND_ORIGIN")
        .unwrap_or_else(|_| "http://localhost:4773".to_string());
    let state = server::AppState::new(
        courses,
        store,
        auth::CredentialService::new(&auth_url),
        secure_cookies,
        frontend_origin.clone(),
    );
    let app = server::router(state.clone());
    println!("using private authentication service {auth_url}");
    println!("accepting browser requests from {frontend_origin}");
    tokio::spawn(poll(wiki, snapshot, language_codes, state));

    let host = std::env::var("HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = std::env::var("PORT").unwrap_or_else(|_| "4772".to_string());
    let addr = format!("{host}:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    println!("mimi_backend listening on http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

// Build every course and its dictionary from one coherent snapshot. Serving
// all of them from the same generation means a chooser never advertises a
// course whose content belongs to a different point in wiki history.
fn content_of(
    snapshot: &snapshot::Snapshot,
    language_codes: &HashMap<String, String>,
) -> Result<HashMap<String, (course::Course, dictionary::Dictionary)>, String> {
    let conversion = convert::convert(snapshot, language_codes);
    for warning in conversion.warnings {
        eprintln!("wiki content warning: {warning}");
    }
    let mut courses = HashMap::new();
    for converted in conversion.courses {
        let id = converted.id.clone();
        let dictionary = dictionary::Dictionary::from_entries(converted.glossary);
        let course = loader::assemble(
            converted.index,
            converted.words,
            converted.layout,
            converted.skills,
        )
        .map_err(|error| format!("course '{id}': {error}"))?;
        courses.insert(id, (course, dictionary));
    }
    if courses.is_empty() {
        return Err("the wiki has no usable courses".to_string());
    }
    Ok(courses)
}

// Keep the last known-good snapshot and content alive. A network, conversion
// or validation failure is reported and retried on the next tick; it never
// takes the current courses away from learners.
async fn poll(
    wiki: Arc<wiki::Wiki>,
    mut snapshot: snapshot::Snapshot,
    language_codes: HashMap<String, String>,
    state: Arc<server::AppState>,
) {
    let mut ticks = tokio::time::interval(POLL_EVERY);
    // `interval`'s first tick is immediate; boot already fetched the wiki.
    ticks.tick().await;
    loop {
        ticks.tick().await;
        let newest = match wiki.newest_change().await {
            Ok(value) => value,
            Err(error) => {
                eprintln!("wiki poll failed: {error}");
                continue;
            }
        };
        let Some(newest) = newest else {
            continue;
        };
        if newest == snapshot.timestamp {
            continue;
        }

        let mut report = |message: &str| println!("wiki: {message}");
        let refreshed =
            match snapshot::refresh_at(&wiki, Some(&snapshot), newest, &mut report).await {
                Ok(snapshot) => snapshot,
                Err(error) => {
                    eprintln!("wiki refresh failed: {error}");
                    continue;
                }
            };
        let courses = match content_of(&refreshed, &language_codes) {
            Ok(content) => content,
            Err(error) => {
                eprintln!("wiki rebuild failed: {error}");
                continue;
            }
        };
        println!("reloaded {} course(s)", courses.len());
        state.replace_content(courses);
        snapshot = refreshed;
    }
}

// The built-in language table covers common courses. Authors can extend it
// without rebuilding the server, matching mimi_integrator's repeatable CLI
// option: `mimi_backend --language Toki=tok --language Klingon=tlh`.
fn language_codes() -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
    let mut codes = HashMap::new();
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        let pair = if argument == "--language" {
            args.next().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "--language expects NAME=CODE")
            })?
        } else if let Some(pair) = argument.strip_prefix("--language=") {
            pair.to_string()
        } else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("unknown argument '{argument}'"),
            )
            .into());
        };
        let (name, code) = pair.split_once('=').ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "--language expects NAME=CODE")
        })?;
        if name.trim().is_empty() || code.trim().is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "--language expects non-empty NAME=CODE",
            )
            .into());
        }
        codes.insert(name.trim().to_string(), code.trim().to_string());
    }
    Ok(codes)
}
