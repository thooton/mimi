(function () {
    "use strict";
    const {
        createMwApp,
        ref,
        computed,
        nextTick,
        onMounted,
        onBeforeUnmount,
        watch,
    } = require("vue");
    const {
        CdxButton,
        CdxIcon,
        CdxTextInput,
        CdxTextArea,
        CdxMessage,
    } = require("./codex.js");
    const codexIcons = require("./icons.json");
    const skillIconRules = require("./skill-icons.json");
    const config = mw.config.get("mimiEditorConfig");
    if (!config) {
        return;
    }
    const editorRoot = document.getElementById("mimi-editor-root");
    const editorRootParent = editorRoot.parentNode;
    const editorRootNextSibling = editorRoot.nextSibling;
    const mobileViewport = window.matchMedia("(max-width: 1023px)");
    // Vector gives both its page container and #bodyContent a z-index of 0. A
    // fixed editor left inside them can never cover the skin header, even with
    // its own high z-index, so lift the mobile app to <body>, and put it back
    // where the skin left it when the viewport widens again.
    const placeEditor = () => {
        if (mobileViewport.matches) {
            document.body.append(editorRoot);
        } else if (editorRoot.parentNode !== editorRootParent) {
            editorRootParent.insertBefore(editorRoot, editorRootNextSibling);
        }
    };
    editorRoot.classList.add("mimi-editor-root");
    placeEditor();
    mobileViewport.addEventListener("change", placeEditor);
    document.getElementById("firstHeading")?.classList.add("font-sans");

    /**
     * A list the editor can bind to. Several fields are optional in the schema
     * and absent from a page that never filled them in, but every v-model
     * behind them needs an array to push into.
     */
    function list(value) {
        return Array.isArray(value) ? value : [];
    }

    /**
     * Fill in what the schema leaves optional, so the templates can bind
     * without guarding every field. This is not a migration: every stored page
     * is already at the version stamped here.
     */
    function normalize(content) {
        if (config.kind === "skill") {
            content.words =
                Array.isArray(content.words) && content.words.length
                    ? content.words
                    : [{ word: "example", sentences: [] }];
            content.words.forEach((word) => {
                word.sentences = list(word.sentences);
                word.sentences.forEach((sentence) => {
                    sentence.notes = sentence.notes || "";
                    sentence.disabled = !!sentence.disabled;
                    sentence.translation = sentence.translation || "";
                    sentence.alternativeSentences = list(
                        sentence.alternativeSentences,
                    );
                    sentence.alternativeTranslations = list(
                        sentence.alternativeTranslations,
                    );
                });
            });
            content.schemaVersion = 5;
        }
        if (config.kind === "course") {
            content.castles = list(content.castles);
            content.schemaVersion = 5;
        }
        if (config.kind === "glossary") {
            content.entries = list(content.entries);
            content.entries.forEach((entry) => {
                entry.lemma = entry.lemma || "";
                entry.forms = list(entry.forms);
                // Every entry opens with the row standing for the lemma, so
                // one that has lost it is given it back rather than promoting
                // a form into a seat that is not its own.
                if (!entry.forms.length) {
                    entry.forms.push({ form: "", translations: [] });
                }
                entry.forms.forEach((form, index) => {
                    form.form = index === 0 ? "" : form.form || "";
                    form.translations = list(form.translations);
                });
            });
            content.schemaVersion = 3;
        }
        if (config.kind === "tips") {
            content.tips = list(content.tips);
            content.tips.forEach((tip) => {
                tip.title = tip.title || "";
                tip.body = tip.body || "";
                // The editor always carries a lesson; the stored JSON only
                // carries one when the tip is pinned to a lesson.
                tip.lesson = Number.isInteger(tip.lesson) ? tip.lesson : null;
            });
            content.schemaVersion = 1;
        }
        return content;
    }

    function shortSkillName(title) {
        return title.replace(/^Skill:/, "").replace(/^.*\//, "");
    }

    // -----------------------------------------------------------------------
    // The Markdown subset a tip is written in. includes/Markdown.php renders
    // the same grammar for the read view and documents it in full; the two have
    // to agree, because the point of the editing canvas is to show the page
    // that will be published. Change one and change the other.
    // -----------------------------------------------------------------------

    /** Links may only point where a reader can safely follow: the web, or this wiki. */
    const SAFE_LINK = /^(?:https?:\/\/|\/|#)/i;

    /** Element styling shared by the canvas and the read view, kept in step with Markdown::PROSE_CLASSES. */
    const PROSE_CLASSES =
        "[&_h3]:mb-1 [&_h3]:mt-4 [&_h3]:border-0 [&_h3]:p-0 [&_h3]:font-sans " +
        "[&_h3]:text-base [&_h3]:font-semibold [&_h3:first-child]:mt-0 " +
        "[&_p]:my-2 [&_p]:text-sm [&_p]:leading-relaxed [&_p:first-child]:mt-0 [&_p:last-child]:mb-0 " +
        "[&_ul]:my-2 [&_ul]:list-disc [&_ol]:my-2 [&_ol]:list-decimal [&_ul]:pl-6 [&_ol]:pl-6 " +
        "[&_li]:text-sm [&_li]:leading-relaxed " +
        "[&_strong]:font-semibold [&_em]:italic [&_u]:underline [&_a]:text-[#3366cc]";

    function escapeHtml(text) {
        return text.replace(
            /[&<>"]/g,
            (character) =>
                ({
                    "&": "&amp;",
                    "<": "&lt;",
                    ">": "&gt;",
                    '"': "&quot;",
                })[character],
        );
    }

    /**
     * The formatted run opening this text, as HTML and the length it used up,
     * or null when the text does not open one.
     */
    function markdownRun(text) {
        // The closing  may not be followed by a third asterisk: "a *b***"
        // ends with an italic run inside the bold one, and stopping at the first
        // pair would close the bold early and strand the odd asterisk.
        const bold = /^\*\*((?:\\.|[^\\])+?)\*\*(?!\*)/s.exec(text);
        if (bold) {
            return [
                "<strong>" + renderInline(bold[1]) + "</strong>",
                bold[0].length,
            ];
        }
        const italic = /^\*((?:\\.|[^\\*])+?)\*/s.exec(text);
        if (italic) {
            return [
                "<em>" + renderInline(italic[1]) + "</em>",
                italic[0].length,
            ];
        }
        const underline = /^<u>(.+?)<\/u>/s.exec(text);
        if (underline) {
            return [
                "<u>" + renderInline(underline[1]) + "</u>",
                underline[0].length,
            ];
        }
        const link = /^\[((?:\\.|[^\\\]])+?)\]\(([^()\s]*)\)/s.exec(text);
        if (link && SAFE_LINK.test(link[2])) {
            return [
                '<a href="' +
                    escapeHtml(link[2]) +
                    '" rel="nofollow">' +
                    renderInline(link[1]) +
                    "</a>",
                link[0].length,
            ];
        }
        return null;
    }

    /**
     * Inline runs within one block. Walks the text rather than replacing
     * patterns in it, so an escaped marker is never mistaken for a real one and
     * the text between markers is escaped exactly once.
     */
    function renderInline(text) {
        let html = "";
        let plain = "";
        let index = 0;
        while (index < text.length) {
            if (text[index] === "\\" && index + 1 < text.length) {
                plain += text[index + 1];
                index += 2;
                continue;
            }
            const run = markdownRun(text.slice(index));
            if (!run) {
                plain += text[index];
                index++;
                continue;
            }
            html += escapeHtml(plain) + run[0];
            plain = "";
            index += run[1];
        }
        return html + escapeHtml(plain);
    }

    /** One tip body as HTML, for the canvas to start from. */
    function renderMarkdown(text) {
        let html = "";
        let paragraph = [];
        let items = [];
        let listTag = "";
        // A trailing blank line closes whatever block the last real line opened.
        text.split(/\r\n|[\r\n]/)
            .concat([""])
            .forEach((rawLine) => {
                const line = rawLine.trim();
                const bullet = /^-\s+(.*)$/.exec(line);
                const number = /^\d+\.\s+(.*)$/.exec(line);
                const heading = /^#{1,6}\s+(.*)$/.exec(line);
                const wanted = bullet ? "ul" : number ? "ol" : "";
                if (items.length && wanted !== listTag) {
                    html +=
                        "<" +
                        listTag +
                        ">" +
                        items.join("") +
                        "</" +
                        listTag +
                        ">";
                    items = [];
                }
                listTag = wanted;
                if (
                    paragraph.length &&
                    (wanted !== "" || heading || line === "")
                ) {
                    html += "<p>" + renderInline(paragraph.join(" ")) + "</p>";
                    paragraph = [];
                }
                if (wanted !== "") {
                    items.push(
                        "<li>" + renderInline((bullet || number)[1]) + "</li>",
                    );
                } else if (heading) {
                    html += "<h3>" + renderInline(heading[1]) + "</h3>";
                } else if (line !== "") {
                    paragraph.push(line);
                }
            });
        return html;
    }

    const INLINE_MARKERS = { b: "**", strong: "**", i: "*", em: "*" };
    const HEADING_TAGS = ["h1", "h2", "h3", "h4", "h5", "h6"];
    const BLOCK_TAGS = ["p", "div", "blockquote", "pre", "section", "article"];

    function escapeMarkdown(text) {
        // A newline inside a text node is only whitespace to HTML, so it has to
        // become whitespace here too; a line break the writer asked for arrives
        // as a <br>, which serializeNode turns into a newline after this.
        // Typing at the end of a line leaves a non-breaking space behind, which
        // would otherwise be stored and shown as one.
        return text
            .replace(/\n/g, " ")
            .replace(/\u00a0/g, " ")
            .replace(/[\\*[]/g, "\\$&")
            .replace(/<(\/?u>)/gi, "\\<$1");
    }

    /** One node of the canvas as Markdown. Unknown tags give up their markup, not their text. */
    function serializeNode(node) {
        if (node.nodeType === Node.TEXT_NODE) {
            return escapeMarkdown(node.nodeValue);
        }
        if (node.nodeType !== Node.ELEMENT_NODE) {
            return "";
        }
        const tag = node.nodeName.toLowerCase();
        if (tag === "br") {
            return "\n";
        }
        const inner = serializeInline(node);
        // A marker with nothing between its halves is not markup, and would be
        // read back as literal asterisks.
        if (inner.trim() === "") {
            return inner;
        }
        if (INLINE_MARKERS[tag]) {
            return INLINE_MARKERS[tag] + inner + INLINE_MARKERS[tag];
        }
        if (tag === "u" || tag === "ins") {
            return "<u>" + inner + "</u>";
        }
        const href = tag === "a" ? node.getAttribute("href") || "" : "";
        if (tag === "a" && SAFE_LINK.test(href)) {
            return "[" + inner + "](" + href + ")";
        }
        return inner;
    }

    function serializeInline(node) {
        let markdown = "";
        node.childNodes.forEach((child) => {
            markdown += serializeNode(child);
        });
        return markdown;
    }

    function oneLine(text) {
        return text.replace(/\s+/g, " ").trim();
    }

    /** Text that would be read back as a block marker has to say that it is not one. */
    function escapeBlockStart(line) {
        return line.replace(/^(#{1,6}\s|-\s|\d+\.\s)/, "\\$1");
    }

    /** The canvas as Markdown, which is what a tip body is stored as. */
    function serializeMarkdown(root) {
        const blocks = [];
        let inline = "";
        const flushInline = () => {
            // A <br> inside a block starts a new paragraph: the subset has no
            // line break of its own, and two paragraphs read the same way.
            inline
                .split(/\n+/)
                .map(oneLine)
                .filter((line) => line !== "")
                .forEach((line) => blocks.push(escapeBlockStart(line)));
            inline = "";
        };
        root.childNodes.forEach((node) => {
            const tag =
                node.nodeType === Node.ELEMENT_NODE
                    ? node.nodeName.toLowerCase()
                    : "";
            if (HEADING_TAGS.includes(tag)) {
                flushInline();
                const text = oneLine(serializeInline(node));
                if (text !== "") {
                    blocks.push("## " + text);
                }
                return;
            }
            if (tag === "ul" || tag === "ol") {
                flushInline();
                const lines = [];
                node.querySelectorAll(":scope > li").forEach((item, index) => {
                    const text = oneLine(serializeInline(item));
                    if (text !== "") {
                        lines.push(
                            tag === "ul"
                                ? "- " + text
                                : index + 1 + ". " + text,
                        );
                    }
                });
                if (lines.length) {
                    blocks.push(lines.join("\n"));
                }
                return;
            }
            if (BLOCK_TAGS.includes(tag)) {
                flushInline();
                inline = serializeInline(node);
                flushInline();
                return;
            }
            // Anything left is inline: browsers leave bare text and <br> at the
            // top of a canvas that has not been given a block to sit in yet.
            inline += serializeNode(node);
        });
        flushInline();
        return blocks.join("\n\n");
    }

    /** A course row holds at most this many skills; the schema enforces it too. */
    const ROW_LIMIT = 4;
    // The pointer can cross a row seam without targeting distant page content.
    const COURSE_DROP_MARGIN = 32;

    // Glossaries can contain thousands of terms. A fixed row height lets the
    // editor keep only the visible slice in the DOM without changing scrolling.
    const ENTRY_ROW_HEIGHT = 48;
    const ENTRY_OVERSCAN = 5;

    // What a phone shows one at a time, in the order the editor walks them: a
    // list, then what the chosen row opens. Wide viewports show every screen of
    // a kind at once, so this only decides which way the others slide away.
    // The course tree has a single screen; it is here so that every editor is
    // the same shape, and so it gets the same app shell.
    const SCREENS = {
        skill: ["words", "sentences", "sentence"],
        glossary: ["entries", "entry", "form"],
        tips: ["tips", "tip"],
        course: ["rows"],
    };

    /**
     * Where an index ends up once the row at `source` has been lifted out of a
     * list and put back down at `destination`.
     */
    function indexAfterMove(index, source, destination) {
        if (index === source) {
            return destination;
        }
        const lifted = index > source ? index - 1 : index;
        return lifted >= destination ? lifted + 1 : lifted;
    }

    /**
     * Reordering for a vertical list of rows, dragged with the pointer.
     *
     * Five lists are ordered by hand, a skill's words and sentences, and a
     * glossary's entries, forms and translations, and all five behave the same
     * way: the row picked up follows the pointer as a ghost, the list holds a
     * gap open where it would land, and the array is only rewritten once the
     * ghost has flown into that gap. Rows move with the pointer rather than with
     * HTML drag and drop for the same reason the course tree does: the browser's
     * drag image is a washed-out snapshot the cursor cannot keep up with, and it
     * paints a "no drop" badge over everything that is not a registered drop
     * zone.
     *
     * `items()` is the array being ordered, or null while the list has no owner
     * yet: no word selected, no form selected. `label()` is the text the ghost
     * carries. `onMove()` hears about a committed reorder, so a caller that
     * remembers a selection by index can keep it on the row it was pointing at.
     * `minIndex` keeps any structural rows at the head of a list fixed.
     *
     * What comes back is deliberately flat, refs and functions, not an object
     * of them, because a template only unwraps the refs setup() hands it by
     * name, so each list destructures these under names of its own.
     */
    function createRowDrag({ items, label, onMove, minIndex = 0 }) {
        const listElement = ref(null);
        const drag = ref(null);
        const dropIndex = ref(null);
        // True while the ghost flies into its final seat (or back home) after
        // the pointer is released; the data only changes when the flight ends.
        const settling = ref(false);
        let grabbed = null;
        let settleTimer = null;
        // Rows carry their identity in this side band rather than in their
        // position: a translation is a plain string with nothing to key on, and
        // keying by index would pair every row with the wrong content the moment
        // the list is reordered.
        let keySeed = 0;
        let keyOwner = null;
        let keys = [];
        let groupOwner = null;
        let groupSeed = 0;

        /**
         * An identity for the list as a whole, which the template keys the
         * transition-group by, so that showing another word's sentences swaps
         * the entire group in one patch. Keyed rows would otherwise all leave as
         * their replacements entered, and a leaving row holds its place in the
         * flow until its leave transition resolves, there is none here, but
         * that still takes a frame or two, long enough to read as both words'
         * sentences at once. Positioning leaving rows out of the flow instead is
         * the usual answer, and the wrong one: they would be laid over the list
         * that is arriving.
         *
         * Remembering the array between reads is memoisation rather than state:
         * the answer only changes when the list does.
         */
        const group = computed(() => {
            const rows = items();
            if (rows !== groupOwner) {
                groupOwner = rows;
                groupSeed += 1;
            }
            return groupSeed;
        });

        /**
         * The keys for the current list, renewed whenever the list it belongs to
         * is exchanged for another. A length that no longer matches means rows
         * were added or removed without telling this side band, so the safe
         * answer is a fresh set, addKey and removeKey exist to avoid that.
         */
        function rowKeys() {
            const rows = items();
            const length = rows ? rows.length : 0;
            if (rows !== keyOwner || keys.length !== length) {
                keyOwner = rows;
                keys = Array.from({ length }, () => "row" + ++keySeed);
            }
            return keys;
        }

        /** Both of these run before the array changes, while the keys still line up. */
        function addKey() {
            rowKeys().push("row" + ++keySeed);
        }

        function removeKey(index) {
            rowKeys().splice(index, 1);
        }

        const cells = computed(() => {
            const keyList = rowKeys();
            const cells = (items() || []).map((item, index) => ({
                type: "row",
                item,
                index,
                key: keyList[index],
            }));
            if (!drag.value) {
                cells.forEach((cell, position) => {
                    cell.position = position;
                    cell.moveIndex = position;
                });
                return cells;
            }
            cells.splice(drag.value.index, 1);
            if (dropIndex.value !== null) {
                cells.splice(dropIndex.value, 0, {
                    type: "drop",
                    // The gap borrows the dragged row's key, so committing or
                    // cancelling morphs the placeholder into the row in place.
                    // A distinct key would leave by transition-group rules,
                    // which keep the element for an extra frame while the real
                    // row pops in below it.
                    key: keyList[drag.value.index],
                });
            }
            let moveIndex = 0;
            cells.forEach((cell, position) => {
                cell.position = position;
                if (cell.type === "row") {
                    cell.moveIndex = moveIndex++;
                }
            });
            return cells;
        });

        function move(sourceIndex, destinationIndex) {
            const rows = items();
            destinationIndex = Math.max(
                minIndex,
                Math.min(destinationIndex, rows.length - 1),
            );
            if (
                sourceIndex < minIndex ||
                destinationIndex === sourceIndex
            ) {
                return;
            }
            const keyList = rowKeys();
            keyList.splice(
                destinationIndex,
                0,
                keyList.splice(sourceIndex, 1)[0],
            );
            rows.splice(destinationIndex, 0, rows.splice(sourceIndex, 1)[0]);
            if (onMove) {
                onMove(sourceIndex, destinationIndex);
            }
        }

        function start(event, index) {
            if (event.button !== 0 || settling.value || index < minIndex) {
                return;
            }
            event.preventDefault();
            event.currentTarget.focus();
            const row = event.currentTarget.closest("[data-mimi-drag-row]");
            const rect = row.getBoundingClientRect();
            grabbed = {
                index,
                startX: event.clientX,
                startY: event.clientY,
                offsetX: event.clientX - rect.left,
                offsetY: event.clientY - rect.top,
                width: rect.width,
                height: rect.height,
            };
            window.addEventListener("pointermove", track);
            window.addEventListener("pointerup", drop);
            window.addEventListener("pointercancel", returnHome);
            window.addEventListener("keydown", cancel);
        }

        function track(event) {
            if (!grabbed) {
                return;
            }
            let justStarted = false;
            if (!drag.value) {
                if (
                    Math.abs(event.clientX - grabbed.startX) +
                        Math.abs(event.clientY - grabbed.startY) <
                    5
                ) {
                    // A few pixels of slack keep a plain click from becoming a drag.
                    return;
                }
                drag.value = {
                    index: grabbed.index,
                    text: label(items()[grabbed.index]),
                    width: grabbed.width,
                    height: grabbed.height,
                    x: 0,
                    y: 0,
                };
                // Replace the source with the target gap before tracking the
                // pointer, so beginning a drag never makes the list jump.
                dropIndex.value = grabbed.index;
                justStarted = true;
                document.body.style.userSelect = "none";
                document.body.style.cursor = "grabbing";
            }
            event.preventDefault();
            drag.value.x = event.clientX - grabbed.offsetX;
            drag.value.y = event.clientY - grabbed.offsetY;
            if (justStarted) {
                nextTick(() => aim(event.clientY));
            } else {
                aim(event.clientY);
            }
        }

        function aim(y) {
            const list = listElement.value;
            if (!list) {
                return;
            }
            const gap = list.querySelector("[data-mimi-drag-gap]");
            if (gap) {
                const rect = gap.getBoundingClientRect();
                // Holding the current target while the pointer is inside its
                // gap prevents the surrounding rows from shuttling back and forth.
                if (y >= rect.top && y <= rect.bottom) {
                    return;
                }
            }
            const rows = Array.from(
                list.querySelectorAll("[data-mimi-drag-row]"),
            );
            let index = rows.length;
            let found = false;
            rows.some((row, position) => {
                const rect = row.getBoundingClientRect();
                if (y < rect.top + rect.height / 2) {
                    index = Number.isInteger(
                        Number(row.dataset.mimiDragPosition),
                    )
                        ? Number(row.dataset.mimiDragPosition)
                        : position;
                    found = true;
                    return true;
                }
                return false;
            });
            if (rows.length && !found) {
                const lastPosition = Number(
                    rows[rows.length - 1].dataset.mimiDragPosition,
                );
                if (Number.isInteger(lastPosition)) {
                    index = lastPosition + 1;
                }
            }
            dropIndex.value = Math.max(
                minIndex,
                Math.min(index, items().length - 1),
            );
        }

        /**
         * Animate the ghost to a target before ending the drag. Committing the
         * reorder in the same tick the ghost vanishes makes the rows jump, so
         * the data only changes once the ghost has landed in its seat.
         */
        function settle(x, y, commit) {
            settling.value = true;
            detach();
            drag.value.x = x;
            drag.value.y = y;
            settleTimer = window.setTimeout(() => {
                settleTimer = null;
                if (commit && drag.value && dropIndex.value !== null) {
                    move(drag.value.index, dropIndex.value);
                }
                end();
            }, 150);
        }

        function drop() {
            if (drag.value && dropIndex.value !== null) {
                const gap = listElement.value?.querySelector(
                    "[data-mimi-drag-gap]",
                );
                if (gap) {
                    const rect = gap.getBoundingClientRect();
                    settle(rect.left, rect.top, true);
                    return;
                }
                move(drag.value.index, dropIndex.value);
            }
            end();
        }

        /** A cancelled drag flies back to where the row was picked up. */
        function returnHome() {
            if (drag.value && grabbed) {
                // Bring the gap home too, so the row morphs back in place.
                dropIndex.value = drag.value.index;
                settle(
                    grabbed.startX - grabbed.offsetX,
                    grabbed.startY - grabbed.offsetY,
                    false,
                );
            } else {
                end();
            }
        }

        function cancel(event) {
            if (event.key === "Escape") {
                returnHome();
            }
        }

        function moveWithKeyboard(event, index) {
            const rows = items();
            let destinationIndex;
            if (event.key === "ArrowUp" && index > minIndex) {
                destinationIndex = index - 1;
            } else if (event.key === "ArrowDown" && index < rows.length - 1) {
                destinationIndex = index + 1;
            } else {
                return;
            }
            event.preventDefault();
            move(index, destinationIndex);
            nextTick(() => {
                const destinationRow = listElement.value?.querySelector(
                    `[data-mimi-drag-position="${destinationIndex}"]`,
                );
                if (destinationRow) {
                    destinationRow
                        .querySelector("[data-mimi-drag-handle]")
                        ?.focus();
                    return;
                }
                const handles = listElement.value?.querySelectorAll(
                    "[data-mimi-drag-handle]",
                );
                handles?.[destinationIndex - minIndex]?.focus();
            });
        }

        function detach() {
            window.removeEventListener("pointermove", track);
            window.removeEventListener("pointerup", drop);
            window.removeEventListener("pointercancel", returnHome);
            window.removeEventListener("keydown", cancel);
        }

        function end() {
            if (settleTimer !== null) {
                window.clearTimeout(settleTimer);
                settleTimer = null;
            }
            settling.value = false;
            grabbed = null;
            drag.value = null;
            dropIndex.value = null;
            document.body.style.userSelect = "";
            document.body.style.cursor = "";
            detach();
        }

        return {
            list: listElement,
            cells,
            group,
            drag,
            settling,
            start,
            moveWithKeyboard,
            addKey,
            removeKey,
            end,
        };
    }

    const data = ref(normalize(config.content));
    const initialData = JSON.stringify(data.value);

    /** A labelled list of free-text answers, edited in place. */
    const AnswerList = {
        components: { CdxButton, CdxTextInput },
        props: {
            items: { type: Array, required: true },
            label: { type: String, required: true },
            hint: { type: String, default: "" },
            addLabel: { type: String, required: true },
            placeholder: { type: String, default: "" },
            emptyText: { type: String, required: true },
        },
        methods: {
            add() {
                this.items.push("");
            },
            remove(index) {
                this.items.splice(index, 1);
            },
        },
        template: `
		<div>
			<div class="mb-1 flex items-center justify-between gap-3">
				<label class="text-sm font-semibold">{{ label }}</label>
				<cdx-button weight="quiet" action="progressive" @click="add">{{ addLabel }}</cdx-button>
			</div>
			<p v-if="hint" class="mb-2 mt-0 text-xs leading-relaxed text-slate-500">{{ hint }}</p>
			<div v-if="items.length" class="space-y-2">
				<div v-for="(item,i) in items" :key="i" class="flex items-center gap-1">
					<cdx-text-input class="min-w-0 flex-1" :model-value="items[i]" :placeholder="placeholder"
						@update:model-value="value => items[i] = value"></cdx-text-input>
					<button type="button" class="bg-transparent px-2 py-2 text-lg text-slate-500 hover:text-red-600"
						:title="'Remove ' + label.toLowerCase()" @click="remove(i)">&times;</button>
				</div>
			</div>
			<p v-else class="m-0 border border-dashed border-slate-300 px-3 py-2 text-sm text-slate-500">{{ emptyText }}</p>
		</div>`,
    };

    /**
     * The seam between two course rows. Every seam can take a new row, and every
     * seam below the first row can take a castle, so rows and castles are added
     * where they belong instead of only at the end of the tree.
     */
    const RowSeam = {
        components: { CdxIcon },
        props: {
            // The castle standing on this seam, numbered from the top; 0 for none.
            castle: { type: Number, default: 0 },
            canAddCastle: { type: Boolean, default: false },
        },
        emits: ["add-row", "add-castle", "remove-castle"],
        computed: {
            flagIcon() {
                return codexIcons.cdxIconFlag;
            },
        },
        // A seam keeps its actions out of the way until a pointer asks for them.
        // Nothing hovers on a phone, so below the breakpoint they are simply
        // there: in the flow of the seam rather than laid over its label, and
        // tall enough to hit.
        template: `
		<div v-if="castle" class="group relative flex min-h-10 flex-wrap items-center gap-3 border-y border-[#c8ccd1] bg-white px-3 py-1.5 text-[#54595d] lg:flex-nowrap">
			<span class="h-px min-w-4 flex-1 bg-[#c8ccd1]" aria-hidden="true"></span>
			<span class="flex shrink-0 items-center gap-2 text-sm font-semibold">
				<cdx-icon class="h-4 w-4 text-[#36c]" :icon="flagIcon"></cdx-icon>
				Castle {{ castle }}
			</span>
			<span class="h-px min-w-4 flex-1 bg-[#c8ccd1]" aria-hidden="true"></span>
			<span class="flex items-center gap-1 bg-white transition-opacity lg:absolute lg:right-3 lg:pl-2 lg:opacity-0 lg:focus-within:opacity-100 lg:group-hover:opacity-100">
				<button type="button" class="bg-transparent px-2 py-2 text-xs font-semibold text-[#3366cc] hover:underline lg:py-1" @click="$emit('add-row')">Add row</button>
				<button type="button" class="bg-transparent px-2 py-2 text-xs font-semibold text-[#54595d] hover:text-[#b32424] hover:underline lg:py-1" @click="$emit('remove-castle')">Remove castle</button>
			</span>
		</div>
		<div v-else class="group relative flex min-h-9 items-center justify-center border-y border-slate-200 bg-white lg:h-6 lg:min-h-0">
			<span class="pointer-events-none absolute text-xs leading-none text-slate-300 opacity-0 transition-opacity lg:opacity-100 lg:group-hover:opacity-0" aria-hidden="true">+</span>
			<span class="flex items-center gap-1 transition-opacity lg:opacity-0 lg:focus-within:opacity-100 lg:group-hover:opacity-100">
				<button type="button" class="bg-transparent px-2 py-2 text-xs font-semibold text-[#3366cc] hover:underline lg:py-0.5" @click="$emit('add-row')">Add row</button>
				<button v-if="canAddCastle" type="button" class="bg-transparent px-2 py-2 text-xs font-semibold text-[#3366cc] hover:underline lg:py-0.5" @click="$emit('add-castle')">Add castle</button>
			</span>
		</div>`,
    };

    /**
     * A tip body, written as formatted text rather than as markup. The canvas is
     * a contenteditable region, which Vue cannot bind to without moving the
     * caret on every keystroke: it is filled once on mount and read back on
     * input instead. Its parent gives it a :key, and that is what reloads it
     * when a different tip is selected.
     *
     * Formatting goes through execCommand. It is a deprecated API with no
     * replacement that browsers actually implement, and it is what keeps this
     * to one component rather than a document model of its own; whatever markup
     * it leaves behind, serializeMarkdown only reads the tags in the subset.
     */
    const TipBody = {
        components: { CdxButton, CdxIcon, CdxTextInput },
        props: {
            modelValue: { type: String, required: true },
        },
        emits: ["update:modelValue"],
        setup(props, { emit }) {
            const canvas = ref(null);
            const linkDialog = ref(null);
            const linkUrl = ref("");
            const linkError = ref("");
            const linkRange = ref(null);
            const active = ref({});

            const commands = [
                {
                    name: "bold",
                    label: "Bold",
                    title: "Bold (Ctrl+B)",
                    icon: codexIcons.cdxIconBold,
                },
                {
                    name: "italic",
                    label: "Italic",
                    title: "Italic (Ctrl+I)",
                    icon: codexIcons.cdxIconItalic,
                },
                {
                    name: "underline",
                    label: "Underline",
                    title: "Underline (Ctrl+U)",
                    icon: codexIcons.cdxIconUnderline,
                },
                {
                    name: "heading",
                    label: "Heading",
                    title: "Heading",
                    icon: codexIcons.cdxIconLargerText,
                },
                {
                    name: "insertUnorderedList",
                    label: "Bulleted list",
                    title: "Bulleted list",
                    icon: codexIcons.cdxIconListBullet,
                },
                {
                    name: "insertOrderedList",
                    label: "Numbered list",
                    title: "Numbered list",
                    icon: codexIcons.cdxIconListNumbered,
                },
            ];

            /** The link the caret sits in, or null. */
            function currentLink() {
                const selection = document.getSelection();
                let node = selection ? selection.anchorNode : null;
                while (node && node !== canvas.value) {
                    if (node.nodeName === "A") {
                        return node;
                    }
                    node = node.parentNode;
                }
                return null;
            }

            function selectionInCanvas() {
                const selection = document.getSelection();
                return !!(
                    canvas.value &&
                    selection &&
                    selection.anchorNode &&
                    canvas.value.contains(selection.anchorNode)
                );
            }

            /** Light the buttons that describe where the caret currently is. */
            function refreshActive() {
                if (!selectionInCanvas()) {
                    return;
                }
                const state = {};
                commands.forEach((command) => {
                    state[command.name] =
                        command.name === "heading"
                            ? /^h[1-6]$/i.test(
                                  document.queryCommandValue("formatBlock"),
                              )
                            : document.queryCommandState(command.name);
                });
                // Linking needs something to attach to: either words to wrap, or
                // a link already under the caret to point somewhere else.
                state.link = !!currentLink();
                state.canLink =
                    state.link || !document.getSelection().isCollapsed;
                active.value = state;
            }

            function emitValue() {
                emit("update:modelValue", serializeMarkdown(canvas.value));
            }

            function run(name) {
                canvas.value.focus();
                if (name === "heading") {
                    document.execCommand(
                        "formatBlock",
                        false,
                        active.value.heading ? "<p>" : "<h3>",
                    );
                } else {
                    document.execCommand(name, false, null);
                }
                emitValue();
                refreshActive();
            }

            const SHORTCUTS = { b: "bold", i: "italic", u: "underline" };

            /**
             * Browsers bind Ctrl+B, Ctrl+I and Ctrl+U inside a contenteditable
             * themselves, but not dependably: the skin or the browser can take
             * the chord before the canvas ever sees it, and where it is handled
             * natively nothing tells the toolbar its buttons have changed
             * state. Binding all three here makes them behave exactly as
             * pressing the buttons does.
             */
            function handleShortcut(event) {
                if ((!event.ctrlKey && !event.metaKey) || event.altKey) {
                    return;
                }
                const command = SHORTCUTS[event.key.toLowerCase()];
                if (!command) {
                    return;
                }
                event.preventDefault();
                run(command);
            }

            function pastePlainText(event) {
                // Pasted markup would arrive in tags the subset cannot store, so
                // only the text of it is taken.
                event.preventDefault();
                document.execCommand(
                    "insertText",
                    false,
                    event.clipboardData.getData("text/plain"),
                );
                emitValue();
            }

            function openLinkDialog() {
                const existing = currentLink();
                linkUrl.value = existing
                    ? existing.getAttribute("href") || ""
                    : "";
                linkError.value = "";
                // Opening the dialog takes the caret out of the canvas, so where
                // the link belongs has to be remembered before it does.
                const selection = document.getSelection();
                linkRange.value = selection.rangeCount
                    ? selection.getRangeAt(0).cloneRange()
                    : null;
                linkDialog.value.showModal();
                nextTick(() =>
                    linkDialog.value?.querySelector("input")?.focus(),
                );
            }

            function applyLink() {
                const url = linkUrl.value.trim();
                if (url !== "" && !SAFE_LINK.test(url)) {
                    linkError.value =
                        "A link must start with https://, / or #.";
                    return;
                }
                linkDialog.value.close();
                canvas.value.focus();
                if (linkRange.value) {
                    const selection = document.getSelection();
                    selection.removeAllRanges();
                    selection.addRange(linkRange.value);
                }
                // An emptied box is how a link is taken off again.
                document.execCommand(
                    url === "" ? "unlink" : "createLink",
                    false,
                    url || null,
                );
                emitValue();
                refreshActive();
            }

            onMounted(() => {
                canvas.value.innerHTML = renderMarkdown(props.modelValue);
                try {
                    // Otherwise execCommand marks text up with inline styles,
                    // which the serializer would have to guess its way out of.
                    document.execCommand("styleWithCSS", false, false);
                } catch (error) {
                    // Not every browser offers the switch; tags are the default.
                }
                document.addEventListener("selectionchange", refreshActive);
            });

            onBeforeUnmount(() => {
                document.removeEventListener("selectionchange", refreshActive);
            });

            return {
                canvas,
                commands,
                active,
                proseClasses: PROSE_CLASSES,
                cdxIconLink: codexIcons.cdxIconLink,
                linkDialog,
                linkUrl,
                linkError,
                emitValue,
                pastePlainText,
                refreshActive,
                handleShortcut,
                run,
                openLinkDialog,
                applyLink,
            };
        },
        template: `
		<div>
			<div class="flex flex-wrap items-center gap-1 border border-b-0 border-slate-300 bg-slate-50 px-2 py-1.5">
				<button v-for="command in commands" :key="command.name" type="button"
					class="flex h-8 w-8 items-center justify-center border border-solid bg-transparent text-[#202122] hover:bg-slate-200"
					:class="active[command.name] ? 'border-slate-400 bg-slate-200' : 'border-transparent'"
					:aria-pressed="active[command.name] ? 'true' : 'false'"
					:title="command.title" :aria-label="command.label"
					@mousedown.prevent @click="run(command.name)">
					<cdx-icon class="h-4 w-4" :icon="command.icon"></cdx-icon>
				</button>
				<span class="mx-1 h-5 w-px bg-slate-300" aria-hidden="true"></span>
				<button type="button"
					class="flex h-8 w-8 items-center justify-center border border-solid bg-transparent text-[#202122] hover:bg-slate-200 disabled:cursor-default disabled:text-slate-400 disabled:hover:bg-transparent"
					:class="active.link ? 'border-slate-400 bg-slate-200' : 'border-transparent'"
					:disabled="!active.canLink" :aria-pressed="active.link ? 'true' : 'false'"
					title="Link" aria-label="Link"
					@mousedown.prevent @click="openLinkDialog">
					<cdx-icon class="h-4 w-4" :icon="cdxIconLink"></cdx-icon>
				</button>
			</div>
			<div class="relative">
				<div ref="canvas" contenteditable="true" role="textbox" aria-multiline="true" aria-label="Tip body"
					class="max-h-[26rem] min-h-[15rem] overflow-y-auto border border-solid border-slate-300 bg-white px-4 py-3 text-sm outline-none focus:border-[#36c] focus:shadow-[inset_0_0_0_1px_#36c]"
					:class="proseClasses"
					@input="emitValue" @blur="emitValue" @paste="pastePlainText"
					@keydown="handleShortcut" @keyup="refreshActive" @mouseup="refreshActive"></div>
				<p v-if="!modelValue" class="pointer-events-none absolute left-4 top-3 m-0 text-sm text-slate-400" aria-hidden="true">

				</p>
			</div>

			<dialog ref="linkDialog" aria-labelledby="mimi-link-title"
				class="w-[28rem] max-w-[calc(100%_-_2rem)] border border-slate-300 bg-white p-0 text-[#202122] shadow-2xl backdrop:bg-black/40">
				<form class="m-0" @submit.prevent="applyLink">
					<header class="border-b border-slate-200 px-5 py-4">
						<h2 id="mimi-link-title" class="m-0 border-0 p-0 text-lg font-semibold">Link</h2>
					</header>
					<div class="px-5 py-4">
						<label class="mb-1 block text-sm font-semibold" for="mimi-link-url">Address</label>
						<cdx-text-input id="mimi-link-url" v-model="linkUrl" placeholder="https://example.org"
							:status="linkError ? 'error' : 'default'" @update:model-value="linkError = ''"></cdx-text-input>
						<p v-if="linkError" class="mb-0 mt-1 text-sm text-[#b32424]">{{ linkError }}</p>
						<p v-else class="mb-0 mt-2 text-sm text-slate-500">Clear the box to remove the link.</p>
					</div>
					<footer class="flex justify-end gap-2 border-t border-slate-200 bg-slate-50 px-5 py-3">
						<cdx-button type="button" @click="linkDialog.close()">Cancel</cdx-button>
						<cdx-button type="submit" weight="primary" action="progressive">Apply</cdx-button>
					</footer>
				</form>
			</dialog>
		</div>`,
    };

    createMwApp({
        components: {
            CdxButton,
            CdxIcon,
            CdxTextInput,
            CdxTextArea,
            CdxMessage,
            AnswerList,
            RowSeam,
            TipBody,
        },
        setup() {
            const screens = SCREENS[config.kind];
            const screen = ref(screens[0]);
            const selectedWordIndex = ref(0);
            const selectedSentenceIndex = ref(0);
            const selectedEntryIndex = ref(0);
            const selectedFormIndex = ref(0);
            const selectedTipIndex = ref(0);
            const entryFilter = ref("");
            const saving = ref(false);
            const message = ref(null);
            const entryScrollTop = ref(0);
            const entryViewportHeight = ref(480);
            // The skill currently under the pointer and the seat it would land in;
            // both are null while nothing is moving.
            const drag = ref(null);
            const dropSeat = ref(null);
            const skillSettling = ref(false);
            const fullRow = ref(null);
            const publishDialog = ref(null);
            const summary = ref("");
            const addSkillDialog = ref(null);
            const addSkillRow = ref(null);
            const newSkillName = ref("");
            const newSkillError = ref("");
            let grabbed = null;
            let skillSettleTimer = null;
            // The row the pointer last committed to. Aiming sticks to it until the
            // pointer clearly leaves, so hovering the seam between two rows cannot
            // flip the drop seat back and forth with every pointer event.
            let lastAimRow = null;
            let entryResizeObserver = null;
            const words = computed(() =>
                Array.isArray(data.value.words) ? data.value.words : [],
            );
            const selectedWord = computed(
                () => words.value[selectedWordIndex.value] || null,
            );
            const selectedSentence = computed(
                () =>
                    (selectedWord.value &&
                        selectedWord.value.sentences[
                            selectedSentenceIndex.value
                        ]) ||
                    null,
            );
            const completeCount = computed(() =>
                selectedWord.value
                    ? selectedWord.value.sentences.filter(isComplete).length
                    : 0,
            );
            const courseName = config.courseName;
            const backUrl = mw.util.getUrl(config.title);
            const entries = computed(() =>
                Array.isArray(data.value.entries) ? data.value.entries : [],
            );
            const selectedEntry = computed(
                () => entries.value[selectedEntryIndex.value] || null,
            );
            const selectedForm = computed(
                () =>
                    (selectedEntry.value &&
                        selectedEntry.value.forms[selectedFormIndex.value]) ||
                    null,
            );
            const completeForms = computed(() =>
                selectedEntry.value
                    ? selectedEntry.value.forms.filter(isFormComplete).length
                    : 0,
            );
            const isDirty = computed(
                () => JSON.stringify(data.value) !== initialData,
            );

            // The five hand-ordered lists. Each one names the shared machinery
            // after what it holds, because several sit side by side and a drag
            // in any of them must be told apart in the template. Every list
            // selected by index keeps that selection on whichever row moved.
            const {
                list: wordList,
                cells: wordCells,
                group: wordGroup,
                drag: wordDrag,
                settling: wordSettling,
                start: startWordDrag,
                moveWithKeyboard: moveWordWithKeyboard,
                addKey: addWordKey,
                removeKey: removeWordKey,
                end: endWordDrag,
            } = createRowDrag({
                items: () => data.value.words || null,
                label: (word) => word.word || "Untitled word",
                onMove: (source, destination) => {
                    selectedWordIndex.value = indexAfterMove(
                        selectedWordIndex.value,
                        source,
                        destination,
                    );
                },
            });

            const {
                list: sentenceList,
                cells: sentenceCells,
                group: sentenceGroup,
                drag: sentenceDrag,
                settling: sentenceSettling,
                start: startSentenceDrag,
                moveWithKeyboard: moveSentenceWithKeyboard,
                addKey: addSentenceKey,
                removeKey: removeSentenceKey,
                end: endSentenceDrag,
            } = createRowDrag({
                items: () => selectedWord.value?.sentences || null,
                label: (sentence) => sentence.text || "New sentence",
                onMove: (source, destination) => {
                    selectedSentenceIndex.value = indexAfterMove(
                        selectedSentenceIndex.value,
                        source,
                        destination,
                    );
                },
            });

            const {
                list: entryList,
                cells: entryCells,
                drag: entryDrag,
                settling: entrySettling,
                start: startEntryDrag,
                moveWithKeyboard: moveEntryWithKeyboard,
                addKey: addEntryKey,
                removeKey: removeEntryKey,
                end: endEntryDrag,
            } = createRowDrag({
                items: () => data.value.entries || null,
                label: (entry) => entry.lemma || "Untitled entry",
                onMove: (source, destination) => {
                    selectedEntryIndex.value = indexAfterMove(
                        selectedEntryIndex.value,
                        source,
                        destination,
                    );
                },
            });

            const {
                list: formList,
                cells: formCells,
                group: formGroup,
                drag: formDrag,
                settling: formSettling,
                start: startFormDrag,
                moveWithKeyboard: moveFormWithKeyboard,
                addKey: addFormKey,
                removeKey: removeFormKey,
                end: endFormDrag,
            } = createRowDrag({
                items: () => selectedEntry.value?.forms || null,
                label: (form) => form.form || "New form",
                minIndex: 1,
                onMove: (source, destination) => {
                    selectedFormIndex.value = indexAfterMove(
                        selectedFormIndex.value,
                        source,
                        destination,
                    );
                },
            });

            const {
                list: translationList,
                cells: translationCells,
                group: translationGroup,
                drag: translationDrag,
                settling: translationSettling,
                start: startTranslationDrag,
                moveWithKeyboard: moveTranslationWithKeyboard,
                addKey: addTranslationKey,
                removeKey: removeTranslationKey,
                end: endTranslationDrag,
            } = createRowDrag({
                items: () => selectedForm.value?.translations || null,
                label: (translation) => translation || "Empty translation",
            });

            // Only one row can be dragged at a time, so the ghost is
            // drawn once for all of them, and drawn at the top of the app
            // rather than beside its list. A screen carries will-change:
            // transform on a phone, which makes it the containing block for
            // anything fixed inside it, and a ghost placed within one would
            // hang below the finger by however far down the screen begins.
            const rowDrag = computed(
                () =>
                    wordDrag.value ||
                    sentenceDrag.value ||
                    entryDrag.value ||
                    formDrag.value ||
                    translationDrag.value,
            );
            const rowSettling = computed(
                () =>
                    wordSettling.value ||
                    sentenceSettling.value ||
                    entrySettling.value ||
                    formSettling.value ||
                    translationSettling.value,
            );
            const visibleEntries = computed(() => {
                const needle = entryFilter.value.trim().toLowerCase();
                return entryCells.value
                    .filter(
                        (cell) =>
                            cell.type === "drop" ||
                            !needle ||
                            cell.item.lemma.toLowerCase().includes(needle) ||
                            // A form is looked up as readily as the lemma it
                            // belongs to, and finds the whole entry either way.
                            cell.item.forms.some(
                                (form) =>
                                    form.form.toLowerCase().includes(needle) ||
                                    form.translations.some((text) =>
                                        text.toLowerCase().includes(needle),
                                    ),
                            ),
                    )
                    .map((cell) => ({
                        cell,
                        entry: cell.item || null,
                        index: cell.index,
                    }));
            });
            const virtualEntries = computed(() => {
                const start = Math.max(
                    0,
                    Math.floor(entryScrollTop.value / ENTRY_ROW_HEIGHT) -
                        ENTRY_OVERSCAN,
                );
                const end = Math.min(
                    visibleEntries.value.length,
                    Math.ceil(
                        (entryScrollTop.value + entryViewportHeight.value) /
                            ENTRY_ROW_HEIGHT,
                    ) + ENTRY_OVERSCAN,
                );
                return visibleEntries.value
                    .slice(start, end)
                    .map((item, offset) => ({
                        ...item,
                        virtualIndex: start + offset,
                    }));
            });
            const entryVirtualHeight = computed(
                () => visibleEntries.value.length * ENTRY_ROW_HEIGHT,
            );
            const duplicateLemma = computed(() => {
                const lemma = selectedEntry.value
                    ? selectedEntry.value.lemma.trim().toLowerCase()
                    : "";
                return (
                    lemma !== "" &&
                    entries.value.some(
                        (entry, index) =>
                            index !== selectedEntryIndex.value &&
                            entry.lemma.trim().toLowerCase() === lemma,
                    )
                );
            });
            const duplicateForm = computed(() => {
                const spelling = selectedForm.value
                    ? selectedForm.value.form.trim().toLowerCase()
                    : "";
                return (
                    spelling !== "" &&
                    (selectedEntry.value.lemma.trim().toLowerCase() ===
                        spelling ||
                        selectedEntry.value.forms.some(
                            (form, index) =>
                                index !== selectedFormIndex.value &&
                                form.form.trim().toLowerCase() === spelling,
                        ))
                );
            });

            /** The gloss a list row shows beside its lemma: the lemma's own. */
            function entryGloss(entry) {
                return entry.forms[0].translations[0] || "";
            }

            /**
             * What a row of the forms list is called: the form's own spelling,
             * except for the first row, which is the lemma standing for itself
             * and has no spelling to show.
             */
            function formTitle(form, index) {
                if (index === 0) {
                    return "Itself";
                }
                return form.form || "New form";
            }

            /** What that row has to show under its name, if anything yet. */
            function formSummary(form) {
                return form.translations
                    .map((text) => text.trim())
                    .filter((text) => text !== "")
                    .join(", ");
            }

            /**
             * A row is done when nothing is left to fill in. The lemma's own row
             * needs only a translation, having no spelling of its own; every
             * other row needs both.
             */
            function isFormComplete(form, index) {
                const translated = form.translations.some(
                    (text) => text.trim() !== "",
                );
                if (index === 0) {
                    return translated;
                }
                return translated && form.form.trim() !== "";
            }

            function isComplete(sentence) {
                return (
                    sentence.text.trim() !== "" &&
                    sentence.translation.trim() !== ""
                );
            }

            /**
             * Where a screen sits relative to the one on show: the screens
             * before it wait off the left edge, the ones after it off the right.
             * Above the breakpoint the stylesheet ignores all three.
             */
            function screenClass(name) {
                const offset =
                    screens.indexOf(name) - screens.indexOf(screen.value);
                if (offset === 0) {
                    return "mimi-editor-screen-current";
                }
                return offset < 0
                    ? "mimi-editor-screen-left"
                    : "mimi-editor-screen-right";
            }

            function selectWord(index) {
                selectedWordIndex.value = index;
                selectedSentenceIndex.value = 0;
                screen.value = "sentences";
            }

            function selectSentence(index) {
                selectedSentenceIndex.value = index;
                screen.value = "sentence";
            }

            function addWord() {
                addWordKey();
                data.value.words.push({ word: "New word", sentences: [] });
                selectWord(data.value.words.length - 1);
            }

            function removeWord(index) {
                if (
                    data.value.words.length === 1 ||
                    !window.confirm(
                        "Remove this word and all of its sentences?",
                    )
                ) {
                    return;
                }
                removeWordKey(index);
                data.value.words.splice(index, 1);
                selectedWordIndex.value = Math.min(
                    index,
                    data.value.words.length - 1,
                );
                selectedSentenceIndex.value = 0;
                screen.value = "words";
            }

            function addSentence() {
                if (!selectedWord.value) {
                    return;
                }
                addSentenceKey();
                selectedWord.value.sentences.push({
                    text: "",
                    notes: "",
                    disabled: false,
                    alternativeSentences: [],
                    translation: "",
                    alternativeTranslations: [],
                });
                selectSentence(selectedWord.value.sentences.length - 1);
            }

            function removeSentence(index) {
                removeSentenceKey(index);
                selectedWord.value.sentences.splice(index, 1);
                selectedSentenceIndex.value = Math.max(
                    0,
                    Math.min(index, selectedWord.value.sentences.length - 1),
                );
                screen.value = "sentences";
            }

            function selectEntry(index) {
                selectedEntryIndex.value = index;
                selectedFormIndex.value = 0;
                screen.value = "entry";
            }

            function selectForm(index) {
                selectedFormIndex.value = index;
                screen.value = "form";
            }

            function measureEntryViewport() {
                if (!entryList.value) {
                    return;
                }
                entryScrollTop.value = entryList.value.scrollTop;
                entryViewportHeight.value = entryList.value.clientHeight;
            }

            function updateEntryWindow(event) {
                entryScrollTop.value = event.currentTarget.scrollTop;
                entryViewportHeight.value = event.currentTarget.clientHeight;
            }

            function resetEntryScroll() {
                entryScrollTop.value = 0;
                nextTick(() => {
                    if (entryList.value) {
                        entryList.value.scrollTop = 0;
                        measureEntryViewport();
                    }
                });
            }

            function scrollEntryIntoView(index) {
                const position = visibleEntries.value.findIndex(
                    (item) => item.index === index,
                );
                const viewport = entryList.value;
                if (position < 0 || !viewport) {
                    return;
                }
                const top = position * ENTRY_ROW_HEIGHT;
                const bottom = top + ENTRY_ROW_HEIGHT;
                if (top < viewport.scrollTop) {
                    viewport.scrollTop = top;
                } else if (
                    bottom >
                    viewport.scrollTop + viewport.clientHeight
                ) {
                    viewport.scrollTop = bottom - viewport.clientHeight;
                }
                measureEntryViewport();
            }

            function addEntry() {
                entryFilter.value = "";
                addEntryKey();
                data.value.entries.push({
                    lemma: "",
                    // The row for the lemma itself; every entry has one.
                    forms: [{ form: "", translations: [""] }],
                });
                selectEntry(data.value.entries.length - 1);
                // Clearing a filter schedules its own scroll reset. Wait for that
                // render too, then reveal the new row at the end of the full list.
                nextTick(() =>
                    nextTick(() =>
                        scrollEntryIntoView(data.value.entries.length - 1),
                    ),
                );
            }

            function removeEntry(index) {
                if (
                    !window.confirm("Remove this entry and all of its forms?")
                ) {
                    return;
                }
                removeEntryKey(index);
                data.value.entries.splice(index, 1);
                selectEntry(
                    Math.max(0, Math.min(index, data.value.entries.length - 1)),
                );
                // The entry that was on screen is gone, so a phone has nothing
                // left to look at but the list it came from.
                screen.value = "entries";
                nextTick(measureEntryViewport);
            }

            function addForm() {
                addFormKey();
                selectedEntry.value.forms.push({
                    form: "",
                    translations: [""],
                });
                selectForm(selectedEntry.value.forms.length - 1);
            }

            function removeForm(index) {
                // The first row is the lemma standing for itself: an entry
                // without it has nowhere to put its plain translation.
                if (index === 0) {
                    return;
                }
                removeFormKey(index);
                selectedEntry.value.forms.splice(index, 1);
                selectedFormIndex.value = Math.max(0, index - 1);
                screen.value = "entry";
            }

            function addTranslation() {
                addTranslationKey();
                selectedForm.value.translations.push("");
            }

            function removeTranslation(index) {
                removeTranslationKey(index);
                selectedForm.value.translations.splice(index, 1);
            }

            watch(entryFilter, resetEntryScroll);

            onMounted(() => {
                if (config.kind !== "glossary") {
                    return;
                }
                measureEntryViewport();
                if (window.ResizeObserver) {
                    entryResizeObserver = new ResizeObserver(
                        measureEntryViewport,
                    );
                    entryResizeObserver.observe(entryList.value);
                }
            });

            onBeforeUnmount(() => {
                entryResizeObserver?.disconnect();
                endWordDrag();
                endSentenceDrag();
                endEntryDrag();
                endFormDrag();
                endTranslationDrag();
                endSkillDrag();
            });

            function skillName(title) {
                return shortSkillName(title);
            }

            function skillIcon(title) {
                const label = skillName(title).toLowerCase();
                let iconName = "cdxIconPuzzle";
                for (const rule of skillIconRules) {
                    if (rule.terms.some((term) => label.includes(term))) {
                        iconName = rule.icon;
                        break;
                    }
                }
                return codexIcons[iconName] || codexIcons.cdxIconPuzzle;
            }

            function skillUrl(title) {
                return mw.util.getUrl(
                    title,
                    skillPageExists(title)
                        ? undefined
                        : {
                              action: "edit",
                              redlink: 1,
                          },
                );
            }

            function skillPageExists(title) {
                return config.skillExists && config.skillExists[title] === true;
            }

            function courseRowCells(rowIndex) {
                const cells = data.value.rows[rowIndex].map((skill) => ({
                    type: "skill",
                    skill,
                    key: skill,
                }));
                if (!drag.value) {
                    return cells;
                }
                const sourceIndex = cells.findIndex(
                    (cell) => cell.skill === drag.value.skill,
                );
                if (sourceIndex !== -1) {
                    cells.splice(sourceIndex, 1);
                }
                if (dropSeat.value?.row === rowIndex) {
                    cells.splice(dropSeat.value.index, 0, {
                        type: "drop",
                        // Keyed like the dragged skill, so dropping morphs the
                        // placeholder into the card in place; a distinct key would
                        // linger for an extra frame on release.
                        key: drag.value.skill,
                    });
                }
                return cells;
            }

            function emptySkillSeats(rowIndex) {
                return ROW_LIMIT - courseRowCells(rowIndex).length;
            }

            function moveSkill(title, rowIndex, destinationIndex) {
                const sourceRowIndex = data.value.rows.findIndex((row) =>
                    row.includes(title),
                );
                if (
                    sourceRowIndex < 0 ||
                    (sourceRowIndex !== rowIndex &&
                        data.value.rows[rowIndex].length >= ROW_LIMIT)
                ) {
                    return;
                }
                const sourceIndex =
                    data.value.rows[sourceRowIndex].indexOf(title);
                data.value.rows[sourceRowIndex].splice(sourceIndex, 1);
                data.value.rows[rowIndex].splice(destinationIndex, 0, title);
            }

            /**
             * Skills move with the pointer rather than with HTML drag and drop: the
             * browser's drag image is a washed-out snapshot the cursor cannot keep
             * up with, and it paints a "no drop" badge over everything that is not
             * a registered drop zone. Here the card itself follows the pointer and
             * a same-sized gap takes its prospective seat in the grid.
             */
            function aimDrag(x, y) {
                const rows = Array.from(
                    document.querySelectorAll("[data-mimi-row]"),
                );
                let rowElement = null;
                // Stick to the row the pointer last landed in until it leaves a
                // narrow buffer. This hysteresis prevents seam jitter, while the
                // broader target margin below admits castle seams without making
                // distant page content snap to a course row.
                if (lastAimRow !== null) {
                    const current = rows.find(
                        (element) =>
                            Number(element.dataset.mimiRow) === lastAimRow,
                    );
                    if (current) {
                        const rect = current.getBoundingClientRect();
                        if (
                            x >= rect.left - COURSE_DROP_MARGIN &&
                            x <= rect.right + COURSE_DROP_MARGIN &&
                            y >= rect.top - 24 &&
                            y <= rect.bottom + 24
                        ) {
                            rowElement = current;
                        }
                    }
                }
                if (!rowElement) {
                    let rowDistance = Infinity;
                    rows.forEach((element) => {
                        const rect = element.getBoundingClientRect();
                        if (
                            x < rect.left - COURSE_DROP_MARGIN ||
                            x > rect.right + COURSE_DROP_MARGIN
                        ) {
                            return;
                        }
                        const distance = Math.max(
                            rect.top - y,
                            y - rect.bottom,
                            0,
                        );
                        if (
                            distance <= COURSE_DROP_MARGIN &&
                            distance < rowDistance
                        ) {
                            rowDistance = distance;
                            rowElement = element;
                        }
                    });
                }
                if (!rowElement) {
                    fullRow.value = null;
                    dropSeat.value = null;
                    lastAimRow = null;
                    return;
                }
                const rowIndex = Number(rowElement.dataset.mimiRow);
                lastAimRow = rowIndex;
                const sourceRow = data.value.rows.findIndex((row) =>
                    row.includes(drag.value.skill),
                );
                if (
                    rowIndex !== sourceRow &&
                    data.value.rows[rowIndex].length >= ROW_LIMIT
                ) {
                    fullRow.value = rowIndex;
                    dropSeat.value = null;
                    return;
                }
                fullRow.value = null;
                const currentDrop = rowElement.querySelector(
                    "[data-mimi-skill-drop]",
                );
                if (currentDrop) {
                    const rect = currentDrop.getBoundingClientRect();
                    if (
                        x >= rect.left &&
                        x <= rect.right &&
                        y >= rect.top &&
                        y <= rect.bottom
                    ) {
                        return;
                    }
                }
                // Aim at the grid's fixed seats, captured before the drag began,
                // rather than at the cards that happen to remain. Moving the gap
                // reshuffles those cards; near a wrapped-line boundary that made
                // the same pointer position alternate between two destinations.
                const slots = grabbed.rowSlots[rowIndex] || [];
                const rowRect = rowElement.getBoundingClientRect();
                let index = 0;
                let slotDistance = Infinity;
                slots.forEach((slot, position) => {
                    const centerX = rowRect.left + slot.x;
                    const centerY = rowRect.top + slot.y;
                    // Weight the vertical axis so a wrapped row picks the right line.
                    const distance =
                        Math.abs(x - centerX) + Math.abs(y - centerY) * 2;
                    if (distance < slotDistance) {
                        slotDistance = distance;
                        index = position;
                    }
                });
                const remainingSkills =
                    data.value.rows[rowIndex].length -
                    (rowIndex === sourceRow ? 1 : 0);
                index = Math.min(index, remainingSkills);
                dropSeat.value = { row: rowIndex, index };
            }

            function startSkillDrag(event, skill) {
                if (
                    event.button !== 0 ||
                    skillSettling.value ||
                    event.target.closest("button, a, input") ||
                    // A mouse can grab a card anywhere, because it scrolls with
                    // a wheel. A finger has only the card to push against, so on
                    // touch the grip is the one part that does not scroll.
                    (event.pointerType !== "mouse" &&
                        !event.target.closest("[data-mimi-skill-grip]"))
                ) {
                    return;
                }
                const rect = event.currentTarget.getBoundingClientRect();
                const rowSlots = {};
                document.querySelectorAll("[data-mimi-row]").forEach((row) => {
                    const rowRect = row.getBoundingClientRect();
                    rowSlots[Number(row.dataset.mimiRow)] = Array.from(
                        row.children,
                    ).map((cell) => {
                        const cellRect = cell.getBoundingClientRect();
                        return {
                            x:
                                cellRect.left -
                                rowRect.left +
                                cellRect.width / 2,
                            y: cellRect.top - rowRect.top + cellRect.height / 2,
                        };
                    });
                });
                grabbed = {
                    skill,
                    startX: event.clientX,
                    startY: event.clientY,
                    offsetX: event.clientX - rect.left,
                    offsetY: event.clientY - rect.top,
                    width: rect.width,
                    height: rect.height,
                    rowSlots,
                };
                window.addEventListener("pointermove", dragSkill);
                window.addEventListener("pointerup", dropSkill);
                window.addEventListener("pointercancel", returnSkillDrag);
                window.addEventListener("keydown", cancelSkillDrag);
            }

            function dragSkill(event) {
                if (!grabbed) {
                    return;
                }
                let justStarted = false;
                if (!drag.value) {
                    // A few pixels of slack keep a plain click from becoming a drag.
                    if (
                        Math.abs(event.clientX - grabbed.startX) +
                            Math.abs(event.clientY - grabbed.startY) <
                        5
                    ) {
                        return;
                    }
                    drag.value = {
                        skill: grabbed.skill,
                        width: grabbed.width,
                        height: grabbed.height,
                        x: 0,
                        y: 0,
                    };
                    const sourceRow = data.value.rows.findIndex((row) =>
                        row.includes(grabbed.skill),
                    );
                    dropSeat.value = {
                        row: sourceRow,
                        index: data.value.rows[sourceRow].indexOf(
                            grabbed.skill,
                        ),
                    };
                    lastAimRow = sourceRow;
                    justStarted = true;
                    document.body.style.userSelect = "none";
                    document.body.style.cursor = "grabbing";
                }
                event.preventDefault();
                drag.value.x = event.clientX - grabbed.offsetX;
                drag.value.y = event.clientY - grabbed.offsetY;
                if (justStarted) {
                    nextTick(() => aimDrag(event.clientX, event.clientY));
                } else {
                    aimDrag(event.clientX, event.clientY);
                }
            }

            /**
             * Animate the ghost to a target before ending the drag. Committing the
             * move in the same tick the ghost vanishes makes the card teleport, so
             * the data only changes once the ghost has landed in its seat.
             */
            function settleSkillDrag(x, y, commit) {
                skillSettling.value = true;
                detachSkillDragListeners();
                drag.value.x = x;
                drag.value.y = y;
                skillSettleTimer = window.setTimeout(() => {
                    skillSettleTimer = null;
                    if (commit && drag.value && dropSeat.value) {
                        moveSkill(
                            drag.value.skill,
                            dropSeat.value.row,
                            dropSeat.value.index,
                        );
                    }
                    endSkillDrag();
                }, 150);
            }

            function dropSkill() {
                if (drag.value && dropSeat.value) {
                    const row = document.querySelector(
                        `[data-mimi-row="${dropSeat.value.row}"]`,
                    );
                    const gap = row?.querySelector("[data-mimi-skill-drop]");
                    if (gap) {
                        const rect = gap.getBoundingClientRect();
                        settleSkillDrag(rect.left, rect.top, true);
                        return;
                    }
                    moveSkill(
                        drag.value.skill,
                        dropSeat.value.row,
                        dropSeat.value.index,
                    );
                    endSkillDrag();
                    return;
                }
                returnSkillDrag();
            }

            /** A cancelled drag flies back to where the card was picked up. */
            function returnSkillDrag() {
                if (drag.value && grabbed) {
                    // Bring the gap home too, so the card morphs back in place.
                    const sourceRow = data.value.rows.findIndex((row) =>
                        row.includes(drag.value.skill),
                    );
                    dropSeat.value = {
                        row: sourceRow,
                        index: data.value.rows[sourceRow].indexOf(
                            drag.value.skill,
                        ),
                    };
                    settleSkillDrag(
                        grabbed.startX - grabbed.offsetX,
                        grabbed.startY - grabbed.offsetY,
                        false,
                    );
                } else {
                    endSkillDrag();
                }
            }

            function cancelSkillDrag(event) {
                if (event.key === "Escape") {
                    returnSkillDrag();
                }
            }

            function moveSkillWithKeyboard(event, title) {
                if (event.target.closest("a, button, input")) {
                    return;
                }
                const sourceRowIndex = data.value.rows.findIndex((row) =>
                    row.includes(title),
                );
                const sourceIndex =
                    data.value.rows[sourceRowIndex].indexOf(title);
                let rowIndex = sourceRowIndex;
                let destinationIndex = sourceIndex;
                if (event.key === "ArrowLeft" && sourceIndex > 0) {
                    destinationIndex = sourceIndex - 1;
                } else if (
                    event.key === "ArrowRight" &&
                    sourceIndex < data.value.rows[sourceRowIndex].length - 1
                ) {
                    destinationIndex = sourceIndex + 1;
                } else if (event.key === "ArrowUp" && sourceRowIndex > 0) {
                    rowIndex = sourceRowIndex - 1;
                    destinationIndex = Math.min(
                        sourceIndex,
                        data.value.rows[rowIndex].length,
                    );
                } else if (
                    event.key === "ArrowDown" &&
                    sourceRowIndex < data.value.rows.length - 1
                ) {
                    rowIndex = sourceRowIndex + 1;
                    destinationIndex = Math.min(
                        sourceIndex,
                        data.value.rows[rowIndex].length,
                    );
                } else {
                    return;
                }
                if (
                    rowIndex !== sourceRowIndex &&
                    data.value.rows[rowIndex].length >= ROW_LIMIT
                ) {
                    return;
                }
                event.preventDefault();
                moveSkill(title, rowIndex, destinationIndex);
            }

            function detachSkillDragListeners() {
                window.removeEventListener("pointermove", dragSkill);
                window.removeEventListener("pointerup", dropSkill);
                window.removeEventListener("pointercancel", returnSkillDrag);
                window.removeEventListener("keydown", cancelSkillDrag);
            }

            function endSkillDrag() {
                if (skillSettleTimer !== null) {
                    window.clearTimeout(skillSettleTimer);
                    skillSettleTimer = null;
                }
                skillSettling.value = false;
                grabbed = null;
                drag.value = null;
                dropSeat.value = null;
                fullRow.value = null;
                lastAimRow = null;
                document.body.style.userSelect = "";
                document.body.style.cursor = "";
                detachSkillDragListeners();
            }

            // Castles are unnamed, so a castle is identified by how far down the
            // tree it sits rather than by its position in the castles array.
            function castleNumber(row) {
                return data.value.castles.filter(
                    (castle) => castle.afterRow <= row,
                ).length;
            }

            // Seam n lies above row n, so it holds the castle recorded after row n.
            function castleOnSeam(seam) {
                return data.value.castles.some(
                    (castle) => castle.afterRow === seam,
                )
                    ? castleNumber(seam)
                    : 0;
            }

            function openAddSkillDialog(targetRow) {
                addSkillRow.value = targetRow;
                newSkillName.value = "";
                newSkillError.value = "";
                addSkillDialog.value.showModal();
                nextTick(() =>
                    addSkillDialog.value?.querySelector("input")?.focus(),
                );
            }

            function closeAddSkillDialog() {
                addSkillDialog.value?.close();
                addSkillRow.value = null;
                newSkillError.value = "";
            }

            function addSkill() {
                const label = shortSkillName(newSkillName.value).trim();
                if (!label) {
                    newSkillError.value = "Enter a skill name.";
                    return;
                }
                const title = "Skill:" + courseName + "/" + label;
                if (data.value.skills.includes(title)) {
                    newSkillError.value =
                        "That skill is already in this course.";
                    return;
                }
                if (
                    addSkillRow.value === null ||
                    !data.value.rows[addSkillRow.value]
                ) {
                    closeAddSkillDialog();
                    return;
                }
                data.value.skills.push(title);
                data.value.rows[addSkillRow.value].push(title);
                closeAddSkillDialog();
            }

            function addRow(index) {
                data.value.rows.splice(index, 0, []);
                // Castles below the new row sit one row further down the tree.
                data.value.castles.forEach((castle) => {
                    if (castle.afterRow > index) {
                        castle.afterRow++;
                    }
                });
            }

            function removeRow(rowIndex) {
                if (data.value.rows[rowIndex].length) {
                    return;
                }
                data.value.rows.splice(rowIndex, 1);
                data.value.castles = data.value.castles
                    .filter((castle) => castle.afterRow !== rowIndex + 1)
                    .map((castle) => ({
                        ...castle,
                        afterRow:
                            castle.afterRow > rowIndex + 1
                                ? castle.afterRow - 1
                                : castle.afterRow,
                    }));
            }

            function addCastle(seam) {
                data.value.castles.push({ afterRow: seam });
            }

            function removeCastle(seam) {
                data.value.castles.splice(
                    data.value.castles.findIndex(
                        (castle) => castle.afterRow === seam,
                    ),
                    1,
                );
            }

            function removeSkill(title) {
                data.value.skills = data.value.skills.filter(
                    (item) => item !== title,
                );
                data.value.rows = data.value.rows.map((row) =>
                    row.filter((item) => item !== title),
                );
            }

            const selectedTip = computed(
                () =>
                    (config.kind === "tips" &&
                        data.value.tips[selectedTipIndex.value]) ||
                    null,
            );

            const duplicateTipTitle = computed(() => {
                if (!selectedTip.value) {
                    return false;
                }
                const title = selectedTip.value.title.trim().toLowerCase();
                return (
                    title !== "" &&
                    data.value.tips.some(
                        (tip, index) =>
                            index !== selectedTipIndex.value &&
                            tip.title.trim().toLowerCase() === title,
                    )
                );
            });

            /**
             * The lesson a tip is shown before, as the number box holds it. An
             * empty box is a tip with no lesson of its own, which the skill
             * shows throughout.
             */
            const selectedTipLesson = computed({
                get: () =>
                    selectedTip.value && selectedTip.value.lesson !== null
                        ? String(selectedTip.value.lesson)
                        : "",
                set: (value) => {
                    const lesson = parseInt(value, 10);
                    selectedTip.value.lesson =
                        lesson >= 1 && lesson <= 99 ? lesson : null;
                },
            });

            function tipLessonLabel(tip) {
                return tip.lesson === null
                    ? "Tips button only"
                    : "Before lesson " + tip.lesson;
            }

            function selectTip(index) {
                selectedTipIndex.value = index;
                screen.value = "tip";
            }

            function addTip() {
                data.value.tips.push({ title: "", body: "", lesson: null });
                selectTip(data.value.tips.length - 1);
            }

            function removeTip(index) {
                if (!window.confirm("Remove this tip?")) {
                    return;
                }
                data.value.tips.splice(index, 1);
                selectedTipIndex.value = Math.max(
                    0,
                    Math.min(index, data.value.tips.length - 1),
                );
                screen.value = "tips";
            }

            /** Tips are read in the order they are listed, so that order is edited. */
            function moveTip(offset) {
                const destination = selectedTipIndex.value + offset;
                if (destination < 0 || destination >= data.value.tips.length) {
                    return;
                }
                const [tip] = data.value.tips.splice(selectedTipIndex.value, 1);
                data.value.tips.splice(destination, 0, tip);
                selectedTipIndex.value = destination;
            }

            function cleanForSave() {
                const draft = JSON.parse(JSON.stringify(data.value));
                if (config.kind === "skill") {
                    const answers = (list) =>
                        list
                            .map((text) => text.trim())
                            .filter((text) => text !== "");
                    draft.words.forEach((word) => {
                        word.sentences.forEach((sentence) => {
                            sentence.alternativeSentences = answers(
                                sentence.alternativeSentences,
                            );
                            sentence.translation = sentence.translation.trim();
                            sentence.alternativeTranslations = answers(
                                sentence.alternativeTranslations,
                            );
                        });
                    });
                }
                if (config.kind === "glossary") {
                    draft.entries = draft.entries
                        .map((entry) => ({
                            lemma: entry.lemma.trim(),
                            // Forms keep the order they were given: a
                            // paradigm is read in it, led by the lemma's row.
                            forms: entry.forms
                                .map((form, index) => ({
                                    form: index === 0 ? "" : form.form.trim(),
                                    translations: form.translations
                                        .map((text) => text.trim())
                                        .filter((text) => text !== ""),
                                }))
                                // Half a form is still somebody's work in
                                // progress and is kept, the page simply does
                                // not publish it until it has both a spelling
                                // and a translation. Only a wholly blank row
                                // goes.
                                .filter(
                                    (form, index) =>
                                        index === 0 ||
                                        form.form !== "" ||
                                        form.translations.length > 0,
                                ),
                        }))
                        .filter((entry) => entry.lemma !== "")
                        .sort((a, b) => a.lemma.localeCompare(b.lemma));
                }
                if (config.kind === "tips") {
                    // Tips keep the order they were given, because a reader
                    // meets them in it; only empty ones are dropped.
                    draft.tips = draft.tips
                        .map((tip) => {
                            const saved = {
                                title: tip.title.trim(),
                                body: tip.body.trim(),
                            };
                            // A tip with no lesson has no lesson field: the
                            // schema makes it optional rather than nullable.
                            if (tip.lesson !== null) {
                                saved.lesson = tip.lesson;
                            }
                            return saved;
                        })
                        .filter((tip) => tip.title !== "");
                }
                if (config.kind === "course") {
                    const prefix = "Skill:" + courseName + "/";
                    const qualify = (title) => prefix + shortSkillName(title);
                    draft.skills = draft.skills.map(qualify);
                    draft.rows = draft.rows.map((row) => row.map(qualify));
                    // Castle numbering follows the tree, so store them in tree order.
                    draft.castles.sort((a, b) => a.afterRow - b.afterRow);
                }
                return draft;
            }

            function openPublishDialog() {
                if (!isDirty.value || saving.value) {
                    return;
                }
                message.value = null;
                publishDialog.value.showModal();
                nextTick(() =>
                    publishDialog.value?.querySelector("textarea")?.focus(),
                );
            }

            function closePublishDialog() {
                publishDialog.value?.close();
            }

            async function save() {
                if (!isDirty.value || saving.value) {
                    return;
                }
                saving.value = true;
                message.value = null;
                try {
                    await new mw.Api().postWithEditToken({
                        action: "edit",
                        title: config.title,
                        text: JSON.stringify(cleanForSave(), null, 2),
                        contentmodel: config.model,
                        summary: summary.value.trim(),
                        baserevid: config.baseRevisionId,
                        formatversion: 2,
                    });
                    window.location.assign(mw.util.getUrl(config.title));
                } catch (error) {
                    message.value =
                        error && error.error && error.error.info
                            ? error.error.info
                            : String(error);
                    saving.value = false;
                    // The typed summary stays in the field, so reopening the
                    // dialog after reading the error resumes where it left off.
                    closePublishDialog();
                }
            }

            function cancel() {
                window.location.assign(backUrl);
            }

            return {
                config,
                data,
                backUrl,
                screen,
                screenClass,
                selectedWordIndex,
                selectedSentenceIndex,
                courseName,
                selectedWord,
                selectedSentence,
                saving,
                isDirty,
                message,
                completeCount,
                isComplete,
                drag,
                dropSeat,
                skillSettling,
                fullRow,
                courseRowCells,
                emptySkillSeats,
                rowLimit: ROW_LIMIT,
                cdxIconArrowNext: codexIcons.cdxIconArrowNext,
                cdxIconClose: codexIcons.cdxIconClose,
                cdxIconTrash: codexIcons.cdxIconTrash,
                selectedEntryIndex,
                entryFilter,
                visibleEntries,
                virtualEntries,
                entryVirtualHeight,
                entryRowHeight: ENTRY_ROW_HEIGHT,
                entryList,
                entryDrag,
                updateEntryWindow,
                selectedEntry,
                selectedFormIndex,
                selectedForm,
                completeForms,
                duplicateLemma,
                duplicateForm,
                entryGloss,
                formTitle,
                formSummary,
                isFormComplete,
                formList,
                formCells,
                formGroup,
                formDrag,
                translationList,
                translationCells,
                translationGroup,
                translationDrag,
                selectEntry,
                addEntry,
                removeEntry,
                startEntryDrag,
                moveEntryWithKeyboard,
                selectForm,
                addForm,
                removeForm,
                startFormDrag,
                moveFormWithKeyboard,
                addTranslation,
                removeTranslation,
                startTranslationDrag,
                moveTranslationWithKeyboard,
                selectedTipIndex,
                selectedTip,
                selectedTipLesson,
                duplicateTipTitle,
                tipLessonLabel,
                selectTip,
                addTip,
                removeTip,
                moveTip,
                cdxIconArrowUp: codexIcons.cdxIconArrowUp,
                cdxIconArrowDown: codexIcons.cdxIconArrowDown,
                selectWord,
                selectSentence,
                addWord,
                removeWord,
                addSentence,
                removeSentence,
                wordList,
                wordCells,
                wordGroup,
                wordDrag,
                startWordDrag,
                moveWordWithKeyboard,
                sentenceList,
                sentenceCells,
                sentenceGroup,
                sentenceDrag,
                startSentenceDrag,
                moveSentenceWithKeyboard,
                rowDrag,
                rowSettling,
                skillName,
                skillIcon,
                skillUrl,
                skillPageExists,
                addSkillDialog,
                addSkillRow,
                newSkillName,
                newSkillError,
                openAddSkillDialog,
                closeAddSkillDialog,
                addSkill,
                castleOnSeam,
                addRow,
                removeRow,
                addCastle,
                removeCastle,
                removeSkill,
                startSkillDrag,
                moveSkillWithKeyboard,
                cancel,
                publishDialog,
                summary,
                openPublishDialog,
                closePublishDialog,
                save,
            };
        },
        template: `
		<div class="mimi-editor-app fixed inset-0 z-[1000] flex h-[100dvh] flex-col overflow-hidden bg-white font-sans text-[#202122] lg:static lg:z-auto lg:h-auto lg:overflow-visible lg:bg-transparent [&_a]:cursor-pointer [&_button:not(:disabled)]:cursor-pointer [&_h2]:font-sans">
			<div class="mimi-editor-toolbar sticky top-0 z-20 flex shrink-0 items-center justify-between border-b border-[#c8ccd1] bg-[#f8f9fa] px-3 py-2 lg:hidden">
				<cdx-button aria-label="Cancel editing" title="Cancel" @click="cancel">
					<cdx-icon class="h-5 w-5" :icon="cdxIconClose"></cdx-icon>
				</cdx-button>
				<cdx-button weight="primary" action="progressive" :disabled="saving || !isDirty"
					aria-label="Publish changes" title="Publish changes" @click="openPublishDialog">
					<cdx-icon class="h-5 w-5" :icon="cdxIconArrowNext"></cdx-icon>
				</cdx-button>
			</div>
			<div class="mimi-editor-toolbar sticky top-0 z-20 mb-4 hidden shrink-0 items-center justify-end gap-3 border-b border-[#c8ccd1] bg-[#f8f9fa] px-4 py-2 lg:flex">
				<cdx-button weight="primary" action="progressive" :disabled="saving || !isDirty" @click="openPublishDialog">{{ saving ? 'Publishing…' : 'Publish changes' }}</cdx-button>
				<a :href="backUrl">Cancel</a>
			</div>
			<cdx-message v-if="message" type="error" class="shrink-0 lg:mb-4">{{ message }}</cdx-message>

			<template v-if="config.kind === 'skill'">
				<div class="flex min-h-0 flex-1 flex-col">
				<section class="mb-6 hidden gap-3 border-l-4 border-slate-400 bg-slate-50 px-4 py-3 lg:grid lg:grid-cols-[10rem_minmax(0,1fr)]">
					<div>
						<label class="block text-sm font-semibold">Grammar focus</label>
						<p class="mb-0 mt-1 text-xs leading-relaxed text-slate-500">Sentence pattern practiced in this skill.</p>
					</div>
					<cdx-text-area class="max-w-4xl" v-model="data.grammarFocus" :rows="2"></cdx-text-area>
				</section>

				<div class="relative grid min-h-0 flex-1 overflow-hidden border border-slate-300 bg-white lg:min-h-[580px] lg:grid-cols-[minmax(220px,0.8fr)_minmax(280px,1fr)_minmax(380px,1.5fr)]">
					<section class="mimi-editor-screen flex min-h-0 flex-col lg:border-r lg:border-slate-300"
						:class="screenClass('words')">
						<div class="flex min-h-14 items-center gap-3 border-b border-slate-300 bg-slate-50 px-4 py-3">
							<div>
								<h2 class="m-0 border-0 p-0 text-sm font-semibold">Words</h2>
								<small class="text-xs text-slate-500">{{ data.words.length }} total</small>
							</div>
							<cdx-button class="ml-auto" weight="quiet" action="progressive" @click="addWord">Add word</cdx-button>
						</div>
						<div class="border-b border-slate-300 bg-slate-50 p-4 lg:hidden">
							<label class="mb-2 block text-xs font-semibold text-slate-700">Grammar focus</label>
							<p class="mb-2 mt-0 text-xs leading-relaxed text-slate-500">Sentence pattern practiced in this skill.</p>
							<cdx-text-area v-model="data.grammarFocus" :rows="2"></cdx-text-area>
						</div>
						<div ref="wordList" class="min-h-0 flex-1 overflow-y-auto overscroll-contain">
							<transition-group :key="wordGroup" tag="div" move-class="transition-transform duration-150 ease-out">
								<div v-for="cell in wordCells" :key="cell.key"
									:data-mimi-drag-row="cell.type === 'row' ? '' : null"
									:data-mimi-drag-gap="cell.type === 'drop' ? '' : null"
									:class="cell.type === 'drop' ? 'box-border border-y-2 border-dashed border-[#36c] bg-blue-50' : ['box-border flex w-full items-center border-b border-slate-100 border-l-4 border-l-transparent bg-white hover:bg-slate-50', cell.index===selectedWordIndex ? 'lg:border-l-blue-600 lg:bg-slate-200' : '']"
									:style="cell.type === 'drop' ? { height: wordDrag.height + 'px' } : null"
									:aria-hidden="cell.type === 'drop' ? 'true' : null">
									<template v-if="cell.type === 'row'">
									<span role="button" tabindex="0" data-mimi-drag-handle
										class="shrink-0 cursor-grab touch-none bg-transparent py-3 pl-3 pr-1 text-lg leading-none text-slate-400 active:cursor-grabbing"
										:title="'Move ' + (cell.item.word || 'this word')"
										:aria-label="'Move ' + (cell.item.word || 'this word') + '. Drag or use the up and down arrow keys.'"
										@pointerdown="startWordDrag($event,cell.index)"
										@keydown="moveWordWithKeyboard($event,cell.index)">&#9776;</span>
									<button type="button"
										class="flex min-w-0 flex-1 items-center gap-3 bg-transparent py-3 pl-2 pr-4 text-left text-sm"
										@click="selectWord(cell.index)">
										<span class="min-w-0 flex-1 truncate font-semibold">{{ cell.item.word || 'Untitled word' }}</span>
										<span class="min-w-7 rounded-full bg-slate-100 px-2 py-0.5 text-center text-xs text-slate-500">{{ cell.item.sentences.length }}</span>
										<span class="text-xl leading-none text-slate-400 lg:hidden" aria-hidden="true">&#8250;</span>
									</button>
									</template>
									<span v-else></span>
								</div>
							</transition-group>
						</div>
						<div v-if="selectedWord" class="hidden border-t border-slate-300 bg-slate-50 p-4 lg:block">
							<label class="mb-2 block text-xs font-semibold text-slate-700">Selected word</label>
							<cdx-text-input v-model="selectedWord.word"></cdx-text-input>
							<button type="button" class="mt-3 bg-transparent p-0 text-xs text-red-600 hover:underline" @click="removeWord(selectedWordIndex)">Remove word</button>
						</div>
					</section>

					<section class="mimi-editor-screen flex min-h-0 flex-col lg:border-r lg:border-slate-300"
						:class="screenClass('sentences')">
						<div class="flex min-h-14 items-center gap-3 border-b border-slate-300 bg-slate-50 px-4 py-3">
							<button type="button" class="-ml-2 bg-transparent px-2 py-2 text-xl leading-none text-[#3366cc] lg:hidden"
								aria-label="Back to words" @click="screen = 'words'">&#8249;</button>
							<div>
								<h2 class="m-0 border-0 p-0 text-sm font-semibold">Sentences</h2>
								<small v-if="selectedWord" class="text-xs text-slate-500">{{ completeCount }} of {{ selectedWord.sentences.length }} complete</small>
							</div>
							<cdx-button class="ml-auto" weight="quiet" action="progressive" @click="addSentence">Add sentence</cdx-button>
						</div>
						<div v-if="selectedWord" class="border-b border-slate-300 bg-white p-4 lg:hidden">
							<label class="mb-2 block text-xs font-semibold text-slate-700">Word</label>
							<cdx-text-input v-model="selectedWord.word"></cdx-text-input>
							<button type="button" class="mt-3 bg-transparent p-0 text-xs text-red-600 hover:underline" @click="removeWord(selectedWordIndex)">Remove word</button>
						</div>
						<div ref="sentenceList" class="min-h-0 flex-1 overflow-y-auto overscroll-contain">
							<transition-group :key="sentenceGroup" tag="div" move-class="transition-transform duration-150 ease-out">
								<div v-for="cell in sentenceCells" :key="cell.key"
									:data-mimi-drag-row="cell.type === 'row' ? '' : null"
									:data-mimi-drag-gap="cell.type === 'drop' ? '' : null"
									:class="cell.type === 'drop' ? 'box-border border-y-2 border-dashed border-[#36c] bg-blue-50' : ['box-border flex w-full items-start border-b border-slate-100 border-l-4 border-l-transparent bg-white hover:bg-slate-50', cell.index===selectedSentenceIndex ? 'lg:border-l-blue-600 lg:bg-slate-200' : '']"
									:style="cell.type === 'drop' ? { height: sentenceDrag.height + 'px' } : null"
									:aria-hidden="cell.type === 'drop' ? 'true' : null">
									<template v-if="cell.type === 'row'">
									<span role="button" tabindex="0" data-mimi-drag-handle
										class="shrink-0 cursor-grab touch-none bg-transparent py-3 pl-3 pr-1 text-lg leading-none text-slate-400 active:cursor-grabbing"
										:title="'Move sentence ' + (cell.index + 1)"
										:aria-label="'Move sentence ' + (cell.index + 1) + '. Drag or use the up and down arrow keys.'"
										@pointerdown="startSentenceDrag($event,cell.index)"
										@keydown="moveSentenceWithKeyboard($event,cell.index)">&#9776;</span>
									<button type="button"
										class="flex min-w-0 flex-1 items-start gap-3 bg-transparent py-3 pl-2 pr-4 text-left text-sm"
										:class="cell.item.disabled ? 'line-through opacity-50' : ''"
										@click="selectSentence(cell.index)">
										<span class="mt-0.5 flex h-4 w-4 shrink-0 items-center justify-center rounded-sm border text-[10px] font-bold"
											:class="isComplete(cell.item) ? 'border-green-700 bg-green-700 text-white' : 'border-slate-300 bg-white text-transparent'">&#10003;</span>
										<span class="min-w-0 flex-1 leading-5">{{ cell.item.text || 'New sentence' }}</span>
										<span class="text-xl leading-none text-slate-400 lg:hidden" aria-hidden="true">&#8250;</span>
									</button>
									</template>
									<span v-else></span>
								</div>
							</transition-group>
							<p v-if="selectedWord && !selectedWord.sentences.length" class="px-6 py-10 text-center text-sm text-slate-500">
								No sentences for <strong>{{ selectedWord.word }}</strong> yet.
							</p>
						</div>
					</section>

					<section class="mimi-editor-screen flex min-h-0 flex-col"
						:class="screenClass('sentence')">
						<div class="flex min-h-14 items-center border-b border-slate-300 bg-slate-50 px-4 py-3">
							<button type="button" class="-ml-2 mr-3 bg-transparent px-2 py-2 text-xl leading-none text-[#3366cc] lg:hidden"
								aria-label="Back to sentences" @click="screen = 'sentences'">&#8249;</button>
							<h2 class="m-0 border-0 p-0 text-sm font-semibold">Sentence editor</h2>
						</div>
						<div v-if="selectedSentence" class="min-h-0 flex-1 space-y-6 overflow-y-auto p-5">
							<div>
								<label class="mb-2 block text-sm font-semibold">Sentence</label>
								<cdx-text-input v-model="selectedSentence.text" placeholder="Write the sentence"></cdx-text-input>
							</div>
							<answer-list :items="selectedSentence.alternativeSentences"
								label="Alternative sentences" add-label="Add alternative sentence"
								placeholder="Alternative sentence" empty-text="No alternative sentences."></answer-list>
							<div>
								<label class="mb-2 block text-sm font-semibold">Translation</label>
								<cdx-text-input v-model="selectedSentence.translation" placeholder="Translate the sentence"></cdx-text-input>
							</div>
							<answer-list :items="selectedSentence.alternativeTranslations"
								label="Alternative translations" add-label="Add alternative translation"
								placeholder="Alternative translation" empty-text="No alternative translations."></answer-list>
							<div>
								<label class="mb-2 block text-sm font-semibold">Internal notes</label>
								<cdx-text-area v-model="selectedSentence.notes" :rows="2" placeholder="Optional notes for course editors"></cdx-text-area>
							</div>
							<div class="flex items-center justify-end border-t border-slate-200 pt-4">
								<cdx-button action="destructive" weight="quiet" @click="removeSentence(selectedSentenceIndex)">Delete sentence</cdx-button>
							</div>
						</div>
						<p v-else class="m-auto px-8 py-16 text-center text-sm text-slate-500">Select a sentence to edit it, or add a new one.</p>
					</section>
				</div>
				</div>
			</template>

			<template v-else-if="config.kind === 'glossary'">
				<div class="relative grid min-h-0 flex-1 overflow-hidden border border-slate-300 bg-white lg:min-h-[36rem] lg:grid-cols-[minmax(260px,1fr)_minmax(240px,0.9fr)_minmax(360px,1.4fr)]">
					<section class="mimi-editor-screen flex min-h-0 flex-col lg:border-r lg:border-slate-300"
						:class="screenClass('entries')">
						<div class="flex min-h-14 items-center gap-3 border-b border-slate-300 bg-slate-50 px-4 py-3">
							<div>
								<h2 class="m-0 border-0 p-0 text-sm font-semibold">Words and phrases</h2>
								<small class="text-xs text-slate-500">{{ data.entries.length }} total</small>
							</div>
							<cdx-button class="ml-auto" weight="quiet" action="progressive" @click="addEntry">Add entry</cdx-button>
						</div>
						<div class="border-b border-slate-300 p-3">
							<cdx-text-input v-model="entryFilter" input-type="search" placeholder="Filter entries"></cdx-text-input>
							<p v-if="entryFilter.trim()" class="mb-0 mt-2 text-xs text-slate-500">Clear the filter to rearrange entries.</p>
						</div>
						<div ref="entryList" class="min-h-0 flex-1 overflow-y-auto overscroll-contain" @scroll="updateEntryWindow">
							<div v-if="visibleEntries.length" class="relative" :style="{ height: entryVirtualHeight + 'px' }">
							<div v-for="item in virtualEntries" :key="item.cell.key"
								:data-mimi-drag-row="item.cell.type === 'row' ? '' : null"
								:data-mimi-drag-gap="item.cell.type === 'drop' ? '' : null"
								:data-mimi-drag-position="item.cell.moveIndex"
								class="absolute left-0 h-12 w-full box-border"
								:style="{ top: (item.virtualIndex * entryRowHeight) + 'px' }"
								:class="item.cell.type === 'drop' ? 'border-y-2 border-dashed border-[#36c] bg-blue-50' : ['flex items-center border-b border-slate-100 border-l-4 border-l-transparent bg-white hover:bg-slate-50', item.index===selectedEntryIndex ? 'lg:border-l-blue-600 lg:bg-slate-200' : '']"
								:aria-hidden="item.cell.type === 'drop' ? 'true' : null">
								<template v-if="item.cell.type === 'row'">
								<span v-if="!entryFilter.trim()" role="button" tabindex="0" data-mimi-drag-handle
									class="shrink-0 cursor-grab touch-none bg-transparent py-3 pl-3 pr-1 text-lg leading-none text-slate-400 active:cursor-grabbing"
									:title="'Move ' + (item.entry.lemma || 'this entry')"
									:aria-label="'Move ' + (item.entry.lemma || 'this entry') + '. Drag or use the up and down arrow keys.'"
									@pointerdown="startEntryDrag($event,item.index)"
									@keydown="moveEntryWithKeyboard($event,item.index)">&#9776;</span>
								<span v-else class="w-8 shrink-0" aria-hidden="true"></span>
								<button type="button" class="flex min-w-0 flex-1 items-baseline gap-3 self-stretch bg-transparent py-3 pl-2 pr-4 text-left text-sm"
									@click="selectEntry(item.index)">
								<span class="min-w-0 flex-1 truncate font-semibold">{{ item.entry.lemma || 'Untitled entry' }}</span>
								<span class="min-w-0 max-w-[45%] truncate text-xs text-slate-500">{{ entryGloss(item.entry) || 'No translation' }}</span>
								<!-- The count is how a missing paradigm shows itself in a list of
								     thousands: an entry at one has no forms written yet. -->
								<span class="min-w-7 self-center rounded-full bg-slate-100 px-2 py-0.5 text-center text-xs text-slate-500"
									:title="item.entry.forms.length + ' form(s)'">{{ item.entry.forms.length }}</span>
								<span class="self-center text-xl leading-none text-slate-400 lg:hidden" aria-hidden="true">&#8250;</span>
								</button>
								</template>
								<span v-else></span>
							</div>
							</div>
							<p v-if="!visibleEntries.length" class="px-6 py-10 text-center text-sm text-slate-500">
								{{ data.entries.length ? 'No entries match that filter.' : 'No entries yet. Use “Add entry” to begin.' }}
							</p>
						</div>
					</section>

					<section class="mimi-editor-screen flex min-h-0 flex-col lg:border-r lg:border-slate-300"
						:class="screenClass('entry')">
						<div class="flex min-h-14 items-center gap-3 border-b border-slate-300 bg-slate-50 px-4 py-3">
							<button type="button" class="-ml-2 bg-transparent px-2 py-2 text-xl leading-none text-[#3366cc] lg:hidden"
								aria-label="Back to words and phrases" @click="screen = 'entries'">&#8249;</button>
							<div>
								<h2 class="m-0 border-0 p-0 text-sm font-semibold">Forms</h2>
								<small v-if="selectedEntry" class="text-xs text-slate-500">{{ completeForms }} of {{ selectedEntry.forms.length }} complete</small>
							</div>
							<cdx-button v-if="selectedEntry" class="ml-auto" weight="quiet" action="progressive" @click="addForm">Add form</cdx-button>
						</div>
						<div v-if="selectedEntry" class="border-b border-slate-300 bg-white p-4">
							<cdx-text-input v-model="selectedEntry.lemma" placeholder="Word or phrase" aria-label="Word or phrase"></cdx-text-input>
							<p v-if="duplicateLemma" class="mb-0 mt-2 text-xs text-red-600">Another entry already uses this word or phrase.</p>
						</div>
						<div v-if="selectedEntry" ref="formList" class="min-h-0 flex-1 overflow-y-auto overscroll-contain">
							<transition-group :key="formGroup" tag="div" move-class="transition-transform duration-150 ease-out">
							<div v-for="cell in formCells" :key="cell.key"
								:data-mimi-drag-row="cell.type === 'row' ? '' : null"
								:data-mimi-drag-gap="cell.type === 'drop' ? '' : null"
								:data-mimi-drag-position="cell.moveIndex"
								:class="cell.type === 'drop' ? 'box-border border-y-2 border-dashed border-[#36c] bg-blue-50' : ['box-border flex w-full items-start border-b border-slate-100 border-l-4 border-l-transparent bg-white hover:bg-slate-50', cell.index===selectedFormIndex ? 'lg:border-l-blue-600 lg:bg-slate-200' : '']"
								:style="cell.type === 'drop' ? { height: formDrag.height + 'px' } : null"
								:aria-hidden="cell.type === 'drop' ? 'true' : null">
								<template v-if="cell.type === 'row'">
								<span v-if="cell.index" role="button" tabindex="0" data-mimi-drag-handle
									class="shrink-0 cursor-grab touch-none bg-transparent py-3 pl-3 pr-1 text-lg leading-none text-slate-400 active:cursor-grabbing"
									:title="'Move ' + (cell.item.form || 'this form')"
									:aria-label="'Move ' + (cell.item.form || 'this form') + '. Drag or use the up and down arrow keys.'"
									@pointerdown="startFormDrag($event,cell.index)"
									@keydown="moveFormWithKeyboard($event,cell.index)">&#9776;</span>
								<span v-else class="w-8 shrink-0" aria-hidden="true"></span>
								<button type="button" class="flex min-w-0 flex-1 items-start gap-3 bg-transparent py-3 pl-2 pr-4 text-left text-sm"
									@click="selectForm(cell.index)">
								<!-- Done is marked, not nagged about, exactly as a skill marks a
								     finished sentence. -->
								<span class="mt-0.5 flex h-4 w-4 shrink-0 items-center justify-center rounded-sm border text-[10px] font-bold"
									:class="isFormComplete(cell.item,cell.index) ? 'border-green-700 bg-green-700 text-white' : 'border-slate-300 bg-white text-transparent'">&#10003;</span>
								<span class="flex min-w-0 flex-1 flex-col gap-0.5">
									<span class="w-full truncate font-semibold" :class="cell.index === 0 ? 'italic text-slate-500' : ''">{{ formTitle(cell.item,cell.index) }}</span>
									<span v-if="formSummary(cell.item)" class="w-full truncate text-xs text-slate-500">{{ formSummary(cell.item) }}</span>
								</span>
								<span class="text-xl leading-none text-slate-400 lg:hidden" aria-hidden="true">&#8250;</span>
								</button>
								</template>
								<span v-else></span>
							</div>
							</transition-group>
						</div>
						<div v-if="selectedEntry" class="flex justify-end border-t border-slate-300 bg-slate-50 px-4 py-3">
							<cdx-button action="destructive" weight="quiet" @click="removeEntry(selectedEntryIndex)">Delete entry</cdx-button>
						</div>
						<p v-if="!selectedEntry" class="m-auto px-8 py-16 text-center text-sm text-slate-500">Select an entry to edit it, or add a new one.</p>
					</section>

					<section class="mimi-editor-screen flex min-h-0 flex-col"
						:class="screenClass('form')">
						<div class="flex min-h-14 items-center gap-1 border-b border-slate-300 bg-slate-50 px-4 py-3">
							<button type="button" class="-ml-2 mr-2 bg-transparent px-2 py-2 text-xl leading-none text-[#3366cc] lg:hidden"
								aria-label="Back to forms" @click="screen = 'entry'">&#8249;</button>
							<h2 class="m-0 border-0 p-0 text-sm font-semibold">Form editor</h2>
							<span v-if="selectedForm" class="ml-auto hidden text-xs text-slate-500 sm:inline">{{ selectedFormIndex + 1 }} of {{ selectedEntry.forms.length }}</span>
						</div>
						<div v-if="selectedForm" class="min-h-0 flex-1 space-y-6 overflow-y-auto p-5">
							<!-- The lemma's own row has no spelling to edit, so it opens
							     straight on its translations. -->
							<div v-if="selectedFormIndex">
								<label class="mb-2 block text-sm font-semibold">Form</label>
								<cdx-text-input v-model="selectedForm.form"></cdx-text-input>
								<p v-if="duplicateForm" class="mb-0 mt-2 text-xs text-red-600">The lemma or another form already uses this spelling.</p>
							</div>
							<div>
								<div class="mb-2 flex items-center justify-between gap-3">
									<label class="text-sm font-semibold">Translations</label>
									<cdx-button weight="quiet" action="progressive" @click="addTranslation">Add translation</cdx-button>
								</div>
								<div ref="translationList" class="border border-slate-300">
									<transition-group :key="translationGroup" tag="div" move-class="transition-transform duration-150 ease-out">
										<div v-for="cell in translationCells" :key="cell.key"
											:data-mimi-drag-row="cell.type === 'row' ? '' : null"
											:data-mimi-drag-gap="cell.type === 'drop' ? '' : null"
											:class="cell.type === 'drop' ? 'box-border flex items-center justify-center border-y-2 border-dashed border-[#36c] bg-blue-50 px-4 text-xs font-semibold text-[#3366cc]' : 'flex items-center border-b border-slate-200 last:border-b-0'"
											:style="cell.type === 'drop' ? { height: translationDrag.height + 'px' } : null"
											:aria-hidden="cell.type === 'drop' ? 'true' : null">
										<template v-if="cell.type === 'row'">
										<span role="button" tabindex="0"
											data-mimi-drag-handle
											class="cursor-grab touch-none shrink-0 bg-transparent px-3 py-3 text-lg text-slate-500 active:cursor-grabbing"
											:title="'Move translation ' + (cell.index + 1)" :aria-label="'Move translation ' + (cell.index + 1) + '. Drag or use the up and down arrow keys.'"
											@pointerdown="startTranslationDrag($event,cell.index)"
											@keydown="moveTranslationWithKeyboard($event,cell.index)">&#9776;</span>
										<input v-model="selectedForm.translations[cell.index]" class="min-w-0 flex-1 px-2 py-3 text-sm outline-none" type="text" placeholder="Translation">
										<button type="button" class="bg-transparent px-3 py-2 text-lg text-slate-500 hover:text-red-600" title="Remove translation" @click="removeTranslation(cell.index)">&times;</button>
										</template>
										<span v-else></span>
										</div>
									</transition-group>
									<p v-if="!selectedForm.translations.length" class="m-0 px-4 py-3 text-sm text-slate-500">No translations yet.</p>
								</div>
							</div>
							<div v-if="selectedFormIndex" class="flex justify-end border-t border-slate-200 pt-4">
								<cdx-button action="destructive" weight="quiet" @click="removeForm(selectedFormIndex)">Delete form</cdx-button>
							</div>
						</div>
						<p v-else class="m-auto px-8 py-16 text-center text-sm text-slate-500">Select a form to edit it, or add a new one.</p>
					</section>
				</div>
			</template>

			<template v-else-if="config.kind === 'tips'">
				<div class="relative grid min-h-0 flex-1 overflow-hidden border border-slate-300 bg-white lg:min-h-[42rem] lg:grid-cols-[minmax(260px,1fr)_minmax(420px,1.9fr)]">
					<section class="mimi-editor-screen flex min-h-0 flex-col lg:border-r lg:border-slate-300"
						:class="screenClass('tips')">
						<div class="flex min-h-14 items-center gap-3 border-b border-slate-300 bg-slate-50 px-4 py-3">
							<div>
								<h2 class="m-0 border-0 p-0 text-sm font-semibold">Tips</h2>
								<small class="text-xs text-slate-500">{{ data.tips.length }} total</small>
							</div>
							<cdx-button class="ml-auto" weight="quiet" action="progressive" @click="addTip">Add tip</cdx-button>
						</div>
						<div class="min-h-0 flex-1 overflow-y-auto overscroll-contain">
							<button v-for="(tip,index) in data.tips" :key="index" type="button"
								class="flex w-full items-center gap-3 border-b border-slate-100 border-l-4 border-l-transparent bg-white px-4 py-3 text-left text-sm hover:bg-slate-50"
								:class="index===selectedTipIndex ? 'lg:border-l-blue-600 lg:bg-slate-200' : ''"
								@click="selectTip(index)">
								<span class="flex min-w-0 flex-1 flex-col gap-0.5">
									<span class="w-full truncate font-semibold">{{ tip.title || 'Untitled tip' }}</span>
									<span class="text-xs text-slate-500">{{ tipLessonLabel(tip) }}</span>
								</span>
								<span class="text-xl leading-none text-slate-400 lg:hidden" aria-hidden="true">&#8250;</span>
							</button>
							<p v-if="!data.tips.length" class="px-6 py-10 text-center text-sm text-slate-500">
								No tips yet
							</p>
						</div>
					</section>

					<section class="mimi-editor-screen flex min-h-0 flex-col"
						:class="screenClass('tip')">
						<div class="flex min-h-14 items-center gap-1 border-b border-slate-300 bg-slate-50 px-4 py-3">
							<button type="button" class="-ml-2 mr-2 bg-transparent px-2 py-2 text-xl leading-none text-[#3366cc] lg:hidden"
								aria-label="Back to tips" @click="screen = 'tips'">&#8249;</button>
							<h2 class="m-0 border-0 p-0 text-sm font-semibold">Tip editor</h2>
							<span v-if="selectedTip" class="ml-auto flex items-center gap-1">
								<!-- The position is the first thing to go: a phone has the list one tap away. -->
								<span class="mr-1 hidden text-xs text-slate-500 sm:inline">{{ selectedTipIndex + 1 }} of {{ data.tips.length }}</span>
								<button type="button" class="flex h-7 w-7 items-center justify-center bg-transparent text-slate-500 hover:bg-slate-200 disabled:cursor-default disabled:text-slate-300 disabled:hover:bg-transparent"
									:disabled="selectedTipIndex === 0" title="Move tip up" aria-label="Move tip up" @click="moveTip(-1)">
									<cdx-icon class="h-4 w-4" :icon="cdxIconArrowUp"></cdx-icon>
								</button>
								<button type="button" class="flex h-7 w-7 items-center justify-center bg-transparent text-slate-500 hover:bg-slate-200 disabled:cursor-default disabled:text-slate-300 disabled:hover:bg-transparent"
									:disabled="selectedTipIndex === data.tips.length - 1" title="Move tip down" aria-label="Move tip down" @click="moveTip(1)">
									<cdx-icon class="h-4 w-4" :icon="cdxIconArrowDown"></cdx-icon>
								</button>
							</span>
						</div>
						<div v-if="selectedTip" class="min-h-0 flex-1 space-y-5 overflow-y-auto p-5">
							<div>
								<label class="mb-2 block text-sm font-semibold" for="mimi-tip-title">Title</label>
								<cdx-text-input id="mimi-tip-title" v-model="selectedTip.title" placeholder=""></cdx-text-input>
								<p v-if="duplicateTipTitle" class="mb-0 mt-2 text-xs text-red-600">Another tip already uses this title.</p>
							</div>
							<div>
								<label class="mb-2 block text-sm font-semibold" for="mimi-tip-lesson">Show before lesson</label>
								<cdx-text-input id="mimi-tip-lesson" class="max-w-[10rem]" v-model="selectedTipLesson"
									input-type="number" min="1" max="99" placeholder="No lesson"></cdx-text-input>
							</div>
							<div>
								<label class="mb-2 block text-sm font-semibold">Tip</label>
								<tip-body :key="selectedTipIndex" v-model="selectedTip.body"></tip-body>
							</div>
							<div class="flex justify-end border-t border-slate-200 pt-4">
								<cdx-button action="destructive" weight="quiet" @click="removeTip(selectedTipIndex)">Delete tip</cdx-button>
							</div>
						</div>
						<p v-else class="m-auto px-8 py-16 text-center text-sm text-slate-500">Select a tip to edit it, or add a new one.</p>
					</section>
				</div>
			</template>

			<template v-else>
				<div class="relative min-h-0 flex-1 lg:flex-none">
				<section class="mimi-editor-screen flex min-h-0 flex-col lg:block" :class="screenClass('rows')">
					<div class="mb-3 flex shrink-0 flex-wrap items-center gap-x-4 gap-y-2 px-3 pt-3 lg:px-0 lg:pt-0">
						<h2 class="m-0 border-0 p-0 text-xl font-semibold">Course skills</h2>
						<span class="text-sm text-slate-500">{{ data.skills.length }} skills &middot; {{ data.rows.length }} rows &middot; {{ data.castles.length }} castles</span>
						<cdx-button class="ml-auto" action="progressive" @click="addRow(data.rows.length)">Add row</cdx-button>
					</div>

					<div class="min-h-0 flex-1 overflow-y-auto overscroll-contain border-y border-slate-300 bg-white lg:overflow-visible lg:border-x">
						<template v-for="(row,rowIndex) in data.rows" :key="rowIndex">
							<row-seam :castle="castleOnSeam(rowIndex)" :can-add-castle="rowIndex > 0"
								@add-row="addRow(rowIndex)" @add-castle="addCastle(rowIndex)" @remove-castle="removeCastle(rowIndex)"></row-seam>
							<div class="flex items-stretch bg-white">
								<div class="flex w-6 shrink-0 select-none items-center justify-center text-xs font-semibold text-slate-400 sm:w-9" :title="'Row ' + (rowIndex + 1)">{{ rowIndex + 1 }}</div>
								<!-- A retained leaving cell would become a fifth grid item and wrap the row during a drag. -->
								<transition-group tag="div" :css="!drag" move-class="transition-transform duration-150 ease-out"
									class="grid min-w-0 flex-1 grid-cols-2 gap-2 p-2 transition-colors sm:grid-cols-4 sm:gap-3 sm:p-3" :data-mimi-row="rowIndex"
									:class="fullRow === rowIndex ? 'bg-red-50' : ''">
									<div v-for="cell in courseRowCells(rowIndex)" :key="cell.key"
										:data-mimi-skill="cell.type === 'skill' ? cell.skill : null"
										:data-mimi-skill-drop="cell.type === 'drop' ? '' : null"
										:tabindex="cell.type === 'skill' ? 0 : null"
										:class="cell.type === 'drop' ? 'box-border min-h-[46px] border-2 border-dashed border-[#36c] bg-blue-50' : 'group flex min-w-0 cursor-grab select-none items-center gap-2 border border-[#a2a9b1] bg-white py-2.5 pr-2 text-[#202122] transition-colors duration-100 hover:border-[#72777d] focus:border-[#36c] focus:shadow-[inset_0_0_0_1px_#36c] focus:outline focus:outline-1 focus:outline-transparent active:cursor-grabbing'"
										:style="cell.type === 'drop' ? { height: drag.height + 'px' } : null"
										:aria-label="cell.type === 'skill' ? 'Move ' + skillName(cell.skill) + '. Drag or use arrow keys.' : null"
										:aria-hidden="cell.type === 'drop' ? 'true' : null"
										@pointerdown="cell.type === 'skill' && startSkillDrag($event,cell.skill)"
										@keydown="cell.type === 'skill' && moveSkillWithKeyboard($event,cell.skill)">
										<template v-if="cell.type === 'skill'">
										<!-- The icon doubles as the grip: it is the full height of the card so
										     that a finger can find it, and the one part that a drag beats a scroll on. -->
										<span data-mimi-skill-grip class="-my-2.5 flex shrink-0 touch-none items-center self-stretch pl-3 pr-0.5 text-[#54595d]" aria-hidden="true">
											<cdx-icon class="h-5 w-5" :icon="skillIcon(cell.skill)"></cdx-icon>
										</span>
										<a :href="skillUrl(cell.skill)" target="_blank" rel="noopener"
											class="min-w-0 cursor-pointer truncate text-sm font-semibold leading-tight hover:underline focus:underline"
											:class="skillPageExists(cell.skill) ? 'text-[#3366cc]' : 'text-[#ba0000]'"
											:title="skillPageExists(cell.skill) ? 'Open ' + skillName(cell.skill) + ' in a new tab' : 'Create ' + skillName(cell.skill) + ' in a new tab'">{{ skillName(cell.skill) }}</a>
										<span class="min-w-0 flex-1" aria-hidden="true"></span>
										<!-- Nothing hovers on a phone, so what hovering would reveal is simply there. -->
										<button type="button" class="shrink-0 bg-transparent p-1 text-[#72777d] transition-opacity hover:text-[#bf3c2c] focus:opacity-100 group-hover:opacity-100 lg:opacity-0"
											:aria-label="'Remove ' + skillName(cell.skill)" title="Remove skill" @click="removeSkill(cell.skill)">
											<cdx-icon class="h-4 w-4" :icon="cdxIconTrash"></cdx-icon>
										</button>
										</template>
									</div>
									<button v-for="seat in emptySkillSeats(rowIndex)" :key="'seat' + seat" type="button"
										class="flex min-h-[46px] items-center justify-center border border-dashed border-slate-300 bg-transparent text-lg leading-none text-slate-400 transition-colors hover:border-[#36c] hover:bg-white hover:text-[#36c]"
										:aria-label="'Add a skill to row ' + (rowIndex + 1)" title="Add skill" @click="openAddSkillDialog(rowIndex)">+</button>
								</transition-group>
								<div class="flex w-6 shrink-0 items-center justify-center sm:w-9">
									<button v-if="!row.length" type="button" class="bg-transparent px-2 py-1 text-lg leading-none text-slate-400 hover:text-[#bf3c2c]"
										:aria-label="'Remove row ' + (rowIndex + 1)" title="Remove row" @click="removeRow(rowIndex)">&times;</button>
								</div>
							</div>
						</template>
						<row-seam :castle="castleOnSeam(data.rows.length)" :can-add-castle="data.rows.length > 0"
							@add-row="addRow(data.rows.length)" @add-castle="addCastle(data.rows.length)" @remove-castle="removeCastle(data.rows.length)"></row-seam>
						<p v-if="!data.rows.length" class="m-0 px-6 py-10 text-center text-sm text-slate-500">No course rows yet. Use “Add row” to begin arranging skills.</p>
					</div>
				</section>
				</div>

				<div v-if="drag" class="pointer-events-none fixed z-50 flex items-center gap-2 border border-[#36c] bg-white py-2.5 pl-3 pr-2 shadow-lg"
					:style="{ left: drag.x + 'px', top: drag.y + 'px', width: drag.width + 'px', transform: skillSettling ? 'rotate(0deg)' : 'rotate(1.5deg)', transition: skillSettling ? 'left 150ms ease-out, top 150ms ease-out, transform 150ms ease-out' : 'none' }" aria-hidden="true">
					<span class="shrink-0 text-[#54595d]"><cdx-icon class="h-5 w-5" :icon="skillIcon(drag.skill)"></cdx-icon></span>
					<strong class="min-w-0 flex-1 truncate text-sm font-semibold leading-tight"
						:class="skillPageExists(drag.skill) ? 'text-[#3366cc]' : 'text-[#ba0000]'">{{ skillName(drag.skill) }}</strong>
				</div>

				<dialog ref="addSkillDialog" aria-labelledby="mimi-add-skill-title"
					class="w-[32rem] max-w-[calc(100%_-_2rem)] border border-slate-300 bg-white p-0 text-[#202122] shadow-2xl backdrop:bg-black/40"
					@close="addSkillRow = null" @cancel="newSkillError = ''">
					<form class="m-0" @submit.prevent="addSkill">
						<header class="border-b border-slate-200 px-5 py-4">
							<h2 id="mimi-add-skill-title" class="m-0 border-0 p-0 text-lg font-semibold">Add a skill</h2>
						</header>
						<div class="px-5 py-4">
							<label class="mb-1 block text-sm font-semibold" for="mimi-new-skill-name">Skill name</label>
							<cdx-text-input id="mimi-new-skill-name" v-model="newSkillName" placeholder="For example, Food"
								:status="newSkillError ? 'error' : 'default'" @update:model-value="newSkillError = ''"></cdx-text-input>
							<p v-if="newSkillError" class="mb-0 mt-1 text-sm text-[#b32424]">{{ newSkillError }}</p>
							<p v-else class="mb-0 mt-2 text-sm text-slate-500">The skill page can be created after it is added to the course.</p>
						</div>
						<footer class="flex justify-end gap-2 border-t border-slate-200 bg-slate-50 px-5 py-3">
							<cdx-button type="button" @click="closeAddSkillDialog">Cancel</cdx-button>
							<cdx-button type="submit" weight="primary" action="progressive">Add skill</cdx-button>
						</footer>
					</form>
				</dialog>
			</template>

			<dialog ref="publishDialog" aria-labelledby="mimi-publish-title"
				class="w-[32rem] max-w-[calc(100%_-_2rem)] border border-slate-300 bg-white p-0 text-[#202122] shadow-2xl backdrop:bg-black/40">
				<form class="m-0" @submit.prevent="save">
					<header class="border-b border-slate-200 px-5 py-4">
						<h2 id="mimi-publish-title" class="m-0 border-0 p-0 text-lg font-semibold">Publish changes</h2>
					</header>
					<div class="px-5 py-4">
						<label class="mb-1 block text-sm font-semibold" for="mimi-edit-summary">Edit summary</label>
						<cdx-text-area id="mimi-edit-summary" v-model="summary" :disabled="saving" :rows="4"
							placeholder="Describe what you changed"></cdx-text-area>

						<p class="mb-0 mt-3 text-sm text-slate-600">By clicking <strong class="font-semibold">Publish changes</strong>, you
							irrevocably agree to release your contribution under the <a class="text-[#3366cc] underline"
								href="https://creativecommons.org/licenses/by-nc-sa/4.0/" target="_blank"
								rel="noopener noreferrer">Creative Commons Attribution-NonCommercial-ShareAlike 4.0 International</a> license.</p>
					</div>
					<footer class="flex justify-end gap-2 border-t border-slate-200 bg-slate-50 px-5 py-3">
						<cdx-button type="button" :disabled="saving" @click="closePublishDialog">Cancel</cdx-button>
						<cdx-button type="submit" weight="primary" action="progressive" :disabled="saving || !isDirty">{{ saving ? 'Publishing…' : 'Publish changes' }}</cdx-button>
					</footer>
				</form>
			</dialog>

			<!-- The row being dragged, whichever list it was picked up from. It
			     lives here rather than beside its list so that a screen's own
			     transform cannot become the frame it is positioned against. -->
			<div v-if="rowDrag" class="pointer-events-none fixed z-50 flex items-center border border-[#36c] bg-white shadow-lg"
				:style="{ left: rowDrag.x + 'px', top: rowDrag.y + 'px', width: rowDrag.width + 'px', transform: rowSettling ? 'rotate(0deg)' : 'rotate(0.5deg)', transition: rowSettling ? 'left 150ms ease-out, top 150ms ease-out, transform 150ms ease-out' : 'none' }" aria-hidden="true">
				<span class="shrink-0 px-3 py-3 text-lg leading-none text-slate-400">&#9776;</span>
				<span class="min-w-0 flex-1 truncate px-2 py-3 text-sm">{{ rowDrag.text }}</span>
			</div>
		</div>`,
    }).mount("#mimi-editor-root");
})();
