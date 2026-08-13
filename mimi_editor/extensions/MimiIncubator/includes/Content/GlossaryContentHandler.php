<?php

namespace MediaWiki\Extension\MimiIncubator\Content;

use MediaWiki\Extension\MimiIncubator\CourseName;
use MediaWiki\Html\Html;
use MediaWiki\MediaWikiServices;
use MediaWiki\Page\PageReference;
use MediaWiki\Title\Title;

final class GlossaryContentHandler extends StructuredContentHandler
{
    /**
     * How many entries the page itself is written with. Enough that a small
     * glossary is published whole and a large one still reads as a glossary
     * with JavaScript off; the rest arrive as data and are rendered on
     * approach. See renderStructuredView() and `view.js`.
     */
    private const PREVIEW_ENTRIES = 50;

    public function __construct()
    {
        parent::__construct("mimi-glossary");
    }
    protected function getContentClass()
    {
        return GlossaryContent::class;
    }
    protected function getSchemaFile(): string
    {
        return "glossary.schema.json";
    }
    protected function getEditorKind(): string
    {
        return "glossary";
    }
    public function makeEmptyContent()
    {
        return new GlossaryContent('{"schemaVersion":3,"entries":[]}');
    }

    /** Lemmas, forms and translations are compared case-insensitively, ignoring padding. */
    private static function comparable(mixed $list): array
    {
        $texts = [];
        foreach (is_array($list) ? $list : [] as $text) {
            if (is_string($text)) {
                $texts[] = mb_strtolower(trim($text));
            }
        }
        return $texts;
    }

    protected function validateSemantics(
        object $data,
        string $courseName,
    ): array {
        if (!isset($data->entries) || !is_array($data->entries)) {
            return [];
        }
        // Anything of the wrong shape is skipped here; the schema reports it.
        $lemmas = [];
        $repeatedTranslation = false;
        $repeatedForm = false;
        $repeatedLemmaForm = false;
        $namedLemmaRow = false;
        foreach ($data->entries as $entry) {
            if (!is_object($entry)) {
                continue;
            }
            $lemmas[] = $entry->lemma ?? null;
            $spellings = [];
            foreach (
                is_array($entry->forms ?? null) ? $entry->forms : []
                as $index => $form
            ) {
                if (!is_object($form)) {
                    continue;
                }
                $spelling = trim((string) ($form->form ?? ""));
                // The first row of an entry is the lemma standing for itself, so
                // it holds the plain gloss and has no spelling of its own.
                if ($index === 0) {
                    $namedLemmaRow = $namedLemmaRow || $spelling !== "";
                } elseif ($spelling !== "") {
                    $repeatedLemmaForm =
                        $repeatedLemmaForm ||
                        mb_strtolower($spelling) ===
                            mb_strtolower(trim((string) ($entry->lemma ?? "")));
                    // Two blank forms are one complaint, not two, so only the
                    // written ones are compared with each other. A form missing
                    // its spelling or its translations is saved as it stands —
                    // half a paradigm is worth keeping — and simply left
                    // unpublished by renderStructuredView() until somebody
                    // finishes it.
                    $spellings[] = $spelling;
                }
                $translations = self::comparable($form->translations ?? null);
                $repeatedTranslation =
                    $repeatedTranslation ||
                    count($translations) !== count(array_unique($translations));
            }
            $spellings = self::comparable($spellings);
            $repeatedForm =
                $repeatedForm ||
                count($spellings) !== count(array_unique($spellings));
        }
        $lemmas = self::comparable($lemmas);
        $errors = [];
        if (count($lemmas) !== count(array_unique($lemmas))) {
            $errors[] = "lemmas must be unique within a glossary";
        }
        if ($namedLemmaRow) {
            $errors[] =
                "the first form of an entry is the lemma itself, and carries no spelling of its own";
        }
        if ($repeatedForm) {
            $errors[] = "forms must be unique within an entry";
        }
        if ($repeatedLemmaForm) {
            $errors[] =
                "the lemma already represents its own form and must not be repeated as a spelling";
        }
        if ($repeatedTranslation) {
            $errors[] = "translations must be unique within a form";
        }
        return $errors;
    }

