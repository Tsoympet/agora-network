import { useEffect, useMemo, useRef, useState } from "react";
import { CameraView, useCameraPermissions } from "expo-camera";
import { Pressable, StyleSheet, Text, TextInput, View } from "react-native";
import { agoraBrand } from "../shared/brand/tokens";
import {
  PAIRING_GUIDE_STEPS,
  buildRestorePairing,
  buildWatchPairing,
  pairingImportBlocker,
  parsePairingPayload,
  qrMatrix,
  rpcTrustWarning,
  serializePairing,
  shortHash,
  watchWithRpc,
  type ParsedPairing,
  type WatchOnlyWallet,
} from "../shared/light-client";

function QrMark({ text, label }: { text: string; label: string }) {
  const matrix = useMemo(() => qrMatrix(text), [text]);
  const cell = matrix.size > 80 ? 3 : 4;
  const rows: boolean[][] = [];
  for (let y = 0; y < matrix.size; y += 1) {
    rows.push(matrix.dark.slice(y * matrix.size, (y + 1) * matrix.size));
  }
  return (
    <View
      accessibilityLabel={label}
      style={{ backgroundColor: "#f4f1ea", padding: 8, alignSelf: "flex-start", marginTop: 10 }}
    >
      {rows.map((row, y) => (
        <View key={y} style={{ flexDirection: "row" }}>
          {row.map((on, x) => (
            <View
              key={x}
              style={{ width: cell, height: cell, backgroundColor: on ? "#101218" : "#f4f1ea" }}
            />
          ))}
        </View>
      ))}
    </View>
  );
}

