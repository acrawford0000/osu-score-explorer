function normalizeStatistics(values, mode) {
  const normalized = Object.fromEntries(Object.entries(values).map(([key,value]) => [
    key.includes('_') || key[0] === key[0]?.toLowerCase()
      ? key.split('_').map(part => part[0].toUpperCase()+part.slice(1)).join('') : key,
    value,
  ]));
  // Older standard replay metadata called the 100 judgment Good; modern API records call it Ok.
  if (mode === 0 && Object.hasOwn(values, 'Good') && !Object.hasOwn(values, 'Ok')) normalized.Ok = values.Good;
  return normalized;
}
module.exports = { normalizeStatistics };
