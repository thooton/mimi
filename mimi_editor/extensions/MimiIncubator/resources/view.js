( function () {
	'use strict';
	document.getElementById( 'firstHeading' )?.classList.add( 'font-sans' );

	// Skill view: click a word to reveal its sentences.
	document.querySelectorAll( '[data-mimi-skill-view]' ).forEach( ( view ) => {
		const buttons = view.querySelectorAll( '[data-mimi-word]' );
		const sentenceGroups = view.querySelectorAll( '[data-mimi-sentences]' );
		buttons.forEach( ( button ) => {
			button.addEventListener( 'click', () => {
				const index = button.dataset.wordIndex;
				buttons.forEach( ( item ) => {
					item.classList.remove( 'border-l-[#36c]', 'bg-[#eaecf0]' );
					item.classList.add( 'border-l-transparent', 'bg-white', 'hover:bg-[#f8f9fa]' );
				} );
				sentenceGroups.forEach( ( item ) => {
					item.classList.remove( 'block' );
					item.classList.add( 'hidden' );
				} );
				button.classList.remove( 'border-l-transparent', 'bg-white', 'hover:bg-[#f8f9fa]' );
				button.classList.add( 'border-l-[#36c]', 'bg-[#eaecf0]' );
				const selected = view.querySelector( '[data-mimi-sentences][data-word-index="' + index + '"]' );
				selected.classList.remove( 'hidden' );
				selected.classList.add( 'block' );
			} );
		} );
	} );

	// Glossary view: build the entry table from the page's own data, and filter
	// it by word, form or translation. Each entry is one row group, so a hit
	// shows the whole paradigm it was found in.
	//
	// The page arrives with its first entries written out as rows and all of
	// them as JSON, because a segment of five hundred entries is fifteen
	// thousand rows: laying those out costs a browser far more than fetching
	// them did, and a reader is looking at twenty. So rows are built a block at
	// a time as the block comes into view, and taken down again once it is well
	// out of view, the table holds little more than the screenful being read.
	//
	// A block's height is measured before it is taken down, so scrolling back
	// through it does not move the ground under the reader; only the first
	// guess at a height is a guess.
	const ENTRIES_PER_BLOCK = 10;
	// A row is its padding plus a line per translation, since the translations
	// of a form are stacked in the cell. Guessing a fixed row height instead
	// would put a paradigm with four meanings a row at a third of its size, and
	// the scrollbar would lurch every time one was reached.
	const ROW_PADDING = 24;
	const LINE_HEIGHT = 21;
	// How far beyond the screen rows are built, so that scrolling meets them
	// already there rather than watching them arrive.
	const MARGIN = 1200;

	document.querySelectorAll( '[data-mimi-glossary-view]' ).forEach( ( view ) => {
		const table = view.querySelector( '[data-mimi-glossary-table]' );
		const payload = view.querySelector( '[data-mimi-glossary-data]' );
		const filter = view.querySelector( '[data-mimi-glossary-filter]' );
		const empty = view.querySelector( '[data-mimi-glossary-empty]' );
		const rest = view.querySelector( '[data-mimi-glossary-rest]' );
		if ( !table || !payload || !filter || !empty ) {
			return;
		}
		let entries;
		try {
			entries = JSON.parse( payload.textContent );
		} catch ( e ) {
			// The page as written is still a readable page: leave it alone.
			return;
		}
		// A page with nothing on it has already said so, in words that suit why
		// it is empty, a glossary nobody has written yet, or the page a split
		// one is filed under. Taking that over would replace it with a report
		// about a filter that has not been typed into.
		if ( !entries.length ) {
			return;
		}
		// The rest of the entries are here after all, so the notice saying they
		// are not goes.
		if ( rest ) {
			rest.remove();
		}

		// What an entry is searched by: lowercased once, then remembered.
		const haystacks = new Array( entries.length );
		function haystack( index ) {
			if ( haystacks[ index ] === undefined ) {
				const forms = entries[ index ][ 1 ];
				haystacks[ index ] = [ entries[ index ][ 0 ] ]
					.concat( forms.map( ( form ) => form[ 0 ] ) )
					.concat( forms.reduce( ( all, form ) => all.concat( form[ 1 ] ), [] ) )
					.join( ' ' )
					.toLowerCase();
			}
			return haystacks[ index ];
		}

		function cell( tag, className, text ) {
			const node = document.createElement( tag );
			node.className = className;
			if ( text !== undefined ) {
				node.textContent = text;
			}
			return node;
		}

		// One entry as its row group: the lemma in a cell spanning the group,
		// then a row per form. Kept in step with `renderStructuredView()`, which
		// writes the same rows for a reader without JavaScript.
		function group( index ) {
			const lemma = entries[ index ][ 0 ];
			const forms = entries[ index ][ 1 ];
			const body = document.createElement( 'tbody' );
			body.className = 'border-0 border-b border-[#c8ccd1]';
			forms.forEach( ( form, position ) => {
				const spelling = form[ 0 ];
				const translations = form[ 1 ];
				const row = document.createElement( 'tr' );
				row.className = position === forms.length - 1 ?
					'' :
					'border-0 border-b border-[#eaecf0]';
				if ( position === 0 ) {
					const head = cell(
						'th',
						'border-0 border-r border-[#eaecf0] px-4 py-3 text-left align-top text-sm font-semibold',
						lemma
					);
					head.setAttribute( 'scope', 'rowgroup' );
					head.rowSpan = forms.length;
					row.appendChild( head );
				}
				const spelt = cell( 'td', 'px-4 py-3 align-top text-sm' );
				if ( spelling === '' ) {
					// The lemma's own row: it stands for the dictionary form, so
					// it names no form of its own.
					const dash = cell( 'span', 'text-[#72777d]', '—' );
					dash.title = 'The lemma itself';
					spelt.appendChild( dash );
				} else {
					spelt.appendChild( cell( 'div', '', spelling ) );
				}
				row.appendChild( spelt );
				const meanings = cell( 'td', 'px-4 py-3 align-top text-sm' );
				if ( translations.length ) {
					translations.forEach( ( text ) => {
						meanings.appendChild( cell( 'div', '', text ) );
					} );
				} else {
					meanings.appendChild( cell( 'em', 'text-[#72777d]', 'No translation yet' ) );
				}
				row.appendChild( meanings );
				body.appendChild( row );
			} );
			return body;
		}

		// What stands in for a block until it is scrolled to: one empty row as
		// tall as the rows it is keeping the place of.
		function spacer( height ) {
			const body = document.createElement( 'tbody' );
			const row = document.createElement( 'tr' );
			const gap = document.createElement( 'td' );
			gap.colSpan = 3;
			gap.style.height = height + 'px';
			gap.style.padding = '0';
			row.appendChild( gap );
			body.appendChild( row );
			return body;
		}

		let blocks = [];

		function show( block ) {
			if ( block.shown ) {
				return;
			}
			const groups = block.entries.map( group );
			block.nodes[ 0 ].replaceWith( ...groups );
			block.nodes = groups;
			block.shown = true;
		}

		function hide( block ) {
			if ( !block.shown ) {
				return;
			}
			// Measured before it goes, so the space it leaves is the space it
			// took: an estimate here is a jump under the reader's scroll.
			block.height = block.nodes.reduce( ( total, node ) => total + node.offsetHeight, 0 );
			const placeholder = spacer( block.height );
			block.nodes[ 0 ].replaceWith( placeholder );
			block.nodes.slice( 1 ).forEach( ( node ) => node.remove() );
			block.nodes = [ placeholder ];
			block.shown = false;
		}

		// Which blocks are near enough to the viewport to be worth building.
		//
		// This asks each block where it is rather than being told, because a
		// block is far taller than any margin one could watch it through: an
		// entry alone can run to forty forms, and an IntersectionObserver
		// watching a block's first row calls a block gone the moment that row
		// leaves: with the rest of it still filling the screen. So the whole
		// span of a block, from the top of its first row group to the bottom of
		// its last, is what decides.
		//
		// Building a block that begins above the viewport lengthens the page
		// above the reader. Browsers undo that themselves, scroll anchoring is
		// on by default, and there is nothing here to switch it off.
		function update() {
			const min = -MARGIN;
			const max = window.innerHeight + MARGIN;
			blocks.forEach( ( block ) => {
				const first = block.nodes[ 0 ].getBoundingClientRect();
				const last = block.nodes[ block.nodes.length - 1 ].getBoundingClientRect();
				if ( last.bottom > min && first.top < max ) {
					show( block );
				} else {
					hide( block );
				}
			} );
		}

		let scheduled = false;
		function schedule() {
			if ( scheduled ) {
				return;
			}
			scheduled = true;
			requestAnimationFrame( () => {
				scheduled = false;
				update();
			} );
		}

		// Lay the table out for a list of entry indices: one block per
		// ENTRIES_PER_BLOCK of them, each a spacer until it is scrolled near.
		function layout( indices ) {
			blocks = [];
			table.querySelectorAll( 'tbody' ).forEach( ( body ) => body.remove() );
			for ( let start = 0; start < indices.length; start += ENTRIES_PER_BLOCK ) {
				const chosen = indices.slice( start, start + ENTRIES_PER_BLOCK );
				const guess = chosen.reduce( ( total, index ) => total + entries[ index ][ 1 ].reduce(
					( rows, form ) => rows + ROW_PADDING + Math.max( 1, form[ 1 ].length ) * LINE_HEIGHT,
					0
				), 0 );
				const block = {
					entries: chosen,
					height: guess,
					shown: false
				};
				blocks.push( block );
				block.nodes = [ spacer( block.height ) ];
				table.appendChild( block.nodes[ 0 ] );
			}
			empty.classList.toggle( 'hidden', indices.length > 0 );
			update();
		}

		layout( entries.map( ( entry, index ) => index ) );
		window.addEventListener( 'scroll', schedule, { passive: true } );
		window.addEventListener( 'resize', schedule, { passive: true } );

		let pending = null;
		filter.addEventListener( 'input', () => {
			// A keystroke should not walk seventy-five thousand strings, so the
			// walk waits for the typing to stop.
			clearTimeout( pending );
			pending = setTimeout( () => {
				const needle = filter.value.trim().toLowerCase();
				const matching = [];
				for ( let index = 0; index < entries.length; index++ ) {
					if ( needle === '' || haystack( index ).includes( needle ) ) {
						matching.push( index );
					}
				}
				layout( matching );
			}, 120 );
		} );
	} );
}() );
