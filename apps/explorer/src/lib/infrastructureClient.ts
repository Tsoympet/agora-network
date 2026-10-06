import { createDeviceInfrastructureClient } from "../../../shared/data-plane/deviceClient";

const baseUrl = (import.meta.env.VITE_AGORA_INFRA_URL as string | undefined) ?? "";

/**
 * Explorer calls infrastructure over HTTP when an origin is configured.
 * Chain rows still come from the full node through the light client.
 */
export const infrastructureClient = createDeviceInfrastructureClient({ baseUrl });
