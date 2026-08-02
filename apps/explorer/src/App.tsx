import { DagField } from "./components/DagField";
import { MarkRow } from "./components/MarkRow";

export default function App() {
  return (
    <div className="relative min-h-screen overflow-x-hidden">
      <header className="absolute inset-x-0 top-0 z-20 flex items-center justify-between px-6 py-5 md:px-10">
        <div className="flex items-center gap-3">
          <img
            src="/brand/nexus-icon.svg"
            alt=""
            className="h-10 w-10 agora-rise"
          />
          <span className="agora-brand text-lg tracking-[0.14em] agora-rise agora-rise-delay-1">
            AGORA
          </span>
        </div>
        <a href="#marks" className="agora-btn agora-btn-ghost text-sm agora-rise agora-rise-delay-2">
          Marks
        </a>
      </header>

      <main>
        <section className="relative min-h-screen w-full">
          <DagField />
          <div className="relative z-10 flex min-h-screen flex-col justify-end px-6 pb-16 pt-28 md:px-10 md:pb-24">
            <p className="agora-eyebrow agora-rise">Sovereign BlockDAG</p>
            <h1 className="agora-brand mt-4 max-w-4xl text-5xl leading-[1.05] md:text-7xl agora-rise agora-rise-delay-1">
              Agora Network
            </h1>
            <p className="agora-lede mt-5 agora-rise agora-rise-delay-2">
              Sub-second ordering across a gold-lit dag of parallel blocks—built
              for wallets, miners, and the public square.
            </p>
            <div className="mt-8 flex flex-wrap gap-3 agora-rise agora-rise-delay-3">
              <a className="agora-btn agora-btn-primary" href="#marks">
                Enter the square
              </a>
              <a
                className="agora-btn agora-btn-ghost"
                href="https://github.com/Tsoympet/agora-network"
                target="_blank"
                rel="noreferrer"
              >
                Source
              </a>
            </div>
          </div>
        </section>

        <section
          id="marks"
          className="relative border-t border-[var(--agora-line)] px-6 py-20 md:px-10"
        >
          <p className="agora-eyebrow">Identity marks</p>
          <h2 className="agora-display mt-3 text-3xl md:text-4xl">
            Talanton · Drachma · Ovolos
          </h2>
          <p className="agora-lede mt-4">
            Three marks for scales, helm, and shield—anchors of Agora’s civic
            language across desktop, mobile, and explorer.
          </p>
          <MarkRow />
        </section>
      </main>
    </div>
  );
}