    /**
     * The segments a glossary is spread over, in title order.
     *
     * A glossary outgrows a page long before a language runs out of words:
     * five thousand Spanish lemmas and the forms they take are some fourteen
     * megabytes of JSON, and MediaWiki refuses an article past
     * `$wgMaxArticleSize`. So a large glossary is written to
     * `Glossary:<course>/<letter>` subpages, and the whole glossary is the page
     * the course is named after *together with* those segments — which is how
     * the read views below, `CourseCatalogue` and the learner backend all read
     * it.
     *
     * Nothing points anywhere. A segment is found by being a subpage of its
     * glossary, exactly as a skill is found by being a subpage of its course,
     * so the two cannot disagree about which segments exist. The index is built
     * when the page is parsed, so a segment created afterwards appears on the
     * pages linking to it once their parser cache next turns over.
     *
     * @return Title[]
     */
    private static function segments(Title $root): array
    {
        $pages = MediaWikiServices::getInstance()
            ->getPageStore()
            ->newSelectQueryBuilder()
            ->whereTitlePrefix($root->getNamespace(), $root->getDBkey() . "/")
            ->orderByTitle()
            ->caller(__METHOD__)
            ->fetchPageRecords();
        $titles = [];
        foreach ($pages as $segment) {
            $titles[] = Title::newFromPageIdentity($segment);
        }
        return $titles;
    }

    /**
     * The index a split glossary carries on every one of its pages: the whole
     * of it is a click away from any part of it, and the part being read is
     * named rather than linked. An unsplit glossary has no index and shows
     * none.
     */
    private static function segmentIndex(
        Title $root,
        PageReference $page,
        array $segments,
    ): string {
        if (!$segments) {
            return "";
        }
        $links = [
            $root->isSamePageAs($page)
                ? Html::element(
                    "strong",
                    ["class" => "px-2 py-1 text-sm"],
                    "Index",
                )
                : Html::element(
                    "a",
                    [
                        "class" => "px-2 py-1 text-sm",
                        "href" => $root->getLocalURL(),
                    ],
                    "Index",
                ),
        ];
        foreach ($segments as $segment) {
            $name = CourseName::subpageName($segment);
            $links[] = $segment->isSamePageAs($page)
                ? Html::element(
                    "strong",
                    ["class" => "bg-[#eaecf0] px-2 py-1 text-sm"],
                    $name,
                )
                : Html::element(
                    "a",
                    [
                        "class" => "px-2 py-1 text-sm",
                        "href" => $segment->getLocalURL(),
                    ],
                    $name,
                );
        }
        return Html::rawElement(
            "nav",
            [
                "class" =>
                    "mb-3 flex flex-wrap items-center gap-1 border border-solid border-[#a2a9b1] bg-[#f8f9fa] px-2 py-2",
                "aria-label" => "Glossary segments",
            ],
            implode("", $links),
        );
    }

