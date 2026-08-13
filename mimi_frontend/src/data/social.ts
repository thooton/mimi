/** locale-independent thousands separator (keeps SSR and client HTML identical) */
export function formatXp(n: number): string {
  return String(n).replace(/\B(?=(\d{3})+(?!\d))/g, ',');
}
