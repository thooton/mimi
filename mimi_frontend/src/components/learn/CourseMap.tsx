import { useEffect, useRef, useState } from "react";
import { Button, Popover, Tooltip } from "@blueprintjs/core";
import {
    BookIcon,
    ChatIcon,
    GlobeIcon,
    HeartIcon,
    LockIcon,
    NumericalIcon,
    PeopleIcon,
    ShoppingCartIcon,
    TimeIcon,
    TranslateIcon,
} from "@blueprintjs/icons";
import type { ApiPosition } from "../../data/api";
import type { CastleGroup, SkillNode } from "../../data/course";
import { levelProgress, skillAction, stateLabel } from "../../data/course";
import TipsDialog from "./TipsDialog";

/* Static icon components render their SVG immediately. Blueprint's generic
   <Icon> first renders a font glyph which inherits the page's 14px type,
   making these deliberately oversized medallion icons appear tiny. */
const ICONS = [
    [/number|count/i, NumericalIcon],
    [/greet|phrase|talk/i, ChatIcon],
    [/food|drink/i, ShoppingCartIcon],
    [/family|people/i, PeopleIcon],
    [/place|travel/i, GlobeIcon],
    [/body|health/i, HeartIcon],
    [/time|past|future/i, TimeIcon],
    [/verb|grammar/i, TranslateIcon],
] as const;

function iconOf(skill: SkillNode) {
    return (
        ICONS.find(([pattern]) =>
            pattern.test(`${skill.id} ${skill.name}`),
        )?.[1] ?? BookIcon
    );
}

/* Where a skill's card can go.
 *
 * Beside the stone is the right answer on a desktop — the tree is a column
 * down the middle of a wide page and the card fills the margin next to it.
 * On a phone there is no margin: the tree *is* the page, the stone sits at
 * its centre, and a 265px card asked for at `right` has nowhere to be. It
 * used to be placed there anyway and hang off the edge of the screen with
 * its Start button past the fold — which is the bug this answers.
 *
 * So under this width the card goes *under* the stone instead, where the
 * full width of the screen is available to it.
 *
 * 720px is the navbar's breakpoint (chrome.css), reused deliberately: it is
 * already the width at which this site says "this is a phone".
 */
const BESIDE_MIN = "(min-width: 721px)";

/** true when there is room beside a stone for its card */
function useBeside(): boolean {
    /* Starts false on both sides of hydration and corrects in the effect.
       The server has no viewport to measure, so anything else here would be
       a guess that React would have to reconcile — and guessing "desktop"
       is the guess that renders wrong on the phones this is for. Nothing
       moves when it corrects: every popover is closed at mount. */
    const [beside, setBeside] = useState(false);
    useEffect(() => {
        const query = window.matchMedia(BESIDE_MIN);
        const sync = () => setBeside(query.matches);
        sync();
        query.addEventListener("change", sync);
        return () => query.removeEventListener("change", sync);
    }, []);
    return beside;
}

function CrownIcon({ level }: { level: number }) {
    return (
        <svg className="level-crown" viewBox="0 0 30 28" aria-hidden="true">
            <path d="M3 7 9 13 15 4 21 13 27 7 24 24H6Z" />
            <text x="15" y="21" textAnchor="middle">
                {level}
            </text>
        </svg>
    );
}

function SkillCard({
    skill,
    lessonXp,
    onStart,
    starting,
    onShowTips,
}: {
    skill: SkillNode;
    lessonXp: number;
    onStart: (position: ApiPosition) => void;
    starting: boolean;
    onShowTips: (position: ApiPosition) => void;
}) {
    const done = skill.state === "completed";
    const next = done
        ? Math.max(1, skill.level * skill.lessons + skill.lessons_done)
        : skill.level * skill.lessons + skill.lessons_done + 1;
    return (
        <div className="skill-card">
            <div className="skill-card-top">
                <span className="eyebrow">Level {skill.level}</span>
                <span className="skill-card-count">
                    {skill.lessons_done} / {skill.lessons}
                </span>
            </div>
            <h3>{skill.name}</h3>
            <p>{skill.focus}</p>
            <div className="meter">
                <div
                    className="meter-fill"
                    style={{ width: `${levelProgress(skill) * 100}%` }}
                />
            </div>
            {/* Tips is the secondary action, so it keeps the plain gray button;
          Start stays the coloured one */}
            <Button
                fill
                icon="lightbulb"
                text="Tips"
                onClick={() => onShowTips({ skill: skill.id, lesson: next })}
            />
            <Button
                fill
                intent="primary"
                icon={done ? "refresh" : "play"}
                text={skillAction(skill, lessonXp)}
                loading={starting}
                onClick={() => onStart({ skill: skill.id, lesson: next })}
            />
        </div>
    );
}

