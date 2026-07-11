export const pathSegment = (value: string | number) => encodeURIComponent(String(value));
export const sunshineHostPath = (id: string) => `/api/services/sunshine/hosts/${pathSegment(id)}`;
export const monitoringHostPath = (id: string) => `/api/monitoring/hosts/${pathSegment(id)}`;
