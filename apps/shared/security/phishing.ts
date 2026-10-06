/**
 * Warnings shown before a user trusts a node address.
 * These are hints. They do not prove the remote is honest.
 */

export function phishingWarnings(nodeUrl: string, expectedHost?: string): string[] {
  const warnings: string[] = [];
  let url: URL;
  try {
    url = new URL(nodeUrl);
  } catch {
    return ["This node address is not valid. Do not sign against it."];
  }
  if (url.username || url.password) {
    warnings.push("This node address embeds credentials. Remove them before connecting.");
  }
  if (url.protocol !== "https:" && url.protocol !== "http:") {
    warnings.push("Only http and https node addresses are accepted.");
  }
  if (url.protocol === "http:") {
    warnings.push("Plain HTTP lets a network attacker change what this device is shown.");
  }
  if (url.hostname === "localhost" || url.hostname === "127.0.0.1" || url.hostname === "::1") {
    warnings.push("Localhost is this machine only. Another device cannot use it.");
  }
  if (url.hostname.startsWith("xn--") || /[^\u0000-\u007f]/.test(url.hostname)) {
    warnings.push("This host uses punycode or non-ASCII characters. Confirm it is the node you chose.");
  }
  if (expectedHost && url.hostname !== expectedHost) {
    warnings.push(`This host does not match the saved endpoint (${expectedHost}).`);
  }
  return warnings;
}