function SkillMedallion({
    skill,
    lessonXp,
    onStart,
    starting,
    focusRef,
}: {
    skill: SkillNode;
    lessonXp: number;
    onStart: (position: ApiPosition) => void;
    starting: boolean;
    focusRef?: React.Ref<HTMLDivElement>;
}) {
    const locked = skill.state === "locked";
    const progress = levelProgress(skill);
    const SkillIcon = locked ? LockIcon : iconOf(skill);
    /* which lesson the tips dialog is showing; null keeps it closed. Lives
     here rather than in the card because the popover unmounts its content
     when it closes, which would take an open dialog down with it */
    const [tipsFor, setTipsFor] = useState<ApiPosition | null>(null);
    const beside = useBeside();
    const button = (
        <button
            className="skill-button"
            type="button"
            disabled={locked}
            aria-label={`${skill.name}, level ${skill.level}, ${stateLabel(skill.state)}, ${skill.lessons_done} of ${skill.lessons} lessons`}
        >
            <svg
                className="skill-ring"
                viewBox="0 0 100 100"
                aria-hidden="true"
            >
                <circle className="skill-ring-track" cx="50" cy="50" r="45" />
                {!locked && (
                    <circle
                        className="skill-ring-fill"
                        cx="50"
                        cy="50"
                        r="45"
                        pathLength="1"
                        strokeDasharray={`${progress} 1`}
                    />
                )}
            </svg>
            <span className="skill-face">
                <SkillIcon size={28} />
            </span>
            {!locked && (
                <span className="level-badge">
                    <CrownIcon level={skill.level} />
                </span>
            )}
        </button>
    );
    return (
        <div
            ref={focusRef}
            className={`skill-node skill-level-${skill.level} skill-${skill.state}`}
        >
            {locked ? (
                <Tooltip content="Complete the earlier row and its castle first">
                    {button}
                </Tooltip>
            ) : (
                <Popover
                    placement={beside ? "right" : "bottom"}
                    popoverClassName="node-popover bp6-popover-minimal-animation"
                    /* Placement is where the card would *like* to be; this is
                       what stops it leaving the screen when it can't. Popper
                       shifts an overflowing popover back along its axis, and
                       the padding keeps a margin of paper between the card and
                       the edge instead of letting it sit flush against it —
                       which on the outermost stone of a row is the difference
                       between a card and a card with a corner cut off. */
                    modifiers={{
                        preventOverflow: {
                            enabled: true,
                            options: { padding: 8 },
                        },
                    }}
                    content={
                        <SkillCard
                            skill={skill}
                            lessonXp={lessonXp}
                            onStart={onStart}
                            starting={starting}
                            onShowTips={setTipsFor}
                        />
                    }
                >
                    {button}
                </Popover>
            )}
            <span className="skill-label">{skill.name}</span>
            <TipsDialog
                skillName={skill.name}
                position={tipsFor}
                onClose={() => setTipsFor(null)}
            />
        </div>
    );
}

/* The castle between stretches: a crenellated silhouette — body, three
   merlons, and the dark rim that gives it thickness — wearing its state
   inside: a door when the test is open, a padlock before, a tick after.
   The same drawing the landing page's skill tree uses (SkillTreeArt.astro),
   in shared local coordinates. */
