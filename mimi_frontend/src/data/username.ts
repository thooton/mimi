/* This is immediate signup feedback, not the authority for the rule. The
   credential service applies the same list to every registration regardless
   of which consumer sent it; keep this mirror aligned with mimi_auth's
   reservedUsernameTerms. */
const RESERVED_USERNAME_TERMS = [
  'administrator',
  'bureaucrat',
  'steward',
  'checkuser',
  'oversight',
  'admin',
  'sysop',
  'moderator',
] as const;

export function usernameContainsReservedTerm(username: string): boolean {
  const folded = username.toLowerCase();
  return RESERVED_USERNAME_TERMS.some((term) => folded.includes(term));
}
