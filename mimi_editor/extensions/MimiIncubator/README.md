# MimiIncubator

Structured MediaWiki content models and editors for Mimi course design.

## Models

- `mimi-skill` in `Skill:` stores a grammar focus and a word list. Each word owns its example sentences; every sentence has one text and one translation, each with its own list of accepted alternatives, plus optional internal notes and a disabled flag. Skill pages list the sentence with its translation and expand on click to show the alternatives; the completion check mark is editor-only, and a sentence without one, text or translation still missing, does not appear on the skill page at all.
- `mimi-course-layout` in `Course:` stores the target and source languages, the complete skill list, branching rows, and castle/checkpoint boundaries. Courses are displayed as “Spanish for English speakers,” never as an ambiguous language name.
- `mimi-glossary` in `Glossary:` files every word the course teaches under the lemma it belongs to: the dictionary form, `comer`, `subir`, `mucho gusto`, and beneath it the forms that lemma takes, each with its own ordered list of translations as plain strings. The first row of an entry is the lemma standing for itself, so it carries no spelling of its own, only the plain gloss (`comer` → “to eat”); every row after it carries a form and what that form means (`comemos` → “we eat”). It shows as a dash in the form column, because there is nothing there to name: the lemma is already spelt out beside it. An explicit form may not repeat that lemma spelling, because the dash already covers it. Nothing names the slot a form fills: that was a free-text label, and an unconstrained field that could say anything was more trouble than the little it added, a paradigm is read by its spellings. A half-written row saves anyway, a paradigm is worth keeping mid-conjugation, and an editor should be able to stop, but the glossary page does not publish it. What it publishes is exactly what the editor puts a check mark against: a form with a spelling and at least one translation. Anything less waits in the source, visible where it can be finished. The lemma's own row is the exception and always shows, because it is what the entry is. By convention, the first translation is preferred. Lemmas are unique within a glossary and forms within an entry, the editor sorts entries alphabetically on save, and forms keep the order they were written in, because a paradigm is read in it, which is also what makes a missing one visible.
- `mimi-tips` in `Tips:` stores the short notes a skill shows before it is practised, what `buenos días` literally means, when to stop saying it. A tip is a title, a body written in the Markdown subset below, and optionally the lesson number it appears before. A tip with a lesson is put in front of the learner once, as that lesson begins; a tip without one is shown in no lesson at all and waits behind the skill's tips button for a learner who goes looking. Titles are unique within a page, and tips keep the order they were written in, because a learner meets them in it.

All four models are JSON, validated on every save against the schemas in `schemas/`, and rendered as domain-specific read and diff views. Their edit action loads a Vue 3 + Codex interface that follows MediaWiki's visual conventions and saves through MediaWiki's edit API.

Every page carries a `schemaVersion`, and every stored page is at the current
one: skill 5, course layout 5, glossary 3, tips 1. The wiki passed through
several earlier shapes while it was being worked out, and was rewritten in
place once those settled; nothing reads an older shape now, so a bump has to
rewrite the stored pages along with the code. Revisions older than the current
versions remain in page history and no longer render in full, a diff reaching
one shows the fields it still understands and marks the rest as not set.

## Diffs

Nobody edits the JSON, so nobody has to read it in page history either. A diff
is a card for each thing that changed, a word, a glossary entry, a tip, the
course tree, headed by its own name and by what became of it, and holding one
line per changed field with the label written once and the two versions beside
it. Sentences fixed in place are highlighted word by word, using MediaWiki's own
word-level differ, so the correction shows rather than the sentence. A bar above
the cards tallies the edit; the counts that used to be diffed as fields live
there now, since a word count changing says nothing the cards do not.

Two things are worth knowing before reading one. A card is found by the name the
author gave it, so a word dragged up the list reads as **moved** rather than as
two rewrites, and inserting a lemma at the top of a five-thousand-entry glossary
disturbs nothing below it, but renaming a word is a removal and an addition,
because that is what replacing a word is. Within a card, sentences and glossary
forms are numbered by position, having no names of their own, so inserting one
renumbers those after it and a whole sentence added or taken away is labelled as
such on its heading.

Course pages show each skill's completion percentage (share of sentences with
text and at least one translation), computed live from the skill pages.
Skill icons are selected automatically from a shared keyword map and rendered
from MediaWiki's bundled, open-source Codex Icons package. Course editors can
drag skills within and between rows, and every list an author decides the order
of: a skill's words, a word's sentences, a form's translations, is dragged by
the grip at the left of its rows, or reordered from the keyboard with that grip
focused and the up and down arrows. Course summaries link to the course's
`Glossary:<course name>` page, which replaced the never-built `Words:` one.
Glossary pages and the glossary editor both filter entries as you type, because
a course glossary grows well past what a single screen holds. A glossary page
is one table of three columns, lemma, form, translations, where an entry is a
row group rather than a row, so a filter hit shows the whole paradigm it was
found in, and a lemma with nothing under it is visibly a lemma nobody has
written the forms for yet.

