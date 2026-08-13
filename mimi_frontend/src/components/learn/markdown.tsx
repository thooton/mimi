/** The inline marks material text may carry — **bold** and *italic* — shared
    by the lesson player's teaching tasks and the map's tips dialog. */
export function markdown(text: string): React.ReactNode[] {
  return text.split(/(\*\*[^*]+\*\*|\*[^*]+\*)/).filter(Boolean).map((part, i) => {
    if (part.startsWith('**') && part.endsWith('**')) return <strong key={i}>{part.slice(2, -2)}</strong>;
    if (part.startsWith('*') && part.endsWith('*')) return <em key={i}>{part.slice(1, -1)}</em>;
    return <span key={i}>{part}</span>;
  });
}