function CastleGlyph({ state }: { state: CastleGroup["state"] }) {
    return (
        <svg className="castle-glyph" viewBox="0 0 44 42" aria-hidden="true">
            <rect
                className="castle-glyph-edge"
                x={0}
                y={12}
                width={44}
                height={30}
                rx={5}
            />
            <rect
                className="castle-glyph-body"
                x={0}
                y={8}
                width={44}
                height={30}
                rx={5}
            />
            <rect
                className="castle-glyph-body"
                x={3}
                y={0}
                width={10}
                height={12}
                rx={1.5}
            />
            <rect
                className="castle-glyph-body"
                x={17}
                y={0}
                width={10}
                height={12}
                rx={1.5}
            />
            <rect
                className="castle-glyph-body"
                x={31}
                y={0}
                width={10}
                height={12}
                rx={1.5}
            />
            {state === "available" && (
                <path
                    className="castle-glyph-door"
                    d="M 16 38 v -9 a 6 6 0 0 1 12 0 v 9 z"
                />
            )}
            {state === "locked" && (
                <g className="castle-glyph-mark">
                    <rect x={15} y={21} width={14} height={11} rx={2} />
                    <path d="M 18.5 21 v -3.5 a 3.5 3.5 0 0 1 7 0 v 3.5" />
                </g>
            )}
            {state === "passed" && (
                <path
                    className="castle-glyph-mark"
                    d="M 14 23.5 l 5.5 5.5 l 11 -11"
                />
            )}
        </svg>
    );
}

function CastleNode({
    group,
    onStart,
    starting,
    focusRef,
}: {
    group: CastleGroup;
    onStart: () => void;
    starting: boolean;
    focusRef?: React.Ref<HTMLDivElement>;
}) {
    const locked = group.state === "locked";
    return (
        <div ref={focusRef} className={`castle-node castle-${group.state}`}>
            <Tooltip
                content={
                    locked
                        ? "Raise every skill in this stretch to level 2"
                        : group.state === "passed"
                          ? "Castle passed"
                          : "Test your skills"
                }
            >
                <button
                    type="button"
                    className="castle-button"
                    disabled={locked || group.state === "passed"}
                    onClick={onStart}
                >
                    <CastleGlyph state={group.state} />
                </button>
            </Tooltip>
            <span className="castle-label">
                {group.state === "passed"
                    ? "Castle passed"
                    : `Castle ${group.castle + 1}`}
            </span>
            {group.state === "available" && (
                <Button
                    small
                    intent="warning"
                    text="Take the test"
                    loading={starting}
                    onClick={onStart}
                />
            )}
        </div>
    );
}

export default function CourseMap({
    tree,
    lessonXp,
    onStart,
    onCastle,
    starting,
}: {
    tree: CastleGroup[];
    lessonXp: number;
    onStart: (position: ApiPosition) => void;
    onCastle: () => void;
    starting: boolean;
}) {
    const target = useRef<HTMLDivElement>(null);
    /* block body, not a concise arrow: whatever scrollIntoView returns must
       not become the effect's cleanup — React would call it on the next run */
    useEffect(() => {
        target.current?.scrollIntoView({
            block: "center",
            behavior: "auto",
        });
    }, [tree]);
    let marked = false;
    return (
        <div className="course-map skill-tree">
            {tree.map((group) => (
                <section
                    className="castle-group"
                    key={group.castle}
                    aria-label={`Castle ${group.castle + 1} skills`}
                >
                    {group.rows.map((row) => (
                        <div className="skill-row" key={row.id}>
                            {row.skills.map((skill) => {
                                const relevant =
                                    !marked && skill.state === "available";
                                if (relevant) marked = true;
                                return (
                                    <SkillMedallion
                                        key={skill.id}
                                        skill={skill}
                                        lessonXp={lessonXp}
                                        onStart={onStart}
                                        starting={starting}
                                        focusRef={relevant ? target : undefined}
                                    />
                                );
                            })}
                        </div>
                    ))}
                    {(() => {
                        const relevant = !marked && group.state === "available";
                        if (relevant) marked = true;
                        return (
                            <CastleNode
                                group={group}
                                onStart={onCastle}
                                starting={starting}
                                focusRef={relevant ? target : undefined}
                            />
                        );
                    })()}
                </section>
            ))}
        </div>
    );
}
