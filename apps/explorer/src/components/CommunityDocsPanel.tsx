import {
  COMMUNITY_DOCS,
  communityDocHref,
} from "../../../shared/light-client/communityDocs";

export function CommunityDocsPanel() {
  return (
    <div>
      <p className="agora-eyebrow">Docs / About</p>
      <h2 className="agora-display mt-3 text-3xl md:text-4xl">Repository documents</h2>
      <p className="agora-lede mt-4 max-w-3xl">
        Static links to the community and light-client notes. This list does not
        query a node. MY AGORA beginner home is PLANNED.
      </p>
      <ul className="mt-8 grid gap-4 md:grid-cols-2">
        {COMMUNITY_DOCS.map((doc) => (
          <li
            key={doc.id}
            className="border border-[var(--agora-line)] p-4"
          >
            <a
              className="text-[var(--agora-gold)]"
              href={communityDocHref(doc.path)}
              target="_blank"
              rel="noreferrer"
            >
              {doc.title}
            </a>
            <p className="mt-2 text-sm text-[var(--agora-cyan)]">{doc.maturity}</p>
            <p className="mt-2 text-sm text-[var(--agora-ink)]">{doc.summary}</p>
            <p className="mt-2 font-mono text-xs text-[var(--agora-ink-muted)]">{doc.path}</p>
          </li>
        ))}
      </ul>
    </div>
  );
}
