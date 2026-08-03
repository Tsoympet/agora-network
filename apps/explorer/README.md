# Agora Explorer

Web BlockDAG surface using the Agora Brand System (Obsidian & Gold).

## Dev

```bash
# Terminal A — node with RPC
AGORA_TEMPLATE_BITS=0 cargo run -p agora-node

# Terminal B — explorer
cd apps/explorer
npm install
npm run dev
```

Open `http://127.0.0.1:5173`. The **Live DAG** section polls `agora_getDagTips` + `agora_getBlock` every 2s (override with `VITE_AGORA_POLL_MS`). **Tx lookup** (`#tx`) calls `agora_getTransaction` and polls while status is `pending` (Live DAG block txs deep-link here). **Mempool** (`#mempool`) polls `agora_getMempool` and links rows into `#tx`.

| Env | Default | Meaning |
| --- | --- | --- |
| `VITE_AGORA_RPC_URL` | `/rpc` | JSON-RPC endpoint (vite proxies to node) |
| `VITE_AGORA_POLL_MS` | `2000` | Tip / pending-tx poll interval |
| `AGORA_RPC_PROXY` | `http://127.0.0.1:8545` | Vite proxy target for `/rpc` |

Brand source: `apps/shared/brand/Agora_Brand_System.css`  
Marks: Talanton (TLT), Drachma (DRC), Ovolos (OBL) + Nexus icon.