export function PairingPanel({
  network,
  genesisHash,
  rpcUrl,
  rpcToken,
  onSaveRpc,
  spendMnemonic,
  watchWallet,
  onImportWatch,
  onImportRestore,
  copyText,
}: {
  network: string | null;
  genesisHash: string | null;
  rpcUrl: string;
  rpcToken: string;
  onSaveRpc: (url: string, token: string) => Promise<void>;
  spendMnemonic: string;
  watchWallet: WatchOnlyWallet | null;
  onImportWatch: (watch: WatchOnlyWallet) => void;
  onImportRestore: (mnemonic: string) => void;
  copyText: (text: string) => Promise<boolean>;
}) {
  const [rpcDraft, setRpcDraft] = useState(rpcUrl);
  const [tokenDraft, setTokenDraft] = useState(rpcToken);
  const [rpcMsg, setRpcMsg] = useState<string | null>(null);
  const [revealArmed, setRevealArmed] = useState(false);
  const [restoreText, setRestoreText] = useState<string | null>(null);
  const [restoreError, setRestoreError] = useState<string | null>(null);
  const [importDraft, setImportDraft] = useState("");
  const [parsed, setParsed] = useState<ParsedPairing | null>(null);
  const [importError, setImportError] = useState<string | null>(null);
  const [importMsg, setImportMsg] = useState<string | null>(null);
  const [copyHint, setCopyHint] = useState<string | null>(null);
  const [scanning, setScanning] = useState(false);
  const [permission, requestPermission] = useCameraPermissions();
  const scanLock = useRef(false);

  useEffect(() => setRpcDraft(rpcUrl), [rpcUrl]);
  useEffect(() => setTokenDraft(rpcToken), [rpcToken]);
  useEffect(() => {
    setRestoreText(null);
    setRevealArmed(false);
  }, [spendMnemonic]);

  const watchExport = useMemo(() => {
    if (!network || !genesisHash) return null;
    try {
      const watch = spendMnemonic.trim()
        ? buildWatchPairing({
            mnemonic: spendMnemonic,
            network,
            genesis: genesisHash,
            rpcUrl,
          })
        : watchWallet
          ? watchWithRpc(watchWallet, rpcUrl)
          : null;
      if (!watch) return null;
      return { text: serializePairing(watch), omittedLoopback: watch.rpcUrl === null };
    } catch {
      return null;
    }
  }, [genesisHash, network, rpcUrl, spendMnemonic, watchWallet]);

  let draftWarning: string | null = null;
  let draftError: string | null = null;
  try {
    draftWarning = rpcTrustWarning(rpcDraft);
  } catch (err) {
    draftError = err instanceof Error ? err.message : "RPC URL must be http or https";
  }

  const blocker = parsed
    ? pairingImportBlocker(
        parsed.kind === "watch" ? parsed.watch : parsed.restore,
        { network, genesis: genesisHash },
      )
    : null;

  async function onSave() {
    setRpcMsg(null);
    try {
      await onSaveRpc(rpcDraft, tokenDraft);
      setRpcMsg("RPC endpoint saved on this device.");
    } catch (err) {
      setRpcMsg(err instanceof Error ? err.message : "Could not save RPC URL");
    }
  }

  function onReveal() {
    if (!revealArmed || !spendMnemonic.trim() || !network || !genesisHash) return;
    try {
      setRestoreText(
        serializePairing(
          buildRestorePairing({
            mnemonic: spendMnemonic,
            network,
            genesis: genesisHash,
          }),
        ),
      );
      setRestoreError(null);
    } catch (err) {
      setRestoreText(null);
      setRestoreError(err instanceof Error ? err.message : "Could not build restore code");
    }
  }

  function stage(raw: string) {
    if (!raw.trim()) {
      setParsed(null);
      setImportDraft("");
      setImportError(null);
      return;
    }
    try {
      const next = parsePairingPayload(raw);
      setParsed(next);
      setImportDraft("");
      setImportError(null);
      setImportMsg(null);
    } catch (err) {
      setParsed(null);
      setImportDraft(raw);
      setImportError(err instanceof Error ? err.message : "malformed pairing payload");
    }
  }

  function onImport() {
    if (!parsed || blocker) return;
    if (parsed.kind === "watch") {
      onImportWatch(parsed.watch);
      setImportMsg("Watch-only imported. This phone can check balances and cannot spend.");
    } else {
      onImportRestore(parsed.restore.mnemonic);
      setImportMsg(
        "Mnemonic restored in memory. Anyone with these words can spend. Save the vault, then hide the words.",
      );
    }
    setParsed(null);
  }

  async function openScanner() {
    scanLock.current = false;
    if (!permission?.granted) {
      const next = await requestPermission();
      if (!next.granted) {
        setImportError("Camera permission is off. Paste the pairing payload instead.");
        return;
      }
    }
    setScanning(true);
  }

  return (
    <View>
      <Text style={styles.eyebrow}>Pair with another device</Text>
      <Text style={styles.meta}>Device to device. Keys stay on the devices. There is no Agora login.</Text>
      {PAIRING_GUIDE_STEPS.map((step, index) => (
        <Text key={step} style={styles.meta}>
          {index + 1}. {step}
        </Text>
      ))}
      <Text style={styles.meta}>
        This node: {network ?? "not connected"}
        {genesisHash ? ` · genesis ${shortHash(genesisHash)}` : " · genesis not reported yet"}
      </Text>

      <TextInput
        value={rpcDraft}
        onChangeText={setRpcDraft}
        placeholder="https://your-node.example:8545/rpc"
        placeholderTextColor={agoraBrand.colors.inkMuted}
        autoCapitalize="none"
        autoCorrect={false}
        style={styles.input}
      />
      <TextInput
        value={tokenDraft}
        onChangeText={setTokenDraft}
        placeholder="Node token (optional, stays on this phone)"
        placeholderTextColor={agoraBrand.colors.inkMuted}
        secureTextEntry
        autoCapitalize="none"
        autoCorrect={false}
        style={styles.input}
      />
      <Pressable onPress={() => void onSave()} style={styles.button}>
        <Text style={styles.buttonLabel}>Save RPC endpoint</Text>
      </Pressable>
      {draftError ? <Text style={styles.error}>{draftError}</Text> : null}
      {draftWarning ? <Text style={styles.warn}>{draftWarning}</Text> : null}
      {rpcMsg ? <Text style={styles.meta}>{rpcMsg}</Text> : null}

      <Text style={styles.subhead}>Watch-only (recommended for travel)</Text>
      <Text style={styles.meta}>
        Public TLT receive and change addresses, the OVL account id, the DRC account id, and the
        account xpub. The other device can check balances away from home and cannot spend.
      </Text>
      {watchExport ? (
        <>
          <QrMark text={watchExport.text} label="Watch-only pairing code" />
          {watchExport.omittedLoopback ? (
            <Text style={styles.meta}>
              This device&apos;s RPC is localhost, so it was left out of the code. Set a LAN or public
              RPC on the phone before you leave home.
            </Text>
          ) : null}
          <Pressable
            onPress={() => {
              void copyText(watchExport.text).then((ok) => {
                setCopyHint(ok ? "Copied watch-only payload" : "Clipboard unavailable");
              });
            }}
            style={styles.button}
          >
            <Text style={styles.buttonLabel}>Copy watch-only payload</Text>
          </Pressable>
        </>
      ) : (
        <Text style={styles.meta}>
          Unlock or create a wallet, and wait for genesis, to show a watch-only code.
        </Text>
      )}

      <Text style={styles.subhead}>Same-spend restore</Text>
      <Text style={styles.error}>
        Anyone who sees this mnemonic can spend. The reveal-once code stays on this screen only
        until you hide it. It is not sent to a node.
      </Text>
      {spendMnemonic.trim() && network && genesisHash ? (
        <>
          <Pressable onPress={() => setRevealArmed((armed) => !armed)} style={styles.button}>
            <Text style={styles.buttonLabel}>
              {revealArmed ? "Spend warning acknowledged" : "I understand anyone with the mnemonic can spend"}
            </Text>
          </Pressable>
          <Pressable onPress={onReveal} disabled={!revealArmed} style={styles.button}>
            <Text style={styles.buttonLabel}>Show restore code once</Text>
          </Pressable>
          {restoreText ? (
            <>
              <QrMark text={restoreText} label="Reveal-once restore code" />
              <Pressable
                onPress={() => {
                  void copyText(restoreText).then((ok) => {
                    setCopyHint(
                      ok
                        ? "Copied restore payload. Hide it after the other device imports."
                        : "Clipboard unavailable",
                    );
                  });
                }}
                style={styles.button}
              >
                <Text style={styles.buttonLabel}>Copy restore payload</Text>
              </Pressable>
              <Pressable
                onPress={() => {
                  setRestoreText(null);
                  setRevealArmed(false);
                }}
                style={styles.button}
              >
                <Text style={styles.buttonLabel}>Hide restore code</Text>
              </Pressable>
            </>
          ) : null}
        </>
      ) : (
        <Text style={styles.meta}>
          Unlock the spend wallet on this phone to reveal a restore code. Watch-only cannot create one.
        </Text>
      )}
      {restoreError ? <Text style={styles.error}>{restoreError}</Text> : null}

      <Text style={styles.subhead}>Import a pairing payload</Text>
      <TextInput
        value={importDraft}
        onChangeText={stage}
        placeholder="Paste a watch-only or restore payload"
        placeholderTextColor={agoraBrand.colors.inkMuted}
        autoCapitalize="none"
        autoCorrect={false}
        multiline
        style={[styles.input, styles.payload]}
      />
      <Pressable onPress={() => void openScanner()} style={styles.button}>
        <Text style={styles.buttonLabel}>{scanning ? "Scanning…" : "Scan pairing QR"}</Text>
      </Pressable>
      {scanning ? (
        <CameraView
          style={styles.camera}
          facing="back"
          barcodeScannerSettings={{ barcodeTypes: ["qr"] }}
          onBarcodeScanned={({ data }) => {
            if (scanLock.current || !data) return;
            scanLock.current = true;
            setScanning(false);
            stage(data);
          }}
        />
      ) : null}
      {importError ? <Text style={styles.error}>{importError}</Text> : null}
      {parsed?.kind === "watch" ? (
        <View>
          <Text style={styles.meta}>Watch-only for {parsed.watch.network}.</Text>
          <Text style={styles.mono}>TLT receive {parsed.watch.tltReceive}</Text>
          <Text style={styles.mono}>TLT change {parsed.watch.tltChange}</Text>
          <Text style={styles.mono}>OVL account {parsed.watch.ovlAccount}</Text>
          <Text style={styles.mono}>DRC account {parsed.watch.drcAccount}</Text>
          <Text style={styles.meta}>Genesis {shortHash(parsed.watch.genesis)}</Text>
        </View>
      ) : null}
      {parsed?.kind === "restore" ? (
        <Text style={styles.meta}>
          Same-spend restore for {parsed.restore.network}. Genesis {shortHash(parsed.restore.genesis)}.
          The mnemonic is held for import and is not shown.
        </Text>
      ) : null}
      {parsed && blocker ? <Text style={styles.error}>{blocker}</Text> : null}
      {parsed && !blocker ? <Text style={styles.meta}>Network and genesis match this node.</Text> : null}
      {parsed ? (
        <View style={styles.row}>
          <Pressable onPress={onImport} disabled={!!blocker} style={styles.button}>
            <Text style={styles.buttonLabel}>Import</Text>
          </Pressable>
          <Pressable
            onPress={() => {
              setParsed(null);
              setImportDraft("");
            }}
            style={styles.button}
          >
            <Text style={styles.buttonLabel}>Discard</Text>
          </Pressable>
        </View>
      ) : null}
      {importMsg ? <Text style={styles.meta}>{importMsg}</Text> : null}
      {copyHint ? <Text style={styles.meta}>{copyHint}</Text> : null}
    </View>
  );
}

