export function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="rounded-2xl border border-border/80 bg-card/90 p-5 shadow-[var(--card-shadow)]">
      <h3 className="font-display mb-4 text-lg font-semibold">{title}</h3>
      {children}
    </section>
  );
}
