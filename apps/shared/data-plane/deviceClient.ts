import { createInfrastructureFacades, type InfrastructureFacades, type InfrastructureFetch } from "./facades.ts";

export type { DataPlane } from "./planes.ts";
export { DATA_PLANES, DEVICE_FORBIDDEN_IMPLEMENTATIONS, planeLabel } from "./planes.ts";
export { DATA_PLANE_FEATURES, featureById, type FeatureDescriptor } from "./features.ts";
export {
  InfrastructureResponseError,
  InfrastructureUnconfigured,
  createInfrastructureFacades,
  type InfrastructureFacades,
  type InfrastructureFetch,
} from "./facades.ts";

/**
 * The object PC, phone, and explorer code should hold.
 * `configured` is false until the operator sets an infrastructure origin.
 */
export function createDeviceInfrastructureClient(options: {
  baseUrl?: string | null;
  fetchImpl?: InfrastructureFetch;
}): { configured: boolean; baseUrl: string | null; facades: InfrastructureFacades } {
  const baseUrl = options.baseUrl?.replace(/\/$/, "") ?? "";
  return {
    configured: baseUrl.length > 0,
    baseUrl: baseUrl || null,
    facades: createInfrastructureFacades({ baseUrl, fetchImpl: options.fetchImpl }),
  };
}
