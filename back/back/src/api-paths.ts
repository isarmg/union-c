export const pathSegment = (value: string | number) =>
  encodeURIComponent(String(value));

export const ramInstancePath = (id: string) =>
  `/api/services/ram/instances/${pathSegment(id)}`;

export const sunshineHostPath = (id: string) =>
  `/api/services/sunshine/hosts/${pathSegment(id)}`;

export const pveHostPath = (id: string) => `/api/pve/hosts/${pathSegment(id)}`;

export const pveNodePath = (id: string, node: string) =>
  `${pveHostPath(id)}/nodes/${pathSegment(node)}`;

export const pveVmPath = (
  id: string,
  node: string,
  vmid: number,
  collection: "vms" | "containers"
) => `${pveNodePath(id, node)}/${collection}/${pathSegment(vmid)}`;

export const pveSnapshotPath = (
  id: string,
  node: string,
  vmid: number,
  collection: "vms" | "containers",
  snap: string
) => `${pveVmPath(id, node, vmid, collection)}/snapshots/${pathSegment(snap)}`;
