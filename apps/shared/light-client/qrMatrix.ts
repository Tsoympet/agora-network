/**
 * QR modules for pairing payloads. Callers must not log the input when it is a
 * restore mnemonic; only the module matrix is meant for the screen.
 */
import { encode } from "uqr";

export type QrMatrix = {
  size: number;
  /** Row-major, true = dark module. */
  dark: boolean[];
};

export function qrMatrix(text: string): QrMatrix {
  if (!text) throw new Error("could not build pairing code");
  let encoded: { size: number; data: boolean[][] };
  try {
    encoded = encode(text, { ecc: "M", border: 2 });
  } catch {
    throw new Error("could not build pairing code");
  }
  if (!encoded?.data || encoded.data.length !== encoded.size || encoded.size < 21) {
    throw new Error("could not build pairing code");
  }
  const dark: boolean[] = [];
  for (const row of encoded.data) {
    if (row.length !== encoded.size) throw new Error("could not build pairing code");
    for (const cell of row) dark.push(cell === true);
  }
  return { size: encoded.size, dark };
}