    protected function renderStructuredView(
        object $data,
        string $courseName,
        PageReference $page,
    ): string {
        // The page a glossary is filed under, which is the page itself unless
        // this is one of its segments.
        $root =
            Title::makeTitleSafe($page->getNamespace(), $courseName) ??
            Title::castFromPageReference($page);
        $segments = self::segments($root);
        $entries = $data->entries ?? [];
        // What the page publishes, as plain arrays: `[lemma, [[form,
        // [translation…]]…]]`. Both the rows below and the payload the client
        // renders from are built from this, so a reader with JavaScript and one
        // without are looking at the same glossary.
        $published = [];
        $formCount = 0;
        foreach ($entries as $entry) {
            $lemma = (string) ($entry->lemma ?? "");
            $forms = $entry->forms ?? [];
            // A half-written form stays in the page's source, so its author can
            // come back and finish it, but it is not published. The rule is the
            // editor's check mark, so that the two cannot disagree: a form
            // appears once it has a spelling and something to translate it as.
            // The lemma's own row is shown whatever state it is in, because it
            // is what the entry is.
            $shown = [];
            foreach (is_array($forms) ? $forms : [] as $index => $form) {
                $translations = array_values(
                    array_filter(
                        array_map(
                            static fn($text) => (string) $text,
                            is_array($form->translations ?? null)
                                ? $form->translations
                                : [],
                        ),
                        static fn(string $text) => trim($text) !== "",
                    ),
                );
                $spelling = trim((string) ($form->form ?? ""));
                if ($index === 0 || ($spelling !== "" && $translations !== [])) {
                    $shown[] = [$spelling, $translations];
                }
            }
            if (!$shown) {
                $shown = [["", []]];
            }
            $formCount += count($shown) - 1;
            $published[] = [$lemma, $shown];
        }
        $rows = "";
        // Only the first entries are written into the page. The whole of it goes
        // out as data below, and `view.js` builds the rest of the rows as they
        // are scrolled to: a five-hundred-entry segment is fifteen thousand rows
        // and several megabytes of HTML, which is a page no browser should be
        // asked to lay out at once for a reader looking at twenty of them. What
        // is written here is what a reader without JavaScript gets, so it is a
        // usable page rather than an empty one.
        foreach (array_slice($published, 0, self::PREVIEW_ENTRIES) as $entry) {
            [$lemma, $shown] = $entry;
            $formRows = "";
            foreach ($shown as $index => [$spelling, $translations]) {
                $translationLines = implode(
                    "",
                    array_map(
                        static fn($translation) => Html::element(
                            "div",
                            [],
                            $translation,
                        ),
                        $translations,
                    ),
                );
                // One cell spans the whole group: every form beneath it is a
                // form of that one lemma, which is what the entry is filed under.
                $lemmaCell =
                    $index === 0
                        ? Html::element(
                            "th",
                            [
                                "class" =>
                                    "border-0 border-r border-[#eaecf0] px-4 py-3 text-left align-top text-sm font-semibold",
                                "scope" => "rowgroup",
                                "rowspan" => (string) count($shown),
                            ],
                            $lemma,
                        )
                        : "";
                $formRows .= Html::rawElement(
                    "tr",
                    [
                        "class" =>
                            $index === count($shown) - 1
                                ? ""
                                : "border-0 border-b border-[#eaecf0]",
                    ],
                    $lemmaCell .
                        Html::rawElement(
                            "td",
                            ["class" => "px-4 py-3 align-top text-sm"],
                            $spelling === ""
                                ? // The lemma's own row: it stands for the
                                  // dictionary form, so it names no form of its own.
                                  Html::element(
                                    "span",
                                    [
                                        "class" => "text-[#72777d]",
                                        "title" => "The lemma itself",
                                    ],
                                    "—",
                                )
                                : Html::element("div", [], $spelling),
                        ) .
                        Html::rawElement(
                            "td",
                            ["class" => "px-4 py-3 align-top text-sm"],
                            $translationLines === ""
                                ? Html::element(
                                    "em",
                                    ["class" => "text-[#72777d]"],
                                    "No translation yet",
                                )
                                : $translationLines,
                        ),
                );
            }
            // An entry is one row group, because a lemma without its forms and a
            // form without its lemma are both half an answer — which is also
            // why filtering, in `view.js`, keeps or drops a whole entry.
            $rows .= Html::rawElement(
                "tbody",
                ["class" => "border-0 border-b border-[#c8ccd1]"],
                $formRows,
            );
        }
        if ($rows === "") {
            $rows = Html::rawElement(
                "tbody",
                [],
                Html::rawElement(
                    "tr",
                    [],
                    Html::element(
                        "td",
                        [
                            "class" =>
                                "px-4 py-8 text-center text-sm text-[#72777d]",
                            "colspan" => "3",
                        ],
                        $segments === []
                            ? "This glossary has no entries yet."
                            : "This page holds no entries of its own. The glossary is filed in the segments above.",
                    ),
                ),
            );
        }
        // A glossary page named outside the course convention still needs headers.
        [$target, $source] = CourseName::languages($courseName);
        $courseTitle = "Course:" . $courseName;
        // Only this page's own entries are counted, because only they were read:
        // the segments are whole pages of their own, each carrying its own
        // count, and the page a split glossary is filed under usually holds
        // nothing but the index of them.
        $held =
            count($entries) .
            " words and phrases, " .
            $formCount .
            " further forms";
        if ($segments !== []) {
            $held =
                $entries === []
                    ? "Filed in " . count($segments) . " segments, listed below"
                    : $held . " on this page, of " . count($segments) . " segments";
        }
        return Html::rawElement(
            "div",
            [
                "class" =>
                    "my-6 max-w-4xl font-sans text-[#202122] [&_a]:cursor-pointer [&_h2]:font-sans",
                "data-mimi-glossary-view" => "",
            ],
            Html::rawElement(
                "dl",
                [
                    "class" =>
                        "mb-6 grid gap-1 border-l-4 border-[#a2a9b1] bg-[#f8f9fa] px-4 py-3 sm:grid-cols-[8rem_minmax(0,1fr)] sm:gap-4",
                ],
                Html::element(
                    "dt",
                    ["class" => "text-sm font-semibold text-[#54595d]"],
                    "Entries",
                ) .
                    Html::element(
                        "dd",
                        ["class" => "m-0 text-sm text-[#202122]"],
                        $held,
                    ) .
                    Html::element(
                        "dt",
                        ["class" => "text-sm font-semibold text-[#54595d]"],
                        "Course",
                    ) .
                    Html::rawElement(
                        "dd",
                        ["class" => "m-0 text-sm"],
                        Html::element(
                            "a",
                            [
                                "href" =>
                                    wfScript("index") .
                                    "?title=" .
                                    rawurlencode($courseTitle),
                            ],
                            $courseTitle,
                        ),
                    ),
            ) .
                self::segmentIndex($root, $page, $segments) .
                Html::rawElement(
                    "div",
                    ["class" => "mb-3"],
                    Html::element("input", [
                        "type" => "search",
                        "class" =>
                            "w-full max-w-sm rounded-sm border border-solid border-[#a2a9b1] px-3 py-2 text-sm",
                        // The filter is the page's own, because the page is all
                        // that was loaded; a split glossary is searched a
                        // segment at a time, or through the wiki's search.
                        "placeholder" =>
                            $segments === []
                                ? "Filter words, forms and translations"
                                : "Filter this segment",
                        "aria-label" => "Filter glossary entries",
                        "data-mimi-glossary-filter" => "",
                    ]),
                ) .
                Html::rawElement(
                    "table",
                    [
                        "class" =>
                            "w-full table-fixed border-collapse border border-solid border-[#a2a9b1] bg-white",
                        "data-mimi-glossary-table" => "",
                    ],
                    Html::rawElement(
                        "thead",
                        [],
                        Html::rawElement(
                            "tr",
                            [
                                "class" =>
                                    "border-0 border-b border-[#c8ccd1] bg-[#f8f9fa]",
                            ],
                            // table-fixed takes its column widths from this row,
                            // so the body cells carry none: the lemma's spans a group.
                            Html::element(
                                "th",
                                [
                                    "class" =>
                                        "w-1/4 px-4 py-3 text-left text-sm font-semibold",
                                    "scope" => "col",
                                ],
                                $target ?: "Term",
                            ) .
                                Html::element(
                                    "th",
                                    [
                                        "class" =>
                                            "w-1/4 px-4 py-3 text-left text-sm font-semibold",
                                        "scope" => "col",
                                    ],
                                    "Form",
                                ) .
                                Html::element(
                                    "th",
                                    [
                                        "class" =>
                                            "px-4 py-3 text-left text-sm font-semibold",
                                        "scope" => "col",
                                    ],
                                    $source ?: "Translation",
                                ),
                        ),
                    ) . $rows,
                ) .
                Html::element(
                    "p",
                    [
                        "class" =>
                            "hidden py-6 text-center text-sm text-[#72777d]",
                        "data-mimi-glossary-empty" => "",
                    ],
                    "No entries match that filter.",
                ) .
                    // Said only where it is true, and taken away by `view.js` the
                    // moment it isn't: the rest of the entries are already here,
                    // as data, waiting to be scrolled to.
                    (count($published) > self::PREVIEW_ENTRIES
                        ? Html::element(
                            "p",
                            [
                                "class" => "py-4 text-center text-sm text-[#72777d]",
                                "data-mimi-glossary-rest" => "",
                            ],
                            "Showing the first " .
                                self::PREVIEW_ENTRIES .
                                " of " .
                                count($published) .
                                " entries. The rest load as you scroll; without JavaScript, use the wiki's search.",
                        )
                        : "") .
                    self::payload($published),
        );
    }

    /**
     * Every entry the page publishes, for the client to render rows from.
     *
     * The rows themselves are the expensive part — an entry is a row group of
     * up to eighty rows, and a segment runs to five hundred entries — so what
     * goes over the wire is the glossary rather than a rendering of it, and
     * `view.js` builds the handful of rows that are actually on screen. The
     * shape is positional (`[lemma, [[form, [translation…]]…]]`) because the
     * field names would otherwise be repeated seventy-five thousand times.
     *
     * `<` and `&` are escaped as `<` and `&`, which is what keeps a
     * translation containing `</script>` from ending the element early.
     */
    private static function payload(array $published): string
    {
        return Html::rawElement(
            "script",
            [
                "type" => "application/json",
                "data-mimi-glossary-data" => "",
            ],
            json_encode(
                $published,
                JSON_HEX_TAG |
                    JSON_HEX_AMP |
                    JSON_UNESCAPED_SLASHES |
                    JSON_UNESCAPED_UNICODE,
            ),
        );
    }
}
