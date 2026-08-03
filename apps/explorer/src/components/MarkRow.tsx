const marks = [
  {
    code: "TLT",
    name: "Talanton",
    meaning: "L1 native · 100M max",
    src: "/brand/talanton.png",
  },
  {
    code: "DRC",
    name: "Drachma",
    meaning: "L2+ medium · 6B max",
    src: "/brand/drachma.png",
  },
  {
    code: "OVL",
    name: "Ovolos",
    meaning: "L2 rollup · 21B max",
    src: "/brand/ovolos.png",
  },
] as const;

export function MarkRow() {
  return (
    <ul className="mt-12 grid gap-10 md:grid-cols-3">
      {marks.map((mark) => (
        <li key={mark.code} className="flex flex-col items-start gap-4">
          <img src={mark.src} alt={`${mark.name} (${mark.code})`} className="agora-icon-lg" />
          <div>
            <p className="agora-eyebrow">{mark.code}</p>
            <h3 className="agora-display mt-2 text-2xl">{mark.name}</h3>
            <p className="mt-2 text-mist">{mark.meaning}</p>
          </div>
        </li>
      ))}
    </ul>
  );
}
