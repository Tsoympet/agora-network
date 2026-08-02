const marks = [
  {
    code: "TLT",
    name: "Talanton",
    meaning: "Scales",
    src: "/brand/talanton.svg",
  },
  {
    code: "DRC",
    name: "Drachma",
    meaning: "Helmet",
    src: "/brand/drachma.svg",
  },
  {
    code: "OBL",
    name: "Ovolos",
    meaning: "Shield / Spears",
    src: "/brand/ovolos.svg",
  },
] as const;

export function MarkRow() {
  return (
    <ul className="mt-12 grid gap-10 md:grid-cols-3">
      {marks.map((mark) => (
        <li key={mark.code} className="flex flex-col items-start gap-4">
          <img src={mark.src} alt="" className="agora-icon-lg" />
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
