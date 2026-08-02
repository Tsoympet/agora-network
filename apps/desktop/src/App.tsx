export function App() {
  return (
    <main className="agora-shell">
      <img src="/nexus-icon.svg" alt="" className="agora-icon-lg agora-rise" />
      <h1 className="agora-brand agora-rise agora-rise-delay-1" style={{ fontSize: "2.75rem", marginTop: "1.25rem" }}>
        Agora Network
      </h1>
      <p className="agora-lede agora-rise agora-rise-delay-2" style={{ marginTop: "0.85rem" }}>
        Desktop wallet shell with RandomX sidecar hooks. Brand system and Nexus
        icon are wired for Tauri packaging.
      </p>
    </main>
  );
}
