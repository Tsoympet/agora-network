import { COMMUNITY_DOCS, communityDocHref } from "../../../shared/light-client/communityDocs";

export function DocsAboutPanel() {
  return (
    <section className="agora-rise" style={{ marginTop: "2.75rem" }}>
      <p className="agora-eyebrow">Docs / About</p>
      <h2
        style={{
          marginTop: "0.4rem",
          fontFamily: "var(--font-display, Cinzel, serif)",
          fontSize: "1.6rem",
          color: "var(--agora-gold)",
        }}
      >
        Repository documents
      </h2>
      <p style={{ marginTop: "0.65rem", maxWidth: 640, color: "var(--agora-ink-muted)" }}>
        Links open the markdown notes for this wallet. The list is static. It does not
        query a node, and it does not add balances or program screens. MY AGORA beginner
        home is PLANNED.
      </p>
      <ul
        style={{
          listStyle: "none",
          padding: 0,
          marginTop: "1.25rem",
          display: "grid",
          gap: "0.75rem",
        }}
      >
        {COMMUNITY_DOCS.map((doc) => (
          <li
            key={doc.id}
            style={{
              border: "1px solid color-mix(in srgb, var(--agora-gold) 35%, transparent)",
              padding: "0.75rem 0.9rem",
            }}
          >
            <a
              href={communityDocHref(doc.path)}
              target="_blank"
              rel="noreferrer"
              style={{ color: "var(--agora-gold)", textDecoration: "none" }}
            >
              {doc.title}
            </a>
            <p style={{ margin: "0.35rem 0 0", color: "var(--agora-cyan)", fontSize: "0.8rem" }}>
              {doc.maturity}
            </p>
            <p style={{ margin: "0.35rem 0 0", color: "var(--agora-ink)" }}>{doc.summary}</p>
            <p
              style={{
                margin: "0.35rem 0 0",
                fontFamily: "ui-monospace, monospace",
                fontSize: "0.75rem",
                color: "var(--agora-ink-muted)",
              }}
            >
              {doc.path}
            </p>
          </li>
        ))}
      </ul>
    </section>
  );
}