**A glossary page sends its entries, not its rows.** Five hundred entries are
fifteen thousand rows, which is megabytes of HTML and a layout no browser
should be asked for on behalf of a reader looking at twenty of them. So the
page carries its first fifty entries as rows, a readable page with JavaScript
off: and all of them as JSON, and `view.js` builds the rows of a block of
entries as it is scrolled near, taking them down again once it is well past.
The table holds little more than the screenful being read, filtering runs over
the data rather than the DOM, and the two renderers, `renderStructuredView()`
in PHP and `group()` in `view.js`, write the same rows and have to be kept in
step.

A glossary that has outgrown a page is spread over `Glossary:<course name>/A`,
`/B`, `/C` … segments: a language has more words than one MediaWiki article may
hold, and five thousand Spanish lemmas with their forms come to some fourteen
megabytes. The glossary is then the page the course names *together with* its
subpages, which is how the read views, the front-page totals and the learner
backend all read it, nothing lists the segments, so nothing can be out of date
about which exist. Every page of a split glossary carries the index of the
others, and the page they hang from usually holds nothing else. Filtering and
the uniqueness of lemmas are per page, because a page is what was loaded.

## Tips

Tips are optional and are named after the skill they belong to: the tips for
`Skill:Spanish for English speakers/Greetings` live at
`Tips:Spanish for English speakers/Greetings`. Neither page stores a pointer to
the other, one page name in two namespaces is the whole link, which is what
`CourseName::sibling()` builds. A skill without tips still shows the link, red,
because on this wiki a red link is the invitation to write what is missing, and
following it opens the tips editor on a blank page.

Bodies are stored in a small, closed Markdown subset: `## heading`, `- bullet`,
`1. numbered` and blank-line paragraphs, with `**bold**`, `*italic*`,
`<u>underline</u>` and `[label](url)` inside them, and a backslash escaping the
character after it. Underline is the one borrowing from Markdown's inline HTML,
because Markdown has no underline of its own and the editor offers one. Links
may only point at `https://`, `/` or `#`; anything else is left as the text it
was written as.

Nobody types that syntax. The editor gives a formatting canvas, a
contenteditable region with a Bold / Italic / Underline / heading / list / link
toolbar, and the Ctrl+B, Ctrl+I and Ctrl+U that browsers already bind, and
serialises what it holds back to the subset on every keystroke. Storing Markdown
rather than the canvas's HTML is what keeps a revision diffable as text in page
history and keeps the read view safe to render: nothing outside the grammar
above can survive a trip through the editor, and pasted markup is reduced to its
text on the way in.

The consequence is two renderers for one grammar, `includes/Markdown.php` for
the read view and the top of `resources/editor.js` for the canvas, which has to
show the page that will be published. **They have to agree.** Change one and
change the other; the fastest check is to render the same battery of inputs
through both and diff the output.

## Front page

The main page is laid out the way MediaWiki main pages have always been,
bordered boxes with tinted heading bars, arranged by the page's own wikitext,
after the English Wikipedia's, down to its colours: mint down the left column,
blue down the right, grey for the banner and the footer. Six parser tags fill
those boxes with whatever the wiki currently holds, so the prose around them
stays editable while the content inside cannot drift:

- `<mimilearn />` explains that this is the editing site and links to the
  learner-facing site configured by `MIMI_LEARNER_URL` (by default,
  `http://localhost:4773`).
- `<mimistats />` counts the courses, skills, sentences and glossary terms for
  the line under the banner.
- `<mimicourses />` gives every `Course:` page a card of its own: the name
  under its pair's flags (`Flag::forLanguage()` draws them as inline SVG), then
  its bare counts, skills, glossary terms, sentences.
- `<miminewcourse />` is the two boxes of a course's name, “_ for _ speakers”.
  Naming a pair goes to that course, or to a blank one to write when nobody has
  started it. `Special:NewCourse` is what the form submits to and decides which
  of the two it is, so the box needs no JavaScript.
- `<mimisentences limit="4" />` shows a few sentences the courses actually
  teach, drawn at random, in the spirit of Wikipedia's “Did you know”. Disabled
  and half-written sentences are left out.
- `<mimiactivity limit="8" />` lists the structured pages edited most recently,
  one row per page rather than per edit.

Their styling is `resources/frontpage.css`, written by hand rather than in
Tailwind: the wikitext shares those classes, and Tailwind only ever sees this
repository. It is the one stylesheet here that is not generated; the Skill,
Course and Glossary views stay on Tailwind.

Summarising a course opens every skill it lists, so pages read once are held for
the rest of the request, and the parser cache expires after ten minutes rather
than waiting for an edit to the main page itself.

`maintenance/SeedMainPage.php` writes the front page, and the container runs it
at start-up. It only replaces the placeholder MediaWiki's installer leaves
behind; pass `--force` to overwrite a main page that has been edited since.

## Development

Template styling uses Tailwind utilities. Build the checked-in extension CSS
with the standalone CLI; the script downloads the pinned, gitignored binary on
first use and then works locally:

```sh
./build-tailwind.sh
```

The repository's Compose file bind-mounts this directory and `LocalSettings.php` loads it. Restart after changing `extension.json`:

```sh
docker compose restart mediawiki
```

The local container starts with an empty wiki apart from the front page. Create
the first course through `Special:NewCourse`, then add pages beneath
`Skill:<course>/`, a `Glossary:<course>` for the vocabulary those skills use,
and `Tips:<course>/<skill>` pages where a skill needs explaining. Namespace
defaults select the correct model automatically.