const styles = StyleSheet.create({
  eyebrow: {
    marginTop: 36,
    color: agoraBrand.colors.goldSoft,
    fontFamily: "Cinzel",
    fontSize: 12,
    letterSpacing: 2,
    textTransform: "uppercase",
  },
  subhead: {
    marginTop: 22,
    color: agoraBrand.colors.gold,
    fontFamily: "Cinzel",
    fontSize: 16,
  },
  meta: {
    marginTop: 10,
    color: agoraBrand.colors.inkMuted,
    fontFamily: "Inter",
    fontSize: 15,
  },
  warn: {
    marginTop: 8,
    color: agoraBrand.colors.gold,
    fontFamily: "Inter",
    fontSize: 14,
  },
  error: {
    marginTop: 8,
    color: "#d65a5a",
    fontFamily: "Inter",
    fontSize: 14,
  },
  mono: {
    marginTop: 6,
    color: agoraBrand.colors.ink,
    fontFamily: "monospace",
    fontSize: 12,
  },
  input: {
    marginTop: 12,
    borderWidth: 1,
    borderColor: "rgba(197, 152, 53, 0.35)",
    borderRadius: 8,
    paddingHorizontal: 12,
    paddingVertical: 10,
    color: agoraBrand.colors.ink,
    fontFamily: "monospace",
    fontSize: 13,
  },
  payload: {
    minHeight: 72,
    textAlignVertical: "top",
  },
  button: {
    marginTop: 8,
    alignSelf: "flex-start",
    borderWidth: 1,
    borderColor: "rgba(6, 187, 223, 0.45)",
    borderRadius: 8,
    paddingHorizontal: 12,
    paddingVertical: 8,
  },
  buttonLabel: {
    color: agoraBrand.colors.cyan,
    fontFamily: "Inter",
    fontSize: 14,
  },
  row: {
    flexDirection: "row",
    gap: 10,
  },
  camera: {
    marginTop: 12,
    width: "100%",
    height: 240,
    borderRadius: 8,
    overflow: "hidden",
  },
});
