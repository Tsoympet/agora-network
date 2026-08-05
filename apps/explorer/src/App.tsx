import { DagField } from "./components/DagField";
import { GovernancePanel } from "./components/GovernancePanel";
import { LiveDag } from "./components/LiveDag";
import { MarkRow } from "./components/MarkRow";
import { MempoolPanel } from "./components/MempoolPanel";
import { NetworkBadge } from "./components/NetworkBadge";
import { NodeStatus } from "./components/NodeStatus";
import { TxLookup } from "./components/TxLookup";

export default function App() {
  return (
    <div className="relative min-h-screen overflow-x-hidden">
      <header className="absolute inset-x-0 top-0 z-20 flex items-center justify-between px-6 py-5 md:px-10">
        <div className="flex flex-col gap-2">
          <div className="flex items-center gap-3">
            <img
              src="/brand/agora-network.png"
              alt=""
              className="h-10 w-10 agora-rise"
            />
            <span className="agora-brand text-lg tracking-[0.14em] agora-rise agora-rise-delay-1">
              AGORA
            </span>
          </div>
          <div className="agora-rise agora-rise-delay-1 pl-[3.25rem]">
            <NetworkBadge />
          </div>
        </div>
        <nav className="flex items-center gap-4 agora-rise agora-rise-delay-2">
          <a href="#live" className="agora-btn agora-btn-ghost text-sm">
            Live DAG
          </a>
          <a href="#tx" className="agora-btn agora-btn-ghost text-sm">
            Tx lookup
          </a>
          <a href="#mempool" className="agora-btn agora-btn-ghost text-sm">
            Mempool
          </a>
          <a href="#node" className="agora-btn agora-btn-ghost text-sm">
            Node
          </a>
          <a href="#gov" className="agora-btn agora-btn-ghost text-sm">
            Ballot
          </a>
          <a href="#marks" className="agora-btn agora-btn-ghost text-sm">
            Marks
          </a>
        </nav>
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
              <a className="agora-btn agora-btn-primary" href="#live">
                Watch the tips
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
          id="live"
          className="relative border-t border-[var(--agora-line)] px-6 py-20 md:px-10"
        >
          <LiveDag />
        </section>

        <section
          id="tx"
          className="relative border-t border-[var(--agora-line)] px-6 py-20 md:px-10"
        >
          <TxLookup />
        </section>

        <section
          id="mempool"
          className="relative border-t border-[var(--agora-line)] px-6 py-20 md:px-10"
        >
          <MempoolPanel />
        </section>

        <section
          id="node"
          className="relative border-t border-[var(--agora-line)] px-6 py-20 md:px-10"
        >
          <NodeStatus />
        </section>

        <section
          id="gov"
          className="relative border-t border-[var(--agora-line)] px-6 py-20 md:px-10"
        >
          <GovernancePanel />
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
