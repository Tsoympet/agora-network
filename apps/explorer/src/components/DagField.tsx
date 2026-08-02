/** Full-bleed atmospheric BlockDAG field for the hero plane. */
export function DagField() {
  return (
    <div
      aria-hidden
      className="pointer-events-none absolute inset-0 overflow-hidden"
    >
      <div className="agora-glow absolute -top-24 left-1/2 h-[28rem] w-[40rem] -translate-x-1/2 rounded-full bg-[radial-gradient(circle,rgba(197,152,53,0.22),transparent_65%)]" />
      <svg
        className="agora-dag-drift absolute inset-0 h-full w-[110%] opacity-70"
        viewBox="0 0 1200 800"
        fill="none"
        xmlns="http://www.w3.org/2000/svg"
      >
        <g stroke="#C59835" strokeOpacity="0.35" strokeWidth="1.5">
          <path d="M120 520 C260 420 340 380 480 360" />
          <path d="M480 360 C620 330 700 300 860 280" />
          <path d="M480 360 C560 470 640 520 820 560" />
          <path d="M860 280 C940 360 980 420 1040 520" />
          <path d="M220 280 C320 300 400 320 480 360" />
          <path d="M300 620 C420 560 520 500 620 420" />
        </g>
        {[
          [120, 520],
          [220, 280],
          [480, 360],
          [620, 420],
          [860, 280],
          [820, 560],
          [1040, 520],
          [300, 620],
        ].map(([x, y], i) => (
          <g key={`${x}-${y}`}>
            <circle
              cx={x}
              cy={y}
              r={i === 2 ? 10 : 7}
              fill={i % 3 === 0 ? "#06BBDF" : "#C59835"}
              fillOpacity={i === 2 ? 0.95 : 0.75}
            />
            <circle
              cx={x}
              cy={y}
              r={i === 2 ? 22 : 16}
              stroke="#C59835"
              strokeOpacity="0.25"
            />
          </g>
        ))}
      </svg>
      <div className="absolute inset-x-0 bottom-0 h-40 bg-gradient-to-t from-[var(--agora-obsidian)] to-transparent" />
    </div>
  );
}
