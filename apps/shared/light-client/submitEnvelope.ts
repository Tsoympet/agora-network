import type { BuiltEnvelope } from "./envelopes";
import type { LightClient } from "./rpc";

export async function submitBuiltEnvelope(
  client: LightClient,
  envelope: BuiltEnvelope,
): Promise<unknown> {
  return client.call(envelope.method, { [envelope.paramKey]: envelope.body });
}
