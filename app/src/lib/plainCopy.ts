/** `value` with its objects and arrays copied as plain ones, so
 * `postMessage` can clone it: a component's `$state` (a Proxy) passed
 * straight to `invoke` would fail there with a DataCloneError, where
 * Tauri's own `invoke` (JSON) takes it. Other values are kept as they are. */
export function plainCopy<T>(value: T): T {
  if (Array.isArray(value)) return value.map(plainCopy) as T;
  if (value === null || typeof value !== 'object') return value;
  const proto = Object.getPrototypeOf(value);
  if (proto !== Object.prototype && proto !== null) return value;
  const out: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(value)) out[k] = plainCopy(v);
  return out as T;
}
