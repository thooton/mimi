/* Top-level item and menu removed from the active navigation. */
export const FUTURE_COMMUNITY_NAV = {
  item: { id: 'community', label: 'Community' },
  menu: [
    { href: '/community/clubs', label: 'Clubs' },
    { href: '/community/forums', label: 'Forums' },
    { href: '/community/chat', label: 'Chat match' },
  ],
} as const;

