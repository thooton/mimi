import { Icon, Menu, MenuDivider, MenuItem, Popover } from '@blueprintjs/core';
import Flag from './Flag';
import { AVAILABLE } from '../data/languages';
import type { LanguageOption } from '../data/languages';
import { useTargetLang } from '../data/targetLang';

/* The one piece of chrome that answers "what am I learning right now?".

   It sits in the navbar rather than on the learn page because the answer is
   global: the profile, the leaderboard and the practice drills all mean
   something different depending on it. Being always visible is the point —
   a setting you have to go looking for is one people forget they set. */

export default function LanguagePicker({ visible = true }: { visible?: boolean }) {
  const { lang, ready: loaded, setLang } = useTargetLang();
  const current = AVAILABLE.find((language) => language.code === lang);

  /* Until the backend has answered there is no button at all: rendering
     the globe here would be a flicker at best and the wrong flag's stand-in
     at worst. The picker simply appears once the choice is known — blank
     first, flag (or the genuinely unpicked globe) after.

     `visible` is the navbar's half of the same idea: the bar holds the
     picker back until the profile request settles, so the flag, the streak
     and the account button reveal together in one paint instead of popping
     in one after another. The component stays mounted the whole time —
     `loaded` has long flipped by then, so the flag is ready the moment
     it's allowed to show. */
  if (!visible || !loaded) return null;

  function item(l: LanguageOption) {
    return (
      <MenuItem
        key={l.code}
        roleStructure="listoption"
        selected={l.code === lang}
        icon={<Flag region={l.region} size={18} className="flag-menu" />}
        text={l.name}
        labelElement={<span className="lang-endonym">{l.endonym}</span>}
        onClick={() => setLang(l.code)}
      />
    );
  }

  return (
    <Popover
      placement="bottom-end"
      minimal
      content={
        /* a listbox, not a menu: the items are `listoption`s, since this
           picks one of a set of values rather than firing one of a set of
           commands — and Blueprint only draws the selected tick for those.
           Left as Menu's default `menu` role, the options would be sitting
           in a list that doesn't allow them. */
        <Menu className="lang-menu" role="listbox" aria-label="Language">
          <MenuDivider title="Courses" />
          {AVAILABLE.map(item)}
        </Menu>
      }
    >
      <button
        className="lang-btn"
        type="button"
        aria-label={current ? `Learning ${current.name} — change language` : 'Choose a language'}
      >
        {/* nothing picked yet — a hollow slot with the same footprint as the
            flag, and the only state left once `!loaded` returns early above */}
        {current ? (
          <Flag region={current.region} size={22} className="flag-btn" aspect="4x3" />
        ) : (
          <span className="lang-empty" aria-hidden="true">
            <Icon icon="globe" size={13} />
          </span>
        )}
        <Icon className="caret" icon="caret-down" size={12} />
      </button>
    </Popover>
  );
}
